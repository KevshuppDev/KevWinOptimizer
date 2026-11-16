//! RED → conexiones y VPN: activar/desactivar adaptadores (como ncpa.cpl) y Cloudflare WARP.

use crate::*;
use eframe::egui::{self, Ui};
use std::sync::mpsc::{channel, Receiver};
use std::thread;

const WARP_CLI: &str = r"C:\Program Files\Cloudflare\Cloudflare WARP\warp-cli.exe";

/// Servicio que necesita cada VPN para funcionar: (texto en la descripción del adaptador, servicio)
const VPN_SVC: &[(&str, &str)] = &[("Radmin", "RvControlSvc"), ("ZeroTier", "ZeroTierOneService"), ("Hamachi", "Hamachi2Svc"), ("Tailscale", "Tailscale")];
const VPN_WORDS: &[&str] = &["VPN", "Radmin", "WARP", "TAP-", "Wintun", "WireGuard", "ZeroTier", "Hamachi", "Tailscale", "OpenVPN", "Virtual"];

#[derive(Clone)]
pub struct Adapter {
    pub name: String,
    pub desc: String,
    /// Habilitado (estado administrativo)
    pub on: bool,
    pub connected: bool,
}

impl Adapter {
    fn is_vpn(&self) -> bool {
        VPN_WORDS.iter().any(|w| self.desc.contains(w) || self.name.contains(w))
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Warp {
    Missing,
    Off,
    On,
}

const SCRIPT: &str = "$ErrorActionPreference='SilentlyContinue'; \
    foreach($a in Get-NetAdapter){ 'AD|' + $a.Name + '|' + $a.InterfaceDescription + '|' + $a.AdminStatus + '|' + $a.Status }";

fn warp_state() -> Warp {
    if !std::path::Path::new(WARP_CLI).exists() {
        return Warp::Missing;
    }
    let (_, out) = run_status(WARP_CLI, &["status"]);
    let txt = out.join(" ");
    if txt.contains("Connected") && !txt.contains("Disconnected") { Warp::On } else { Warp::Off }
}

fn load() -> (Vec<Adapter>, Warp) {
    let (_, out) = ps(SCRIPT);
    let mut v: Vec<Adapter> = out
        .iter()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('|').collect();
            (f.len() >= 5 && f[0] == "AD").then(|| Adapter {
                name: f[1].to_string(),
                desc: f[2].to_string(),
                on: f[3] == "Up",
                connected: f[4] == "Up",
            })
        })
        .filter(|a| !a.desc.contains("Cloudflare WARP")) // WARP va con su propio interruptor
        .collect();
    v.sort_by_key(|a| (!a.is_vpn(), a.name.clone()));
    (v, warp_state())
}

pub fn set_adapter(r: &Rep, a: &Adapter, on: bool) {
    r.step(1, 1, &format!("{} {}", if on { "Activar" } else { "Desactivar" }, a.name));
    let svc = VPN_SVC.iter().find(|(k, _)| a.desc.contains(k)).map(|(_, s)| *s);
    if let (true, Some(s)) = (on, svc) {
        // sin su servicio la VPN queda habilitada pero no conecta
        let (ok, _) = ps(&format!("if((Get-Service '{s}').StartType -eq 'Disabled'){{ Set-Service '{s}' -StartupType Manual }}; Start-Service '{s}'"));
        r.say(Kind::Dim, format!("      Servicio {s}: {}", if ok { "iniciado" } else { "no se pudo iniciar" }));
    }
    let (ok, out) = run_status(
        "netsh",
        &["interface", "set", "interface", &format!("name={}", a.name), &format!("admin={}", if on { "enabled" } else { "disabled" })],
    );
    if ok {
        r.say(Kind::Ok, if on { "      Activada" } else { "      Desactivada" });
    } else {
        r.tail(Kind::Err, &out, 2);
    }
    if let (false, Some(s)) = (on, svc) {
        let _ = ps(&format!("Stop-Service '{s}' -Force"));
        r.say(Kind::Dim, format!("      Servicio {s} detenido"));
    }
}

pub fn set_warp(r: &Rep, on: bool) {
    r.step(1, 1, if on { "Conectar Cloudflare WARP" } else { "Desconectar Cloudflare WARP" });
    if on {
        let (ok, out) = ps("if((Get-Service CloudflareWARP).StartType -eq 'Disabled'){ Set-Service CloudflareWARP -StartupType Manual }; Start-Service CloudflareWARP");
        if !ok {
            r.tail(Kind::Err, &out, 2);
            return;
        }
        // el servicio tarda un momento en aceptar órdenes
        for _ in 0..10 {
            if run_status(WARP_CLI, &["status"]).0 {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(500));
        }
    }
    let (ok, out) = run_status(WARP_CLI, &[if on { "connect" } else { "disconnect" }]);
    if !ok {
        r.tail(Kind::Err, &out, 2);
        return;
    }
    if on {
        for _ in 0..12 {
            if warp_state() == Warp::On {
                r.say(Kind::Ok, "      WARP conectado");
                return;
            }
            thread::sleep(std::time::Duration::from_millis(500));
        }
        r.say(Kind::Dim, "      Orden enviada; WARP sigue conectando.");
    } else {
        r.say(Kind::Ok, "      WARP desconectado");
    }
}

pub enum Action {
    Adapter(Adapter, bool),
    Warp(bool),
}

#[derive(Default)]
pub struct State {
    list: Vec<Adapter>,
    warp: Option<Warp>,
    rx: Option<Receiver<(Vec<Adapter>, Warp)>>,
    loaded: bool,
}

impl State {
    pub fn invalidate(&mut self) {
        self.loaded = false;
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        self.loaded = true;
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(load());
            ctx.request_repaint();
        });
    }

    pub fn ui(&mut self, ui: &mut Ui, ctx: &egui::Context, enabled: bool) -> Option<Action> {
        if !self.loaded && enabled {
            self.refresh(ctx);
        }
        if let Some(Ok((l, w))) = self.rx.as_ref().map(|r| r.try_recv()) {
            self.list = l;
            self.warp = Some(w);
            self.rx = None;
        }
        let mut act = None;
        pixel_frame(PANEL).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("CONEXIONES Y VPN").color(YELLOW).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if pixel_small(ui, "RECARGAR", GRAY, self.rx.is_none()).clicked() {
                        self.refresh(ctx);
                    }
                    if pixel_small(ui, "ABRIR CONEXIONES", BLUE, true).on_hover_text("El panel clásico de Conexiones de red").clicked() {
                        run_silent("cmd", &["/c", "start", "", "ncpa.cpl"]);
                    }
                });
            });
            if self.warp.is_none() {
                note(ui, "Leyendo adaptadores...");
            }
            let row = |ui: &mut Ui, name: &str, desc: &str, tag: &str, col: egui::Color32, on: bool| -> bool {
                let mut clicked = false;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!(" {tag} ")).color(DARK).background_color(col).size(11.0));
                    ui.label(egui::RichText::new(name).color(TEXT).size(13.0)).on_hover_text(desc);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (label, c) = if on { ("DESACTIVAR", ORANGE) } else { ("ACTIVAR", GREEN) };
                        clicked = pixel_small(ui, label, c, enabled).clicked();
                    });
                });
                clicked
            };
            match self.warp {
                Some(Warp::On) => {
                    if row(ui, "Cloudflare WARP", "VPN de Cloudflare (1.1.1.1)", "CONECTADA", LIME, true) {
                        act = Some(Action::Warp(false));
                    }
                }
                Some(Warp::Off) => {
                    if row(ui, "Cloudflare WARP", "VPN de Cloudflare (1.1.1.1)", "APAGADA", GRAY, false) {
                        act = Some(Action::Warp(true));
                    }
                }
                _ => {}
            }
            for a in &self.list {
                let (tag, col) = match (a.on, a.connected) {
                    (false, _) => ("DESHABILITADA", GRAY),
                    (true, true) => ("CONECTADA", LIME),
                    (true, false) => ("SIN CONEXIÓN", YELLOW),
                };
                let name = if a.is_vpn() { format!("{}  (VPN)", a.name) } else { a.name.clone() };
                if row(ui, &name, &a.desc, tag, col, a.on) {
                    act = Some(Action::Adapter(a.clone(), !a.on));
                }
            }
            note(ui, "Igual que Activar/Desactivar en Conexiones de red. Las VPN también arrancan o paran su servicio.");
        });
        act
    }
}
