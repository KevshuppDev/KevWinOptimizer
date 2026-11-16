//! Configuración guardada, icono de bandeja, inicio con Windows y limpieza programada.

use crate::{optimizar, ps, run_status, Kind, Msg, Rep, OPTS};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_HIDE, SW_RESTORE, SW_SHOW};

pub const TASK_NAME: &str = "KevWinOptimizer";
pub const DAY_OPTS: [u32; 5] = [1, 3, 7, 14, 30];

// ---------------------------------------------------------------------
//  Configuración (%APPDATA%\KevWinOptimizer\config.txt, clave=valor)
// ---------------------------------------------------------------------

#[derive(Clone)]
pub struct Config {
    pub tray_on_close: bool,
    /// Modo simple: solo acciones seguras para quien no sabe de informática.
    pub simple: bool,
    pub sched_on: bool,
    pub sched_days: u32,
    pub sched_hour: u32,
    pub sched_sel: Vec<bool>,
    /// Día local (número de día) de la última limpieza programada.
    pub last_day: i64,
    pub last_text: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tray_on_close: true,
            simple: true,
            sched_on: false,
            sched_days: 7,
            sched_hour: 12,
            sched_sel: OPTS.iter().map(|o| o.2).collect(),
            last_day: 0,
            last_text: String::new(),
        }
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("KevWinOptimizer").join("config.txt")
}

impl Config {
    pub fn load() -> Self {
        let mut c = Self::default();
        let Ok(txt) = std::fs::read_to_string(config_path()) else { return c };
        for line in txt.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "tray_on_close" => c.tray_on_close = v == "1",
                "simple" => c.simple = v == "1",
                "sched_on" => c.sched_on = v == "1",
                "sched_days" => c.sched_days = v.parse().unwrap_or(7),
                "sched_hour" => c.sched_hour = v.parse::<u32>().unwrap_or(12).min(23),
                "sched_sel" => {
                    for (s, ch) in c.sched_sel.iter_mut().zip(v.chars()) {
                        *s = ch == '1';
                    }
                }
                "last_day" => c.last_day = v.parse().unwrap_or(0),
                "last_text" => c.last_text = v.to_string(),
                _ => {}
            }
        }
        c
    }

    pub fn save(&self) {
        let b = |x: bool| if x { "1" } else { "0" };
        let sel: String = self.sched_sel.iter().map(|&s| b(s)).collect();
        let txt = format!(
            "tray_on_close={}\nsimple={}\nsched_on={}\nsched_days={}\nsched_hour={}\nsched_sel={sel}\nlast_day={}\nlast_text={}\n",
            b(self.tray_on_close), b(self.simple), b(self.sched_on), self.sched_days, self.sched_hour, self.last_day, self.last_text
        );
        let p = config_path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, txt);
    }
}

// ---------------------------------------------------------------------
//  Hora local (sin dependencias de fechas)
// ---------------------------------------------------------------------

pub struct LocalTime {
    pub day: i64,
    pub hour: u32,
    pub text: String,
}

pub fn local_now() -> LocalTime {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut st = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    let (y, m, d) = (st.wYear as i64, st.wMonth as i64, st.wDay as i64);
    // días desde la época (algoritmo de Howard Hinnant)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    LocalTime {
        day: era * 146_097 + doe - 719_468,
        hour: st.wHour as u32,
        text: format!("{:02}/{:02}/{} {:02}:{:02}", d, m, y, st.wHour, st.wMinute),
    }
}

/// ¿Toca la limpieza programada ahora?
fn due(c: &Config, now: &LocalTime) -> bool {
    c.sched_on && c.sched_sel.contains(&true) && now.day - c.last_day >= c.sched_days as i64 && now.hour >= c.sched_hour
}

/// Texto de la próxima ejecución.
pub fn next_text(c: &Config) -> String {
    if !c.sched_on {
        return "desactivada".into();
    }
    let now = local_now();
    let left = (c.last_day + c.sched_days as i64 - now.day).max(0);
    match left {
        0 if now.hour >= c.sched_hour => "en cuanto la app esté abierta".into(),
        0 => format!("hoy a las {:02}:00", c.sched_hour),
        1 => format!("mañana a las {:02}:00", c.sched_hour),
        n => format!("en {n} días a las {:02}:00", c.sched_hour),
    }
}

// ---------------------------------------------------------------------
//  Inicio con Windows (Programador de tareas, con privilegios: sin UAC)
// ---------------------------------------------------------------------

pub fn autostart_enabled() -> bool {
    run_status("schtasks", &["/Query", "/TN", TASK_NAME]).0
}

pub fn set_autostart(on: bool) -> Result<(), String> {
    if !on {
        let (ok, out) = run_status("schtasks", &["/Delete", "/TN", TASK_NAME, "/F"]);
        return if ok { Ok(()) } else { Err(out.join(" ")) };
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.display().to_string().replace('\'', "''");
    // solo comillas simples: el script va por línea de comandos
    let script = format!(
        "$a = New-ScheduledTaskAction -Execute '{exe}' -Argument '--tray'; \
         $t = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME; \
         $s = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit (New-TimeSpan -Seconds 0); \
         $p = New-ScheduledTaskPrincipal -UserId $env:USERNAME -RunLevel Highest -LogonType Interactive; \
         Register-ScheduledTask -TaskName '{TASK_NAME}' -Action $a -Trigger $t -Settings $s -Principal $p -Force | Out-Null"
    );
    let (ok, out) = ps(&script);
    if ok { Ok(()) } else { Err(out.join(" ")) }
}

// ---------------------------------------------------------------------
//  Estado compartido entre la UI, la bandeja y el programador
// ---------------------------------------------------------------------

pub static HWND: AtomicIsize = AtomicIsize::new(0);
pub static UI_BUSY: AtomicBool = AtomicBool::new(false);
pub static SCHED_RUNNING: AtomicBool = AtomicBool::new(false);
static FORCE_CLEAN: AtomicBool = AtomicBool::new(false);
static CTX: OnceLock<eframe::egui::Context> = OnceLock::new();

pub type SharedLog = Arc<Mutex<Vec<(Kind, String)>>>;

thread_local! {
    static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
    static PLAN_ITEMS: RefCell<Vec<(CheckMenuItem, String)>> = const { RefCell::new(Vec::new()) };
}

pub fn hide_window() {
    let h = HWND.load(Ordering::Relaxed);
    if h != 0 {
        unsafe { ShowWindow(h as _, SW_HIDE) };
    }
}

pub fn show_window() {
    let h = HWND.load(Ordering::Relaxed);
    if h != 0 {
        unsafe {
            ShowWindow(h as _, SW_SHOW);
            ShowWindow(h as _, SW_RESTORE);
            SetForegroundWindow(h as _);
        }
    }
    if let Some(c) = CTX.get() {
        c.request_repaint();
    }
}

fn quit() {
    TRAY.with(|t| t.borrow_mut().take());
    std::process::exit(0);
}

/// Crea el icono de la bandeja. Devuelve false si no se pudo.
pub fn create_tray(ctx: &eframe::egui::Context, icon: &eframe::egui::IconData) -> bool {
    let _ = CTX.set(ctx.clone());
    let open = MenuItem::new("Abrir KevWinOptimizer", true, None);
    let clean = MenuItem::new("Limpiar ahora", true, None);
    let exit = MenuItem::new("Salir", true, None);
    // submenú con los planes de energía (el activo, marcado)
    let power = Submenu::new("Plan de energía", true);
    let mut plan_ids: Vec<(MenuId, String)> = Vec::new();
    let mut items = Vec::new();
    for p in crate::energia::power_plans() {
        let it = CheckMenuItem::new(&p.name, true, p.active, None);
        let _ = power.append(&it);
        plan_ids.push((it.id().clone(), p.guid.clone()));
        items.push((it, p.guid));
    }
    PLAN_ITEMS.with(|v| *v.borrow_mut() = items);
    let menu = Menu::new();
    let _ = menu.append_items(&[&open, &clean, &power, &PredefinedMenuItem::separator(), &exit]);
    let Ok(ico) = tray_icon::Icon::from_rgba(icon.rgba.clone(), icon.width, icon.height) else { return false };
    let tray = TrayIconBuilder::new()
        .with_tooltip("KevWinOptimizer")
        .with_icon(ico)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build();
    let Ok(tray) = tray else { return false };
    TRAY.with(|t| *t.borrow_mut() = Some(tray));

    TrayIconEvent::set_event_handler(Some(|e: TrayIconEvent| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
        | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } = e
        {
            show_window();
        }
    }));
    let (open, clean, exit) = (open.id().clone(), clean.id().clone(), exit.id().clone());
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if e.id == open {
            show_window();
        } else if e.id == clean {
            FORCE_CLEAN.store(true, Ordering::Relaxed);
        } else if let Some((_, guid)) = plan_ids.iter().find(|(id, _)| *id == e.id) {
            crate::energia::set_plan(guid);
            POWER_DIRTY.store(true, Ordering::Relaxed);
            if let Some(c) = CTX.get() {
                c.request_repaint();
            }
        } else if e.id == exit {
            quit();
        }
    }));
    true
}

// ---------------------------------------------------------------------
//  Programador: hilo que lanza la limpieza aunque la ventana esté oculta
// ---------------------------------------------------------------------

pub fn start_scheduler(cfg: Arc<Mutex<Config>>, log: SharedLog) {
    thread::spawn(move || {
        let mut tick = 0u32;
        loop {
            thread::sleep(Duration::from_secs(1));
            tick += 1;
            let force = FORCE_CLEAN.load(Ordering::Relaxed);
            if !force && tick % 30 != 0 {
                continue;
            }
            if UI_BUSY.load(Ordering::Relaxed) {
                continue; // se reintenta cuando la UI termine
            }
            let now = local_now();
            let c = cfg.lock().unwrap().clone();
            if !force && !due(&c, &now) {
                continue;
            }
            FORCE_CLEAN.store(false, Ordering::Relaxed);
            let sel = if c.sched_sel.contains(&true) { c.sched_sel.clone() } else { OPTS.iter().map(|o| o.2).collect() };
            run_clean(&sel, &log, if force { "Limpieza desde la bandeja" } else { "Limpieza programada" });
            let mut g = cfg.lock().unwrap();
            g.last_day = now.day;
            g.last_text = now.text;
            g.save();
        }
    });
}

fn run_clean(sel: &[bool], log: &SharedLog, title: &str) {
    SCHED_RUNNING.store(true, Ordering::Relaxed);
    let push = |k, s: String| {
        log.lock().unwrap().push((k, s));
        if let Some(c) = CTX.get() {
            c.request_repaint();
        }
    };
    push(Kind::Info, format!("== {title} ({}) ==", local_now().text));
    let (tx, rx) = channel();
    let sel = sel.to_vec();
    let h = thread::spawn(move || optimizar(&Rep(&tx), &sel));
    for m in rx {
        match m {
            Msg::Line(k, l) => push(k, l),
            Msg::Stat(n, v) => push(Kind::Ok, format!("{n}: {v}")),
            _ => {}
        }
    }
    let _ = h.join();
    push(Kind::Ok, format!("== {title} terminada =="));
    SCHED_RUNNING.store(false, Ordering::Relaxed);
}

/// El plan de energía cambió desde la bandeja (la UI debe recargar).
pub static POWER_DIRTY: AtomicBool = AtomicBool::new(false);

/// Pone la marca del menú de la bandeja en el plan activo.
pub fn sync_power_menu() {
    let plans = crate::energia::power_plans();
    PLAN_ITEMS.with(|v| {
        for (it, guid) in v.borrow().iter() {
            it.set_checked(plans.iter().any(|p| &p.guid == guid && p.active));
        }
    });
}

/// Si la tarea de inicio apunta a otro .exe (se renombró o movió), la vuelve a crear.
pub fn fix_autostart_path() {
    let (ok, xml) = run_status("schtasks", &["/Query", "/TN", TASK_NAME, "/XML"]);
    let Ok(exe) = std::env::current_exe() else { return };
    let exe = exe.display().to_string().to_lowercase();
    if ok && !xml.join(" ").to_lowercase().contains(&exe) {
        let _ = set_autostart(true);
    }
}
