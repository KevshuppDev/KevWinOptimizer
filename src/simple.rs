//! Modo simple: pocas acciones, todas seguras y explicadas sin tecnicismos.

use crate::*;
use eframe::egui::{self, Ui};

/// Limpieza segura: nada que borre archivos personales ni toque drivers o servicios.
const SAFE_CLEAN: &[&str] = &[
    "Temporales del usuario",
    "Temporales de Windows",
    "Caché de Windows Update",
    "Optimización de entrega",
    "Informes de errores",
    "Miniaturas",
    "Caché DNS",
    "TRIM del disco C:",
    "Logs de Windows",
];

/// Privacidad: lo que no cambia cómo funciona el PC, solo deja de enviar datos o anuncios.
const SAFE_PRIVACY: &[&str] = &[
    "Telemetría",
    "Historial de actividad",
    "Bing en el menú Inicio",
    "Sugerencias y anuncios",
    "ID de publicidad",
];

const SAFE_PERF: &[&str] = &["Desactivar Game DVR", "Modo Juego", "Quitar Widgets"];

const SAFE_REPAIR: &[&str] = &["Punto de restauración", "SFC /scannow", "DISM RestoreHealth"];

/// Tweaks seguros que aún no están aplicados. None = todavía comprobando.
fn pending(list: &'static [Tweak], st: &[Option<bool>], names: &[&str]) -> Option<Vec<&'static Tweak>> {
    let mut v = Vec::new();
    for (t, s) in list.iter().zip(st) {
        if names.contains(&t.name) {
            match s {
                None => return None,
                Some(false) => v.push(t),
                Some(true) => {}
            }
        }
    }
    Some(v)
}

fn card(ui: &mut Ui, title: &str, text: &str, add: impl FnOnce(&mut Ui)) {
    pixel_frame(PANEL).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(title).color(YELLOW).size(14.0));
        ui.label(egui::RichText::new(text).color(TEXT).size(12.0));
        ui.add_space(4.0);
        add(ui);
    });
}

impl App {
    pub(crate) fn simple_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, enabled: bool) {
        let bw = 250.0;
        ui.label(egui::RichText::new("¿Qué quieres hacer?").color(TEXT).size(16.0));
        note(ui, "Todo lo de aquí es seguro: no borra tus archivos ni cambia cómo usas el PC.\nSi algo no te convence, en REPARAR puedes volver atrás con un punto de restauración.");
        ui.add_space(6.0);
        let h = ui.available_height();
        egui::ScrollArea::vertical().id_salt("simple").max_height(h).auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(2, |c| {
                card(&mut c[0], "LIMPIAR MI PC", "Borra archivos basura y temporales\npara liberar espacio en el disco.", |ui| {
                    if pixel_button(ui, bw, Some(&SPR_BROOM), "Limpiar", GREEN, enabled).clicked() {
                        let sel: Vec<bool> = OPTS.iter().map(|o| SAFE_CLEAN.contains(&o.0)).collect();
                        self.start(ctx, "Optimizar", move |r| optimizar(r, &sel));
                    }
                });
                c[0].add_space(8.0);

                let priv_todo = pending(PRIVACY, &self.st_priv, SAFE_PRIVACY);
                card(&mut c[1], "PROTEGER MI PRIVACIDAD", "Windows deja de enviar datos de uso\ny de mostrarte anuncios.", |ui| {
                    match priv_todo {
                        None => note(ui, "Comprobando..."),
                        Some(v) if v.is_empty() => {
                            pixel_button(ui, bw, Some(&SPR_CHECK), "Ya está protegido", GREEN, false);
                        }
                        Some(v) => {
                            if pixel_button(ui, bw, Some(&SPR_CHECK), &format!("Proteger ({})", v.len()), GREEN, enabled).clicked() {
                                self.start(ctx, "Aplicar tweaks", move |r| run_changes(r, &v, &[]));
                            }
                        }
                    }
                });
                c[1].add_space(8.0);

                let perf_todo = pending(PERF, &self.st_perf, SAFE_PERF);
                card(&mut c[0], "ACELERAR WINDOWS", "Quita cosas que gastan recursos sin\nque las uses (grabación de juegos, widgets).", |ui| {
                    match perf_todo {
                        None => note(ui, "Comprobando..."),
                        Some(v) if v.is_empty() => {
                            pixel_button(ui, bw, Some(&SPR_CHECK), "Ya está acelerado", GREEN, false);
                        }
                        Some(v) => {
                            if pixel_button(ui, bw, Some(&SPR_CHECK), &format!("Acelerar ({})", v.len()), GREEN, enabled).clicked() {
                                self.start(ctx, "Aplicar tweaks", move |r| run_changes(r, &v, &[]));
                            }
                        }
                    }
                });
                c[0].add_space(8.0);

                card(&mut c[1], "ARREGLAR INTERNET", "Si las páginas no cargan o la conexión\nva mal, reinicia la red (unos segundos).", |ui| {
                    if pixel_button(ui, bw, Some(&SPR_GLOBE), "Arreglar internet", BLUE, enabled).clicked() {
                        self.start(ctx, "Red rápida", red_rapida);
                    }
                });
                c[1].add_space(8.0);

                card(&mut c[0], "REPARAR WINDOWS", "Si Windows va raro o da errores, busca y\nrepara archivos dañados (~20 minutos).", |ui| {
                    if pixel_button(ui, bw, Some(&SPR_WRENCH), "Reparar", ORANGE, enabled).clicked() {
                        let list: Vec<&'static Tweak> = REPAIRS.iter().filter(|t| SAFE_REPAIR.contains(&t.name)).collect();
                        self.start(ctx, "Reparar", move |r| run_repairs(r, &list));
                    }
                });
                c[0].add_space(8.0);

                card(&mut c[1], "INSTALAR O ACTUALIZAR APPS", "Instala programas conocidos o pon al día\nlos que ya tienes, sin buscar en internet.", |ui| {
                    if pixel_button(ui, bw, Some(&SPR_BOX), "Ir a APPS", PURPLE, true).clicked() {
                        self.tab = Tab::Apps;
                    }
                });
            });
        });
    }
}
