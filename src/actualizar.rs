//! APPS → ACTUALIZAR (winget upgrade con selección) y exportar/importar la lista de apps.

use crate::*;
use eframe::egui::{self, Ui};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::thread;

#[derive(Clone)]
pub struct Upg {
    pub name: String,
    pub id: String,
    pub ver: String,
    pub avail: String,
}

/// Lee la tabla de `winget upgrade`. Las columnas se sacan de la cabecera
/// (los nombres cambian con el idioma, las posiciones no).
fn list_upgrades() -> Result<Vec<Upg>, String> {
    let o = Command::new("winget")
        .args(["upgrade", "--include-unknown", "--accept-source-agreements", "--disable-interactivity"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|_| "winget no está disponible".to_string())?;
    let txt = String::from_utf8_lossy(&o.stdout).into_owned();
    // quitar el spinner: quedarse con lo que hay tras el último \r de cada línea
    let lines: Vec<Vec<char>> = txt
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .map(|l| l.rsplit('\r').next().unwrap_or(l).chars().collect())
        .collect();
    let Some(h) = (1..lines.len()).find(|&i| lines[i].len() > 10 && lines[i].iter().all(|&c| c == '-' || c == '—')) else {
        return Ok(Vec::new()); // sin tabla = no hay actualizaciones
    };
    let head = &lines[h - 1];
    let cols: Vec<usize> = (0..head.len()).filter(|&i| head[i] != ' ' && (i == 0 || head[i - 1] == ' ')).collect();
    if cols.len() < 4 {
        return Err("No se entendió la salida de winget".into());
    }
    let cell = |row: &[char], c: usize| -> String {
        let a = cols[c].min(row.len());
        let b = cols.get(c + 1).copied().unwrap_or(row.len()).min(row.len());
        row[a..b].iter().collect::<String>().trim().to_string()
    };
    let mut v = Vec::new();
    for row in &lines[h + 1..] {
        if row.len() <= cols[2] {
            break; // línea vacía o el resumen final
        }
        let u = Upg { name: cell(row, 0), id: cell(row, 1), ver: cell(row, 2), avail: cell(row, 3) };
        if u.id.is_empty() || u.avail.is_empty() {
            break;
        }
        v.push(u);
    }
    // ids recortados con "…": buscar el completo en `winget export`
    if v.iter().any(|u| u.id.ends_with('…')) {
        let tmp = std::env::temp_dir().join("optimizador_winget_upg.json");
        let _ = run_status("winget", &["export", "-o", &tmp.display().to_string(), "--accept-source-agreements", "--disable-interactivity"]);
        let json = std::fs::read_to_string(&tmp).unwrap_or_default();
        let _ = std::fs::remove_file(&tmp);
        let ids: Vec<&str> = json.split("\"PackageIdentifier\"").skip(1).filter_map(|s| s.split('"').nth(1)).collect();
        for u in &mut v {
            if let Some(pre) = u.id.strip_suffix('…') {
                if let Some(full) = ids.iter().find(|i| i.starts_with(pre)) {
                    u.id = full.to_string();
                }
            }
        }
    }
    Ok(v)
}

/// Paquetes que winget "actualizaría" a algo peor. Devuelve el motivo.
pub fn blocked(u: &Upg) -> Option<&'static str> {
    let major = |v: &str| v.split('.').next().and_then(|n| n.parse::<u32>().ok());
    // El launcher nuevo de Epic (2026) numera desde 0.x con el mismo id de winget
    // que el viejo 1.3.x: "actualizar" reinstalaría el launcher antiguo.
    if u.id.eq_ignore_ascii_case("EpicGames.EpicGamesLauncher") && major(&u.ver) == Some(0) {
        return Some("Tienes el launcher NUEVO de Epic (versiones 0.x). winget cree que el viejo 1.3.x es más nuevo y lo reinstalaría. El launcher nuevo se actualiza solo.");
    }
    None
}

pub fn upgrade_some(r: &Rep, list: &[Upg]) {
    let n = list.len() as u32;
    let mut good = 0;
    for (i, u) in list.iter().enumerate() {
        r.step(i as u32 + 1, n, &format!("{}  {} → {}", u.name, u.ver, u.avail));
        if u.id.ends_with('…') {
            r.say(Kind::Err, "      Id recortado por winget; actualízala con \"Actualizar todo\".");
            continue;
        }
        let mut args = vec!["upgrade", "--id", u.id.as_str(), "-e", "--include-unknown"];
        args.extend(WINGET_FLAGS);
        let (ok, out) = run_status("winget", &args);
        if ok {
            good += 1;
            r.say(Kind::Ok, "      Actualizada");
        } else {
            r.tail(Kind::Err, &out, 2);
        }
    }
    r.stat("ACTUALIZADAS", format!("{good}/{n}"));
}

#[derive(Default)]
pub struct State {
    list: Vec<Upg>,
    sel: Vec<bool>,
    rx: Option<Receiver<Result<Vec<Upg>, String>>>,
    loaded: bool,
    err: String,
}

impl State {
    pub fn refresh(&mut self, ctx: &egui::Context) {
        self.loaded = true;
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(list_upgrades());
            ctx.request_repaint();
        });
    }

    /// Marca para recargar la próxima vez que se muestre.
    pub fn invalidate(&mut self) {
        self.loaded = false;
    }

    /// Devuelve lo que hay que actualizar si se pulsó un botón.
    pub fn ui(&mut self, ui: &mut Ui, ctx: &egui::Context, enabled: bool, list_h: f32) -> Option<Option<Vec<Upg>>> {
        if !self.loaded {
            self.refresh(ctx);
        }
        if let Some(Ok(res)) = self.rx.as_ref().map(|r| r.try_recv()) {
            self.rx = None;
            match res {
                Ok(v) => {
                    self.sel = v.iter().map(|u| blocked(u).is_none()).collect();
                    self.list = v;
                    self.err.clear();
                }
                Err(e) => self.err = e,
            }
        }
        let loading = self.rx.is_some();
        if loading {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        let mut out = None;
        pixel_frame(PANEL).show(ui, |ui| {
            ui.horizontal(|ui| {
                let n = self.sel.iter().filter(|&&s| s).count();
                ui.label(egui::RichText::new("ACTUALIZACIONES").color(YELLOW).size(13.0));
                ui.label(egui::RichText::new(format!("{} disponibles · {n} marcadas", self.list.len())).color(GRAY).size(11.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(enabled && !loading, |ui| {
                        if ui.small_button("Buscar de nuevo").clicked() {
                            self.refresh(ctx);
                        }
                        if ui.small_button("Ninguna").clicked() {
                            self.sel.iter_mut().for_each(|s| *s = false);
                        }
                        if ui.small_button("Todas").clicked() {
                            self.sel.iter_mut().for_each(|s| *s = true);
                        }
                    });
                });
            });
            egui::ScrollArea::vertical().id_salt("upgrades").max_height(list_h.max(80.0)).auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                if loading {
                    note(ui, "Preguntando a winget qué apps tienen versión nueva...");
                    pixel_progress(ui, None, false);
                    return;
                }
                if !self.err.is_empty() {
                    note(ui, &self.err);
                } else if self.list.is_empty() {
                    note(ui, "Todo está al día. ✔");
                }
                for (u, s) in self.list.iter().zip(self.sel.iter_mut()) {
                    let desc = format!("{}  →  {}", u.ver, u.avail);
                    if let Some(why) = blocked(u) {
                        *s = false;
                        let tip = format!("Id: {}\n\n{why}", u.id);
                        pixel_check(ui, s, &u.name, "Ya tienes la versión nueva (se actualiza sola)", false, Some(("NO ACTUALIZAR", ORANGE)), Some(&tip));
                        continue;
                    }
                    let tip = format!("Id: {}", u.id);
                    pixel_check(ui, s, &u.name, &desc, enabled, None, Some(&tip));
                }
            });
        });
        ui.add_space(10.0);
        let picked: Vec<Upg> = self.list.iter().zip(&self.sel).filter(|(_, &s)| s).map(|(u, _)| u.clone()).collect();
        ui.horizontal(|ui| {
            let label = if picked.is_empty() { "Actualizar".to_string() } else { format!("Actualizar ({})", picked.len()) };
            if pixel_button(ui, 230.0, Some(&SPR_UNDO), &label, GREEN, enabled && !loading && !picked.is_empty()).clicked() {
                out = Some(Some(picked));
            }
            if pixel_button(ui, 230.0, None, "Actualizar todo", BLUE, enabled && !loading).clicked() {
                // con algo bloqueado no se usa --all: se actualiza la lista sin eso
                out = if self.list.iter().any(|u| blocked(u).is_some()) {
                    Some(Some(self.list.iter().filter(|u| blocked(u).is_none()).cloned().collect()))
                } else {
                    Some(None)
                };
            }
        });
        note(ui, "Las apps abiertas pueden necesitar cerrarse para actualizarse.");
        out
    }
}

// ---------------------------------------------------------------------
//  Exportar / importar la lista de apps del catálogo
// ---------------------------------------------------------------------

pub fn list_path() -> PathBuf {
    let docs = std::env::var("USERPROFILE").map(|h| PathBuf::from(h).join("Documents")).unwrap_or_else(|_| PathBuf::from("."));
    docs.join("KevWinOptimizer-apps.txt")
}

/// Guarda las apps marcadas + instaladas. Devuelve cuántas.
pub fn export(sel: &[bool], st: &[Option<bool>]) -> Result<(PathBuf, usize), String> {
    let (txt, n) = list_text(sel, st);
    let p = list_path();
    std::fs::write(&p, txt).map_err(|e| e.to_string())?;
    let _ = Command::new("explorer").arg(format!("/select,{}", p.display())).spawn();
    Ok((p, n))
}

/// Texto del archivo de lista (instaladas + marcadas) y cuántas apps lleva.
pub fn list_text(sel: &[bool], st: &[Option<bool>]) -> (String, usize) {
    let ids: Vec<&str> = APPS
        .iter()
        .enumerate()
        .filter(|(i, _)| sel.get(*i) == Some(&true) || st.get(*i) == Some(&Some(true)))
        .map(|(_, a)| a.2)
        .collect();
    let mut txt = String::from("# Lista de apps de KevWinOptimizer. Arrastra este archivo a la ventana de APPS para marcarlas.\r\n");
    for id in &ids {
        txt.push_str(id);
        txt.push_str("\r\n");
    }
    (txt, ids.len())
}

/// Marca las apps del archivo que no estén instaladas. Devuelve (marcadas, desconocidas).
pub fn import(path: &std::path::Path, sel: &mut [bool], st: &[Option<bool>]) -> Result<(usize, usize), String> {
    let txt = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let (mut n, mut unknown) = (0, 0);
    for l in txt.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        match APPS.iter().position(|a| a.2.eq_ignore_ascii_case(l)) {
            Some(i) if st.get(i) != Some(&Some(true)) => {
                if !sel[i] {
                    sel[i] = true;
                    n += 1;
                }
            }
            Some(_) => {}
            None => unknown += 1,
        }
    }
    Ok((n, unknown))
}
