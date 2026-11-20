//! Pestaña SISTEMA: información del equipo, analizador de espacio y batería.

use crate::*;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke, Ui};
use std::collections::HashMap;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// ---------------------------------------------------------------------
//  Información del equipo (un solo script, solo números y texto plano)
// ---------------------------------------------------------------------

/// Cada línea: CLAVE|campo|campo... Solo comillas simples (va por línea de comandos).
const INFO_SCRIPT: &str = r"$ErrorActionPreference='SilentlyContinue';
$o=Get-CimInstance Win32_OperatingSystem;
'SO|' + $o.Caption + '|' + $o.Version;
foreach($c in Get-CimInstance Win32_Processor){ 'CPU|' + $c.Name.Trim() + '|' + $c.NumberOfCores + '|' + $c.NumberOfLogicalProcessors };
'RAM|' + $o.TotalVisibleMemorySize + '|' + $o.FreePhysicalMemory;
foreach($g in Get-CimInstance Win32_VideoController){ 'GPU|' + $g.Name };
$up=(Get-Date)-$o.LastBootUpTime; 'UP|' + $up.Days + '|' + $up.Hours + '|' + $up.Minutes;
$b=Get-CimInstance Win32_BaseBoard; $cs=Get-CimInstance Win32_ComputerSystem; $pr=Get-CimInstance Win32_ComputerSystemProduct; 'EQ|' + $cs.Manufacturer + '|' + $cs.Model + '|' + $b.Product + '|' + $pr.Version;
foreach($d in Get-PhysicalDisk){ $r=$d | Get-StorageReliabilityCounter; 'DISK|' + $d.FriendlyName + '|' + $d.MediaType + '|' + $d.HealthStatus + '|' + [int64]$d.Size + '|' + $r.Wear + '|' + $r.Temperature + '|' + $r.PowerOnHours };
foreach($v in Get-Volume){ if($v.DriveLetter -and $v.Size -gt 0){ 'VOL|' + $v.DriveLetter + '|' + [int64]$v.Size + '|' + [int64]$v.SizeRemaining + '|' + $v.FileSystemLabel } };
$t=Get-CimInstance -Namespace root/wmi MSAcpi_ThermalZoneTemperature | Select-Object -First 1; if($t){ 'TEMP|' + [int]($t.CurrentTemperature/10-273.15) };
$bx=Join-Path $env:TEMP 'kwo-bateria.xml'; powercfg /batteryreport /xml /output $bx | Out-Null; $wb=@(Get-CimInstance Win32_Battery);
if(Test-Path $bx){ [xml]$x=Get-Content $bx; foreach($k in $x.BatteryReport.Batteries.Battery){ $w=$wb | Where-Object { $_.Name -eq $k.Id } | Select-Object -First 1; 'BAT|' + $k.DesignCapacity + '|' + $k.FullChargeCapacity + '|' + $k.CycleCount + '|' + $w.EstimatedChargeRemaining + '|' + $w.BatteryStatus + '|' + $k.Manufacturer + ' ' + $k.Id }; Remove-Item $bx }";

type Info = Vec<Vec<String>>;

fn load_info() -> Info {
    let (_, out) = run_status("powershell", &["-NoProfile", "-Command", &format!("[Console]::OutputEncoding=[Text.Encoding]::UTF8; {}", INFO_SCRIPT.replace('\n', " "))]);
    out.iter()
        .filter(|l| l.contains('|'))
        .map(|l| l.split('|').map(|s| s.trim().to_string()).collect())
        .collect()
}

fn gb(b: f64) -> String {
    if b >= 1_099_511_627_776.0 {
        format!("{:.2} TB", b / 1_099_511_627_776.0)
    } else if b >= 1_073_741_824.0 {
        format!("{:.1} GB", b / 1_073_741_824.0)
    } else if b >= 1_048_576.0 {
        format!("{:.0} MB", b / 1_048_576.0)
    } else {
        format!("{:.0} KB", b / 1024.0)
    }
}

// ---------------------------------------------------------------------
//  Analizador de espacio
// ---------------------------------------------------------------------

const KEEP_DEPTH: u32 = 5;
const LOOSE: &str = "[archivos sueltos]";

#[derive(Clone)]
struct Entry {
    name: String,
    size: u64,
    dir: bool,
}

pub struct Scan {
    root: PathBuf,
    dirs: HashMap<PathBuf, Vec<Entry>>,
    total: u64,
    secs: f32,
}

fn walk(p: &Path, depth: u32, map: &mut HashMap<PathBuf, Vec<Entry>>, count: &AtomicU64) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    let keep = depth < KEEP_DEPTH;
    let (mut kids, mut files) = (Vec::new(), 0u64);
    let mut total = 0;
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if m.is_dir() {
            if m.file_attributes() & 0x400 != 0 {
                continue; // enlace o unión: no seguir (evita contar dos veces)
            }
            let s = walk(&e.path(), depth + 1, map, count);
            total += s;
            if keep {
                kids.push(Entry { name: e.file_name().to_string_lossy().into(), size: s, dir: true });
            }
        } else {
            count.fetch_add(1, Ordering::Relaxed);
            files += m.len();
        }
    }
    total += files;
    if keep {
        if files > 0 {
            kids.push(Entry { name: LOOSE.into(), size: files, dir: false });
        }
        kids.sort_by(|a, b| b.size.cmp(&a.size));
        map.insert(p.to_path_buf(), kids);
    }
    total
}

/// Archivos grandes: los MAX_BIG más pesados por encima de BIG_MIN bajo `p`.
const BIG_MIN: u64 = 100 * 1_048_576;
const MAX_BIG: usize = 80;

fn walk_big(p: &Path, out: &mut Vec<(PathBuf, u64)>, count: &AtomicU64) {
    let Ok(rd) = std::fs::read_dir(p) else { return };
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if m.is_dir() {
            if m.file_attributes() & 0x400 == 0 {
                walk_big(&e.path(), out, count);
            }
        } else {
            count.fetch_add(1, Ordering::Relaxed);
            if m.len() >= BIG_MIN {
                out.push((e.path(), m.len()));
                if out.len() > MAX_BIG * 2 {
                    out.sort_by(|a, b| b.1.cmp(&a.1));
                    out.truncate(MAX_BIG);
                }
            }
        }
    }
}

fn scan(root: PathBuf, count: Arc<AtomicU64>) -> Scan {
    let t = std::time::Instant::now();
    let mut dirs = HashMap::new();
    let total = walk(&root, 0, &mut dirs, &count);
    Scan { root, dirs, total, secs: t.elapsed().as_secs_f32() }
}

// ---------------------------------------------------------------------
//  Estado y UI
// ---------------------------------------------------------------------

#[derive(Default)]
pub struct State {
    pub view: u8,
    info: Info,
    info_rx: Option<Receiver<Info>>,
    loaded: bool,
    scan: Option<Scan>,
    scan_rx: Option<Receiver<Scan>>,
    scan_count: Arc<AtomicU64>,
    scan_root: PathBuf,
    cur: PathBuf,
    report_rx: Option<Receiver<Result<PathBuf, String>>>,
    report_msg: String,
    big: Option<Vec<(PathBuf, u64)>>,
    big_rx: Option<Receiver<Vec<(PathBuf, u64)>>>,
    big_root: PathBuf,
}

impl State {
    fn refresh_info(&mut self, ctx: &egui::Context) {
        self.loaded = true;
        let (tx, rx) = channel();
        self.info_rx = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(load_info());
            ctx.request_repaint();
        });
    }

    fn start_scan(&mut self, ctx: &egui::Context, root: PathBuf) {
        let (tx, rx) = channel();
        self.scan_rx = Some(rx);
        self.scan_count = Arc::new(AtomicU64::new(0));
        self.scan_root = root.clone();
        self.cur = root.clone();
        let (ctx, count) = (ctx.clone(), self.scan_count.clone());
        thread::spawn(move || {
            let _ = tx.send(scan(root, count));
            ctx.request_repaint();
        });
    }

    fn rows(&self, key: &str) -> impl Iterator<Item = &Vec<String>> {
        let key = key.to_string();
        self.info.iter().filter(move |r| r[0] == key)
    }

    fn get(&self, key: &str) -> Option<&Vec<String>> {
        self.rows(key).next()
    }

    fn start_big(&mut self, ctx: &egui::Context, root: PathBuf) {
        let (tx, rx) = channel();
        self.big_rx = Some(rx);
        self.scan_count = Arc::new(AtomicU64::new(0));
        self.big_root = root.clone();
        let (ctx, count) = (ctx.clone(), self.scan_count.clone());
        thread::spawn(move || {
            let mut v = Vec::new();
            walk_big(&root, &mut v, &count);
            v.sort_by(|a, b| b.1.cmp(&a.1));
            v.truncate(MAX_BIG);
            let _ = tx.send(v);
            ctx.request_repaint();
        });
    }

    fn big_ui(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let busy = self.big_rx.is_some();
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Buscar en:").color(GRAY));
            let mut roots: Vec<(String, PathBuf)> = Vec::new();
            if let Ok(home) = std::env::var("USERPROFILE") {
                roots.push(("DESCARGAS".into(), PathBuf::from(&home).join("Downloads")));
                roots.push(("MI USUARIO".into(), PathBuf::from(home)));
            }
            roots.push(("TODO C:".into(), PathBuf::from("C:\\")));
            for (name, p) in roots {
                if pixel_small(ui, &name, BLUE, !busy).clicked() {
                    self.start_big(ctx, p);
                }
            }
        });
        ui.add_space(6.0);
        let list_h = ui.available_height() - 40.0;
        pixel_frame(PANEL).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if busy {
                ui.label(egui::RichText::new(format!("Buscando en {} ...", self.big_root.display())).color(YELLOW).size(13.0));
                note(ui, &format!("{} archivos revisados", self.scan_count.load(Ordering::Relaxed)));
                pixel_progress(ui, None, false);
                return;
            }
            let Some(big) = &self.big else {
                ui.label(egui::RichText::new("ARCHIVOS GRANDES").color(YELLOW).size(13.0));
                note(ui, "Busca los archivos de más de 100 MB (los 80 más pesados).\nRevísalos y bórralos tú desde el Explorador si no los necesitas.");
                return;
            };
            let total: u64 = big.iter().map(|b| b.1).sum();
            ui.label(egui::RichText::new(format!("{} archivos  ·  {} en total  ·  {}", big.len(), gb(total as f64), self.big_root.display())).color(YELLOW).size(13.0));
            egui::ScrollArea::vertical().id_salt("bigfiles").max_height(list_h.max(80.0) - 30.0).auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (p, s) in big {
                    ui.horizontal(|ui| {
                        if pixel_small(ui, "MOSTRAR", GRAY, true).on_hover_text("Abrir su carpeta con el archivo seleccionado").clicked() {
                            let _ = std::process::Command::new("explorer").arg(format!("/select,{}", p.display())).spawn();
                        }
                        ui.label(egui::RichText::new(format!("{:>9}", gb(*s as f64))).color(LIME).monospace().size(12.0));
                        ui.label(egui::RichText::new(p.display().to_string()).color(TEXT).size(12.0));
                    });
                }
                if big.is_empty() {
                    note(ui, "No hay archivos de más de 100 MB aquí.");
                }
            });
        });
        note(ui, "La app no borra nada aquí: solo te los enseña.");
    }

    /// `views`: subpestañas a mostrar (vacío = sin barra; la pestaña la elige quien llama).
    pub fn ui(&mut self, ui: &mut Ui, ctx: &egui::Context, views: &[(u8, &str)]) {
        if let Some(Ok(v)) = self.big_rx.as_ref().map(|r| r.try_recv()) {
            self.big = Some(v);
            self.big_rx = None;
        }
        if self.big_rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        if !self.loaded {
            self.refresh_info(ctx);
        }
        if let Some(Ok(v)) = self.info_rx.as_ref().map(|r| r.try_recv()) {
            self.info = v;
            self.info_rx = None;
        }
        if let Some(Ok(s)) = self.scan_rx.as_ref().map(|r| r.try_recv()) {
            self.scan = Some(s);
            self.scan_rx = None;
        }
        if let Some(Ok(r)) = self.report_rx.as_ref().map(|r| r.try_recv()) {
            self.report_rx = None;
            self.report_msg = match r {
                Ok(p) => {
                    let _ = std::process::Command::new("explorer").arg(&p).spawn();
                    format!("Informe abierto: {}", p.display())
                }
                Err(e) => format!("No se pudo crear el informe: {e}"),
            };
        }
        if self.scan_rx.is_some() || self.info_rx.is_some() || self.report_rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }

        if !views.is_empty() {
            if !views.iter().any(|(v, _)| *v == self.view) {
                self.view = views[0].0;
            }
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if (self.view == 0 || self.view == 2) && pixel_small(ui, "RECARGAR", GRAY, self.info_rx.is_none()).clicked() {
                        self.refresh_info(ctx);
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true), |ui| {
                        for (v, name) in views {
                            if pixel_tab(ui, name, self.view == *v).clicked() {
                                self.view = *v;
                            }
                        }
                    });
                });
            });
            ui.add_space(8.0);
        }
        match self.view {
            1 => self.disk_ui(ui, ctx),
            2 => self.battery_ui(ui, ctx),
            6 => self.big_ui(ui, ctx),
            3..=5 => {}
            _ => self.info_ui(ui),
        }
    }

    fn info_ui(&mut self, ui: &mut Ui) {
        if self.info_rx.is_some() && self.info.is_empty() {
            note(ui, "Leyendo información del equipo...");
            return;
        }
        let h = ui.available_height();
        egui::ScrollArea::vertical().id_salt("sysinfo").max_height(h).auto_shrink([false, false]).show(ui, |ui| {
            pixel_frame(PANEL).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("EQUIPO").color(YELLOW).size(13.0));
                if let Some(r) = self.get("EQ") {
                    let model = if r[2].is_empty() || r[2].contains("To be filled") { r[3].clone() } else { r[2].clone() };
                    // Lenovo y otros ponen el nombre comercial en Version
                    let pretty = r.get(4).filter(|v| v.len() > 2 && !v.contains("None") && !v.contains("To be filled"));
                    let txt = match pretty {
                        Some(v) => format!("{} {v}  ({model})", r[1]),
                        None => format!("{} {model}", r[1]),
                    };
                    kv(ui, "Modelo", &txt);
                }
                if let Some(r) = self.get("SO") {
                    kv(ui, "Windows", &format!("{}  (build {})", r[1].trim_start_matches("Microsoft "), r[2]));
                }
                for r in self.rows("CPU") {
                    kv(ui, "Procesador", &format!("{}  ·  {} núcleos / {} hilos", r[1], r[2], r[3]));
                }
                for r in self.rows("GPU") {
                    kv(ui, "Gráfica", &r[1]);
                }
                if let Some(r) = self.get("UP") {
                    kv(ui, "Encendido", &format!("{} días, {} h, {} min", r[1], r[2], r[3]));
                }
                if let Some(r) = self.get("TEMP") {
                    let t: i32 = r[1].parse().unwrap_or(0);
                    if (1..120).contains(&t) {
                        kv(ui, "Temperatura", &format!("{t} °C (zona térmica ACPI)"));
                    }
                }
            });
            ui.add_space(8.0);
            if let Some(r) = self.get("RAM") {
                let total: f64 = r[1].parse().unwrap_or(0.0);
                let free: f64 = r[2].parse().unwrap_or(0.0);
                if total > 0.0 {
                    pixel_frame(PANEL).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let used = (total - free) / total;
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("MEMORIA RAM").color(YELLOW).size(13.0));
                            ui.label(egui::RichText::new(format!("{} de {} en uso", gb((total - free) * 1024.0), gb(total * 1024.0))).color(GRAY).size(11.0));
                        });
                        meter(ui, used as f32, level_color(used as f32));
                    });
                    ui.add_space(8.0);
                }
            }
            pixel_frame(PANEL).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("DISCOS").color(YELLOW).size(13.0));
                for r in self.rows("DISK") {
                    let size: f64 = r[4].parse().unwrap_or(0.0);
                    let (health, col) = match r[3].as_str() {
                        "Healthy" => ("SANO", GREEN),
                        "Warning" => ("AVISO", ORANGE),
                        "Unhealthy" => ("FALLANDO", RED),
                        _ => ("¿?", GRAY),
                    };
                    let kind = match r[2].as_str() {
                        "SSD" => "SSD",
                        "HDD" => "HDD",
                        _ => "disco",
                    };
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&r[1]).color(TEXT).size(13.0));
                        badge(ui, health, col);
                    });
                    let mut extra = vec![kind.to_string(), gb(size)];
                    if let Some(w) = r.get(5).filter(|s| !s.is_empty()) {
                        extra.push(format!("desgaste {w}%"));
                    }
                    if let Some(t) = r.get(6).filter(|s| !s.is_empty() && *s != "0") {
                        extra.push(format!("{t} °C"));
                    }
                    if let Some(h) = r.get(7).and_then(|s| s.parse::<u64>().ok()).filter(|&h| h > 0) {
                        extra.push(format!("{h} h encendido"));
                    }
                    note(ui, &extra.join("  ·  "));
                }
                ui.add_space(4.0);
                for r in self.rows("VOL") {
                    let size: f64 = r[2].parse().unwrap_or(0.0);
                    let free: f64 = r[3].parse().unwrap_or(0.0);
                    let used = ((size - free) / size.max(1.0)) as f32;
                    let label = if r.get(4).is_some_and(|l| !l.is_empty()) { format!("{}: ({})", r[1], r[4]) } else { format!("{}:", r[1]) };
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(label).color(TEXT).size(12.0));
                        ui.label(egui::RichText::new(format!("{} libres de {}", gb(free), gb(size))).color(GRAY).size(11.0));
                    });
                    meter(ui, used, level_color(used));
                }
            });
        });
    }

    fn disk_ui(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        let scanning = self.scan_rx.is_some();
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Analizar:").color(GRAY));
            let mut roots: Vec<(String, PathBuf)> = self.rows("VOL").map(|r| (format!("{}:", r[1]), PathBuf::from(format!("{}:\\", r[1])))).collect();
            if roots.is_empty() {
                roots.push(("C:".into(), PathBuf::from("C:\\")));
            }
            if let Ok(home) = std::env::var("USERPROFILE") {
                roots.push(("MI USUARIO".into(), PathBuf::from(home)));
            }
            for (name, p) in roots {
                if pixel_small(ui, &name, BLUE, !scanning).clicked() {
                    self.start_scan(ctx, p);
                }
            }
        });
        ui.add_space(6.0);
        let list_h = ui.available_height() - 40.0;
        pixel_frame(PANEL).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if scanning {
                ui.label(egui::RichText::new(format!("Analizando {} ...", self.scan_root.display())).color(YELLOW).size(13.0));
                note(ui, &format!("{} archivos contados", self.scan_count.load(Ordering::Relaxed)));
                pixel_progress(ui, None, false);
                return;
            }
            let Some(scan) = &self.scan else {
                ui.label(egui::RichText::new("ESPACIO EN DISCO").color(YELLOW).size(13.0));
                note(ui, "Elige un disco o tu carpeta de usuario para ver qué carpetas ocupan más.\nPulsa una carpeta para entrar en ella.");
                return;
            };
            let mut go: Option<PathBuf> = None;
            let mut rescan: Option<PathBuf> = None;
            ui.horizontal(|ui| {
                let up = self.cur.parent().map(Path::to_path_buf);
                if pixel_small(ui, "▲ SUBIR", GRAY, up.is_some()).clicked() {
                    if let Some(u) = up {
                        if self.cur == scan.root { rescan = Some(u) } else { go = Some(u) }
                    }
                }
                if pixel_small(ui, "ABRIR", GRAY, true).on_hover_text("Abrir esta carpeta en el Explorador").clicked() {
                    let _ = std::process::Command::new("explorer").arg(&self.cur).spawn();
                }
                ui.label(egui::RichText::new(self.cur.display().to_string()).color(YELLOW).size(13.0));
            });
            let kids = scan.dirs.get(&self.cur);
            let here: u64 = kids.map(|k| k.iter().map(|e| e.size).sum()).unwrap_or(0);
            note(ui, &format!("{} en esta carpeta  ·  {} en total  ·  análisis en {:.1}s", gb(here as f64), gb(scan.total as f64), scan.secs));
            egui::ScrollArea::vertical().id_salt("diskscan").max_height(list_h.max(80.0) - 60.0).auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let Some(kids) = kids else { return };
                let max = kids.first().map(|e| e.size).unwrap_or(1).max(1);
                for e in kids.iter().take(60) {
                    if size_row(ui, e, max, here.max(1)).clicked() && e.dir {
                        let p = self.cur.join(&e.name);
                        if scan.dirs.contains_key(&p) { go = Some(p) } else { rescan = Some(p) }
                    }
                }
                if kids.is_empty() {
                    note(ui, "Carpeta vacía o sin permiso de lectura.");
                }
            });
            if let Some(p) = go {
                self.cur = p;
            }
            if let Some(p) = rescan {
                self.start_scan(ctx, p);
            }
        });
        note(ui, "No se siguen enlaces ni uniones. Las carpetas sin permiso cuentan como vacías.");
    }

    fn battery_ui(&mut self, ui: &mut Ui, ctx: &egui::Context) {
        if self.info_rx.is_some() && self.info.is_empty() {
            note(ui, "Leyendo la batería...");
            return;
        }
        let bats: Vec<Vec<String>> = self.rows("BAT").cloned().collect();
        if bats.is_empty() {
            pixel_frame(PANEL).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("BATERÍA").color(YELLOW).size(13.0));
                note(ui, "No se encontró batería (equipo de sobremesa o el driver no da datos).");
            });
        }
        let many = bats.len() > 1;
        for (i, r) in bats.iter().enumerate() {
        pixel_frame(PANEL).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let title = if many { format!("BATERÍA {}", i + 1) } else { "BATERÍA".to_string() };
            ui.label(egui::RichText::new(title).color(YELLOW).size(13.0));
            let design: f64 = r[1].parse().unwrap_or(0.0);
            let full: f64 = r[2].parse().unwrap_or(0.0);
            if let Some(name) = r.get(6).filter(|s| !s.is_empty()) {
                kv(ui, "Modelo", name);
            }
            if let Ok(c) = r[4].parse::<u32>() {
                let estado = match r[5].as_str() {
                    "2" => "conectado a la corriente",
                    "1" => "descargando",
                    _ => "",
                };
                kv(ui, "Carga", &format!("{c}%  {estado}"));
            }
            if design > 0.0 && full > 0.0 {
                let health = (full / design).min(1.0) as f32;
                kv(ui, "Capacidad", &format!("{:.1} Wh de {:.1} Wh originales", full / 1000.0, design / 1000.0));
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Salud").color(GRAY).size(12.0));
                    let (txt, col) = match health {
                        h if h >= 0.8 => ("BUENA", GREEN),
                        h if h >= 0.6 => ("DESGASTADA", YELLOW),
                        _ => ("CAMBIAR PRONTO", RED),
                    };
                    badge(ui, txt, col);
                });
                meter(ui, health, if health >= 0.8 { GREEN } else if health >= 0.6 { YELLOW } else { RED });
                note(ui, &format!("Desgaste: {:.0}%", (1.0 - health) * 100.0));
            }
            if let Some(c) = r.get(3).and_then(|s| s.parse::<u32>().ok()).filter(|&c| c > 0) {
                kv(ui, "Ciclos", &c.to_string());
            }
        });
        ui.add_space(8.0);
        }
        ui.add_space(2.0);
        let busy = self.report_rx.is_some();
        if pixel_button(ui, 300.0, None, if busy { "Generando..." } else { "Informe completo" }, BLUE, !busy).clicked() {
            let (tx, rx) = channel();
            self.report_rx = Some(rx);
            let ctx = ctx.clone();
            thread::spawn(move || {
                let p = std::env::temp_dir().join("informe-bateria.html");
                let (ok, out) = run_status("powercfg", &["/batteryreport", "/output", &p.display().to_string()]);
                let _ = tx.send(if ok && p.exists() { Ok(p) } else { Err(out.join(" ")) });
                ctx.request_repaint();
            });
        }
        note(ui, "Abre el informe de Windows (powercfg /batteryreport) con el\nhistorial de uso y capacidad en el navegador.");
        if !self.report_msg.is_empty() {
            note(ui, &self.report_msg);
        }
    }
}

fn level_color(f: f32) -> Color32 {
    if f >= 0.9 {
        RED
    } else if f >= 0.75 {
        ORANGE
    } else {
        GREEN
    }
}

fn kv(ui: &mut Ui, k: &str, v: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(110.0, 16.0), Sense::hover());
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, k, FontId::monospace(12.0), GRAY);
        ui.label(egui::RichText::new(v).color(TEXT).size(12.0));
    });
}

fn badge(ui: &mut Ui, txt: &str, col: Color32) {
    let w = txt.chars().count() as f32 * 6.0 + 10.0;
    let (r, _) = ui.allocate_exact_size(vec2(w, 15.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, col);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, txt, FontId::monospace(10.0), DARK);
}

/// Barra simple en bloques de 6 px.
fn meter(ui: &mut Ui, frac: f32, col: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, DARK);
    let inner = rect.shrink(3.0);
    let cells = (inner.width() / 6.0).floor() as usize;
    let on = (frac.clamp(0.0, 1.0) * cells as f32).round() as usize;
    for c in 0..cells {
        let r = Rect::from_min_size(pos2(inner.min.x + c as f32 * 6.0, inner.min.y), vec2(5.0, inner.height()));
        p.rect_filled(r, 0.0, if c < on { col } else { mix(DARK, PANEL, 0.25) });
    }
    let txt = format!("{}%", (frac * 100.0).round() as u32);
    p.text(rect.right_center() - vec2(6.0, 0.0), Align2::RIGHT_CENTER, txt, FontId::monospace(11.0), TEXT);
}

/// Fila del analizador: nombre, tamaño y barra proporcional a la mayor.
fn size_row(ui: &mut Ui, e: &Entry, max: u64, here: u64) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), if e.dir { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    if e.dir && resp.hovered() {
        p.rect_filled(rect, 0.0, mix(PANEL, TEXT, 0.08));
    }
    let bar = Rect::from_min_size(pos2(rect.min.x + 4.0, rect.max.y - 9.0), vec2((rect.width() - 8.0) * e.size as f32 / max as f32, 5.0));
    p.rect_filled(Rect::from_min_size(bar.min, vec2(rect.width() - 8.0, 5.0)), 0.0, DARK);
    p.rect_filled(bar, 0.0, if e.dir { BLUE } else { DIMGRAY });
    let icon = if e.dir { "▸ " } else { "  " };
    let name: String = e.name.chars().take(48).collect();
    p.text(pos2(rect.min.x + 4.0, rect.min.y + 9.0), Align2::LEFT_CENTER, format!("{icon}{name}"), FontId::monospace(12.0), if e.dir { TEXT } else { GRAY });
    let pct = e.size as f64 * 100.0 / here as f64;
    p.text(pos2(rect.max.x - 4.0, rect.min.y + 9.0), Align2::RIGHT_CENTER, format!("{}  {:>4.1}%", gb(e.size as f64), pct), FontId::monospace(12.0), LIME);
    p.rect_stroke(rect, 0.0, Stroke::NONE);
    if e.dir { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}
