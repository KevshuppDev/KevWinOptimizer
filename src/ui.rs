//! Paleta, sprites y widgets pixel art.

use crate::*;
use eframe::egui::{
    self, epaint::Shadow, pos2, vec2, Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect,
    Response, Rounding, Sense, Stroke, Ui,
};

// =====================================================================
//  PALETA Y SPRITES
// =====================================================================

pub(crate) const BG: Color32 = Color32::from_rgb(0x1a, 0x1c, 0x2c);
pub(crate) const PANEL: Color32 = Color32::from_rgb(0x33, 0x3c, 0x57);
pub(crate) const DARK: Color32 = Color32::from_rgb(0x0f, 0x0f, 0x1b);
pub(crate) const TEXT: Color32 = Color32::from_rgb(0xf4, 0xf4, 0xf4);
pub(crate) const GRAY: Color32 = Color32::from_rgb(0x94, 0xb0, 0xc2);
pub(crate) const DIMGRAY: Color32 = Color32::from_rgb(0x56, 0x6c, 0x86);
pub(crate) const GREEN: Color32 = Color32::from_rgb(0x38, 0xb7, 0x64);
pub(crate) const LIME: Color32 = Color32::from_rgb(0xa7, 0xf0, 0x70);
pub(crate) const YELLOW: Color32 = Color32::from_rgb(0xff, 0xcd, 0x75);
pub(crate) const ORANGE: Color32 = Color32::from_rgb(0xef, 0x7d, 0x57);
pub(crate) const RED: Color32 = Color32::from_rgb(0xb1, 0x3e, 0x53);
pub(crate) const BLUE: Color32 = Color32::from_rgb(0x41, 0xa6, 0xf6);
pub(crate) const CYAN: Color32 = Color32::from_rgb(0x73, 0xef, 0xf7);
pub(crate) const PURPLE: Color32 = Color32::from_rgb(0xb4, 0x8c, 0xf2);

pub(crate) const PX: f32 = 3.0; // tamaño de "pixel" para bordes y sombras

pub(crate) const SPR_BROOM: [&str; 8] = [
    "......##", ".....#y#", "....#y#.", "...#y#..", "..#oo#..", ".#oooo#.", "#oo#oo#.", "#######.",
];
pub(crate) const SPR_GLOBE: [&str; 8] = [
    "..####..", ".#bgbb#.", "#bggbbb#", "#bbggbb#", "#gbbbgg#", "#bbbbgb#", ".#bbbb#.", "..####..",
];
pub(crate) const SPR_WRENCH: [&str; 8] = [
    ".##..##.", ".#w##w#.", ".#wwww#.", "..#ww#..", "..#ww#..", "..#ww#..", "..#ww#..", "...##...",
];
pub(crate) const SPR_CHECK: [&str; 8] = [
    "........", "......##", ".....#w#", "##..#w#.", "#w##w#..", ".#ww#...", "..##....", "........",
];
pub(crate) const SPR_UNDO: [&str; 8] = [
    "..#.....", ".#w####.", "#wwwwww#", ".#w###w#", "..#...w#", ".....#w#", "..####w#", "..#####.",
];
pub(crate) const SPR_BOX: [&str; 8] = [
    "..####..", ".#yyyy#.", "#yyyyyy#", "########", "#oo##oo#", "#oo##oo#", "#oooooo#", "########",
];
pub(crate) const SPR_PC: [&str; 10] = [
    "############",
    "#cccccccccc#",
    "#cwcccccccc#",
    "#ccc#cc#ccc#",
    "#cccccccccc#",
    "#ccc#..#ccc#",
    "#cccc##cccc#",
    "############",
    ".....##.....",
    "...######...",
];
pub(crate) const SPR_PC_BUSY: [&str; 10] = [
    "############",
    "#cccccccccc#",
    "#cwcccccccc#",
    "#cc##cc##cc#",
    "#cccccccccc#",
    "#cccc##cccc#",
    "#cccc##cccc#",
    "############",
    ".....##.....",
    "...######...",
];
pub(crate) const SPR_ICON: [&str; 16] = [
    "................",
    ".##############.",
    ".#cccccccccccc#.",
    ".#cwwccccccccc#.",
    ".#cwcccccccccc#.",
    ".#cccc#cc#cccc#.",
    ".#cccccccccccc#.",
    ".#cccc#..#cccc#.",
    ".#ccccc##ccccc#.",
    ".#cccccccccccc#.",
    ".##############.",
    "......####......",
    "......#yy#......",
    "...##########...",
    "...#gggggggg#...",
    "...##########...",
];

pub(crate) fn sprite_color(c: char) -> Option<Color32> {
    Some(match c {
        '#' => DARK,
        'w' => TEXT,
        'y' => YELLOW,
        'o' => ORANGE,
        'b' => BLUE,
        'g' => GREEN,
        'c' => CYAN,
        'r' => RED,
        _ => return None,
    })
}

pub(crate) fn draw_sprite(p: &Painter, top_left: Pos2, rows: &[&str], scale: f32) {
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if let Some(col) = sprite_color(ch) {
                let min = top_left + vec2(x as f32 * scale, y as f32 * scale);
                p.rect_filled(Rect::from_min_size(min, vec2(scale, scale)), 0.0, col);
            }
        }
    }
}

/// Icono de la ventana generado desde el sprite (escalado x4, 64x64).
pub(crate) fn window_icon() -> egui::IconData {
    let (k, n) = (4usize, 16usize);
    let size = n * k;
    let mut rgba = vec![0u8; size * size * 4];
    for (y, row) in SPR_ICON.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let Some(c) = sprite_color(ch) else { continue };
            for dy in 0..k {
                for dx in 0..k {
                    let i = ((y * k + dy) * size + x * k + dx) * 4;
                    rgba[i..i + 4].copy_from_slice(&[c.r(), c.g(), c.b(), 255]);
                }
            }
        }
    }
    egui::IconData { rgba, width: size as u32, height: size as u32 }
}

pub(crate) fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

// =====================================================================
//  WIDGETS PIXEL
// =====================================================================

pub(crate) fn pixel_button(
    ui: &mut Ui,
    width: f32,
    icon: Option<&[&str]>,
    label: &str,
    color: Color32,
    enabled: bool,
) -> Response {
    let sh = PX + 1.0;
    let (rect, resp) = ui.allocate_exact_size(
        vec2(width, 46.0),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    let pressed = enabled && resp.is_pointer_button_down_on();
    let hovered = enabled && resp.hovered();
    let off = if pressed { sh } else { 0.0 };
    let body = Rect::from_min_max(rect.min + vec2(off, off), rect.max - vec2(sh - off, sh - off));
    let p = ui.painter();

    if !pressed {
        p.rect_filled(body.translate(vec2(sh, sh)), 0.0, DARK);
    }
    let fill = if !enabled {
        DIMGRAY
    } else if hovered {
        mix(color, TEXT, 0.25)
    } else {
        color
    };
    p.rect_filled(body, 0.0, fill);
    let inner = body.shrink(PX);
    p.rect_filled(Rect::from_min_size(inner.min, vec2(inner.width(), PX)), 0.0, mix(fill, TEXT, 0.35));
    p.rect_filled(
        Rect::from_min_max(pos2(inner.min.x, inner.max.y - PX), inner.max),
        0.0,
        mix(fill, DARK, 0.3),
    );
    p.rect_stroke(body, 0.0, Stroke::new(PX, DARK));

    let mut x = body.min.x + 12.0;
    if let Some(rows) = icon {
        let s = 3.0;
        draw_sprite(p, pos2(x, body.center().y - 4.0 * s), rows, s);
        x += 8.0 * s + 10.0;
    }
    let tc = if enabled { DARK } else { BG };
    p.text(pos2(x, body.center().y), Align2::LEFT_CENTER, label, FontId::monospace(14.0), tc);

    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// Botón pixel pequeño (ancho según el texto).
pub(crate) fn pixel_small(ui: &mut Ui, label: &str, color: Color32, enabled: bool) -> Response {
    let sh = 3.0;
    let w = label.chars().count() as f32 * 7.4 + 22.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), if enabled { Sense::click() } else { Sense::hover() });
    let pressed = enabled && resp.is_pointer_button_down_on();
    let off = if pressed { sh } else { 0.0 };
    let body = Rect::from_min_max(rect.min + vec2(off, off), rect.max - vec2(sh - off, sh - off));
    let p = ui.painter();
    if !pressed {
        p.rect_filled(body.translate(vec2(sh, sh)), 0.0, DARK);
    }
    let fill = if !enabled {
        DIMGRAY
    } else if resp.hovered() {
        mix(color, TEXT, 0.25)
    } else {
        color
    };
    p.rect_filled(body, 0.0, fill);
    p.rect_filled(Rect::from_min_size(body.min + vec2(2.0, 2.0), vec2(body.width() - 4.0, 2.0)), 0.0, mix(fill, TEXT, 0.35));
    p.rect_stroke(body, 0.0, Stroke::new(2.0_f32, DARK));
    p.text(body.center(), Align2::CENTER_CENTER, label, FontId::monospace(12.0), if enabled { DARK } else { BG });
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

pub(crate) fn pixel_tab(ui: &mut Ui, label: &str, selected: bool) -> Response {
    let w = label.chars().count() as f32 * 8.5 + 28.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
    let p = ui.painter();
    let fill = if selected {
        YELLOW
    } else if resp.hovered() {
        mix(PANEL, TEXT, 0.15)
    } else {
        mix(PANEL, DARK, 0.35)
    };
    let body = if selected { rect } else { rect.translate(vec2(0.0, 3.0)) };
    p.rect_filled(body, 0.0, fill);
    p.rect_stroke(body, 0.0, Stroke::new(PX, DARK));
    let tc = if selected { DARK } else { GRAY };
    p.text(body.center(), Align2::CENTER_CENTER, label, FontId::monospace(13.0), tc);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub(crate) fn pixel_check(
    ui: &mut Ui,
    on: &mut bool,
    name: &str,
    desc: &str,
    enabled: bool,
    badge: Option<(&str, Color32)>,
    tip: Option<&str>,
) {
    let (rect, resp) = ui.allocate_exact_size(
        vec2(ui.available_width(), 36.0),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    if enabled && resp.clicked() {
        *on = !*on;
    }
    let p = ui.painter();
    if enabled && resp.hovered() {
        p.rect_filled(rect, 0.0, mix(PANEL, TEXT, 0.08));
    }
    let b = Rect::from_min_size(pos2(rect.min.x + 6.0, rect.center().y - 9.0), vec2(18.0, 18.0));
    p.rect_filled(b, 0.0, DARK);
    if *on {
        p.rect_filled(b.shrink(4.0), 0.0, if enabled { LIME } else { DIMGRAY });
    }
    let x = b.max.x + 10.0;
    let tc = if enabled { TEXT } else { GRAY };
    p.text(pos2(x, rect.center().y - 7.0), Align2::LEFT_CENTER, name, FontId::monospace(13.0), tc);
    p.text(pos2(x, rect.center().y + 9.0), Align2::LEFT_CENTER, desc, FontId::monospace(10.0), GRAY);
    if let Some((txt, col)) = badge {
        // etiqueta pixel justo después del nombre
        let name_w = name.chars().count() as f32 * 7.8;
        let w = txt.chars().count() as f32 * 6.0 + 8.0;
        let tag = Rect::from_min_size(pos2(x + name_w + 6.0, rect.center().y - 14.0), vec2(w, 14.0));
        p.rect_filled(tag, 0.0, col);
        p.text(tag.center(), Align2::CENTER_CENTER, txt, FontId::monospace(10.0), DARK);
    }
    let resp = if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp };
    if let Some(t) = tip {
        resp.on_hover_ui(|ui| {
            ui.set_max_width(280.0);
            ui.label(egui::RichText::new(name).color(YELLOW).size(13.0));
            ui.label(egui::RichText::new(t).color(TEXT).size(12.0));
        });
    }
}

/// Lista de casillas con cabecera (Todas / Ninguna / Recomendadas).
pub(crate) fn check_list(
    ui: &mut Ui,
    title: &str,
    items: &[(&str, &str)],
    sel: &mut [bool],
    defaults: Option<&[bool]>,
    states: Option<&[Option<bool>]>,
    enabled: bool,
    cols: usize,
    height: f32,
    // false = sin botones Todas/Ninguna/Recomendadas (la pestaña tiene sus presets)
    buttons: bool,
) {
    // Con `states`, la casilla es el estado deseado: si difiere del real, se marca el cambio.
    let badge = |i: usize, want: bool| -> Option<(&'static str, Color32)> {
        let st = *states?.get(i)?;
        match (st, want) {
            (None, _) => Some(("...", DIMGRAY)),
            (Some(true), true) => Some(("ACTIVO", GREEN)),
            (Some(true), false) => Some(("DESHACER", ORANGE)),
            (Some(false), true) => Some(("APLICAR", LIME)),
            (Some(false), false) => None,
        }
    };
    pixel_frame(PANEL).show(ui, |ui| {
        ui.horizontal(|ui| {
            let n = sel.iter().filter(|&&s| s).count();
            ui.label(egui::RichText::new(title).color(YELLOW).size(13.0));
            ui.label(egui::RichText::new(format!("{n}/{}", sel.len())).color(GRAY).size(11.0));
            if !buttons {
                return;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    if states.is_none() && ui.small_button("Ninguna").clicked() {
                        sel.iter_mut().for_each(|o| *o = false);
                    }
                    if ui.small_button("Todas").clicked() {
                        sel.iter_mut().for_each(|o| *o = true);
                    }
                    if let Some(st) = states {
                        if ui
                            .small_button("Sin cambios")
                            .on_hover_text("Vuelve todas las casillas al estado actual del sistema")
                            .clicked()
                        {
                            for (o, s) in sel.iter_mut().zip(st) {
                                *o = *s == Some(true);
                            }
                        }
                    }
                    if let Some(d) = defaults {
                        if ui
                            .small_button("Recomendadas")
                            .on_hover_text("Marca las recomendadas (sin deshacer lo que ya tienes)")
                            .clicked()
                        {
                            for (i, (o, &def)) in sel.iter_mut().zip(d).enumerate() {
                                let active = states.and_then(|s| s.get(i).copied().flatten()) == Some(true);
                                *o = if states.is_some() { def || active } else { def };
                            }
                        }
                    }
                });
            });
        });
        egui::ScrollArea::vertical()
            .id_salt(title)
            .max_height(height.max(80.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                if cols <= 1 {
                    for (i, (o, (name, desc))) in sel.iter_mut().zip(items).enumerate() {
                        let b = badge(i, *o);
                        pixel_check(ui, o, name, desc, enabled, b, None);
                    }
                } else {
                    ui.columns(cols, |c| {
                        for (i, (o, (name, desc))) in sel.iter_mut().zip(items).enumerate() {
                            c[i % cols].spacing_mut().item_spacing.y = 2.0;
                            let b = badge(i, *o);
                            pixel_check(&mut c[i % cols], o, name, desc, enabled, b, None);
                        }
                    });
                }
            });
    });
}

/// Ficha compacta de una app: casilla + nombre. Instalada = ✔ verde y bloqueada.
/// `state`: Some(true) instalada, Some(false) no, None comprobando.
#[allow(clippy::too_many_arguments)]
pub(crate) fn app_chip(
    ui: &mut Ui,
    width: f32,
    on: &mut bool,
    name: &str,
    id: &str,
    desc: &str,
    state: Option<bool>,
    enabled: bool,
    star: bool,
) {
    let installed = state == Some(true);
    // instalada + marcada = se va a desinstalar
    let removing = installed && *on;
    let clickable = enabled;
    let (rect, resp) = ui.allocate_exact_size(
        vec2(width, 28.0),
        if clickable { Sense::click() } else { Sense::hover() },
    );
    if clickable && resp.clicked() {
        *on = !*on;
    }
    let p = ui.painter();
    let hovered = clickable && resp.hovered();
    let fill = if removing {
        mix(PANEL, ORANGE, 0.22)
    } else if installed {
        mix(PANEL, GREEN, if hovered { 0.28 } else { 0.18 })
    } else if *on {
        mix(PANEL, LIME, 0.22)
    } else if hovered {
        mix(PANEL, TEXT, 0.10)
    } else {
        mix(PANEL, DARK, 0.30)
    };
    p.rect_filled(rect, 0.0, fill);
    let border = if removing { ORANGE } else if *on { LIME } else if star { YELLOW } else { DARK };
    p.rect_stroke(rect, 0.0, Stroke::new(2.0_f32, border));

    // casilla
    let b = Rect::from_min_size(pos2(rect.min.x + 7.0, rect.center().y - 6.0), vec2(12.0, 12.0));
    p.rect_filled(b, 0.0, DARK);
    if removing {
        // casilla desmarcada con ✖ naranja: se desinstalará
        for (x, y) in [(1, 1), (2, 2), (3, 3), (4, 4), (4, 1), (3, 2), (2, 3), (1, 4)] {
            p.rect_filled(Rect::from_min_size(b.min + vec2(x as f32 * 2.0, y as f32 * 2.0), vec2(2.0, 2.0)), 0.0, ORANGE);
        }
    } else if installed {
        // ✔ pixel verde dentro del cuadro oscuro
        for (x, y) in [(1, 3), (2, 4), (3, 3), (4, 2), (5, 1)] {
            p.rect_filled(Rect::from_min_size(b.min + vec2(x as f32 * 2.0, y as f32 * 2.0), vec2(2.0, 2.0)), 0.0, LIME);
        }
    } else if *on {
        p.rect_filled(b.shrink(3.0), 0.0, if enabled { LIME } else { DIMGRAY });
    } else if state.is_none() {
        p.rect_filled(b.shrink(5.0), 0.0, DIMGRAY); // comprobando
    }

    // nombre (recortado si no entra)
    let max_chars = ((width - 30.0) / 7.2).max(4.0) as usize;
    let name_full = if star { format!("★ {name}") } else { name.to_string() };
    let name = name_full.as_str();
    let shown: String = if name.chars().count() > max_chars {
        name.chars().take(max_chars - 1).chain(std::iter::once('…')).collect()
    } else {
        name.to_string()
    };
    let tc = if removing { ORANGE } else if installed { LIME } else if clickable { TEXT } else { GRAY };
    p.text(pos2(b.max.x + 8.0, rect.center().y), Align2::LEFT_CENTER, shown, FontId::monospace(12.0), tc);

    let resp = if clickable { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp };
    resp.on_hover_ui(|ui| {
        ui.set_max_width(280.0);
        ui.label(egui::RichText::new(name).color(YELLOW).size(13.0));
        ui.label(egui::RichText::new(desc).color(TEXT).size(12.0));
        ui.label(egui::RichText::new(id).color(DIMGRAY).size(10.0));
        if removing {
            ui.label(egui::RichText::new("✖ Se desinstalará (clic para cancelar)").color(ORANGE).size(11.0));
        } else if installed {
            ui.label(egui::RichText::new("✔ Ya instalada · clic para desinstalar").color(LIME).size(11.0));
        }
        if star {
            ui.label(egui::RichText::new("★ Recomendado para tu PC").color(YELLOW).size(11.0));
        }
    });
}

/// Lista de apps agrupada por categoría, con packs y tooltips.
/// Minúsculas y sin tildes, para buscar sin importar acentos.
pub(crate) fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            c => c,
        })
        .collect()
}

/// Devuelve `true` si se pulsó RECARGAR (volver a detectar lo instalado).
pub(crate) fn apps_list(ui: &mut Ui, sel: &mut [bool], st: &[Option<bool>], hw: &str, search: &mut String, enabled: bool, height: f32) -> bool {
    let mut reload = false;
    let installed = |i: usize| st.get(i) == Some(&Some(true));
    // todas las palabras deben aparecer en nombre, id, descripción o categoría
    let words: Vec<String> = fold(search).split_whitespace().map(String::from).collect();
    let visible: Vec<bool> = APPS
        .iter()
        .map(|(cat, name, id, desc)| {
            let hay = fold(&format!("{cat} {name} {id} {desc}"));
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .collect();
    pixel_frame(PANEL).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            let del = (0..APPS.len()).filter(|&i| sel[i] && installed(i)).count();
            let n = sel.iter().filter(|&&s| s).count() - del;
            let inst = (0..APPS.len()).filter(|&i| installed(i)).count();
            ui.label(egui::RichText::new("INSTALAR CON WINGET").color(YELLOW).size(13.0));
            ui.label(egui::RichText::new(format!("{n} marcadas · {inst} instaladas")).color(GRAY).size(11.0));
            if del > 0 {
                ui.label(egui::RichText::new(format!("· {del} a desinstalar")).color(ORANGE).size(11.0));
            }
            ui.add_space(8.0);
            ui.add_enabled_ui(enabled, |ui| {
                for (pack, ids) in APP_PACKS {
                    let tip = ids
                        .iter()
                        .filter_map(|id| APPS.iter().find(|a| a.2 == *id).map(|a| a.1))
                        .collect::<Vec<_>>()
                        .join(", ");
                    // activo = todas sus apps (no instaladas) están marcadas; clic alterna
                    let active = (0..APPS.len())
                        .filter(|&i| ids.contains(&APPS[i].2) && !installed(i))
                        .all(|i| sel[i]);
                    let label = if active { format!("✔ {pack}") } else { pack.to_string() };
                    let btn = egui::Button::new(
                        egui::RichText::new(label).size(11.0).color(if active { DARK } else { TEXT }),
                    )
                    .selected(active)
                    .fill(if active { LIME } else { mix(PANEL, DARK, 0.35) })
                    .stroke(Stroke::new(2.0_f32, DARK));
                    let hover = if active { format!("Quitar: {tip}") } else { format!("Marcar: {tip}") };
                    if ui.add(btn).on_hover_text(hover).clicked() {
                        for i in 0..APPS.len() {
                            if ids.contains(&APPS[i].2) && !installed(i) {
                                sel[i] = !active;
                            }
                        }
                    }
                }
                if ui.small_button("Ninguna").clicked() {
                    sel.iter_mut().for_each(|o| *o = false);
                }
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("BUSCAR").color(CYAN).size(12.0));
            if ui
                .add_enabled(enabled, egui::Button::new(egui::RichText::new("RECARGAR").size(11.0)).small())
                .on_hover_text("Vuelve a comprobar qué apps están instaladas")
                .clicked()
            {
                reload = true;
            }
            let w = (ui.available_width() - 70.0).max(120.0);
            ui.add(egui::TextEdit::singleline(search).desired_width(w).hint_text("nombre, categoría o id (ej. python, navegador)..."));
            if !search.is_empty() && ui.small_button("✖").on_hover_text("Limpiar búsqueda").clicked() {
                search.clear();
            }
        });
        let shown = visible.iter().filter(|&&v| v).count();
        egui::ScrollArea::vertical()
            .id_salt("apps")
            .max_height((height - 30.0).max(80.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if shown == 0 {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("No hay apps que coincidan con la búsqueda.").color(GRAY).size(12.0));
                }
                let mut start = 0;
                while start < APPS.len() {
                    let cat = APPS[start].0;
                    let end = start + APPS[start..].iter().take_while(|a| a.0 == cat).count();
                    if !(start..end).any(|i| visible[i]) {
                        start = end;
                        continue;
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(6.0, 14.0), Sense::hover());
                        ui.painter().rect_filled(r, 0.0, YELLOW);
                        ui.label(egui::RichText::new(cat).color(CYAN).size(12.0));
                        if cat == "DRIVERS" && !hw.is_empty() {
                            ui.label(egui::RichText::new(format!("Tu PC: {hw}")).color(YELLOW).size(10.0))
                                .on_hover_text("Lo recomendado para tu hardware tiene borde amarillo ★");
                        }
                        let free: Vec<usize> = (start..end).filter(|&i| visible[i] && !installed(i)).collect();
                        let all = free.iter().all(|&i| sel[i]);
                        ui.add_enabled_ui(enabled && !free.is_empty(), |ui| {
                            if ui.small_button(if all && !free.is_empty() { "quitar" } else { "todas" }).clicked() {
                                free.iter().for_each(|&i| sel[i] = !all);
                            }
                        });
                    });
                    // fichas compactas que fluyen de lado (rejilla adaptable)
                    let gap = 6.0;
                    let w = ui.available_width();
                    let cols = ((w + gap) / (150.0 + gap)).floor().max(1.0);
                    let chip_w = ((w - gap * (cols - 1.0)) / cols).floor();
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(gap, gap);
                        for i in (start..end).filter(|&i| visible[i]) {
                            let (_, name, id, desc) = APPS[i];
                            let state = st.get(i).copied().flatten();
                            let star = matches_hw(id, hw);
                            app_chip(ui, chip_w, &mut sel[i], name, id, desc, state, enabled, star);
                        }
                    });
                    start = end;
                }
            });
    });
    reload
}

/// Barra de progreso pixel art: celdas de 4px con brillo arriba y sombra abajo.
pub(crate) fn pixel_progress(ui: &mut Ui, frac: Option<f32>, idle: bool) {
    const CELL: f32 = 4.0;
    const ROWS: usize = 4;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROWS as f32 * CELL + 2.0 * (PX + 1.0)), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, DARK);
    let inner = rect.shrink(PX + 1.0);
    let cols = (inner.width() / CELL).floor() as usize;
    let x0 = inner.min.x + (inner.width() - cols as f32 * CELL) / 2.0;
    let t = ui.input(|i| i.time);

    // rango de columnas encendidas
    let (from, to) = match frac {
        Some(f) => (0, ((f.clamp(0.0, 1.0) * cols as f32) as usize).min(cols)),
        None if idle => (0, 0),
        None => {
            let w = (cols / 6).max(8);
            let head = ((t * 90.0) as usize) % (cols + w);
            (head.saturating_sub(w), head.min(cols))
        }
    };
    const DGREEN: Color32 = Color32::from_rgb(0x25, 0x71, 0x79);
    let shade = [LIME, GREEN, GREEN, DGREEN];
    for c in 0..cols {
        let on = c >= from && c < to;
        let edge = on && c + 1 == to; // columna del borde: destello
        for r in 0..ROWS {
            let col = if on {
                if edge { mix(shade[r], TEXT, 0.45) } else { shade[r] }
            } else if (c + r) % 2 == 0 {
                mix(DARK, PANEL, 0.28)
            } else {
                mix(DARK, PANEL, 0.18)
            };
            let min = pos2(x0 + c as f32 * CELL, inner.min.y + r as f32 * CELL);
            p.rect_filled(Rect::from_min_size(min, vec2(CELL, CELL)), 0.0, col);
        }
    }
    if let Some(f) = frac {
        let txt = format!("{}%", (f * 100.0).round() as u32);
        let pos = pos2(rect.max.x - 10.0, rect.center().y);
        let font = FontId::monospace(12.0);
        p.text(pos + vec2(1.0, 1.0), Align2::RIGHT_CENTER, &txt, font.clone(), DARK);
        p.text(pos, Align2::RIGHT_CENTER, &txt, font, TEXT);
    }
}

pub(crate) fn pixel_frame(fill: Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(fill)
        .stroke(Stroke::new(PX, DARK))
        .inner_margin(12.0)
        .shadow(Shadow { offset: vec2(PX + 1.0, PX + 1.0), blur: 0.0, spread: 0.0, color: DARK })
}

pub(crate) fn note(ui: &mut Ui, s: &str) {
    ui.label(egui::RichText::new(s).color(GRAY).size(11.0));
}

pub(crate) fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    for font in style.text_styles.values_mut() {
        font.family = FontFamily::Monospace;
    }
    style.spacing.item_spacing = vec2(8.0, 8.0);
    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = DARK;
    v.window_rounding = Rounding::ZERO;
    v.menu_rounding = Rounding::ZERO;
    v.window_stroke = Stroke::new(PX, DARK);
    v.window_shadow = Shadow { offset: vec2(6.0, 6.0), blur: 0.0, spread: 0.0, color: DARK };
    v.popup_shadow = v.window_shadow;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = BLUE;
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.rounding = Rounding::ZERO;
    }
    ctx.set_style(style);
}

