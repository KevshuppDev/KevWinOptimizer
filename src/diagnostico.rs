//! DIAGNÓSTICO (busca cosas que fallan y las arregla) y DRIVERS (dispositivos con
//! problemas, impresoras, enlaces a drivers propios y copia de seguridad).

use crate::*;
use eframe::egui::{self, Ui};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::thread;

// ---------------------------------------------------------------------
//  Comprobaciones
// ---------------------------------------------------------------------

pub struct Check {
    pub name: &'static str,
    pub desc: &'static str,
    /// Script que escribe `OK` o `FAIL|detalle` (solo comillas simples).
    test: &'static str,
    /// Script que lo arregla; vacío = solo se avisa.
    fix: &'static str,
}

pub const CHECKS: &[Check] = &[
    Check {
        name: "Conexión a internet",
        desc: "Comprueba que hay red y que los nombres (DNS) se resuelven.",
        test: "$p=Test-Connection 1.1.1.1 -Count 1 -Quiet; $d=$false; try { [void][System.Net.Dns]::GetHostAddresses('www.microsoft.com'); $d=$true } catch {}; if(-not $p){ 'FAIL|Sin respuesta de internet' } elseif(-not $d){ 'FAIL|Hay red pero el DNS no responde' } else { 'OK' }",
        fix: "ipconfig /flushdns | Out-Null; ipconfig /release | Out-Null; ipconfig /renew | Out-Null; netsh winsock reset | Out-Null; 'Red reiniciada (winsock pide reiniciar para quedar completo)'",
    },
    Check {
        name: "Dispositivos con error",
        desc: "Dispositivos con un aviso amarillo en el Administrador de dispositivos.",
        test: "$e=@(Get-PnpDevice -PresentOnly | Where-Object { $_.ConfigManagerErrorCode -ne 0 }); if($e.Count){ 'FAIL|' + $e.Count + ' con error: ' + (($e | Select-Object -First 3 | ForEach-Object { $_.FriendlyName }) -join ', ') } else { 'OK' }",
        fix: "pnputil /scan-devices | Out-Null; Get-PnpDevice -PresentOnly | Where-Object { $_.ConfigManagerErrorCode -eq 22 } | Enable-PnpDevice -Confirm:$false; 'Hardware vuelto a detectar'",
    },
    Check {
        name: "Cola de impresión",
        desc: "Servicio de impresión detenido o trabajos atascados.",
        test: "$s=Get-Service Spooler; $j=@(Get-ChildItem (Join-Path $env:SystemRoot 'System32\\spool\\PRINTERS') -File).Count; if($s.Status -ne 'Running'){ 'FAIL|Servicio de impresión detenido' } elseif($j -gt 0){ 'FAIL|' + $j + ' archivos atascados en la cola' } else { 'OK' }",
        fix: "Stop-Service Spooler -Force; Remove-Item (Join-Path $env:SystemRoot 'System32\\spool\\PRINTERS\\*') -Force; Set-Service Spooler -StartupType Automatic; Start-Service Spooler; 'Cola vaciada y servicio iniciado'",
    },
    Check {
        name: "Sonido",
        desc: "El servicio de audio de Windows tiene que estar en marcha.",
        test: "$s=Get-Service Audiosrv; if($s.Status -ne 'Running'){ 'FAIL|Servicio de audio detenido' } else { 'OK' }",
        fix: "Set-Service Audiosrv -StartupType Automatic; Set-Service AudioEndpointBuilder -StartupType Automatic; Restart-Service AudioEndpointBuilder -Force; Start-Service Audiosrv; 'Audio reiniciado'",
    },
    Check {
        name: "Windows Update",
        desc: "Servicios de actualización desactivados (sin ellos no llegan parches ni drivers).",
        test: "$b=@('wuauserv','bits','UsoSvc') | Where-Object { (Get-Service $_).StartType -eq 'Disabled' }; if($b){ 'FAIL|Desactivado: ' + ($b -join ', ') } else { 'OK' }",
        fix: "foreach($n in 'wuauserv','bits','UsoSvc'){ Set-Service $n -StartupType Manual }; Start-Service wuauserv; 'Servicios de actualización en Manual'",
    },
    Check {
        name: "Hora del sistema",
        desc: "Servicio de hora detenido: causa errores de certificados en webs.",
        test: "$s=Get-Service W32Time; if($s.Status -ne 'Running'){ 'FAIL|Servicio de hora detenido' } else { 'OK' }",
        fix: "Set-Service W32Time -StartupType Automatic; Start-Service W32Time; w32tm /resync /force | Out-Null; 'Hora sincronizada'",
    },
    Check {
        name: "Antivirus",
        desc: "Protección en tiempo real de Windows Defender apagada.",
        test: "$m=Get-MpComputerStatus; if(-not $m){ 'OK' } elseif(-not $m.AntivirusEnabled){ 'OK' } elseif(-not $m.RealTimeProtectionEnabled){ 'FAIL|Protección en tiempo real apagada' } else { 'OK' }",
        fix: "Set-MpPreference -DisableRealtimeMonitoring $false; 'Protección activada'",
    },
    Check {
        name: "Búsqueda de Windows",
        desc: "El servicio de indexado: sin él la búsqueda del menú Inicio va lenta o vacía.",
        test: "$s=Get-Service WSearch; if(-not $s){ 'OK' } elseif($s.StartType -eq 'Disabled'){ 'OK' } elseif($s.Status -ne 'Running'){ 'FAIL|Servicio de búsqueda detenido' } else { 'OK' }",
        fix: "Start-Service WSearch; 'Búsqueda iniciada'",
    },
    Check {
        name: "Espacio en disco",
        desc: "Menos del 10 % libre en la unidad de Windows hace todo más lento.",
        test: "$d=Get-PSDrive ($env:SystemDrive.Substring(0,1)); $t=$d.Used+$d.Free; $p=[int](100*$d.Free/$t); if($p -lt 10){ 'FAIL|Solo ' + $p + ' % libre (' + [int]($d.Free/1GB) + ' GB)' } else { 'OK|' + $p + ' % libre' }",
        fix: "",
    },
    Check {
        name: "Reinicio pendiente",
        desc: "Una actualización espera reinicio; hasta entonces algunas cosas fallan.",
        test: "$a=Test-Path 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\WindowsUpdate\\Auto Update\\RebootRequired'; $b=Test-Path 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Component Based Servicing\\RebootPending'; if($a -or $b){ 'FAIL|Reinicia el PC para terminar de actualizar' } else { 'OK' }",
        fix: "",
    },
];

#[derive(Clone, PartialEq)]
pub enum Res {
    Ok(String),
    Fail(String),
}

fn run_checks() -> Vec<Res> {
    let script: String = CHECKS
        .iter()
        .enumerate()
        .map(|(i, c)| format!("try {{ $r = & {{ {} }} | Select-Object -Last 1 }} catch {{ $r='OK' }}; 'C{i}|' + $r;", c.test))
        .collect();
    let (_, out) = ps(&script);
    let mut res = vec![Res::Ok(String::new()); CHECKS.len()];
    for l in &out {
        let Some(rest) = l.strip_prefix('C') else { continue };
        let mut f = rest.splitn(3, '|');
        let Some(i) = f.next().and_then(|s| s.parse::<usize>().ok()).filter(|&i| i < CHECKS.len()) else { continue };
        let st = f.next().unwrap_or("OK");
        let det = f.next().unwrap_or("").to_string();
        res[i] = if st == "FAIL" { Res::Fail(det) } else { Res::Ok(det) };
    }
    res
}

pub fn fix(r: &Rep, list: &[usize]) {
    let n = list.len() as u32;
    for (k, &i) in list.iter().enumerate() {
        let c = &CHECKS[i];
        r.step(k as u32 + 1, n, c.name);
        let (ok, out) = ps(c.fix);
        if ok {
            r.say(Kind::Ok, format!("      {}", out.last().map(|s| s.as_str()).unwrap_or("Hecho")));
        } else {
            r.tail(Kind::Err, &out, 2);
        }
    }
}

// ---------------------------------------------------------------------
//  Drivers
// ---------------------------------------------------------------------

/// (nombre, texto a buscar en las impresoras instaladas, web de soporte)
const MY_DRIVERS: &[(&str, &str, &str)] = &[
    ("Epson L3210", "L3210", "https://epson.cl/Soporte/Impresoras/Impresoras-multifuncionales/Epson-L/Epson-L3210/s/SPT_C11CJ68301"),
    ("Xerox Phaser 3320", "3320", "https://www.support.xerox.com/es-es/product/phaser-3320/downloads?platform=win10x64&category=&language=es&attributeId="),
];

/// Instalador oficial de Epson ("Instalador de drivers y utilidades": impresora + escáner).
const EPSON_EXE: &str = "https://ftp.epson.com/latin/drivers/inkjet/L3210_Lite_LA.exe";
/// Para la Phaser 3320 en Windows 10/11, Xerox da el Global Print Driver (zip con el INF).
const XEROX_PAGE: &str = "https://www.support.xerox.com/es-es/product/global-printer-driver/downloads?language=es&platform=win10x64";
const XEROX_ZIP: &str = "https://download.support.xerox.com/pub/drivers/GLOBALPRINTDRIVER/drivers/win10x64/ar/UNIV_5.1116.2.0_PCL6_x64.zip";
const XEROX_DRIVER: &str = "Xerox Global Print Driver PCL6";

fn drivers_dir() -> std::path::PathBuf {
    let p = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join(r"Documents\KevWinOptimizer-drivers");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Descarga con la propia app (sin PowerShell) desde la web oficial.
fn download(r: &Rep, url: &str, dest: &std::path::Path) -> bool {
    r.say(Kind::Dim, format!("      Descargando {url}"));
    let resp = match ureq::get(url).set("User-Agent", "Mozilla/5.0").timeout(std::time::Duration::from_secs(600)).call() {
        Ok(x) => x,
        Err(e) => {
            r.say(Kind::Err, format!("      No se pudo descargar: {e}"));
            return false;
        }
    };
    let mut f = match std::fs::File::create(dest) {
        Ok(f) => f,
        Err(e) => {
            r.say(Kind::Err, format!("      No se pudo guardar: {e}"));
            return false;
        }
    };
    match std::io::copy(&mut resp.into_reader(), &mut f) {
        Ok(n) => {
            r.say(Kind::Ok, format!("      Descargado ({:.1} MB) en {}", n as f64 / 1_048_576.0, dest.display()));
            true
        }
        Err(e) => {
            r.say(Kind::Err, format!("      Descarga cortada: {e}"));
            false
        }
    }
}

/// Busca en la web de Xerox el zip PCL6 x64 más nuevo; si no, el conocido.
fn xerox_latest() -> String {
    let body = ureq::get(XEROX_PAGE)
        .set("User-Agent", "Mozilla/5.0")
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .ok()
        .and_then(|x| x.into_string().ok())
        .unwrap_or_default();
    let ver = |u: &str| -> Vec<u32> {
        u.rsplit('/').next().unwrap_or("").trim_start_matches("UNIV_").split('_').next().unwrap_or("").split('.').filter_map(|n| n.parse().ok()).collect()
    };
    let mut best = XEROX_ZIP.to_string();
    for (i, _) in body.match_indices("https://download.support.xerox.com/") {
        let u: String = body[i..].chars().take_while(|c| !matches!(c, '"' | '\'' | '<' | ' ')).collect();
        if u.ends_with("_PCL6_x64.zip") && u.contains("/win10x64/") && ver(&u) > ver(&best) {
            best = u;
        }
    }
    best
}

pub fn install_epson(r: &Rep) {
    r.step(1, 2, "Descargar instalador oficial de Epson L3210");
    let exe = drivers_dir().join("L3210_Lite_LA.exe");
    if !download(r, EPSON_EXE, &exe) {
        return;
    }
    r.step(2, 2, "Abrir instalador");
    match Command::new(&exe).spawn() {
        Ok(_) => {
            r.say(Kind::Ok, "      Instalador de Epson abierto: sigue sus pasos (instala impresora y escáner).");
            r.say(Kind::Dim, "      Ten la impresora encendida y conectada por USB o Wi-Fi.");
        }
        Err(e) => r.say(Kind::Err, format!("      No se pudo abrir: {e}")),
    }
}

pub fn install_xerox(r: &Rep) {
    r.step(1, 4, "Buscar la última versión del driver de Xerox");
    let url = xerox_latest();
    let name = url.rsplit('/').next().unwrap_or("xerox.zip").to_string();
    r.say(Kind::Info, format!("      {name}"));
    r.step(2, 4, "Descargar driver de Xerox");
    let dir = drivers_dir();
    let zip = dir.join(&name);
    if !download(r, &url, &zip) {
        return;
    }
    r.step(3, 4, "Descomprimir e instalar el driver");
    let out = dir.join(name.trim_end_matches(".zip"));
    let _ = std::fs::create_dir_all(&out);
    let (ok, o) = run_status("tar", &["-xf", &zip.to_string_lossy(), "-C", &out.to_string_lossy()]);
    if !ok {
        r.tail(Kind::Err, &o, 2);
        return;
    }
    let inf = format!(r"{}\*.inf", out.display());
    let (_, o) = run_status("pnputil", &["/add-driver", &inf, "/subdirs", "/install"]);
    r.tail(Kind::Dim, &o, 2);
    r.step(4, 4, "Registrar driver de impresora");
    let (ok, o) = ps(&format!(
        "Add-PrinterDriver -Name '{XEROX_DRIVER}' -ErrorAction Stop; \
         $p=Get-Printer | Where-Object {{ $_.Name -like '*3320*' -or $_.DriverName -like '*3320*' }}; \
         foreach($x in $p){{ Set-Printer -Name $x.Name -DriverName '{XEROX_DRIVER}'; 'Asignado a ' + $x.Name }}"
    ));
    if ok {
        r.say(Kind::Ok, format!("      Driver '{XEROX_DRIVER}' instalado."));
        for l in &o {
            r.say(Kind::Ok, format!("      {l}"));
        }
        if o.is_empty() {
            r.say(Kind::Dim, "      Conecta la Phaser 3320 (USB o red) y Windows usará este driver; si no la detecta,");
            r.say(Kind::Dim, "      agrégala en Impresoras de Windows y elige el driver de la lista.");
        }
    } else {
        r.tail(Kind::Err, &o, 2);
    }
}

const DRV_SCRIPT: &str = "$ErrorActionPreference='SilentlyContinue';
foreach($d in Get-PnpDevice -PresentOnly | Where-Object { $_.ConfigManagerErrorCode -ne 0 }){ 'BAD|' + $d.FriendlyName + '|' + $d.Class + '|' + $d.ConfigManagerErrorCode };
foreach($p in Get-CimInstance Win32_Printer){ 'PRN|' + $p.Name + '|' + $p.DriverName + '|' + $p.Default }";

#[derive(Default, Clone)]
struct DrvInfo {
    /// (nombre, clase, código de error)
    bad: Vec<(String, String, String)>,
    /// (impresora, driver)
    printers: Vec<(String, String, bool)>,
}

fn load_drivers() -> DrvInfo {
    let (_, out) = ps(&DRV_SCRIPT.replace('\n', " "));
    let mut d = DrvInfo::default();
    for l in &out {
        let f: Vec<String> = l.split('|').map(|s| s.to_string()).collect();
        let g = |i: usize| f.get(i).cloned().unwrap_or_default();
        match f[0].as_str() {
            "BAD" => d.bad.push((g(1), g(2), g(3))),
            "PRN" => d.printers.push((g(1), g(2), g(3) == "True")),
            _ => {}
        }
    }
    d
}

fn err_text(code: &str) -> &'static str {
    match code {
        "28" => "sin driver",
        "22" => "desactivado",
        "10" => "no arranca",
        "43" => "Windows lo detuvo por fallo",
        "31" | "39" => "driver dañado",
        "45" => "desconectado",
        _ => "error",
    }
}

pub fn scan_devices(r: &Rep) {
    r.step(1, 2, "Volver a detectar hardware");
    let (ok, out) = run_status("pnputil", &["/scan-devices"]);
    if ok { r.say(Kind::Ok, "      Hecho") } else { r.tail(Kind::Err, &out, 2) }
    r.step(2, 2, "Buscar drivers en Windows Update");
    run_silent("UsoClient", &["StartScan"]);
    r.say(Kind::Ok, "      Búsqueda iniciada. Los drivers aparecen en Windows Update → Opciones avanzadas → Actualizaciones opcionales.");
    open("ms-settings:windowsupdate-optionalupdates");
}

pub fn backup_drivers(r: &Rep) {
    r.step(1, 1, "Copia de seguridad de drivers");
    let dir = std::env::var("USERPROFILE").map(|p| format!(r"{p}\Documents\KevWinOptimizer-drivers")).unwrap_or_default();
    let _ = std::fs::create_dir_all(&dir);
    let (ok, out) = run_status("pnputil", &["/export-driver", "*", &dir]);
    if ok {
        r.say(Kind::Ok, format!("      Drivers guardados en {dir}"));
        r.say(Kind::Dim, "      Para reinstalarlos: Administrador de dispositivos → Actualizar → Buscar en mi equipo → esa carpeta.");
        open(&dir);
    } else {
        r.tail(Kind::Err, &out, 3);
    }
}

// ---------------------------------------------------------------------
//  Impresoras
// ---------------------------------------------------------------------

/// Copias duplicadas: "Impresora (Copia 1)" / "(Copy 2)" cuando existe la original.
fn ghost_printers(list: &[(String, String, bool)]) -> Vec<String> {
    list.iter()
        .filter_map(|(p, _, _)| {
            let i = p.rfind(" (Cop")?;
            let base = &p[..i];
            (p.ends_with(')') && list.iter().any(|(o, _, _)| o == base)).then(|| p.clone())
        })
        .collect()
}

fn q(s: &str) -> String {
    s.replace('\'', "''")
}

pub fn test_page(r: &Rep, name: &str) {
    r.step(1, 1, &format!("Página de prueba: {name}"));
    let (ok, out) = ps(&format!(
        "$p=Get-CimInstance Win32_Printer | Where-Object {{ $_.Name -eq '{}' }}; $x=Invoke-CimMethod -InputObject $p -MethodName PrintTestPage; $x.ReturnValue",
        q(name)
    ));
    if ok && out.last().map(|s| s.trim()) == Some("0") {
        r.say(Kind::Ok, "      Enviada a la impresora.");
    } else {
        r.tail(Kind::Err, &out, 2);
        r.say(Kind::Err, "      No se pudo imprimir: revisa que esté encendida y conectada.");
    }
}

pub fn set_default(r: &Rep, name: &str) {
    r.step(1, 1, &format!("Predeterminada: {name}"));
    let (ok, out) = ps(&format!(
        "SetReg 'HKCU:\\Software\\Microsoft\\Windows NT\\CurrentVersion\\Windows' LegacyDefaultPrinterMode 1; \
         $p=Get-CimInstance Win32_Printer | Where-Object {{ $_.Name -eq '{}' }}; $x=Invoke-CimMethod -InputObject $p -MethodName SetDefaultPrinter; $x.ReturnValue",
        q(name)
    ));
    if ok && out.last().map(|s| s.trim()) == Some("0") {
        r.say(Kind::Ok, "      Hecho (y Windows ya no la cambiará solo).");
    } else {
        r.tail(Kind::Err, &out, 2);
    }
}

pub fn remove_printers(r: &Rep, list: &[String]) {
    let n = list.len() as u32;
    for (i, p) in list.iter().enumerate() {
        r.step(i as u32 + 1, n, &format!("Quitar {p}"));
        let (ok, out) = ps(&format!("Remove-Printer -Name '{}'", q(p)));
        if ok { r.say(Kind::Ok, "      Quitada") } else { r.tail(Kind::Err, &out, 2) }
    }
}

// ---------------------------------------------------------------------
//  Antes de formatear: respaldo y restauración
// ---------------------------------------------------------------------

fn docs() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join("Documents")
}

/// Respaldos existentes (más nuevo primero).
pub fn backups() -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(docs())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("KevWinOptimizer-respaldo-")))
        .collect();
    v.sort();
    v.reverse();
    v
}

/// `apps` = contenido del archivo de lista de apps (ya armado en main).
pub fn full_backup(r: &Rep, apps: String) {
    let stamp = {
        let (_, o) = ps("Get-Date -Format 'yyyy-MM-dd_HHmm'");
        o.last().cloned().unwrap_or_else(|| "respaldo".into())
    };
    let dir = docs().join(format!("KevWinOptimizer-respaldo-{}", stamp.trim()));
    let _ = std::fs::create_dir_all(&dir);
    r.say(Kind::Info, format!("Carpeta: {}", dir.display()));

    r.step(1, 5, "Lista de apps");
    match std::fs::write(dir.join("apps.txt"), apps) {
        Ok(_) => r.say(Kind::Ok, "      apps.txt guardado"),
        Err(e) => r.say(Kind::Err, format!("      {e}")),
    }

    r.step(2, 5, "Redes Wi-Fi y contraseñas");
    let wifi = dir.join("wifi");
    let _ = std::fs::create_dir_all(&wifi);
    let (ok, out) = run_status("netsh", &["wlan", "export", "profile", "key=clear", &format!("folder={}", wifi.display())]);
    let n = std::fs::read_dir(&wifi).map(|d| d.count()).unwrap_or(0);
    if ok && n > 0 { r.say(Kind::Ok, format!("      {n} redes guardadas")) } else { r.tail(Kind::Dim, &out, 1) }

    r.step(3, 5, "Clave de Windows");
    let (_, out) = ps(
        "$k=(Get-CimInstance SoftwareLicensingService).OA3xOriginalProductKey; \
         $e=(Get-CimInstance Win32_OperatingSystem).Caption; \
         'Windows: ' + $e; if($k){ 'Clave de fábrica (BIOS): ' + $k } else { 'Sin clave en la BIOS: la licencia digital va ligada al equipo o a tu cuenta Microsoft.' }",
    );
    let _ = std::fs::write(dir.join("windows.txt"), out.join("\r\n"));
    r.say(Kind::Ok, "      windows.txt guardado");

    r.step(4, 5, "Impresoras");
    let (_, out) = ps("Get-Printer | ForEach-Object { $_.Name + '  |  ' + $_.DriverName + '  |  ' + $_.PortName }");
    let _ = std::fs::write(dir.join("impresoras.txt"), out.join("\r\n"));
    r.say(Kind::Ok, format!("      {} impresoras anotadas", out.len()));

    r.step(5, 5, "Drivers (puede tardar)");
    let drv = dir.join("drivers");
    let _ = std::fs::create_dir_all(&drv);
    let (ok, out) = run_status("pnputil", &["/export-driver", "*", &drv.to_string_lossy()]);
    if ok { r.say(Kind::Ok, "      Drivers exportados") } else { r.tail(Kind::Err, &out, 2) }

    r.say(Kind::Info, "Copia esta carpeta a un pendrive o la nube ANTES de formatear (el archivo wifi tiene contraseñas).");
    open(&dir.to_string_lossy());
}

pub fn restore(r: &Rep, dir: &std::path::Path) {
    r.say(Kind::Info, format!("Restaurando desde {}", dir.display()));
    r.step(1, 2, "Redes Wi-Fi");
    let mut n = 0;
    for e in std::fs::read_dir(dir.join("wifi")).into_iter().flatten().flatten() {
        let p = e.path();
        if run_status("netsh", &["wlan", "add", "profile", &format!("filename={}", p.display()), "user=all"]).0 {
            n += 1;
        }
    }
    r.say(Kind::Ok, format!("      {n} redes añadidas"));
    r.step(2, 2, "Drivers");
    let drv = dir.join("drivers");
    if drv.is_dir() {
        let (_, out) = run_status("pnputil", &["/add-driver", &format!(r"{}\*.inf", drv.display()), "/subdirs", "/install"]);
        r.tail(Kind::Dim, &out, 3);
    } else {
        r.say(Kind::Dim, "      Sin carpeta de drivers");
    }
    r.say(Kind::Ok, "Las apps del respaldo quedaron marcadas en APPS → INSTALAR: revisa y pulsa Instalar.");
}

fn open(target: &str) {
    run_silent("cmd", &["/c", "start", "", target]);
}

// ---------------------------------------------------------------------
//  Estado y UI
// ---------------------------------------------------------------------

pub enum Action {
    Fix(Vec<usize>),
    Scan,
    Backup,
    Epson,
    Xerox,
    TestPage(String),
    SetDefault(String),
    RemovePrinters(Vec<String>),
    Backup2,
    Restore(std::path::PathBuf),
}

#[derive(Default)]
pub struct State {
    res: Option<Vec<Res>>,
    rx: Option<Receiver<Vec<Res>>>,
    drv: Option<DrvInfo>,
    drx: Option<Receiver<DrvInfo>>,
}

impl State {
    pub fn invalidate(&mut self) {
        self.res = None;
        self.drv = None;
    }

    fn poll(&mut self) {
        if let Some(Ok(r)) = self.rx.as_ref().map(|r| r.try_recv()) {
            self.res = Some(r);
            self.rx = None;
        }
        if let Some(Ok(d)) = self.drx.as_ref().map(|r| r.try_recv()) {
            self.drv = Some(d);
            self.drx = None;
        }
    }

    fn scan(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(run_checks());
            ctx.request_repaint();
        });
    }

    fn scan_drv(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        self.drx = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(load_drivers());
            ctx.request_repaint();
        });
    }

    pub fn diag_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, enabled: bool) -> Option<Action> {
        self.poll();
        if self.res.is_none() && self.rx.is_none() && enabled {
            self.scan(ctx);
        }
        let busy = self.rx.is_some();
        let mut act = None;
        let fails: Vec<usize> = self
            .res
            .iter()
            .flatten()
            .enumerate()
            .filter(|(i, r)| matches!(r, Res::Fail(_)) && !CHECKS[*i].fix.is_empty())
            .map(|(i, _)| i)
            .collect();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("DIAGNÓSTICO DEL SISTEMA").color(YELLOW).size(13.0));
            if pixel_small(ui, if busy { "ANALIZANDO..." } else { "VOLVER A ANALIZAR" }, BLUE, enabled && !busy).clicked() {
                self.scan(ctx);
            }
            if pixel_small(ui, &format!("ARREGLAR TODO ({})", fails.len()), GREEN, enabled && !busy && !fails.is_empty()).clicked() {
                act = Some(Action::Fix(fails.clone()));
            }
        });
        ui.add_space(6.0);
        let h = ui.available_height();
        egui::ScrollArea::vertical().id_salt("diag").max_height(h).auto_shrink([false, false]).show(ui, |ui| {
            for (i, c) in CHECKS.iter().enumerate() {
                let r = self.res.as_ref().map(|v| &v[i]);
                pixel_frame(PANEL).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (tag, col) = match r {
                            None => ("  ...  ", GRAY),
                            Some(Res::Ok(_)) => ("  OK   ", LIME),
                            Some(Res::Fail(_)) => (" FALLA ", ORANGE),
                        };
                        ui.label(egui::RichText::new(tag).color(DARK).background_color(col).size(12.0));
                        ui.label(egui::RichText::new(c.name).color(TEXT).size(13.0)).on_hover_text(c.desc);
                        if let Some(Res::Fail(_)) = r {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if !c.fix.is_empty() && pixel_small(ui, "ARREGLAR", GREEN, enabled && !busy).clicked() {
                                    act = Some(Action::Fix(vec![i]));
                                }
                            });
                        }
                    });
                    match r {
                        Some(Res::Fail(d)) | Some(Res::Ok(d)) if !d.is_empty() => note(ui, d),
                        _ => note(ui, c.desc),
                    }
                    if let (Some(Res::Fail(_)), true) = (r, c.fix.is_empty()) {
                        note(ui, if i == CHECKS.len() - 1 { "Reinicia cuando puedas." } else { "Usa LIMPIEZA (o Limpiar en MI PC) y el analizador de SISTEMA." });
                    }
                });
                ui.add_space(4.0);
            }
        });
        act
    }

    pub fn backup_ui(&mut self, ui: &mut Ui, enabled: bool) -> Option<Action> {
        let mut act = None;
        let h = ui.available_height();
        egui::ScrollArea::vertical().id_salt("respaldo").max_height(h).auto_shrink([false, false]).show(ui, |ui| {
            pixel_frame(PANEL).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("ANTES DE FORMATEAR").color(YELLOW).size(13.0));
                ui.label(egui::RichText::new("Guarda en una carpeta todo lo que cuesta recuperar:").color(TEXT).size(12.0));
                for l in [
                    "• Lista de tus apps (instaladas + marcadas en APPS)",
                    "• Redes Wi-Fi con su contraseña",
                    "• Clave de Windows (si viene en la BIOS) y edición",
                    "• Impresoras y sus drivers",
                    "• Todos los drivers del equipo",
                ] {
                    note(ui, l);
                }
                ui.add_space(4.0);
                if pixel_small(ui, "CREAR RESPALDO", GREEN, enabled).clicked() {
                    act = Some(Action::Backup2);
                }
                note(ui, "Se guarda en Documentos\\KevWinOptimizer-respaldo-FECHA.\nCópialo a un pendrive o la nube: formatear borra Documentos.");
            });
            ui.add_space(8.0);
            pixel_frame(PANEL).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("RESTAURAR EN EL PC NUEVO").color(YELLOW).size(13.0));
                let list = backups();
                if list.is_empty() {
                    note(ui, "No hay respaldos en Documentos. Copia ahí la carpeta\nKevWinOptimizer-respaldo-... y vuelve a esta pestaña.");
                }
                for p in list.iter().take(5) {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").trim_start_matches("KevWinOptimizer-respaldo-").to_string();
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("▪ {name}")).color(TEXT).size(12.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if pixel_small(ui, "ABRIR", GRAY, true).clicked() {
                                open(&p.to_string_lossy());
                            }
                            if pixel_small(ui, "RESTAURAR", GREEN, enabled)
                                .on_hover_text("Añade las redes Wi-Fi, instala los drivers\ny marca tus apps en APPS → INSTALAR.")
                                .clicked()
                            {
                                act = Some(Action::Restore(p.clone()));
                            }
                        });
                    });
                }
            });
        });
        act
    }

    pub fn drivers_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, enabled: bool) -> Option<Action> {
        self.poll();
        if self.drv.is_none() && self.drx.is_none() && enabled {
            self.scan_drv(ctx);
        }
        let mut act = None;
        let h = ui.available_height();
        egui::ScrollArea::vertical().id_salt("drivers").max_height(h).auto_shrink([false, false]).show(ui, |ui| {
            pixel_frame(PANEL).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("DISPOSITIVOS CON PROBLEMAS").color(YELLOW).size(13.0));
                match &self.drv {
                    None => note(ui, "Revisando dispositivos..."),
                    Some(d) if d.bad.is_empty() => {
                        ui.label(egui::RichText::new("Todos los dispositivos funcionan bien.").color(LIME).size(12.0));
                    }
                    Some(d) => {
                        for (n, cls, code) in &d.bad {
                            let n = if n.is_empty() { "(dispositivo desconocido)" } else { n };
                            ui.label(egui::RichText::new(format!("✗ {n}  [{cls}] — {}", err_text(code))).color(ORANGE).size(12.0));
                        }
                    }
                }
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if pixel_small(ui, "BUSCAR DRIVERS", GREEN, enabled)
                        .on_hover_text("Vuelve a detectar el hardware y pide a Windows Update\nlos drivers que falten (salen en Actualizaciones opcionales).")
                        .clicked()
                    {
                        act = Some(Action::Scan);
                    }
                    if pixel_small(ui, "ADMIN. DE DISPOSITIVOS", BLUE, true).clicked() {
                        open("devmgmt.msc");
                    }
                    if pixel_small(ui, "REVISAR DE NUEVO", GRAY, enabled && self.drx.is_none()).clicked() {
                        self.scan_drv(ctx);
                    }
                });
                note(ui, "Windows Update tiene drivers firmados de casi todos los fabricantes.\nPara gráficas y chipset, mejor la app del fabricante (APPS → DRIVERS).");
            });
            ui.add_space(8.0);
            pixel_frame(PANEL).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("MIS DRIVERS").color(YELLOW).size(13.0));
                let printers = self.drv.as_ref().map(|d| d.printers.clone()).unwrap_or_default();
                for (name, key, url) in MY_DRIVERS {
                    let found = printers.iter().find(|(p, d, _)| p.contains(key) || d.contains(key));
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(*name).color(TEXT).size(13.0));
                        match found {
                            Some((_, drv, _)) => {
                                ui.label(egui::RichText::new(" INSTALADA ").color(DARK).background_color(LIME).size(11.0)).on_hover_text(format!("Driver: {drv}"));
                            }
                            None if self.drv.is_some() => {
                                ui.label(egui::RichText::new(" NO INSTALADA ").color(DARK).background_color(GRAY).size(11.0));
                            }
                            None => {}
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if pixel_small(ui, "WEB", GRAY, true).on_hover_text(*url).clicked() {
                                open(url);
                            }
                            let tip = if key == &"L3210" {
                                "Descarga el instalador oficial de Epson y lo abre."
                            } else {
                                "Descarga el driver oficial de Xerox (zip) y lo instala solo."
                            };
                            if pixel_small(ui, if found.is_some() { "REINSTALAR" } else { "INSTALAR" }, GREEN, enabled).on_hover_text(tip).clicked() {
                                act = Some(if key == &"L3210" { Action::Epson } else { Action::Xerox });
                            }
                        });
                    });
                }
                note(ui, "Se descargan de la web oficial a Documentos\\KevWinOptimizer-drivers.\nWEB abre la página de soporte del fabricante.");
            });
            ui.add_space(8.0);
            pixel_frame(PANEL).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("IMPRESORAS INSTALADAS").color(YELLOW).size(13.0));
                match &self.drv {
                    None => note(ui, "Leyendo..."),
                    Some(d) if d.printers.is_empty() => note(ui, "Ninguna."),
                    Some(d) => {
                        for (p, drv, def) in &d.printers {
                            ui.horizontal(|ui| {
                                let txt = if *def { format!("★ {p}") } else { format!("• {p}") };
                                ui.label(egui::RichText::new(txt).color(if *def { YELLOW } else { TEXT }).size(12.0))
                                    .on_hover_text(if *def { "Predeterminada" } else { "" });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if pixel_small(ui, "PRUEBA", GRAY, enabled).on_hover_text("Imprime la página de prueba de Windows").clicked() {
                                        act = Some(Action::TestPage(p.clone()));
                                    }
                                    if !def && pixel_small(ui, "PREDETERMINADA", BLUE, enabled).clicked() {
                                        act = Some(Action::SetDefault(p.clone()));
                                    }
                                });
                            });
                            note(ui, &format!("   {drv}"));
                        }
                    }
                }
                let ghosts = self.drv.as_ref().map(|d| ghost_printers(&d.printers)).unwrap_or_default();
                ui.horizontal_wrapped(|ui| {
                    if pixel_small(ui, "IMPRESORAS DE WINDOWS", BLUE, true).clicked() {
                        open("ms-settings:printers");
                    }
                    if !ghosts.is_empty()
                        && pixel_small(ui, &format!("QUITAR COPIAS ({})", ghosts.len()), ORANGE, enabled)
                            .on_hover_text(format!("Impresoras duplicadas que deja Windows al reconectar:\n{}", ghosts.join("\n")))
                            .clicked()
                    {
                        act = Some(Action::RemovePrinters(ghosts.clone()));
                    }
                });
                note(ui, "★ = predeterminada. \"Desactivar que Windows elija la predeterminada\" está en Impresoras de Windows.");
            });
            ui.add_space(8.0);
            pixel_frame(PANEL).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("COPIA DE SEGURIDAD").color(YELLOW).size(13.0));
                if pixel_small(ui, "GUARDAR MIS DRIVERS", GREEN, enabled).clicked() {
                    act = Some(Action::Backup);
                }
                note(ui, "Exporta los drivers instalados a Documentos\\KevWinOptimizer-drivers.\nÚtil antes de formatear: así no hay que buscarlos otra vez.");
            });
        });
        act
    }
}
