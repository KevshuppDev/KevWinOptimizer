//! RENDIMIENTO → ENERGÍA: planes de energía, archivo de paginación y almacenamiento reservado.

use crate::*;
use eframe::egui::{self, Ui};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::thread;

// ---------------------------------------------------------------------
//  Planes de energía (powercfg)
// ---------------------------------------------------------------------

#[derive(Clone)]
pub struct Plan {
    pub guid: String,
    pub name: String,
    pub active: bool,
}

/// powercfg escribe en la página de códigos OEM (850): traducir lo típico del español.
fn cp850(b: &[u8]) -> String {
    b.iter()
        .map(|&c| match c {
            0x00..=0x7f => c as char,
            0xa0 => 'á', 0x82 => 'é', 0xa1 => 'í', 0xa2 => 'ó', 0xa3 => 'ú',
            0xa4 => 'ñ', 0xa5 => 'Ñ', 0x81 => 'ü', 0xb5 => 'Á', 0x90 => 'É',
            0xd6 => 'Í', 0xe0 => 'Ó', 0xe9 => 'Ú',
            _ => '?',
        })
        .collect()
}

pub fn power_plans() -> Vec<Plan> {
    let Ok(o) = Command::new("powercfg").arg("/list").creation_flags(CREATE_NO_WINDOW).output() else { return Vec::new() };
    cp850(&o.stdout)
        .lines()
        .filter_map(|l| {
            let guid = l.split_whitespace().find(|w| w.len() == 36 && w.matches('-').count() == 4)?.to_string();
            let name = l.split_once('(')?.1.rsplit_once(')')?.0.trim().to_string();
            Some(Plan { guid, name, active: l.trim_end().ends_with('*') })
        })
        .collect()
}

pub fn set_plan(guid: &str) -> bool {
    run_status("powercfg", &["/setactive", guid]).0
}

// ---------------------------------------------------------------------
//  Paginación y almacenamiento reservado
// ---------------------------------------------------------------------

const MEM_SCRIPT: &str = r"$ErrorActionPreference='SilentlyContinue';
$cs=Get-CimInstance Win32_ComputerSystem; 'AUTO|' + $cs.AutomaticManagedPagefile; 'RAM|' + [int64]($cs.TotalPhysicalMemory/1MB);
foreach($p in Get-CimInstance Win32_PageFileSetting){ 'SET|' + $p.Name + '|' + $p.InitialSize + '|' + $p.MaximumSize };
foreach($u in Get-CimInstance Win32_PageFileUsage){ 'USE|' + $u.Name + '|' + $u.AllocatedBaseSize + '|' + $u.CurrentUsage + '|' + $u.PeakUsage };
$r=Get-WindowsReservedStorageState; if($r){ 'RES|' + $r.ReservedStorageState }";

#[derive(Default, Clone)]
pub struct MemInfo {
    auto: bool,
    ram_mb: u64,
    /// (archivo, inicial MB, máximo MB) configurado
    set: Vec<(String, u64, u64)>,
    /// (archivo, tamaño actual MB, en uso MB, pico MB)
    usage: Vec<(String, u64, u64, u64)>,
    reserved: Option<bool>,
}

fn load_mem() -> MemInfo {
    let (_, out) = run_status("powershell", &["-NoProfile", "-Command", &MEM_SCRIPT.replace('\n', " ")]);
    let mut m = MemInfo::default();
    let n = |s: Option<&&str>| s.and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0);
    for l in &out {
        let f: Vec<&str> = l.split('|').collect();
        match f[0] {
            "AUTO" => m.auto = f.get(1) == Some(&"True"),
            "RAM" => m.ram_mb = n(f.get(1)),
            "SET" => m.set.push((f.get(1).unwrap_or(&"").to_string(), n(f.get(2)), n(f.get(3)))),
            "USE" => m.usage.push((f.get(1).unwrap_or(&"").to_string(), n(f.get(2)), n(f.get(3)), n(f.get(4)))),
            "RES" => m.reserved = Some(f.get(1) == Some(&"Enabled")),
            _ => {}
        }
    }
    m
}

/// `None` = automático; `Some((ini, max))` en MB, en la unidad del sistema.
pub fn set_pagefile(r: &Rep, size: Option<(u64, u64)>) {
    r.step(1, 1, "Archivo de paginación");
    let script = match size {
        None => "$cs=Get-CimInstance Win32_ComputerSystem; Set-CimInstance -InputObject $cs -Property @{AutomaticManagedPagefile=$true}".to_string(),
        Some((ini, max)) => format!(
            "$cs=Get-CimInstance Win32_ComputerSystem; Set-CimInstance -InputObject $cs -Property @{{AutomaticManagedPagefile=$false}}; \
             $f=Join-Path $env:SystemDrive 'pagefile.sys'; \
             $pf=Get-CimInstance Win32_PageFileSetting | Where-Object {{ $_.Name -eq $f }}; \
             if(-not $pf){{ $pf=New-CimInstance -ClassName Win32_PageFileSetting -Property @{{Name=$f}} }}; \
             Set-CimInstance -InputObject $pf -Property @{{InitialSize=[uint32]{ini}; MaximumSize=[uint32]{max}}}"
        ),
    };
    let (ok, out) = ps(&script);
    if ok {
        r.say(Kind::Ok, match size {
            None => "      Windows gestionará el tamaño automáticamente.".to_string(),
            Some((a, b)) => format!("      Tamaño fijado: {a} MB inicial, {b} MB máximo."),
        });
        let _ = r.0.send(Msg::AskRestart);
    } else {
        r.tail(Kind::Err, &out, 2);
    }
}

pub fn set_reserved(r: &Rep, on: bool) {
    r.step(1, 1, if on { "Activar almacenamiento reservado" } else { "Desactivar almacenamiento reservado" });
    let (ok, out) = ps(&format!("Set-WindowsReservedStorageState -State {}", if on { "Enabled" } else { "Disabled" }));
    if ok {
        r.say(Kind::Ok, "      Hecho.");
    } else {
        r.tail(Kind::Err, &out, 2);
        r.say(Kind::Dim, "      Si hay una actualización de Windows a medias, termina de instalarla y reinicia antes.");
    }
}

// ---------------------------------------------------------------------
//  Estado y UI
// ---------------------------------------------------------------------

pub enum Action {
    Pagefile(Option<(u64, u64)>),
    Reserved(bool),
}

#[derive(Default)]
pub struct State {
    plans: Vec<Plan>,
    mem: Option<MemInfo>,
    rx: Option<Receiver<(Vec<Plan>, MemInfo)>>,
    loaded: bool,
}

impl State {
    pub fn refresh(&mut self, ctx: &egui::Context) {
        self.loaded = true;
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send((power_plans(), load_mem()));
            ctx.request_repaint();
        });
    }

    pub fn invalidate(&mut self) {
        self.loaded = false;
    }

    pub fn ui(&mut self, ui: &mut Ui, ctx: &egui::Context, enabled: bool) -> Option<Action> {
        if !self.loaded {
            self.refresh(ctx);
        }
        if let Some(Ok((p, m))) = self.rx.as_ref().map(|r| r.try_recv()) {
            self.plans = p;
            self.mem = Some(m);
            self.rx = None;
        }
        let loading = self.rx.is_some();
        let mut act = None;
        let h = ui.available_height();
        egui::ScrollArea::vertical().id_salt("energia").max_height(h).auto_shrink([false, false]).show(ui, |ui| {
            pixel_frame(PANEL).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("PLAN DE ENERGÍA").color(YELLOW).size(13.0));
                if loading && self.plans.is_empty() {
                    note(ui, "Leyendo planes...");
                }
                ui.horizontal_wrapped(|ui| {
                    let mut chosen = None;
                    for p in &self.plans {
                        let label = if p.active { format!("● {}", p.name) } else { p.name.clone() };
                        if pixel_small(ui, &label, if p.active { LIME } else { GRAY }, enabled && !p.active).clicked() {
                            chosen = Some(p.guid.clone());
                        }
                    }
                    if let Some(g) = chosen {
                        if set_plan(&g) {
                            for p in &mut self.plans {
                                p.active = p.guid == g;
                            }
                            fondo::sync_power_menu();
                        }
                    }
                });
                note(ui, "También se puede cambiar desde el icono de la bandeja.\n\"Máximo rendimiento\" se añade en TWEAKS → Ultimate Performance.");
            });
            ui.add_space(8.0);
            pixel_frame(PANEL).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("ARCHIVO DE PAGINACIÓN").color(YELLOW).size(13.0));
                let Some(m) = &self.mem else {
                    note(ui, "Leyendo...");
                    return;
                };
                let modo = if m.auto {
                    "Automático (lo gestiona Windows)".to_string()
                } else if let Some((f, a, b)) = m.set.first() {
                    if *a == 0 && *b == 0 { format!("{f}: tamaño administrado por el sistema") } else { format!("{f}: {a} MB inicial, {b} MB máximo") }
                } else {
                    "Sin archivo de paginación".to_string()
                };
                ui.label(egui::RichText::new(format!("Ahora: {modo}")).color(TEXT).size(12.0));
                for (f, size, used, peak) in &m.usage {
                    note(ui, &format!("{f}: {size} MB reservados · {used} MB en uso · pico {peak} MB"));
                }
                let ram = m.ram_mb.max(1024);
                // fijo: inicial = RAM (máx. 16 GB), máximo = el doble (máx. 32 GB)
                let ini = ram.min(16_384);
                let max = (ram * 2).min(32_768);
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if pixel_small(ui, "AUTOMÁTICO", BLUE, enabled && !loading && !m.auto)
                        .on_hover_text("Lo recomendado para casi todos: Windows ajusta el tamaño.")
                        .clicked()
                    {
                        act = Some(Action::Pagefile(None));
                    }
                    if pixel_small(ui, &format!("FIJO {} / {} MB", ini, max), ORANGE, enabled && !loading)
                        .on_hover_text("Tamaño fijo según tu RAM. Evita que crezca y se fragmente;\nútil si el disco es pequeño o con juegos que avisan de memoria virtual.")
                        .clicked()
                    {
                        act = Some(Action::Pagefile(Some((ini, max))));
                    }
                });
                note(ui, "Cambiarlo necesita reiniciar. Nunca se desactiva: sin paginación\nalgunos programas fallan aunque sobre RAM.");
            });
            ui.add_space(8.0);
            pixel_frame(PANEL).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("ALMACENAMIENTO RESERVADO").color(YELLOW).size(13.0));
                let res = self.mem.as_ref().and_then(|m| m.reserved);
                let txt = match res {
                    Some(true) => "Activado: Windows guarda ~7 GB para actualizaciones.",
                    Some(false) => "Desactivado: ese espacio está libre para ti.",
                    None => "Estado desconocido (Windows no lo informa).",
                };
                ui.label(egui::RichText::new(txt).color(TEXT).size(12.0));
                ui.horizontal(|ui| {
                    if pixel_small(ui, "DESACTIVAR", ORANGE, enabled && !loading && res == Some(true)).clicked() {
                        act = Some(Action::Reserved(false));
                    }
                    if pixel_small(ui, "ACTIVAR", BLUE, enabled && !loading && res == Some(false)).clicked() {
                        act = Some(Action::Reserved(true));
                    }
                });
                note(ui, "Desactivarlo libera espacio en discos pequeños; las actualizaciones\ngrandes pueden pedir más espacio libre al instalarse.");
            });
        });
        act
    }
}
