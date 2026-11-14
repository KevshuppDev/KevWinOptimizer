#![windows_subsystem = "windows"]

use eframe::egui::{
    self, epaint::Shadow, pos2, vec2, Align2, Color32, FontFamily, FontId, Painter, Pos2, Rect,
    Response, Rounding, Sense, Stroke, Ui,
};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const INTEL_SERVICES: [&str; 4] = [
    "DSAService",
    "ESRV_SVC_QUEENCREEK",
    "SystemUsageReportSvc_QUEENCREEK",
    "USER_ESRV_SVC_QUEENCREEK",
];

// =====================================================================
//  DATOS: limpieza, tweaks, reparaciones, apps
// =====================================================================

/// (nombre, descripción, activado por defecto)
const OPTS: &[(&str, &str, bool)] = &[
    ("Temporales del usuario", "%TEMP%", true),
    ("Temporales de Windows", "C:\\Windows\\Temp", true),
    ("Caché de Windows Update", "SoftwareDistribution\\Download", true),
    ("Optimización de entrega", "Caché P2P de Windows Update", true),
    ("Informes de errores", "Reportes WER y volcados", true),
    ("Miniaturas", "thumbcache (se regenera solo)", true),
    ("Caché de navegadores", "Chrome, Edge, Brave, Firefox (ciérralos)", false),
    ("Papelera de reciclaje", "Vacía la papelera", false),
    ("Caché DNS", "ipconfig /flushdns", true),
    ("Servicios de Intel", "Mantenerlos desactivados", true),
    ("TRIM del disco C:", "Solo SSD, mantiene la velocidad", true),
    ("Limpieza de componentes", "DISM, tarda varios minutos", false),
    ("Logs de Windows", "C:\\Windows\\Logs (CBS, DISM...)", true),
    ("Cachés de programación", "cargo, npm, pip, bun, uv, go", true),
    ("Instaladores de drivers", "C:\\NVIDIA, C:\\AMD, C:\\Intel", true),
];

/// Un cambio con script de aplicar y de deshacer (PowerShell).
/// Funciones disponibles en los scripts: SetReg ruta nombre valor [tipo], DelReg ruta nombre.
struct Tweak {
    name: &'static str,
    desc: &'static str,
    apply: &'static str,
    undo: &'static str,
    default: bool,
}

const PRIVACY: &[Tweak] = &[
    Tweak {
        name: "Telemetría",
        desc: "Datos de diagnóstico a Microsoft",
        apply: r"SetReg 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection' AllowTelemetry 0; foreach($s in 'DiagTrack','dmwappushservice'){ Stop-Service $s -Force -EA SilentlyContinue; Set-Service $s -StartupType Disabled -EA SilentlyContinue }",
        undo: r"DelReg 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection' AllowTelemetry; Set-Service DiagTrack -StartupType Automatic -EA SilentlyContinue; Set-Service dmwappushservice -StartupType Manual -EA SilentlyContinue; Start-Service DiagTrack -EA SilentlyContinue",
        default: true,
    },
    Tweak {
        name: "Historial de actividad",
        desc: "Línea de tiempo y actividades",
        apply: r"$p='HKLM:\SOFTWARE\Policies\Microsoft\Windows\System'; 'EnableActivityFeed','PublishUserActivities','UploadUserActivities' | % { SetReg $p $_ 0 }",
        undo: r"$p='HKLM:\SOFTWARE\Policies\Microsoft\Windows\System'; 'EnableActivityFeed','PublishUserActivities','UploadUserActivities' | % { DelReg $p $_ }",
        default: true,
    },
    Tweak {
        name: "Rastreo de ubicación",
        desc: "Las apps no podrán usar tu ubicación",
        apply: r"SetReg 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\location' Value 'Deny' String",
        undo: r"SetReg 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\location' Value 'Allow' String",
        default: false,
    },
    Tweak {
        name: "Copilot y Recall",
        desc: "Desactiva la IA integrada de Windows",
        apply: r"SetReg 'HKCU:\Software\Policies\Microsoft\Windows\WindowsCopilot' TurnOffWindowsCopilot 1; SetReg 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot' TurnOffWindowsCopilot 1; SetReg 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsAI' DisableAIDataAnalysis 1; SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' ShowCopilotButton 0",
        undo: r"DelReg 'HKCU:\Software\Policies\Microsoft\Windows\WindowsCopilot' TurnOffWindowsCopilot; DelReg 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot' TurnOffWindowsCopilot; DelReg 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\WindowsAI' DisableAIDataAnalysis; SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' ShowCopilotButton 1",
        default: true,
    },
    Tweak {
        name: "Bing en el menú Inicio",
        desc: "La búsqueda solo busca en tu PC",
        apply: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Search' BingSearchEnabled 0; SetReg 'HKCU:\Software\Policies\Microsoft\Windows\Explorer' DisableSearchBoxSuggestions 1",
        undo: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Search' BingSearchEnabled 1; DelReg 'HKCU:\Software\Policies\Microsoft\Windows\Explorer' DisableSearchBoxSuggestions",
        default: true,
    },
    Tweak {
        name: "Sugerencias y anuncios",
        desc: "Apps promocionadas y tips en Inicio",
        apply: r"$p='HKCU:\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager'; 'SilentInstalledAppsEnabled','SystemPaneSuggestionsEnabled','SoftLandingEnabled','SubscribedContent-338388Enabled','SubscribedContent-338389Enabled','SubscribedContent-353694Enabled' | % { SetReg $p $_ 0 }",
        undo: r"$p='HKCU:\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager'; 'SilentInstalledAppsEnabled','SystemPaneSuggestionsEnabled','SoftLandingEnabled','SubscribedContent-338388Enabled','SubscribedContent-338389Enabled','SubscribedContent-353694Enabled' | % { SetReg $p $_ 1 }",
        default: true,
    },
    Tweak {
        name: "ID de publicidad",
        desc: "Anuncios personalizados en apps",
        apply: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo' Enabled 0",
        undo: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo' Enabled 1",
        default: true,
    },
    Tweak {
        name: "Apps en segundo plano",
        desc: "Las apps de la Store no corren ocultas",
        apply: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications' GlobalUserDisabled 1",
        undo: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications' GlobalUserDisabled 0",
        default: false,
    },
];

const PERF: &[Tweak] = &[
    Tweak {
        name: "Plan Ultimate Performance",
        desc: "Máxima potencia (más consumo)",
        apply: r"$o = powercfg -duplicatescheme e9a42b02-d5df-448d-aa00-03f14749eb61; if ($o -match '([0-9a-fA-F-]{36})') { powercfg /setactive $matches[1] } else { throw 'No se pudo crear el plan' }",
        undo: r"powercfg /setactive 381b4222-f694-41f0-9685-ff5bb260df2e",
        default: false,
    },
    Tweak {
        name: "Desactivar hibernación",
        desc: "Libera GBs (hiberfil.sys), quita inicio rápido",
        apply: r"powercfg /hibernate off",
        undo: r"powercfg /hibernate on",
        default: false,
    },
    Tweak {
        name: "Desactivar Game DVR",
        desc: "Grabación de fondo de Xbox Game Bar",
        apply: r"SetReg 'HKCU:\System\GameConfigStore' GameDVR_Enabled 0; SetReg 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\GameDVR' AllowGameDVR 0",
        undo: r"SetReg 'HKCU:\System\GameConfigStore' GameDVR_Enabled 1; DelReg 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\GameDVR' AllowGameDVR",
        default: true,
    },
    Tweak {
        name: "Efectos visuales mínimos",
        desc: "Menos animaciones, menús instantáneos",
        apply: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects' VisualFXSetting 2; SetReg 'HKCU:\Control Panel\Desktop' MenuShowDelay '0' String",
        undo: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects' VisualFXSetting 0; SetReg 'HKCU:\Control Panel\Desktop' MenuShowDelay '400' String",
        default: false,
    },
    Tweak {
        name: "Mostrar extensiones",
        desc: "Ver .exe, .pdf, .txt en los archivos",
        apply: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' HideFileExt 0",
        undo: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' HideFileExt 1",
        default: true,
    },
    Tweak {
        name: "Mostrar archivos ocultos",
        desc: "Ver carpetas como AppData",
        apply: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' Hidden 1",
        undo: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' Hidden 2",
        default: false,
    },
    Tweak {
        name: "Modo oscuro",
        desc: "Windows y apps en tema oscuro",
        apply: r"$p='HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'; SetReg $p AppsUseLightTheme 0; SetReg $p SystemUsesLightTheme 0",
        undo: r"$p='HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'; SetReg $p AppsUseLightTheme 1; SetReg $p SystemUsesLightTheme 1",
        default: false,
    },
    Tweak {
        name: "Finalizar tarea en barra",
        desc: "Clic derecho > Finalizar tarea (Win11)",
        apply: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\TaskbarDeveloperSettings' TaskbarEndTask 1",
        undo: r"SetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\TaskbarDeveloperSettings' TaskbarEndTask 0",
        default: true,
    },
    Tweak {
        name: "Modo Juego",
        desc: "Windows prioriza el juego abierto",
        apply: r"SetReg 'HKCU:\Software\Microsoft\GameBar' AutoGameModeEnabled 1; SetReg 'HKCU:\Software\Microsoft\GameBar' AllowAutoGameMode 1",
        undo: r"SetReg 'HKCU:\Software\Microsoft\GameBar' AutoGameModeEnabled 0; SetReg 'HKCU:\Software\Microsoft\GameBar' AllowAutoGameMode 0",
        default: true,
    },
    Tweak {
        name: "GPU: programación acelerada",
        desc: "HAGS, menos latencia. Requiere reiniciar",
        apply: r"SetReg 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' HwSchMode 2",
        undo: r"SetReg 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' HwSchMode 1",
        default: false,
    },
    Tweak {
        name: "Sin aceleración del ratón",
        desc: "Movimiento 1:1, mejor puntería",
        apply: r"$p='HKCU:\Control Panel\Mouse'; SetReg $p MouseSpeed '0' String; SetReg $p MouseThreshold1 '0' String; SetReg $p MouseThreshold2 '0' String",
        undo: r"$p='HKCU:\Control Panel\Mouse'; SetReg $p MouseSpeed '1' String; SetReg $p MouseThreshold1 '6' String; SetReg $p MouseThreshold2 '10' String",
        default: false,
    },
    Tweak {
        name: "Menú contextual clásico",
        desc: "Win11: sin 'Mostrar más opciones'",
        apply: r"$k='HKCU:\Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32'; New-Item -Path $k -Force | Out-Null; Set-ItemProperty -Path $k -Name '(default)' -Value ''; Stop-Process -Name explorer -Force -EA SilentlyContinue",
        undo: r"Remove-Item 'HKCU:\Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}' -Recurse -Force -EA SilentlyContinue; Stop-Process -Name explorer -Force -EA SilentlyContinue",
        default: false,
    },
    Tweak {
        name: "Quitar Widgets",
        desc: "Noticias y clima de la barra de tareas",
        apply: r"SetReg 'HKLM:\SOFTWARE\Policies\Microsoft\Dsh' AllowNewsAndInterests 0",
        undo: r"DelReg 'HKLM:\SOFTWARE\Policies\Microsoft\Dsh' AllowNewsAndInterests",
        default: true,
    },
];

/// Comprobaciones (PowerShell, deben dar $true si el tweak ya está aplicado).
/// Mismo orden que PRIVACY / PERF.
const PRIVACY_CHECK: &[&str] = &[
    r"(GetReg 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection' AllowTelemetry) -eq 0",
    r"(GetReg 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\System' EnableActivityFeed) -eq 0",
    r"(GetReg 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\location' Value) -eq 'Deny'",
    r"(GetReg 'HKCU:\Software\Policies\Microsoft\Windows\WindowsCopilot' TurnOffWindowsCopilot) -eq 1",
    r"(GetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Search' BingSearchEnabled) -eq 0",
    r"$p='HKCU:\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager'; ((GetReg $p SystemPaneSuggestionsEnabled) -eq 0) -and ((GetReg $p SilentInstalledAppsEnabled) -eq 0)",
    r"(GetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo' Enabled) -eq 0",
    r"(GetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications' GlobalUserDisabled) -eq 1",
];
const PERF_CHECK: &[&str] = &[
    r"(powercfg /getactivescheme | Out-String) -match 'Ultimate|ximo rendimiento'",
    r"(GetReg 'HKLM:\SYSTEM\CurrentControlSet\Control\Power' HibernateEnabled) -eq 0",
    r"(GetReg 'HKCU:\System\GameConfigStore' GameDVR_Enabled) -eq 0",
    r"(GetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects' VisualFXSetting) -eq 2",
    r"(GetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' HideFileExt) -eq 0",
    r"(GetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' Hidden) -eq 1",
    r"(GetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' AppsUseLightTheme) -eq 0",
    r"(GetReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\TaskbarDeveloperSettings' TaskbarEndTask) -eq 1",
    r"(GetReg 'HKCU:\Software\Microsoft\GameBar' AutoGameModeEnabled) -ne 0", // sin valor = activo por defecto
    r"(GetReg 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' HwSchMode) -eq 2",
    r"(GetReg 'HKCU:\Control Panel\Mouse' MouseSpeed) -eq '0'",
    r"Test-Path 'HKCU:\Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32'",
    r"(GetReg 'HKLM:\SOFTWARE\Policies\Microsoft\Dsh' AllowNewsAndInterests) -eq 0",
];

// ---------- servicios ----------

struct SvcPack {
    name: &'static str,
    desc: &'static str,
    /// Nombres de servicio (admiten comodines de PowerShell).
    pats: &'static [&'static str],
    /// Recomendado detener (pasar a manual).
    rec: bool,
}

const SVC_PACKS: &[SvcPack] = &[
    SvcPack { name: "Innecesarios de Windows", desc: "Fax, mapas, demo de tienda, Insider, control parental, Wallet...", rec: true,
        pats: &["Fax", "MapsBroker", "RetailDemo", "WMPNetworkSvc", "PhoneSvc", "RemoteRegistry", "WpcMonSvc", "SEMgrSvc", "WalletService", "wisvc"] },
    SvcPack { name: "Actualizadores de apps", desc: "Google, Edge, Adobe, Firefox, Brave. Se siguen actualizando al abrirlas", rec: true,
        pats: &["gupdate*", "GoogleUpdater*", "edgeupdate*", "AdobeARMservice", "MozillaMaintenance", "brave*"] },
    SvcPack { name: "Intel (telemetría)", desc: "Driver & Support Assistant y reportes de uso", rec: true,
        pats: &["DSAService", "*QUEENCREEK*"] },
    SvcPack { name: "Xbox", desc: "Game Pass, logros y mandos Xbox. Detén si no los usas", rec: false,
        pats: &["Xbl*", "Xbox*"] },
    SvcPack { name: "Impresión", desc: "Cola de impresión. Detén si no tienes impresora", rec: false,
        pats: &["Spooler", "PrintNotify"] },
    SvcPack { name: "Oracle", desc: "Base de datos Oracle (XE) y su listener", rec: false,
        pats: &["Oracle*"] },
    SvcPack { name: "SQL Server", desc: "Motor, Agent, Browser y telemetría de SQL Server", rec: false,
        pats: &["MSSQL*", "SQLAgent*", "SQLBrowser", "SQLWriter", "SQLTELEMETRY*", "MSSQLFDLauncher*", "ReportServer*", "MSOLAP*", "SSIS*"] },
    SvcPack { name: "MySQL / MariaDB", desc: "Servidor MySQL o MariaDB (incluye el de XAMPP)", rec: false,
        pats: &["MySQL*", "MariaDB*"] },
    SvcPack { name: "PostgreSQL", desc: "Servidor PostgreSQL", rec: false, pats: &["postgresql*"] },
    SvcPack { name: "MongoDB", desc: "Servidor MongoDB", rec: false, pats: &["MongoDB"] },
    SvcPack { name: "Apache", desc: "Servidor web Apache (XAMPP)", rec: false, pats: &["Apache*"] },
    SvcPack { name: "Docker", desc: "Motor de Docker Desktop", rec: false, pats: &["com.docker*", "docker"] },
    SvcPack { name: "Máquinas virtuales", desc: "VirtualBox y VMware", rec: false,
        pats: &["VBox*", "VMware*", "VMAuthdService", "VMnet*", "VMUSBArbService"] },
    SvcPack { name: "VPNs", desc: "Cloudflare WARP, Radmin, Proton, OpenVPN, NordVPN, WireGuard", rec: false,
        pats: &["CloudflareWARP", "RvControlSvc", "ProtonVPN*", "OpenVPN*", "NordVPN*", "WireGuard*"] },
    SvcPack { name: "Launchers de juegos", desc: "Steam, EA y Epic", rec: false,
        pats: &["Steam Client Service", "EABackgroundService", "EpicOnlineServices"] },
];

#[derive(Clone)]
struct SvcInfo {
    pack: usize,
    name: String,
    display: String,
    running: bool,
    start: String, // Automatic, Manual, Disabled
}

#[derive(Clone, Copy)]
enum SvcAct {
    Start,
    Stop,
    Auto,
    Manual,
}

fn list_services() -> Vec<SvcInfo> {
    let script: Vec<String> = SVC_PACKS
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let names: Vec<String> = p.pats.iter().map(|n| format!("'{n}'")).collect();
            format!(
                "Get-Service -Name {} -EA SilentlyContinue | % {{ 'SV|{i}|{{0}}|{{1}}|{{2}}|{{3}}' -f $_.Name,[int]($_.Status -eq 'Running'),$_.StartType,$_.DisplayName }}",
                names.join(",")
            )
        })
        .collect();
    let (_, out) = ps(&script.join("\n"));
    let mut seen = std::collections::HashSet::new();
    out.iter()
        .filter_map(|l| {
            let mut it = l.strip_prefix("SV|")?.splitn(5, '|');
            let pack = it.next()?.parse().ok()?;
            let name = it.next()?.to_string();
            let running = it.next()? == "1";
            let start = it.next()?.to_string();
            let display = it.next().unwrap_or("").to_string();
            // un servicio solo en el primer pack que lo nombre
            seen.insert(name.to_lowercase()).then_some(SvcInfo { pack, name, display, running, start })
        })
        .collect()
}

fn svc_action(r: &Rep, list: &[SvcInfo], act: SvcAct) {
    let n = list.len() as u32;
    let mut good = 0;
    for (i, s) in list.iter().enumerate() {
        let q = s.name.replace('\'', "''");
        let (verb, script) = match act {
            SvcAct::Stop => ("Detener", format!("Stop-Service -Name '{q}' -Force -EA SilentlyContinue; Set-Service -Name '{q}' -StartupType Manual")),
            SvcAct::Start => ("Iniciar", format!("if((Get-Service -Name '{q}').StartType -eq 'Disabled'){{ Set-Service -Name '{q}' -StartupType Manual }}; Start-Service -Name '{q}'")),
            SvcAct::Auto => ("Con Windows", format!("Set-Service -Name '{q}' -StartupType Automatic")),
            SvcAct::Manual => ("Manual", format!("Set-Service -Name '{q}' -StartupType Manual")),
        };
        r.step(i as u32 + 1, n, &format!("{verb}: {}", s.display));
        let (ok, out) = ps(&script);
        if ok {
            good += 1;
            r.say(Kind::Ok, "      OK");
        } else {
            r.tail(Kind::Err, &out, 1);
        }
    }
    r.stat("SERVICIOS", format!("{good}/{n}"));
}

// ---------- programas de inicio ----------

#[derive(Clone)]
struct StartupItem {
    src: String, // HKCU, HKLM, HKLM32, UF (carpeta usuario), CF (carpeta común)
    name: String,
    cmd: String,
    enabled: bool,
}

/// Clave StartupApproved que usa el Administrador de tareas para cada origen.
fn approved_key(src: &str) -> &'static str {
    match src {
        "HKCU" => r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
        "HKLM" => r"HKLM:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
        "HKLM32" => r"HKLM:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32",
        "UF" => r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder",
        _ => r"HKLM:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder",
    }
}

// Solo comillas simples: las dobles se pierden al pasar el script por línea de comandos.
const STARTUP_LIST: &str = r"
function St($k,$n){ try { $b=(Get-ItemProperty -Path $k -Name $n -EA Stop).$n; if($b[0] % 2 -eq 1){'0'}else{'1'} } catch {'1'} }
$A='Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved'
$reg=@(@('HKCU','HKCU:\Software\Microsoft\Windows\CurrentVersion\Run',('HKCU:\'+$A+'\Run')),
       @('HKLM','HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run',('HKLM:\'+$A+'\Run')),
       @('HKLM32','HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run',('HKLM:\'+$A+'\Run32')))
foreach($s in $reg){
  $p=Get-ItemProperty -Path $s[1] -EA SilentlyContinue
  if($p){ $p.PSObject.Properties | ? { $_.Name -notlike 'PS*' } | % { 'SU|{0}|{1}|{2}|{3}' -f $s[0],$_.Name,(St $s[2] $_.Name),$_.Value } }
}
$fol=@(@('UF',[Environment]::GetFolderPath('Startup'),('HKCU:\'+$A+'\StartupFolder')),
       @('CF',[Environment]::GetFolderPath('CommonStartup'),('HKLM:\'+$A+'\StartupFolder')))
foreach($f in $fol){
  Get-ChildItem -Path $f[1] -File -EA SilentlyContinue | ? { $_.Name -ne 'desktop.ini' } | % { 'SU|{0}|{1}|{2}|{3}' -f $f[0],$_.Name,(St $f[2] $_.Name),$_.FullName }
}
";

fn list_startup() -> Vec<StartupItem> {
    let (_, out) = ps(STARTUP_LIST);
    out.iter()
        .filter_map(|l| {
            let mut it = l.strip_prefix("SU|")?.splitn(4, '|');
            Some(StartupItem {
                src: it.next()?.to_string(),
                name: it.next()?.to_string(),
                enabled: it.next()? == "1",
                cmd: it.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

fn apply_startup(r: &Rep, changes: &[(StartupItem, bool)]) {
    let n = changes.len() as u32;
    let mut good = 0;
    for (i, (item, on)) in changes.iter().enumerate() {
        r.step(i as u32 + 1, n, &format!("{} {}", if *on { "Activar" } else { "Quitar del inicio:" }, item.name));
        let script = format!(
            "$k='{}'; if(!(Test-Path $k)){{ New-Item -Path $k -Force | Out-Null }}; $b=[byte[]](0,0,0,0,0,0,0,0,0,0,0,0); $b[0]={}; Set-ItemProperty -Path $k -Name '{}' -Value $b -Type Binary -Force",
            approved_key(&item.src),
            if *on { 2 } else { 3 },
            item.name.replace('\'', "''"),
        );
        let (ok, out) = ps(&script);
        if ok {
            good += 1;
            r.say(Kind::Ok, "      OK");
        } else {
            r.tail(Kind::Err, &out, 2);
        }
    }
    r.say(Kind::Dim, "Se nota en el próximo inicio de Windows. No se borró nada.");
    r.stat("CAMBIADOS", format!("{good}/{n}"));
}
const _: () = assert!(PRIVACY.len() == PRIVACY_CHECK.len() && PERF.len() == PERF_CHECK.len());

/// Ejecuta todas las comprobaciones en un solo PowerShell.
fn detect(checks: &[&str]) -> Vec<Option<bool>> {
    let script: Vec<String> = checks
        .iter()
        .enumerate()
        // `& { }` aísla cada comprobación (pueden tener varias sentencias con ';')
        .map(|(i, c)| format!("try {{ if (& {{ {c} }}) {{ 'T{i}=1' }} else {{ 'T{i}=0' }} }} catch {{ 'T{i}=0' }}"))
        .collect();
    let (_, out) = ps(&script.join("\n"));
    // si una comprobación no respondió, se trata como "no aplicado" (no quedarse en "...")
    let mut res = vec![Some(false); checks.len()];
    for l in out {
        if let Some((i, v)) = l.strip_prefix('T').and_then(|s| s.split_once('=')) {
            if let Ok(i) = i.parse::<usize>() {
                if let Some(slot) = res.get_mut(i) {
                    *slot = Some(v == "1");
                }
            }
        }
    }
    res
}

/// Qué apps de la lista ya están instaladas (vía `winget export`, que da IDs exactos).
/// GPU(s) y CPU, para recomendar drivers.
fn detect_hw() -> String {
    let (_, out) = ps("(Get-CimInstance Win32_VideoController).Name; (Get-CimInstance Win32_Processor).Name");
    out.join(" · ")
}

fn detect_apps() -> Vec<Option<bool>> {
    let tmp = std::env::temp_dir().join("optimizador_winget_export.json");
    let tmp_s = tmp.display().to_string();
    let _ = run_status(
        "winget",
        &["export", "-o", &tmp_s, "--accept-source-agreements", "--disable-interactivity"],
    );
    let txt = std::fs::read_to_string(&tmp).unwrap_or_default().to_lowercase();
    let _ = std::fs::remove_file(&tmp);
    let dirs = search_dirs();
    // paquetes AppX (apps de la Microsoft Store)
    let (_, appx) = ps("Get-AppxPackage | % Name");
    // extensiones de VS Code instaladas
    let exts = code_cmd()
        .map(|c| run_output("cmd", &["/c", &c.display().to_string(), "--list-extensions"]))
        .unwrap_or_default()
        .iter()
        .map(|l| l.to_lowercase())
        .collect::<Vec<_>>();
    APPS.iter()
        .map(|a| {
            if let Some(ext) = a.2.strip_prefix("vscode:") {
                return Some(exts.contains(&ext.to_lowercase()));
            }
            if a.2.starts_with("web:") {
                return Some(false); // no se puede saber: es solo un enlace
            }
            // apps de la Store: su nombre de paquete AppX
            let appx_pat = match a.2 {
                "9NKSQGP7F2NH" => Some("*WhatsAppDesktop*"),
                "9NCBCSZSJRSB" => Some("SpotifyAB.SpotifyMusic"),
                _ => None,
            };
            if let Some(p) = appx_pat {
                if appx.iter().any(|n| glob_match(p, n)) {
                    return Some(true);
                }
            }
            if a.2 == "custom:oracle-xe11" {
                return Some(Path::new(r"C:\oraclexe\app\oracle\product\11.2.0\server").exists());
            }
            let by_winget = !txt.is_empty() && txt.contains(&format!("\"{}\"", a.2.to_lowercase()));
            // instalaciones portables / por script que winget no registra
            let by_exe = portable_exes(a.2)
                .iter()
                .any(|exe| dirs.iter().any(|d| d.join(exe).is_file()));
            Some(by_winget || by_exe)
        })
        .collect()
}

/// Ruta a `code.cmd` (CLI de VS Code), buscando también donde se instala
/// por defecto por si se instaló después de abrir la app.
fn code_cmd() -> Option<PathBuf> {
    let local = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default());
    let mut dirs = search_dirs();
    dirs.push(local.join(r"Programs\Microsoft VS Code\bin"));
    dirs.push(PathBuf::from(r"C:\Program Files\Microsoft VS Code\bin"));
    dirs.into_iter().map(|d| d.join("code.cmd")).find(|p| p.is_file())
}

const ORACLE_XE_SCRIPT: &str = r"
$u='https://drive.usercontent.google.com/download?id=1YgGp6t9T0Ti7tWtYQq-JLl-sxYEgldKx&export=download&confirm=t'
$z=Join-Path $env:TEMP 'OracleXE112_Win64.zip'
if(!(Test-Path $z) -or (Get-Item $z).Length -lt 300MB){ Invoke-WebRequest -Uri $u -OutFile $z -UseBasicParsing }
if((Get-Item $z).Length -lt 300MB){ throw 'La descarga no es valida (archivo incompleto)' }
$d=Join-Path $env:TEMP 'OracleXE112'
Expand-Archive -Path $z -DestinationPath $d -Force
$s=Get-ChildItem $d -Recurse -Filter setup.exe | Select-Object -First 1
if(!$s){ throw 'No se encontro setup.exe dentro del zip' }
Start-Process -FilePath $s.FullName -WorkingDirectory $s.DirectoryName -Wait
";

/// Instala lo que no es un paquete de winget. Devuelve (éxito, salida).
fn install_special(r: &Rep, id: &str) -> (bool, Vec<String>) {
    if let Some(ext) = id.strip_prefix("vscode:") {
        let Some(code) = code_cmd() else {
            return (false, vec!["VS Code no está instalado: instálalo primero.".into()]);
        };
        return run_status("cmd", &["/c", &code.display().to_string(), "--install-extension", ext, "--force"]);
    }
    match id {
        "custom:oracle-xe11" => {
            r.say(Kind::Dim, "      Descargando 317 MB de Google Drive (fuente no oficial)...");
            r.say(Kind::Dim, "      Luego se abre el instalador de Oracle: complétalo ahí.");
            let (ok, out) = ps(ORACLE_XE_SCRIPT);
            let installed = Path::new(r"C:\oraclexe\app\oracle\product\11.2.0\server").exists();
            if ok && !installed {
                return (false, vec!["El instalador se cerró sin terminar.".into()]);
            }
            (ok, out)
        }
        _ => (false, vec![format!("Instalador desconocido: {id}")]),
    }
}

/// Ejecutables que delatan una herramienta aunque no se haya instalado con winget.
fn portable_exes(id: &str) -> &'static [&'static str] {
    match id {
        "BrechtSanders.WinLibs.POSIX.UCRT" => &["gcc.exe"],
        "Rustlang.Rustup" => &["rustup.exe"],
        "Git.Git" => &["git.exe"],
        "GitHub.cli" => &["gh.exe"],
        "Microsoft.VisualStudioCode" => &["code.cmd"],
        "Anthropic.ClaudeCode" => &["claude.exe"],
        "OpenAI.Codex" => &["codex.exe", "codex.cmd"],
        "Google.AntigravityCLI" => &["antigravity.exe"],
        "Ollama.Ollama" => &["ollama.exe"],
        "OpenJS.NodeJS.LTS" => &["node.exe"],
        "Oven-sh.Bun" => &["bun.exe"],
        "astral-sh.uv" => &["uv.exe"],
        "GoLang.Go" => &["go.exe"],
        "Kitware.CMake" => &["cmake.exe"],
        "LLVM.LLVM" => &["clang.exe"],
        "Microsoft.PowerShell" => &["pwsh.exe"],
        "Neovim.Neovim" => &["nvim.exe"],
        "JanDeDobbeleer.OhMyPosh" => &["oh-my-posh.exe"],
        "ApacheFriends.Xampp.8.2" => &["xampp-control.exe"],
        _ => &[],
    }
}

/// Carpetas del PATH (usuario + sistema, leídas del registro por si la app
/// se abrió antes de instalar algo) y carpetas típicas de instalaciones portables.
fn search_dirs() -> Vec<PathBuf> {
    let mut paths = std::env::var("PATH").unwrap_or_default();
    for (_, out) in [
        ps("[Environment]::GetEnvironmentVariable('Path','User')"),
        ps("[Environment]::GetEnvironmentVariable('Path','Machine')"),
    ] {
        if let Some(l) = out.first() {
            paths.push(';');
            paths.push_str(l);
        }
    }
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&paths).collect();
    let home = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());
    for d in [r".cargo\bin", r".bun\bin", r".local\bin"] {
        dirs.push(home.join(d));
    }
    dirs.push(PathBuf::from(r"C:\mingw64\bin"));
    dirs.push(PathBuf::from(r"C:\xampp")); // XAMPP instalado a mano / con su instalador
    dirs
}

const RESTORE_POINT: &str = r"SetReg 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore' SystemRestorePointCreationFrequency 0; Enable-ComputerRestore -Drive ($env:SystemDrive + '\'); Checkpoint-Computer -Description 'Optimizador' -RestorePointType MODIFY_SETTINGS";

const REPAIRS: &[Tweak] = &[
    Tweak {
        name: "Punto de restauración",
        desc: "Copia de seguridad del sistema",
        apply: RESTORE_POINT,
        undo: "",
        default: true,
    },
    Tweak {
        name: "SFC /scannow",
        desc: "Repara archivos del sistema (~10 min)",
        apply: "sfc /scannow",
        undo: "",
        default: true,
    },
    Tweak {
        name: "DISM RestoreHealth",
        desc: "Repara la imagen de Windows (~15 min)",
        apply: "DISM /Online /Cleanup-Image /RestoreHealth",
        undo: "",
        default: true,
    },
    Tweak {
        name: "Reiniciar Windows Update",
        desc: "Si las actualizaciones fallan o se atascan",
        apply: r"$s='wuauserv','bits','cryptsvc','msiserver'; Stop-Service $s -Force -EA SilentlyContinue; $w=$env:WINDIR; Remove-Item (Join-Path $w 'SoftwareDistribution.old'),(Join-Path $w 'System32\catroot2.old') -Recurse -Force -EA SilentlyContinue; Rename-Item (Join-Path $w 'SoftwareDistribution') 'SoftwareDistribution.old' -EA SilentlyContinue; Rename-Item (Join-Path $w 'System32\catroot2') 'catroot2.old' -EA SilentlyContinue; Start-Service $s -EA SilentlyContinue",
        undo: "",
        default: false,
    },
    Tweak {
        name: "Comprobar disco C:",
        desc: "chkdsk /scan, sin reiniciar",
        apply: "chkdsk C: /scan",
        undo: "",
        default: false,
    },
];

/// (categoría, nombre, id de winget, descripción para el tooltip)
/// Todos los IDs fueron verificados con `winget show`.
type AppPkg = (&'static str, &'static str, &'static str, &'static str);
const APPS: &[AppPkg] = &[
    // --- IA ---
    ("IA Y AGENTES", "Claude", "Anthropic.Claude", "App de escritorio de Claude (Anthropic): chat, proyectos y archivos."),
    ("IA Y AGENTES", "Claude Code", "Anthropic.ClaudeCode", "Agente de programación de Anthropic para la terminal."),
    ("IA Y AGENTES", "Antigravity IDE", "Google.AntigravityIDE", "Editor de código con agentes de IA de Google."),
    ("IA Y AGENTES", "Antigravity CLI", "Google.AntigravityCLI", "Versión de terminal de Antigravity."),
    ("IA Y AGENTES", "Codex CLI", "OpenAI.Codex", "Agente de programación de OpenAI para la terminal."),
    ("IA Y AGENTES", "Ollama", "Ollama.Ollama", "Ejecuta modelos de IA locales (Llama, Qwen, Gemma...) sin internet."),
    ("IA Y AGENTES", "LM Studio", "ElementLabs.LMStudio", "Interfaz gráfica para descargar y chatear con modelos locales."),
    // --- Programación: base ---
    ("PROGRAMACIÓN · BASE", "Git", "Git.Git", "Control de versiones. Imprescindible para casi cualquier proyecto."),
    ("PROGRAMACIÓN · BASE", "GitHub CLI", "GitHub.cli", "Comando 'gh': PRs, issues y repos de GitHub desde la terminal."),
    ("PROGRAMACIÓN · BASE", "GitHub Desktop", "GitHub.GitHubDesktop", "Git con interfaz gráfica, ideal para empezar."),
    ("PROGRAMACIÓN · BASE", "VS Code", "Microsoft.VisualStudioCode", "Editor de código más usado, con miles de extensiones."),
    ("PROGRAMACIÓN · BASE", "Cursor", "Anysphere.Cursor", "Editor basado en VS Code con IA integrada."),
    ("PROGRAMACIÓN · BASE", "Zed", "ZedIndustries.Zed", "Editor ultrarrápido escrito en Rust."),
    ("PROGRAMACIÓN · BASE", "Windows Terminal", "Microsoft.WindowsTerminal", "Terminal moderna con pestañas para CMD, PowerShell y WSL."),
    ("PROGRAMACIÓN · BASE", "PowerShell 7", "Microsoft.PowerShell", "Versión nueva y multiplataforma de PowerShell."),
    ("PROGRAMACIÓN · BASE", "WSL", "Microsoft.WSL", "Linux dentro de Windows (Ubuntu, Debian...)."),
    ("PROGRAMACIÓN · BASE", "Neovim", "Neovim.Neovim", "Editor de terminal basado en Vim."),
    ("PROGRAMACIÓN · BASE", "Notepad++", "Notepad++.Notepad++", "Bloc de notas avanzado y liviano."),
    ("PROGRAMACIÓN · BASE", "Oh My Posh", "JanDeDobbeleer.OhMyPosh", "Prompt bonito y con info de git para la terminal."),
    // --- Programación: lenguajes ---
    ("PROGRAMACIÓN · LENGUAJES", "Rustup (Rust)", "Rustlang.Rustup", "Instalador oficial de Rust: rustc, cargo y toolchains."),
    ("PROGRAMACIÓN · LENGUAJES", "VS Build Tools (C++)", "Microsoft.VisualStudio.2022.BuildTools", "Compilador MSVC. Lo necesita Rust (toolchain msvc), Node nativo y Python. Se instala con la carga de C++."),
    ("PROGRAMACIÓN · LENGUAJES", "WinLibs MinGW", "BrechtSanders.WinLibs.POSIX.UCRT", "GCC de 64 bits: alternativa a MSVC para Rust (toolchain gnu) y C/C++."),
    ("PROGRAMACIÓN · LENGUAJES", "Node.js LTS", "OpenJS.NodeJS.LTS", "JavaScript fuera del navegador + npm."),
    ("PROGRAMACIÓN · LENGUAJES", "Bun", "Oven-sh.Bun", "Runtime de JavaScript/TypeScript muy rápido, con gestor de paquetes."),
    ("PROGRAMACIÓN · LENGUAJES", "Python 3.13", "Python.Python.3.13", "Lenguaje Python con pip."),
    ("PROGRAMACIÓN · LENGUAJES", "uv", "astral-sh.uv", "Gestor de paquetes y entornos de Python ultrarrápido (hecho en Rust)."),
    ("PROGRAMACIÓN · LENGUAJES", "Go", "GoLang.Go", "Lenguaje Go de Google."),
    ("PROGRAMACIÓN · LENGUAJES", "Java 21 (Temurin)", "EclipseAdoptium.Temurin.21.JDK", "JDK de Java gratuito y de código abierto."),
    ("PROGRAMACIÓN · LENGUAJES", ".NET 9 SDK", "Microsoft.DotNet.SDK.9", "Para programar en C# y F#."),
    ("PROGRAMACIÓN · LENGUAJES", "CMake", "Kitware.CMake", "Sistema de compilación para proyectos C/C++."),
    ("PROGRAMACIÓN · LENGUAJES", "LLVM / Clang", "LLVM.LLVM", "Compilador Clang y herramientas LLVM."),
    // --- Programación: herramientas ---
    ("PROGRAMACIÓN · HERRAMIENTAS", "Docker Desktop", "Docker.DockerDesktop", "Contenedores. Requiere WSL o Hyper-V."),
    ("PROGRAMACIÓN · HERRAMIENTAS", "JetBrains Toolbox", "JetBrains.Toolbox", "Instala y actualiza IDEs de JetBrains (IntelliJ, RustRover, PyCharm...)."),
    ("PROGRAMACIÓN · HERRAMIENTAS", "XAMPP 8.2", "ApacheFriends.Xampp.8.2", "Apache + MariaDB (MySQL) + PHP + phpMyAdmin en un solo paquete, para desarrollo web local. Se instala en C:\\xampp."),
    ("PROGRAMACIÓN · HERRAMIENTAS", "Postman", "Postman.Postman", "Probar APIs HTTP."),
    ("PROGRAMACIÓN · HERRAMIENTAS", "Insomnia", "Insomnia.Insomnia", "Cliente de APIs REST/GraphQL, alternativa a Postman."),
    ("PROGRAMACIÓN · HERRAMIENTAS", "DB Browser SQLite", "DBBrowserForSQLite.DBBrowserForSQLite", "Ver y editar bases de datos SQLite."),
    ("PROGRAMACIÓN · HERRAMIENTAS", "WinSCP", "WinSCP.WinSCP", "Copiar archivos a servidores por SFTP/SCP."),
    ("PROGRAMACIÓN · HERRAMIENTAS", "PuTTY", "PuTTY.PuTTY", "Cliente SSH clásico."),
    ("PROGRAMACIÓN · HERRAMIENTAS", "VS Code Insiders", "Microsoft.VisualStudioCode.Insiders", "Versión preliminar de VS Code con lo último."),
    // --- Bases de datos ---
    // Prefijos especiales en el id: "vscode:" = extensión de VS Code, "custom:" = instalador propio.
    ("BASES DE DATOS", "Oracle XE 11g", "custom:oracle-xe11", "Oracle Database Express Edition 11g R2 (64 bits). Se descarga (317 MB) y abre su instalador para que lo completes (pide la contraseña de SYSTEM). Fuente: copia en Google Drive, no oficial de Oracle."),
    ("BASES DE DATOS", "SQL Developer", "Oracle.SQLDeveloper", "IDE oficial de Oracle para bases de datos Oracle (versión de escritorio, independiente)."),
    ("BASES DE DATOS", "SQL Developer (VS Code)", "vscode:Oracle.sql-developer", "Extensión oficial de Oracle SQL Developer para VS Code. Necesita VS Code instalado."),
    ("BASES DE DATOS", "SQLcl", "Oracle.SQLcl", "Línea de comandos moderna de Oracle (reemplazo de SQL*Plus)."),
    ("BASES DE DATOS", "MySQL Workbench", "Oracle.MySQLWorkbench", "Diseño y administración de bases MySQL/MariaDB."),
    // --- Navegadores ---
    ("NAVEGADORES", "Google Chrome", "Google.Chrome", "Navegador de Google."),
    ("NAVEGADORES", "Firefox", "Mozilla.Firefox", "Navegador libre de Mozilla, enfocado en privacidad."),
    ("NAVEGADORES", "Brave", "Brave.Brave", "Navegador con bloqueador de anuncios integrado."),
    ("NAVEGADORES", "Opera GX", "Opera.OperaGX", "Navegador para gamers, con límite de RAM/CPU."),
    // --- Comunicación ---
    ("COMUNICACIÓN", "WhatsApp", "9NKSQGP7F2NH", "WhatsApp oficial para Windows (desde Microsoft Store)."),
    ("COMUNICACIÓN", "Discord", "Discord.Discord", "Chat de voz y texto para comunidades."),
    ("COMUNICACIÓN", "Telegram", "Telegram.TelegramDesktop", "Mensajería rápida con sincronización en la nube."),
    ("COMUNICACIÓN", "Zoom", "Zoom.Zoom", "Videollamadas y reuniones."),
    ("COMUNICACIÓN", "Microsoft Teams", "Microsoft.Teams", "Chat y reuniones de Microsoft."),
    ("COMUNICACIÓN", "Thunderbird", "Mozilla.Thunderbird", "Cliente de correo de Mozilla."),
    ("COMUNICACIÓN", "Mailspring", "Foundry376.Mailspring", "Cliente de correo moderno: varias cuentas (Gmail, Outlook...) en una sola bandeja."),
    // --- Multimedia ---
    ("MULTIMEDIA", "Spotify", "9NCBCSZSJRSB", "Música y podcasts. Versión de Microsoft Store (la de winget no se instala como administrador)."),
    ("MULTIMEDIA", "VLC", "VideoLAN.VLC", "Reproduce cualquier formato de video o audio."),
    ("MULTIMEDIA", "OBS Studio", "OBSProject.OBSStudio", "Grabar pantalla y hacer streams."),
    ("MULTIMEDIA", "ShareX", "ShareX.ShareX", "Capturas de pantalla y GIFs con muchas opciones."),
    ("MULTIMEDIA", "HandBrake", "HandBrake.HandBrake", "Convertir y comprimir videos."),
    ("MULTIMEDIA", "Audacity", "Audacity.Audacity", "Editar audio."),
    ("MULTIMEDIA", "GIMP 3", "GIMP.GIMP.3", "Editor de imágenes gratuito (alternativa a Photoshop)."),
    ("MULTIMEDIA", "Krita", "KDE.Krita", "Dibujo y pintura digital."),
    ("MULTIMEDIA", "Blender", "BlenderFoundation.Blender", "Modelado y animación 3D."),
    // --- Oficina ---
    ("OFICINA", "Microsoft 365", "Microsoft.Office", "Office 365: Word, Excel, PowerPoint, Outlook, OneNote. Necesita una suscripción o cuenta de estudiante/empresa para activarse. Pesado (~3 GB)."),
    ("OFICINA", "LibreOffice", "TheDocumentFoundation.LibreOffice", "Suite de oficina gratuita (Word, Excel, PowerPoint)."),
    ("OFICINA", "Obsidian", "Obsidian.Obsidian", "Notas en Markdown enlazadas entre sí."),
    ("OFICINA", "Notion", "Notion.Notion", "Notas, tareas y wikis en la nube."),
    ("OFICINA", "SumatraPDF", "SumatraPDF.SumatraPDF", "Lector de PDF muy liviano."),
    // --- Utilidades ---
    ("UTILIDADES", "7-Zip", "7zip.7zip", "Comprimir y descomprimir ZIP, 7z, RAR..."),
    ("UTILIDADES", "Everything", "voidtools.Everything", "Busca cualquier archivo del disco al instante."),
    ("UTILIDADES", "PowerToys", "Microsoft.PowerToys", "Utilidades extra de Microsoft: FancyZones, PowerRename, selector de color..."),
    ("UTILIDADES", "Revo Uninstaller", "RevoUninstaller.RevoUninstaller", "Desinstala programas y borra sus restos."),
    ("UTILIDADES", "WinDirStat", "WinDirStat.WinDirStat", "Ver qué carpetas ocupan más espacio."),
    ("UTILIDADES", "Rufus", "Rufus.Rufus", "Crear USBs booteables de Windows o Linux."),
    ("UTILIDADES", "qBittorrent", "qBittorrent.qBittorrent", "Cliente de torrents sin publicidad."),
    ("UTILIDADES", "Bitwarden", "Bitwarden.Bitwarden", "Gestor de contraseñas gratuito y de código abierto."),
    ("UTILIDADES", "AnyDesk", "AnyDesk.AnyDesk", "Control remoto de otra PC."),
    ("UTILIDADES", "Malwarebytes", "Malwarebytes.Malwarebytes", "Escáner antimalware complementario a Defender."),
    ("UTILIDADES", "Proton VPN", "Proton.ProtonVPN", "VPN con plan gratuito sin límite de datos."),
    ("UTILIDADES", "1.1.1.1 (WARP)", "Cloudflare.Warp", "DNS rápido y privado de Cloudflare, con túnel WARP opcional."),
    ("UTILIDADES", "Radmin VPN", "Famatech.RadminVPN", "Red LAN virtual para jugar en 'red local' por internet con amigos."),
    ("UTILIDADES", "Parsec", "Parsec.Parsec", "Escritorio remoto de baja latencia: jugar o usar otra PC a distancia, co-op en local por internet."),
    ("UTILIDADES", "Mem Reduct", "Henry++.MemReduct", "Libera RAM en un clic y muestra el uso de memoria en la bandeja."),
    ("UTILIDADES", "SuperF4", "StefanSundin.Superf4", "Ctrl+Alt+F4 mata al instante la ventana colgada (juegos que no responden)."),
    // --- Juegos ---
    ("JUEGOS", "Steam", "Valve.Steam", "Tienda y biblioteca de juegos de Valve."),
    ("JUEGOS", "Epic Games", "EpicGames.EpicGamesLauncher", "Tienda de Epic, con juegos gratis cada semana."),
    ("JUEGOS", "EA App", "ElectronicArts.EADesktop", "Launcher de Electronic Arts."),
    ("JUEGOS", "Ubisoft Connect", "Ubisoft.Connect", "Launcher de Ubisoft."),
    ("JUEGOS", "GOG Galaxy", "GOG.Galaxy", "Juegos sin DRM y biblioteca unificada."),
    ("JUEGOS", "GeForce NOW", "Nvidia.GeForceNow", "Juegos en la nube de NVIDIA: juega títulos exigentes en cualquier PC."),
    ("JUEGOS", "RetroArch", "Libretro.RetroArch", "Emulador todo-en-uno de consolas retro (NES, SNES, PS1, GBA...)."),
    // --- Runtimes ---
    ("RUNTIMES PARA JUEGOS", "DirectX (June 2010)", "Microsoft.DirectX", "Librerías DirectX 9/10/11 antiguas (d3dx9, xinput1_3...). Muchos juegos fallan sin ellas."),
    ("RUNTIMES PARA JUEGOS", "VC++ 2015-2022 x64", "Microsoft.VCRedist.2015+.x64", "Visual C++ moderno (64 bits). El más necesario de todos."),
    ("RUNTIMES PARA JUEGOS", "VC++ 2015-2022 x86", "Microsoft.VCRedist.2015+.x86", "Visual C++ moderno (32 bits). Muchos juegos y launchers lo piden."),
    ("RUNTIMES PARA JUEGOS", "VC++ 2013 x64", "Microsoft.VCRedist.2013.x64", "Visual C++ 2013 (64 bits)."),
    ("RUNTIMES PARA JUEGOS", "VC++ 2013 x86", "Microsoft.VCRedist.2013.x86", "Visual C++ 2013 (32 bits)."),
    ("RUNTIMES PARA JUEGOS", "VC++ 2012 x64", "Microsoft.VCRedist.2012.x64", "Visual C++ 2012 (64 bits)."),
    ("RUNTIMES PARA JUEGOS", "VC++ 2012 x86", "Microsoft.VCRedist.2012.x86", "Visual C++ 2012 (32 bits)."),
    ("RUNTIMES PARA JUEGOS", "VC++ 2010 x64", "Microsoft.VCRedist.2010.x64", "Visual C++ 2010 (64 bits)."),
    ("RUNTIMES PARA JUEGOS", "VC++ 2010 x86", "Microsoft.VCRedist.2010.x86", "Visual C++ 2010 (32 bits)."),
    ("RUNTIMES PARA JUEGOS", "VC++ 2008 x64", "Microsoft.VCRedist.2008.x64", "Visual C++ 2008 (64 bits). Juegos antiguos."),
    ("RUNTIMES PARA JUEGOS", "VC++ 2008 x86", "Microsoft.VCRedist.2008.x86", "Visual C++ 2008 (32 bits). Juegos antiguos."),
    ("RUNTIMES PARA JUEGOS", "VC++ 2005 x64", "Microsoft.VCRedist.2005.x64", "Visual C++ 2005 (64 bits). Juegos muy antiguos."),
    ("RUNTIMES PARA JUEGOS", "VC++ 2005 x86", "Microsoft.VCRedist.2005.x86", "Visual C++ 2005 (32 bits). Juegos muy antiguos."),
    ("RUNTIMES PARA JUEGOS", ".NET Desktop 9", "Microsoft.DotNet.DesktopRuntime.9", "Runtime de .NET 9 para apps y launchers de escritorio."),
    ("RUNTIMES PARA JUEGOS", ".NET Desktop 8", "Microsoft.DotNet.DesktopRuntime.8", "Runtime de .NET 8 (LTS). Lo usan muchos mods y launchers."),
    ("RUNTIMES PARA JUEGOS", ".NET Desktop 6", "Microsoft.DotNet.DesktopRuntime.6", "Runtime de .NET 6. Algunas herramientas de mods aún lo piden."),
    ("RUNTIMES PARA JUEGOS", ".NET Framework 4.8", "Microsoft.DotNet.Framework.DeveloperPack_4", ".NET Framework 4.8 completo, para juegos y apps antiguas."),
    ("RUNTIMES PARA JUEGOS", "XNA Framework 4.0", "Microsoft.XNARedist", "Lo piden juegos indie como Terraria, Stardew Valley (versión vieja), Fez..."),
    ("RUNTIMES PARA JUEGOS", "NVIDIA PhysX", "Nvidia.PhysX", "Física de NVIDIA para juegos (Borderlands, Batman Arkham, Mafia II...)."),
    ("RUNTIMES PARA JUEGOS", "Java 21 (JRE)", "EclipseAdoptium.Temurin.21.JRE", "Java para juegos y servidores (Minecraft Java, mods, servidores)."),
    ("RUNTIMES PARA JUEGOS", "WebView2", "Microsoft.EdgeWebView2Runtime", "Motor web que usan muchos launchers modernos."),
    // --- Drivers ---
    // id sin punto = Microsoft Store; "web:" = abre la web oficial.
    ("DRIVERS", "NVIDIA App", "XP8CLZL93F5Z4P", "App oficial de NVIDIA: drivers Game Ready/Studio, grabación y ajustes de GPU. (Desde Microsoft Store)"),
    ("DRIVERS", "Intel DSA", "Intel.IntelDriverAndSupportAssistant", "Driver & Support Assistant de Intel: detecta y actualiza drivers de gráficos Intel, WiFi y Bluetooth. Ojo: el pack 'Intel (telemetría)' de Servicios y la limpieza 'Servicios de Intel' lo desactivan."),
    ("DRIVERS", "AMD Software", "web:https://www.amd.com/es/support/download/drivers.html", "AMD no publica su instalador en winget: abre la web oficial con el detector automático de drivers Radeon/Ryzen."),
    ("DRIVERS", "NVIDIA CUDA", "Nvidia.CUDA", "Toolkit de CUDA para IA y cómputo en GPU NVIDIA (pesado, ~3 GB)."),
    ("DRIVERS", "Logitech G HUB", "Logitech.GHUB", "Software y drivers para mouse/teclados/auriculares Logitech G."),
    ("DRIVERS", "Corsair iCUE", "Corsair.iCUE.5", "Software para periféricos y RGB de Corsair."),
    ("DRIVERS", "SteelSeries GG", "SteelSeries.GG", "Software para periféricos SteelSeries."),
    // --- Hardware ---
    ("HARDWARE", "CPU-Z", "CPUID.CPU-Z", "Info detallada de procesador, RAM y placa."),
    ("HARDWARE", "GPU-Z", "TechPowerUp.GPU-Z", "Info y sensores de la tarjeta gráfica."),
    ("HARDWARE", "CrystalDiskInfo", "CrystalDewWorld.CrystalDiskInfo", "Salud y temperatura de los discos."),
    ("HARDWARE", "CrystalDiskMark", "CrystalDewWorld.CrystalDiskMark", "Mide la velocidad de los discos."),
    ("HARDWARE", "MSI Afterburner", "Guru3D.Afterburner", "Monitoreo en juegos y overclock de GPU."),
];

// ---------- bloatware (apps preinstaladas) ----------

/// (nombre, patrón de paquete AppX con comodines, descripción, recomendado quitar)
const BLOAT: &[(&str, &str, &str, bool)] = &[
    ("Noticias", "Microsoft.BingNews", "App de noticias de MSN.", true),
    ("El Tiempo", "Microsoft.BingWeather", "App del clima de MSN.", true),
    ("Obtener ayuda", "Microsoft.GetHelp", "Chat de soporte de Microsoft.", true),
    ("Sugerencias", "Microsoft.Getstarted", "App de consejos de Windows.", true),
    ("Solitaire Collection", "Microsoft.MicrosoftSolitaireCollection", "Juegos de cartas con anuncios.", true),
    ("Microsoft 365 (hub)", "Microsoft.MicrosoftOfficeHub", "Acceso a la web de Office. NO es Office; no afecta a Word/Excel instalados.", true),
    ("Contactos", "Microsoft.People", "Agenda de contactos.", true),
    ("Power Automate", "Microsoft.PowerAutomateDesktop", "Automatizaciones para empresas.", true),
    ("Centro de opiniones", "Microsoft.WindowsFeedbackHub", "Enviar comentarios a Microsoft.", true),
    ("Mapas", "Microsoft.WindowsMaps", "Mapas de Bing.", true),
    ("Películas y TV", "Microsoft.ZuneVideo", "Reproductor y tienda de videos.", true),
    ("Cortana", "Microsoft.549981C3F5F10", "Asistente descontinuado.", true),
    ("Clipchamp", "Clipchamp.Clipchamp", "Editor de video web.", true),
    ("Dev Home", "Microsoft.Windows.DevHome", "Panel para desarrolladores (descontinuado).", true),
    ("Mixed Reality Portal", "Microsoft.MixedReality.Portal", "Para visores de realidad mixta.", true),
    ("Skype", "Microsoft.SkypeApp", "Descontinuado por Microsoft.", true),
    ("Family Safety", "MicrosoftCorporationII.MicrosoftFamily", "Control parental.", true),
    ("Wallet", "Microsoft.Wallet", "Monedero de Microsoft.", true),
    ("Candy Crush y similares", "*CandyCrush*", "Juegos promocionados preinstalados.", true),
    ("Disney+", "*Disney*", "App promocionada preinstalada.", true),
    ("Facebook", "*Facebook*", "App promocionada preinstalada.", true),
    ("Instagram", "*Instagram*", "App promocionada preinstalada.", true),
    ("TikTok", "*TikTok*", "App promocionada preinstalada.", true),
    ("Prime Video", "AmazonVideo.PrimeVideo", "App promocionada preinstalada.", true),
    ("LinkedIn", "*LinkedIn*", "App promocionada preinstalada.", true),
    ("To Do", "Microsoft.Todos", "Lista de tareas. Quítala solo si no la usas.", false),
    ("Reproductor multimedia", "Microsoft.ZuneMusic", "Reproductor de música/video de Windows. Útil si no usas VLC.", false),
    ("Vínculo móvil", "Microsoft.YourPhone", "Conecta tu Android/iPhone con la PC.", false),
    ("Teams (personal)", "MSTeams", "Teams para uso personal. No afecta al Teams de trabajo instalado con winget.", false),
    ("Outlook (nuevo)", "Microsoft.OutlookForWindows", "Nuevo Outlook web. Quítalo si usas Mailspring/Thunderbird.", false),
    ("Asistencia rápida", "MicrosoftCorporationII.QuickAssist", "Soporte remoto entre PCs con Windows.", false),
    ("Grabadora de voz", "Microsoft.WindowsSoundRecorder", "Grabar audio.", false),
    ("App de Xbox", "Microsoft.GamingApp", "Game Pass y tienda Xbox. Quítala si no usas Game Pass.", false),
    ("Copilot", "Microsoft.Copilot", "App de Copilot (IA de Microsoft).", false),
];

/// Comodín simple: '*' = cualquier texto. Sin distinguir mayúsculas.
fn glob_match(pat: &str, s: &str) -> bool {
    let (pat, s) = (pat.to_lowercase(), s.to_lowercase());
    let parts: Vec<&str> = pat.split('*').collect();
    if parts.len() == 1 {
        return pat == s;
    }
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match s[pos..].find(part) {
            Some(idx) if i > 0 || idx == 0 => pos += idx + part.len(),
            _ => return false,
        }
    }
    parts.last().is_some_and(|l| l.is_empty()) || pos == s.len()
}

/// Qué bloatware está instalado (para cualquier usuario).
fn detect_bloat() -> Vec<bool> {
    // solo lo realmente instalado para algún usuario (no lo "Staged" que queda reservado)
    let (_, names) = ps("Get-AppxPackage -AllUsers | ? { $_.PackageUserInformation.InstallState -contains 'Installed' } | % Name");
    BLOAT.iter().map(|b| names.iter().any(|n| glob_match(b.1, n))).collect()
}

fn remove_bloat(r: &Rep, list: &[&'static (&'static str, &'static str, &'static str, bool)]) {
    let n = list.len() as u32;
    let mut good = 0;
    for (i, (name, pat, _, _)) in list.iter().enumerate() {
        r.step(i as u32 + 1, n, &format!("Quitar {name}"));
        let q = pat.replace('\'', "''");
        // Orden importante: -AllUsers suele fallar (0x80070002) si antes no se quita
        // para el usuario actual; y si no se quita lo "provisionado" queda en estado Staged.
        let script = format!(
            "$n='{q}'; \
             Get-AppxPackage -Name $n | % {{ try {{ Remove-AppxPackage -Package $_.PackageFullName -EA Stop }} catch {{}} }}; \
             Get-AppxPackage -AllUsers -Name $n | % {{ try {{ Remove-AppxPackage -Package $_.PackageFullName -AllUsers -EA Stop }} catch {{}} }}; \
             Get-AppxProvisionedPackage -Online | ? {{ $_.DisplayName -like $n }} | % {{ try {{ Remove-AppxProvisionedPackage -Online -PackageName $_.PackageName -EA Stop | Out-Null }} catch {{}} }}; \
             if (Get-AppxPackage -Name $n) {{ throw 'Windows no dejo quitarla (app protegida del sistema)' }}"
        );
        let (ok, out) = ps(&script);
        if ok {
            good += 1;
            r.say(Kind::Ok, "      Quitada");
        } else {
            r.tail(Kind::Err, &out, 1);
        }
    }
    r.say(Kind::Dim, "Se pueden reinstalar cuando quieras desde Microsoft Store.");
    r.stat("QUITADAS", format!("{good}/{n}"));
}

/// ¿Esta app de drivers corresponde al hardware detectado? (texto con GPU y CPU)
fn matches_hw(id: &str, hw: &str) -> bool {
    let hw = hw.to_lowercase();
    match id {
        "XP8CLZL93F5Z4P" | "Nvidia.PhysX" => hw.contains("nvidia"),
        "Intel.IntelDriverAndSupportAssistant" => hw.contains("intel"),
        _ if id.contains("amd.com") => hw.contains("amd") || hw.contains("radeon"),
        _ => false,
    }
}

/// Packs rápidos: (nombre, ids que marca)
const APP_PACKS: &[(&str, &[&str])] = &[
    ("Pack Personal", &["9NKSQGP7F2NH", "9NCBCSZSJRSB", "Nvidia.GeForceNow", "Parsec.Parsec", "Famatech.RadminVPN", "Cloudflare.Warp", "Henry++.MemReduct", "StefanSundin.Superf4", "Libretro.RetroArch", "Microsoft.Office"]),
    ("Pack Rust", &["Rustlang.Rustup", "Microsoft.VisualStudio.2022.BuildTools", "Git.Git", "Microsoft.VisualStudioCode"]),
    ("Pack Web", &["Git.Git", "Microsoft.VisualStudioCode", "OpenJS.NodeJS.LTS", "Oven-sh.Bun", "Postman.Postman"]),
    ("Pack IA", &["Anthropic.Claude", "Anthropic.ClaudeCode", "Google.AntigravityIDE", "Git.Git", "OpenJS.NodeJS.LTS"]),
    ("Pack Oracle", &["custom:oracle-xe11", "Oracle.SQLDeveloper", "Microsoft.VisualStudioCode", "vscode:Oracle.sql-developer"]),
    ("Pack Gamer", &["Valve.Steam", "EpicGames.EpicGamesLauncher", "Discord.Discord", "Microsoft.DirectX", "Microsoft.VCRedist.2015+.x64", "Microsoft.VCRedist.2015+.x86", "Microsoft.XNARedist", "Microsoft.DotNet.DesktopRuntime.8"]),
];

// =====================================================================
//  PALETA Y SPRITES
// =====================================================================

const BG: Color32 = Color32::from_rgb(0x1a, 0x1c, 0x2c);
const PANEL: Color32 = Color32::from_rgb(0x33, 0x3c, 0x57);
const DARK: Color32 = Color32::from_rgb(0x0f, 0x0f, 0x1b);
const TEXT: Color32 = Color32::from_rgb(0xf4, 0xf4, 0xf4);
const GRAY: Color32 = Color32::from_rgb(0x94, 0xb0, 0xc2);
const DIMGRAY: Color32 = Color32::from_rgb(0x56, 0x6c, 0x86);
const GREEN: Color32 = Color32::from_rgb(0x38, 0xb7, 0x64);
const LIME: Color32 = Color32::from_rgb(0xa7, 0xf0, 0x70);
const YELLOW: Color32 = Color32::from_rgb(0xff, 0xcd, 0x75);
const ORANGE: Color32 = Color32::from_rgb(0xef, 0x7d, 0x57);
const RED: Color32 = Color32::from_rgb(0xb1, 0x3e, 0x53);
const BLUE: Color32 = Color32::from_rgb(0x41, 0xa6, 0xf6);
const CYAN: Color32 = Color32::from_rgb(0x73, 0xef, 0xf7);
const PURPLE: Color32 = Color32::from_rgb(0xb4, 0x8c, 0xf2);

const PX: f32 = 3.0; // tamaño de "pixel" para bordes y sombras

const SPR_BROOM: [&str; 8] = [
    "......##", ".....#y#", "....#y#.", "...#y#..", "..#oo#..", ".#oooo#.", "#oo#oo#.", "#######.",
];
const SPR_GLOBE: [&str; 8] = [
    "..####..", ".#bgbb#.", "#bggbbb#", "#bbggbb#", "#gbbbgg#", "#bbbbgb#", ".#bbbb#.", "..####..",
];
const SPR_WRENCH: [&str; 8] = [
    ".##..##.", ".#w##w#.", ".#wwww#.", "..#ww#..", "..#ww#..", "..#ww#..", "..#ww#..", "...##...",
];
const SPR_CHECK: [&str; 8] = [
    "........", "......##", ".....#w#", "##..#w#.", "#w##w#..", ".#ww#...", "..##....", "........",
];
const SPR_UNDO: [&str; 8] = [
    "..#.....", ".#w####.", "#wwwwww#", ".#w###w#", "..#...w#", ".....#w#", "..####w#", "..#####.",
];
const SPR_BOX: [&str; 8] = [
    "..####..", ".#yyyy#.", "#yyyyyy#", "########", "#oo##oo#", "#oo##oo#", "#oooooo#", "########",
];
const SPR_PC: [&str; 10] = [
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
const SPR_PC_BUSY: [&str; 10] = [
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
const SPR_ICON: [&str; 16] = [
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

fn sprite_color(c: char) -> Option<Color32> {
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

fn draw_sprite(p: &Painter, top_left: Pos2, rows: &[&str], scale: f32) {
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
fn window_icon() -> egui::IconData {
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

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

// =====================================================================
//  WIDGETS PIXEL
// =====================================================================

fn pixel_button(
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
fn pixel_small(ui: &mut Ui, label: &str, color: Color32, enabled: bool) -> Response {
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

fn pixel_tab(ui: &mut Ui, label: &str, selected: bool) -> Response {
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

fn pixel_check(
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
fn check_list(
    ui: &mut Ui,
    title: &str,
    items: &[(&str, &str)],
    sel: &mut [bool],
    defaults: Option<&[bool]>,
    states: Option<&[Option<bool>]>,
    enabled: bool,
    cols: usize,
    height: f32,
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
fn app_chip(
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
    let clickable = enabled && !installed;
    let (rect, resp) = ui.allocate_exact_size(
        vec2(width, 28.0),
        if clickable { Sense::click() } else { Sense::hover() },
    );
    if clickable && resp.clicked() {
        *on = !*on;
    }
    let p = ui.painter();
    let hovered = clickable && resp.hovered();
    let fill = if installed {
        mix(PANEL, GREEN, 0.18)
    } else if *on {
        mix(PANEL, LIME, 0.22)
    } else if hovered {
        mix(PANEL, TEXT, 0.10)
    } else {
        mix(PANEL, DARK, 0.30)
    };
    p.rect_filled(rect, 0.0, fill);
    let border = if *on { LIME } else if star { YELLOW } else { DARK };
    p.rect_stroke(rect, 0.0, Stroke::new(2.0_f32, border));

    // casilla
    let b = Rect::from_min_size(pos2(rect.min.x + 7.0, rect.center().y - 6.0), vec2(12.0, 12.0));
    p.rect_filled(b, 0.0, DARK);
    if installed {
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
    let tc = if installed { LIME } else if clickable { TEXT } else { GRAY };
    p.text(pos2(b.max.x + 8.0, rect.center().y), Align2::LEFT_CENTER, shown, FontId::monospace(12.0), tc);

    let resp = if clickable { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp };
    resp.on_hover_ui(|ui| {
        ui.set_max_width(280.0);
        ui.label(egui::RichText::new(name).color(YELLOW).size(13.0));
        ui.label(egui::RichText::new(desc).color(TEXT).size(12.0));
        ui.label(egui::RichText::new(id).color(DIMGRAY).size(10.0));
        if installed {
            ui.label(egui::RichText::new("✔ Ya instalada").color(LIME).size(11.0));
        }
        if star {
            ui.label(egui::RichText::new("★ Recomendado para tu PC").color(YELLOW).size(11.0));
        }
    });
}

/// Lista de apps agrupada por categoría, con packs y tooltips.
fn apps_list(ui: &mut Ui, sel: &mut [bool], st: &[Option<bool>], hw: &str, enabled: bool, height: f32) {
    let installed = |i: usize| st.get(i) == Some(&Some(true));
    pixel_frame(PANEL).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            let n = sel.iter().filter(|&&s| s).count();
            let inst = (0..APPS.len()).filter(|&i| installed(i)).count();
            ui.label(egui::RichText::new("INSTALAR CON WINGET").color(YELLOW).size(13.0));
            ui.label(egui::RichText::new(format!("{n} marcadas · {inst} instaladas")).color(GRAY).size(11.0));
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
        egui::ScrollArea::vertical()
            .id_salt("apps")
            .max_height(height.max(80.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut start = 0;
                while start < APPS.len() {
                    let cat = APPS[start].0;
                    let end = start + APPS[start..].iter().take_while(|a| a.0 == cat).count();
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(6.0, 14.0), Sense::hover());
                        ui.painter().rect_filled(r, 0.0, YELLOW);
                        ui.label(egui::RichText::new(cat).color(CYAN).size(12.0));
                        if cat == "DRIVERS" && !hw.is_empty() {
                            ui.label(egui::RichText::new(format!("Tu PC: {hw}")).color(YELLOW).size(10.0))
                                .on_hover_text("Lo recomendado para tu hardware tiene borde amarillo ★");
                        }
                        let free: Vec<usize> = (start..end).filter(|&i| !installed(i)).collect();
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
                        for i in start..end {
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
}

/// Barra de progreso pixel art: celdas de 4px con brillo arriba y sombra abajo.
fn pixel_progress(ui: &mut Ui, frac: Option<f32>, idle: bool) {
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

fn pixel_frame(fill: Color32) -> egui::Frame {
    egui::Frame::none()
        .fill(fill)
        .stroke(Stroke::new(PX, DARK))
        .inner_margin(12.0)
        .shadow(Shadow { offset: vec2(PX + 1.0, PX + 1.0), blur: 0.0, spread: 0.0, color: DARK })
}

fn note(ui: &mut Ui, s: &str) {
    ui.label(egui::RichText::new(s).color(GRAY).size(11.0));
}

fn setup_style(ctx: &egui::Context) {
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

// =====================================================================
//  MENSAJES ENTRE HILOS
// =====================================================================

#[derive(Clone, Copy)]
enum Kind {
    Info,
    Step,
    Ok,
    Err,
    Dim,
}

impl Kind {
    fn color(self) -> Color32 {
        match self {
            Kind::Info => TEXT,
            Kind::Step => YELLOW,
            Kind::Ok => LIME,
            Kind::Err => ORANGE,
            Kind::Dim => GRAY,
        }
    }
}

enum Msg {
    Line(Kind, String),
    Progress(u32, u32),
    Stat(&'static str, String),
    Done,
    AskRestart,
}

struct Rep<'a>(&'a Sender<Msg>);

impl Rep<'_> {
    fn say(&self, k: Kind, s: impl Into<String>) {
        let _ = self.0.send(Msg::Line(k, s.into()));
    }
    fn step(&self, i: u32, n: u32, s: &str) {
        let _ = self.0.send(Msg::Progress(i - 1, n));
        self.say(Kind::Step, format!("[{i}/{n}] {s}"));
    }
    fn stat(&self, name: &'static str, v: impl Into<String>) {
        let _ = self.0.send(Msg::Stat(name, v.into()));
    }
    /// Muestra las últimas `n` líneas de una salida, indentadas.
    fn tail(&self, k: Kind, out: &[String], n: usize) {
        for l in out.iter().skip(out.len().saturating_sub(n)) {
            self.say(k, format!("      {l}"));
        }
    }
}

// =====================================================================
//  APP
// =====================================================================

#[derive(Clone, Copy, PartialEq, Default)]
enum Tab {
    #[default]
    Limpieza,
    Red,
    Privacidad,
    Rendimiento,
    Reparar,
    Apps,
}

const TABS: [(Tab, &str); 6] = [
    (Tab::Limpieza, "LIMPIEZA"),
    (Tab::Red, "RED"),
    (Tab::Privacidad, "PRIVACIDAD"),
    (Tab::Rendimiento, "RENDIMIENTO"),
    (Tab::Reparar, "REPARAR"),
    (Tab::Apps, "APPS"),
];

fn main() -> eframe::Result<()> {
    if !is_admin() {
        // Relanzar con UAC y salir
        if let Ok(exe) = std::env::current_exe() {
            let path = exe.display().to_string().replace('\'', "''");
            let _ = Command::new("powershell")
                .args(["-NoProfile", "-Command"])
                .arg(format!("Start-Process '{path}' -Verb RunAs"))
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Optimizador")
            .with_icon(window_icon())
            .with_inner_size([660.0, 680.0])
            .with_min_inner_size([600.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Optimizador",
        options,
        Box::new(|cc| {
            setup_style(&cc.egui_ctx);
            Ok(Box::new(App::new(&cc.egui_ctx)))
        }),
    )
}

#[derive(Default)]
struct App {
    tab: Tab,
    log: Vec<(Kind, String)>,
    rx: Option<Receiver<Msg>>,
    ask_restart: bool,
    task: &'static str,
    progress: Option<(u32, u32)>,
    stats: Vec<(&'static str, String)>,
    started: Option<Instant>,
    elapsed: Option<Duration>,
    sel_clean: Vec<bool>,
    sel_priv: Vec<bool>,
    sel_perf: Vec<bool>,
    sel_fix: Vec<bool>,
    sel_apps: Vec<bool>,
    /// Estado detectado: Some(true) = ya aplicado, None = comprobando.
    st_priv: Vec<Option<bool>>,
    st_perf: Vec<Option<bool>>,
    st_rx: Option<Receiver<(Vec<Option<bool>>, Vec<Option<bool>>)>>,
    /// Panel de registro visible y cuántas líneas ya se vieron.
    show_log: bool,
    seen: usize,
    /// Apps ya instaladas: Some(true) = instalada, None = comprobando.
    st_apps: Vec<Option<bool>>,
    apps_rx: Option<Receiver<(Vec<Option<bool>>, String)>>,
    /// GPU y CPU detectadas (para recomendar drivers).
    hw: String,
    /// Apps: 0 = instalar, 1 = quitar bloatware.
    apps_view: u8,
    bloat_on: Vec<bool>,
    bloat_sel: Vec<bool>,
    bloat_rx: Option<Receiver<Vec<bool>>>,
    bloat_loaded: bool,
    /// Rendimiento: 0 = tweaks, 1 = programas de inicio, 2 = servicios.
    perf_view: u8,
    svc: Vec<SvcInfo>,
    svc_rx: Option<Receiver<Vec<SvcInfo>>>,
    svc_loaded: bool,
    startup: Vec<StartupItem>,
    startup_want: Vec<bool>,
    startup_rx: Option<Receiver<Vec<StartupItem>>>,
    startup_loaded: bool,
    /// Actualizador: versión nueva disponible en GitHub y estado.
    upd: Option<String>,
    upd_rx: Option<Receiver<Option<String>>>,
    upd_dl: Option<Receiver<Result<(), String>>>,
}

const REPO: &str = "KevshuppDev/KevWinOptimizer";
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// "v1.2.3" -> [1, 2, 3]
fn parse_ver(s: &str) -> Vec<u32> {
    s.trim().trim_start_matches(['v', 'V']).split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// Consulta la última release en GitHub; devuelve el tag si es más nueva.
fn check_update() -> Option<String> {
    let (ok, out) = ps(&format!(
        "[Net.ServicePointManager]::SecurityProtocol='Tls12'; (Invoke-RestMethod 'https://api.github.com/repos/{REPO}/releases/latest' -Headers @{{'User-Agent'='KevWinOptimizer'}} -TimeoutSec 10).tag_name"
    ));
    let tag = out.into_iter().rev().find(|l| l.starts_with('v'))?;
    (ok && parse_ver(&tag) > parse_ver(VERSION)).then_some(tag)
}

/// Descarga el nuevo .exe y lanza un script que sustituye el actual al cerrarse la app.
fn download_update() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_s = exe.display().to_string().replace('\'', "''");
    let new_s = format!("{exe_s}.new");
    let (ok, out) = ps(&format!(
        "[Net.ServicePointManager]::SecurityProtocol='Tls12'; Invoke-WebRequest 'https://github.com/{REPO}/releases/latest/download/KevWinOptimizer.exe' -OutFile '{new_s}' -UseBasicParsing; if ((Get-Item '{new_s}').Length -lt 100000) {{ throw 'descarga incompleta' }}"
    ));
    if !ok {
        return Err(out.last().cloned().unwrap_or_else(|| "error al descargar".into()));
    }
    let pid = std::process::id();
    let script = format!(
        "Wait-Process -Id {pid} -EA SilentlyContinue; Start-Sleep -Milliseconds 500; Move-Item -Force '{new_s}' '{exe_s}'; Start-Process '{exe_s}'"
    );
    Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

const LOG_W: f32 = 380.0;

fn tweak_items(t: &[Tweak]) -> Vec<(&'static str, &'static str)> {
    t.iter().map(|t| (t.name, t.desc)).collect()
}
fn tweak_defaults(t: &[Tweak]) -> Vec<bool> {
    t.iter().map(|t| t.default).collect()
}
fn picked<T>(list: &'static [T], sel: &[bool]) -> Vec<&'static T> {
    list.iter().zip(sel).filter(|(_, &s)| s).map(|(t, _)| t).collect()
}

impl App {
    fn new(ctx: &egui::Context) -> Self {
        let mut app = Self {
            sel_clean: OPTS.iter().map(|o| o.2).collect(),
            sel_priv: tweak_defaults(PRIVACY),
            sel_perf: tweak_defaults(PERF),
            sel_fix: tweak_defaults(REPAIRS),
            sel_apps: vec![false; APPS.len()],
            st_priv: vec![None; PRIVACY.len()],
            st_perf: vec![None; PERF.len()],
            st_apps: vec![None; APPS.len()],
            ..Default::default()
        };
        app.refresh_status(ctx);
        app.refresh_apps(ctx);
        let (tx, rx) = channel();
        app.upd_rx = Some(rx);
        let c = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(check_update());
            c.request_repaint();
        });
        app
    }

    fn refresh_bloat(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        self.bloat_rx = Some(rx);
        self.bloat_loaded = true;
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(detect_bloat());
            ctx.request_repaint();
        });
    }

    fn bloat_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, enabled: bool, list_h: f32) {
        if !self.bloat_loaded {
            self.refresh_bloat(ctx);
        }
        let loading = self.bloat_rx.is_some();
        let enabled = enabled && !loading;
        if self.bloat_sel.len() != BLOAT.len() {
            self.bloat_sel = vec![false; BLOAT.len()];
        }
        let installed: Vec<usize> = (0..BLOAT.len()).filter(|&i| self.bloat_on.get(i) == Some(&true)).collect();

        pixel_frame(PANEL).show(ui, |ui| {
            ui.horizontal(|ui| {
                let n = installed.iter().filter(|&&i| self.bloat_sel[i]).count();
                ui.label(egui::RichText::new("APPS PREINSTALADAS").color(YELLOW).size(13.0));
                ui.label(egui::RichText::new(format!("{} encontradas · {n} marcadas", installed.len())).color(GRAY).size(11.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        if ui.small_button("Ninguna").clicked() {
                            self.bloat_sel.iter_mut().for_each(|s| *s = false);
                        }
                        if ui.small_button("Recomendadas").on_hover_text("Marca las que casi nadie usa").clicked() {
                            for &i in &installed {
                                self.bloat_sel[i] = BLOAT[i].3;
                            }
                        }
                    });
                });
            });
            egui::ScrollArea::vertical()
                .id_salt("bloat")
                .max_height(list_h.max(80.0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if loading {
                        note(ui, "Buscando apps preinstaladas...");
                        return;
                    }
                    if installed.is_empty() {
                        note(ui, "No queda bloatware conocido. ¡Limpio!");
                        return;
                    }
                    ui.columns(2, |c| {
                        for (k, &i) in installed.iter().enumerate() {
                            let (name, pat, desc, rec) = BLOAT[i];
                            let col = &mut c[k % 2];
                            col.spacing_mut().item_spacing.y = 2.0;
                            let badge = rec.then_some(("RECOMENDADO", ORANGE));
                            let tip = format!("{desc}\n\nPaquete: {pat}");
                            pixel_check(col, &mut self.bloat_sel[i], name, desc, enabled, badge, Some(&tip));
                        }
                    });
                });
        });
        ui.add_space(10.0);
        let chosen: Vec<&'static (&'static str, &'static str, &'static str, bool)> =
            installed.iter().filter(|&&i| self.bloat_sel[i]).map(|&i| &BLOAT[i]).collect();
        let label = if chosen.is_empty() { "Quitar".to_string() } else { format!("Quitar ({})", chosen.len()) };
        if pixel_button(ui, 230.0, Some(&SPR_BROOM), &label, RED, enabled && !chosen.is_empty()).clicked() {
            self.start(ctx, "Quitar bloatware", move |r| remove_bloat(r, &chosen));
        }
        note(ui, "Se quitan para todos los usuarios y no vuelven en cuentas nuevas.\nSe pueden reinstalar desde Microsoft Store.");
    }

    fn refresh_services(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        self.svc_rx = Some(rx);
        self.svc_loaded = true;
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(list_services());
            ctx.request_repaint();
        });
    }

    fn services_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, enabled: bool, list_h: f32) {
        if !self.svc_loaded {
            self.refresh_services(ctx);
        }
        let loading = self.svc_rx.is_some();
        let enabled = enabled && !loading;
        let mut action: Option<(Vec<SvcInfo>, SvcAct, &'static str)> = None;

        pixel_frame(PANEL).show(ui, |ui| {
            ui.horizontal(|ui| {
                let running = self.svc.iter().filter(|s| s.running).count();
                ui.label(egui::RichText::new("PACKS DE SERVICIOS").color(YELLOW).size(13.0));
                ui.label(egui::RichText::new(format!("{running} de {} en ejecución", self.svc.len())).color(GRAY).size(11.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        if ui.small_button("Recargar").clicked() {
                            self.refresh_services(ctx);
                        }
                    });
                });
            });
            egui::ScrollArea::vertical()
                .id_salt("svc")
                .max_height(list_h.max(80.0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if loading && self.svc.is_empty() {
                        note(ui, "Leyendo servicios...");
                    }
                    let mut hidden = 0;
                    for (pi, pack) in SVC_PACKS.iter().enumerate() {
                        let list: Vec<&SvcInfo> = self.svc.iter().filter(|s| s.pack == pi).collect();
                        if list.is_empty() {
                            hidden += 1;
                            continue;
                        }
                        let running = list.iter().filter(|s| s.running).count();
                        let auto = list.iter().filter(|s| s.start == "Automatic").count();
                        let suggest = pack.rec && (running > 0 || auto > 0);

                        let resp = pixel_frame(mix(PANEL, DARK, 0.3)).inner_margin(8.0).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                                ui.painter().rect_filled(r, 0.0, DARK);
                                ui.painter().rect_filled(r.shrink(2.0), 0.0, if running > 0 { LIME } else { DIMGRAY });
                                ui.label(egui::RichText::new(pack.name).color(TEXT).size(13.0));
                                if suggest {
                                    ui.label(egui::RichText::new(" RECOMENDADO DETENER ").background_color(ORANGE).color(DARK).size(10.0));
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let owned = || list.iter().map(|s| (*s).clone()).collect::<Vec<_>>();
                                    if pixel_small(ui, "Detener", RED, enabled && (running > 0 || auto > 0)).clicked() {
                                        action = Some((owned(), SvcAct::Stop, pack.name));
                                    }
                                    if pixel_small(ui, "Iniciar", GREEN, enabled && running < list.len()).clicked() {
                                        action = Some((owned(), SvcAct::Start, pack.name));
                                    }
                                });
                            });
                            ui.label(egui::RichText::new(pack.desc).color(GRAY).size(11.0));
                            ui.horizontal(|ui| {
                                let inicio = if auto == list.len() {
                                    "Automático".to_string()
                                } else if auto == 0 {
                                    "Manual".to_string()
                                } else {
                                    format!("{auto} automáticos")
                                };
                                ui.label(egui::RichText::new(format!("{running}/{} en ejecución  ·  Inicio: {inicio}", list.len())).color(if running > 0 { LIME } else { GRAY }).size(11.0));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let all_auto = auto == list.len();
                                    let mut on = all_auto;
                                    ui.add_enabled_ui(enabled, |ui| {
                                        if ui.checkbox(&mut on, egui::RichText::new("Arrancar con Windows").size(11.0)).changed() {
                                            let owned = list.iter().map(|s| (*s).clone()).collect();
                                            action = Some((owned, if on { SvcAct::Auto } else { SvcAct::Manual }, pack.name));
                                        }
                                    });
                                });
                            });
                        });
                        resp.response.on_hover_ui(|ui| {
                            ui.label(egui::RichText::new(pack.name).color(YELLOW).size(13.0));
                            for s in &list {
                                let estado = if s.running { "● en ejecución" } else { "○ detenido" };
                                ui.label(egui::RichText::new(format!("{estado}  {}  [{}]", s.display, s.start)).size(11.0).color(if s.running { LIME } else { GRAY }));
                            }
                        });
                        ui.add_space(6.0);
                    }
                    if hidden > 0 && !loading {
                        note(ui, &format!("{hidden} packs sin servicios instalados en este PC (Oracle, SQL Server...)."));
                    }
                });
        });

        ui.add_space(10.0);
        // detener todo lo recomendado de una vez
        let rec: Vec<SvcInfo> = self
            .svc
            .iter()
            .filter(|s| SVC_PACKS[s.pack].rec && (s.running || s.start == "Automatic"))
            .cloned()
            .collect();
        let label = if rec.is_empty() { "Recomendados OK".to_string() } else { format!("Detener recomendados ({})", rec.len()) };
        if pixel_button(ui, 290.0, Some(&SPR_CHECK), &label, ORANGE, enabled && !rec.is_empty()).clicked() {
            action = Some((rec, SvcAct::Stop, "recomendados"));
        }
        note(ui, "Detener = se para y pasa a Manual (arranca solo si algo lo pide).\nNo se desactiva nada a la fuerza; \"Iniciar\" lo revierte.");

        if let Some((list, act, _name)) = action {
            self.start(ctx, "Servicios", move |r| svc_action(r, &list, act));
        }
    }

    /// Lee en segundo plano los programas que arrancan con Windows.
    fn refresh_startup(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        self.startup_rx = Some(rx);
        self.startup_loaded = true;
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(list_startup());
            ctx.request_repaint();
        });
    }

    fn startup_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, enabled: bool, list_h: f32) {
        if !self.startup_loaded {
            self.refresh_startup(ctx);
        }
        let loading = self.startup_rx.is_some();
        pixel_frame(PANEL).show(ui, |ui| {
            ui.horizontal(|ui| {
                let on = self.startup_want.iter().filter(|&&w| w).count();
                ui.label(egui::RichText::new("PROGRAMAS DE INICIO").color(YELLOW).size(13.0));
                ui.label(egui::RichText::new(format!("{on} de {} arrancan con Windows", self.startup.len())).color(GRAY).size(11.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(enabled && !loading, |ui| {
                        if ui.small_button("Recargar").clicked() {
                            self.refresh_startup(ctx);
                        }
                        if ui.small_button("Deshacer cambios").clicked() {
                            self.startup_want = self.startup.iter().map(|s| s.enabled).collect();
                        }
                    });
                });
            });
            egui::ScrollArea::vertical()
                .id_salt("startup")
                .max_height(list_h.max(80.0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if loading {
                        note(ui, "Leyendo programas de inicio...");
                    } else if self.startup.is_empty() {
                        note(ui, "No hay programas configurados para arrancar con Windows.");
                    }
                    for (item, want) in self.startup.iter().zip(self.startup_want.iter_mut()) {
                        let origen = match item.src.as_str() {
                            "HKCU" | "UF" => "solo tu usuario",
                            _ => "todos los usuarios",
                        };
                        let badge = (*want != item.enabled).then_some(("CAMBIO", ORANGE));
                        let short: String = item.cmd.chars().take(48).collect();
                        let tip = format!("{}\n\nOrigen: {origen}", item.cmd);
                        pixel_check(ui, want, &item.name, &short, enabled && !loading, badge, Some(&tip));
                    }
                });
        });
        ui.add_space(10.0);
        let changes: Vec<(StartupItem, bool)> = self
            .startup
            .iter()
            .zip(&self.startup_want)
            .filter(|(s, &w)| s.enabled != w)
            .map(|(s, &w)| (s.clone(), w))
            .collect();
        let label = if changes.is_empty() { "Aplicar".to_string() } else { format!("Aplicar ({})", changes.len()) };
        if pixel_button(ui, 230.0, Some(&SPR_CHECK), &label, GREEN, enabled && !changes.is_empty()).clicked() {
            self.start(ctx, "Programas de inicio", move |r| apply_startup(r, &changes));
        }
        note(ui, "Desmarcado = no arranca con Windows. No se borra nada,\nes lo mismo que el Administrador de tareas.");
    }

    /// Comprueba en segundo plano qué apps ya están instaladas.
    fn refresh_apps(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        self.apps_rx = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send((detect_apps(), detect_hw()));
            ctx.request_repaint();
        });
    }

    /// Comprueba en segundo plano qué tweaks ya están aplicados.
    fn refresh_status(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        self.st_rx = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let res = (detect(PRIVACY_CHECK), detect(PERF_CHECK));
            let _ = tx.send(res);
            ctx.request_repaint();
        });
    }

    fn busy(&self) -> bool {
        self.rx.is_some()
    }

    fn start(
        &mut self,
        ctx: &egui::Context,
        name: &'static str,
        job: impl FnOnce(&Rep) + Send + 'static,
    ) {
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.log.clear();
        self.stats.clear();
        self.progress = None;
        self.task = name;
        self.started = Some(Instant::now());
        self.elapsed = None;
        let ctx = ctx.clone();
        thread::spawn(move || {
            job(&Rep(&tx));
            let _ = tx.send(Msg::Done);
            ctx.request_repaint();
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.st_rx {
            if let Ok((a, b)) = rx.try_recv() {
                // las casillas reflejan el estado real (sin cambios pendientes)
                self.sel_priv = a.iter().map(|s| *s == Some(true)).collect();
                self.sel_perf = b.iter().map(|s| *s == Some(true)).collect();
                self.st_priv = a;
                self.st_perf = b;
                self.st_rx = None;
            }
        }
        if let Some(rx) = &self.bloat_rx {
            if let Ok(v) = rx.try_recv() {
                self.bloat_on = v;
                self.bloat_rx = None;
            }
        }
        if let Some(rx) = &self.svc_rx {
            if let Ok(v) = rx.try_recv() {
                self.svc = v;
                self.svc_rx = None;
            }
        }
        if let Some(rx) = &self.startup_rx {
            if let Ok(v) = rx.try_recv() {
                self.startup_want = v.iter().map(|s| s.enabled).collect();
                self.startup = v;
                self.startup_rx = None;
            }
        }
        if let Some(rx) = &self.apps_rx {
            if let Ok((v, hw)) = rx.try_recv() {
                self.hw = hw;
                // lo instalado no se puede marcar
                for (o, s) in self.sel_apps.iter_mut().zip(&v) {
                    *o &= *s != Some(true);
                }
                self.st_apps = v;
                self.apps_rx = None;
            }
        }
        let Some(rx) = &self.rx else { return };
        let mut done = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Line(k, l) => self.log.push((k, l)),
                Msg::Progress(a, b) => self.progress = Some((a, b)),
                Msg::Stat(n, v) => self.stats.push((n, v)),
                Msg::AskRestart => self.ask_restart = true,
                Msg::Done => done = true,
            }
        }
        if done {
            self.rx = None;
            if let Some((_, n)) = self.progress {
                self.progress = Some((n, n));
            }
            self.elapsed = self.started.map(|s| s.elapsed());
            self.refresh_status(ctx);
            if matches!(self.task, "Instalar apps" | "Actualizar apps") {
                self.refresh_apps(ctx);
            }
            if self.task == "Programas de inicio" {
                self.refresh_startup(ctx);
            }
            if self.task == "Servicios" {
                self.refresh_services(ctx);
            }
            if self.task == "Quitar bloatware" {
                self.bloat_sel.iter_mut().for_each(|s| *s = false);
                self.refresh_bloat(ctx);
            }
        } else {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    /// Contenido de la pestaña activa.
    fn tab_ui(&mut self, ui: &mut Ui, ctx: &egui::Context, enabled: bool) {
        let list_h = ui.available_height() - 150.0;
        let bw = 230.0;
        match self.tab {
            Tab::Limpieza => {
                let items: Vec<_> = OPTS.iter().map(|o| (o.0, o.1)).collect();
                let defs: Vec<_> = OPTS.iter().map(|o| o.2).collect();
                check_list(ui, "TAREAS DE LIMPIEZA", &items, &mut self.sel_clean, Some(&defs), None, enabled, 2, list_h);
                ui.add_space(10.0);
                let any = self.sel_clean.contains(&true);
                if pixel_button(ui, bw, Some(&SPR_BROOM), "Optimizar", GREEN, enabled && any).clicked() {
                    let sel = self.sel_clean.clone();
                    self.start(ctx, "Optimizar", move |r| optimizar(r, &sel));
                }
                note(ui, "Los archivos en uso se saltan; es normal.");
            }
            Tab::Red => {
                pixel_frame(PANEL).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("REINICIAR CONEXIÓN").color(YELLOW).size(13.0));
                    if pixel_button(ui, bw, Some(&SPR_GLOBE), "Red rápida", BLUE, enabled).clicked() {
                        self.start(ctx, "Red rápida", red_rapida);
                    }
                    note(ui, "Renueva IP, reinicia adaptadores y prueba la conexión.");
                    ui.add_space(4.0);
                    if pixel_button(ui, bw, Some(&SPR_WRENCH), "Red completa", ORANGE, enabled).clicked() {
                        self.start(ctx, "Red completa", red_completa);
                    }
                    note(ui, "Reset de Winsock y TCP/IP. Requiere reiniciar la PC.");
                });
                ui.add_space(12.0);
                pixel_frame(PANEL).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("SERVIDORES DNS").color(YELLOW).size(13.0));
                    note(ui, "Cambia el DNS de los adaptadores conectados.");
                    ui.horizontal_wrapped(|ui| {
                        let dns: [(&str, Option<[&'static str; 2]>, Color32); 3] = [
                            ("Cloudflare", Some(["1.1.1.1", "1.0.0.1"]), PURPLE),
                            ("Google", Some(["8.8.8.8", "8.8.4.4"]), BLUE),
                            ("Automático", None, GRAY),
                        ];
                        for (name, servers, col) in dns {
                            if pixel_button(ui, 160.0, None, name, col, enabled).clicked() {
                                self.start(ctx, "Cambiar DNS", move |r| set_dns(r, servers));
                            }
                        }
                    });
                });
            }
            Tab::Privacidad | Tab::Rendimiento => {
                if self.tab == Tab::Rendimiento {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (v, name) in [(0u8, "TWEAKS"), (1, "INICIO"), (2, "SERVICIOS")] {
                            if pixel_tab(ui, name, self.perf_view == v).clicked() {
                                self.perf_view = v;
                            }
                        }
                    });
                    ui.add_space(8.0);
                    match self.perf_view {
                        1 => {
                            let h = ui.available_height() - 150.0;
                            self.startup_ui(ui, ctx, enabled, h);
                            return;
                        }
                        2 => {
                            let h = ui.available_height() - 110.0;
                            self.services_ui(ui, ctx, enabled, h);
                            return;
                        }
                        _ => {}
                    }
                }
                let list_h = ui.available_height() - 150.0;
                let (list, title, sel, st) = if self.tab == Tab::Privacidad {
                    (PRIVACY, "TWEAKS DE PRIVACIDAD", &mut self.sel_priv, &self.st_priv)
                } else {
                    (PERF, "TWEAKS DE RENDIMIENTO", &mut self.sel_perf, &self.st_perf)
                };
                check_list(ui, title, &tweak_items(list), sel, Some(&tweak_defaults(list)), Some(st), enabled, 2, list_h);
                // casilla = estado deseado; solo se toca lo que cambia
                let apply: Vec<&'static Tweak> = list
                    .iter()
                    .zip(sel.iter().zip(st.iter()))
                    .filter(|(_, (&w, &s))| w && s != Some(true))
                    .map(|(t, _)| t)
                    .collect();
                let undo: Vec<&'static Tweak> = list
                    .iter()
                    .zip(sel.iter().zip(st.iter()))
                    .filter(|(_, (&w, &s))| !w && s == Some(true))
                    .map(|(t, _)| t)
                    .collect();
                let n = apply.len() + undo.len();
                let checking = self.st_rx.is_some();
                let enabled = enabled && !checking;
                ui.add_space(10.0);
                let label = if n == 0 { "Sin cambios".to_string() } else { format!("Aplicar cambios ({n})") };
                if pixel_button(ui, 260.0, Some(&SPR_CHECK), &label, GREEN, enabled && n > 0).clicked() {
                    self.start(ctx, "Aplicar tweaks", move |r| run_changes(r, &apply, &undo));
                }
                if checking {
                    note(ui, "Comprobando qué tweaks ya están aplicados...");
                } else {
                    note(ui, "Marcado = activo. Marca para APLICAR, desmarca para DESHACER.\nAntes de aplicar se crea un punto de restauración.");
                }
            }
            Tab::Reparar => {
                check_list(ui, "HERRAMIENTAS DE REPARACIÓN", &tweak_items(REPAIRS), &mut self.sel_fix, Some(&tweak_defaults(REPAIRS)), None, enabled, 1, list_h);
                ui.add_space(10.0);
                let any = self.sel_fix.contains(&true);
                if pixel_button(ui, bw, Some(&SPR_WRENCH), "Ejecutar", GREEN, enabled && any).clicked() {
                    let c = picked(REPAIRS, &self.sel_fix);
                    self.start(ctx, "Reparar", move |r| run_repairs(r, &c));
                }
                note(ui, "SFC y DISM tardan; puedes seguir usando la PC.");
            }
            Tab::Apps => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (v, name) in [(0u8, "INSTALAR"), (1, "QUITAR BLOATWARE")] {
                        if pixel_tab(ui, name, self.apps_view == v).clicked() {
                            self.apps_view = v;
                        }
                    }
                });
                ui.add_space(8.0);
                if self.apps_view == 1 {
                    let h = ui.available_height() - 110.0;
                    self.bloat_ui(ui, ctx, enabled, h);
                    return;
                }
                let list_h = ui.available_height() - 110.0;
                apps_list(ui, &mut self.sel_apps, &self.st_apps, &self.hw, enabled, list_h);
                if self.apps_rx.is_some() {
                    note(ui, "Buscando apps ya instaladas...");
                }
                ui.add_space(10.0);
                let any = self.sel_apps.contains(&true);
                ui.horizontal(|ui| {
                    if pixel_button(ui, bw, Some(&SPR_BOX), "Instalar", GREEN, enabled && any).clicked() {
                        let c = picked(APPS, &self.sel_apps);
                        self.start(ctx, "Instalar apps", move |r| install_apps(r, &c));
                    }
                    if pixel_button(ui, bw, Some(&SPR_UNDO), "Actualizar todo", BLUE, enabled).clicked() {
                        self.start(ctx, "Actualizar apps", upgrade_all);
                    }
                });
                note(ui, "Pasa el cursor sobre una app para ver qué es.\n\"Actualizar todo\" actualiza todo lo que winget conozca.");
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);
        if let Some(Ok(v)) = self.upd_rx.as_ref().map(|r| r.try_recv()) {
            self.upd_rx = None;
            if let Some(tag) = &v {
                self.log.push((Kind::Info, format!("Nueva versión disponible: {tag} (tienes v{VERSION})")));
            }
            self.upd = v;
        }
        if let Some(Ok(r)) = self.upd_dl.as_ref().map(|r| r.try_recv()) {
            self.upd_dl = None;
            match r {
                Ok(()) => std::process::exit(0),
                Err(e) => self.log.push((Kind::Err, format!("No se pudo actualizar: {e}"))),
            }
        }
        let busy = self.busy();
        let enabled = !busy && !self.ask_restart;

        // --- cabecera + pestañas ---
        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::none().fill(PANEL).inner_margin(egui::Margin { left: 14.0, right: 14.0, top: 12.0, bottom: 0.0 }).stroke(Stroke::new(PX, DARK)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(12.0 * 4.0, 10.0 * 4.0), Sense::hover());
                    let blink = busy && (ui.input(|i| i.time) * 3.0) as i64 % 2 == 0;
                    draw_sprite(ui.painter(), r.min, if blink { &SPR_PC_BUSY } else { &SPR_PC }, 4.0);
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(260.0, 26.0), Sense::hover());
                        let f = FontId::monospace(24.0);
                        let p = ui.painter();
                        p.text(r.left_center() + vec2(3.0, 3.0), Align2::LEFT_CENTER, "OPTIMIZADOR", f.clone(), DARK);
                        p.text(r.left_center(), Align2::LEFT_CENTER, "OPTIMIZADOR", f, YELLOW);
                        ui.label(egui::RichText::new(format!("v{VERSION}  ·  modo administrador  ·  Windows")).color(GRAY).size(12.0));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.upd_dl.is_some() {
                            ui.label(egui::RichText::new("Descargando actualización...").color(YELLOW).size(12.0));
                        } else if let Some(tag) = self.upd.clone() {
                            if pixel_small(ui, &format!("ACTUALIZAR A {tag}"), LIME, enabled)
                                .on_hover_text("Descarga la nueva versión desde GitHub y reinicia la app")
                                .clicked()
                            {
                                let (tx, rx) = channel();
                                self.upd_dl = Some(rx);
                                let c = ctx.clone();
                                thread::spawn(move || {
                                    let _ = tx.send(download_update());
                                    c.request_repaint();
                                });
                            }
                        }
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (t, name) in TABS {
                        if pixel_tab(ui, name, self.tab == t).clicked() {
                            self.tab = t;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        let unseen = self.log.len().saturating_sub(self.seen);
                        let label = match (self.show_log, unseen) {
                            (true, _) => "REGISTRO ◂".to_string(),
                            (false, 0) => "REGISTRO ▸".to_string(),
                            (false, n) => format!("REGISTRO ({n}) ▸"),
                        };
                        let resp = pixel_tab(ui, &label, self.show_log)
                            .on_hover_text(if self.show_log { "Ocultar el registro" } else { "Ver el registro de lo que hace la app" });
                        if resp.clicked() {
                            self.show_log = !self.show_log;
                            // la ventana crece/encoge para hacer sitio al panel
                            if let Some(r) = ctx.input(|i| i.viewport().inner_rect) {
                                let dw = if self.show_log { LOG_W } else { -LOG_W };
                                let size = vec2((r.width() + dw).max(600.0), r.height());
                                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                            }
                        }
                    });
                });
            });

        // --- barra de estado ---
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::none().fill(PANEL).inner_margin(10.0).stroke(Stroke::new(PX, DARK)))
            .show(ctx, |ui| {
                let frac = self.progress.map(|(a, b)| a as f32 / b.max(1) as f32);
                pixel_progress(ui, frac, !busy && self.progress.is_none());
                ui.horizontal(|ui| {
                    let (txt, col) = if busy {
                        let dots = ".".repeat((ui.input(|i| i.time) * 3.0) as usize % 4);
                        (format!("► {}{dots}", self.task), YELLOW)
                    } else if self.ask_restart {
                        ("! Esperando respuesta".into(), ORANGE)
                    } else if self.elapsed.is_some() {
                        (format!("✔ {} terminado", self.task), LIME)
                    } else {
                        ("■ Listo para empezar".into(), GRAY)
                    };
                    ui.label(egui::RichText::new(txt).color(col));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let t = self.elapsed.or_else(|| self.started.filter(|_| busy).map(|s| s.elapsed()));
                        if let Some(t) = t {
                            ui.label(egui::RichText::new(format!("{:.1}s", t.as_secs_f32())).color(GRAY));
                        }
                        // con el registro oculto, el resumen va aquí
                        if !self.show_log && !self.stats.is_empty() {
                            let s: Vec<String> = self.stats.iter().map(|(n, v)| format!("{n}: {v}")).collect();
                            ui.label(egui::RichText::new(s.join("  ·  ")).color(LIME).size(12.0));
                        }
                    });
                });
            });

        // --- terminal (derecha, plegable) ---
        if self.show_log {
            self.seen = self.log.len();
        }
        egui::SidePanel::right("log")
            .resizable(false)
            .exact_width(LOG_W)
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin { left: 4.0, right: 18.0, top: 16.0, bottom: 18.0 }))
            .show_animated(ctx, self.show_log, |ui| {
                if !self.stats.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for (name, val) in &self.stats {
                            pixel_frame(PANEL).inner_margin(8.0).show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(*name).color(GRAY).size(10.0));
                                    ui.label(egui::RichText::new(val).color(LIME).size(15.0));
                                });
                            });
                        }
                    });
                    ui.add_space(6.0);
                }
                let log_h = ui.available_height() - 58.0;
                pixel_frame(DARK).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .max_height(log_h)
                        .show(ui, |ui| {
                            if self.log.is_empty() {
                                ui.label(egui::RichText::new("C:\\> _").color(GREEN).monospace());
                                ui.label(egui::RichText::new("Elige una pestaña y una acción.").color(DIMGRAY).monospace());
                            }
                            for (k, l) in &self.log {
                                ui.label(egui::RichText::new(l).color(k.color()).monospace().size(12.0));
                            }
                        });
                });
                ui.add_space(8.0);
                if pixel_button(ui, ui.available_width() - PX, None, "Copiar registro", GRAY, !self.log.is_empty()).clicked() {
                    let txt: Vec<&str> = self.log.iter().map(|(_, l)| l.as_str()).collect();
                    ctx.copy_text(txt.join("\r\n"));
                }
            });

        // --- pestaña activa ---
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin { left: 18.0, right: 14.0, top: 16.0, bottom: 14.0 }))
            .show(ctx, |ui| self.tab_ui(ui, ctx, enabled));

        // --- diálogo reinicio ---
        if self.ask_restart {
            egui::Window::new("¿Reiniciar PC?")
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .frame(pixel_frame(PANEL).inner_margin(18.0))
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new("¿REINICIAR PC?").color(YELLOW).size(18.0));
                    ui.label("Hay que reiniciar para aplicar\nlos cambios de red.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if pixel_button(ui, 150.0, None, "Reiniciar", RED, true).clicked() {
                            run_silent("shutdown", &["/r", "/t", "5"]);
                            self.log.push((Kind::Ok, "La PC se reiniciará en 5 segundos.".into()));
                            self.ask_restart = false;
                        }
                        if pixel_button(ui, 150.0, None, "Más tarde", GRAY, true).clicked() {
                            self.log.push((Kind::Dim, "Reinicia la PC más tarde para aplicar los cambios.".into()));
                            self.ask_restart = false;
                        }
                    });
                });
        }
    }
}

// =====================================================================
//  TAREAS
// =====================================================================

fn optimizar(r: &Rep, sel: &[bool]) {
    let antes = free_gb();
    let env = |k: &str, d: &str| PathBuf::from(std::env::var(k).unwrap_or_else(|_| d.into()));
    let windir = env("WINDIR", r"C:\Windows");
    let local = env("LOCALAPPDATA", r"C:\Users\Default\AppData\Local");
    let progdata = env("ProgramData", r"C:\ProgramData");
    let mut total = 0u64;

    let tasks: Vec<usize> = (0..OPTS.len()).filter(|&i| sel.get(i) == Some(&true)).collect();
    let n = tasks.len() as u32;

    for (k, &i) in tasks.iter().enumerate() {
        r.step(k as u32 + 1, n, OPTS[i].0);
        let mut freed: Option<u64> = Some(0);
        let mut add = |d: PathBuf| {
            if let Some(f) = freed.as_mut() {
                *f += clean_dir(&d);
            }
        };
        match i {
            0 => add(std::env::temp_dir()),
            1 => add(windir.join("Temp")),
            2 => {
                run_silent("net", &["stop", "wuauserv"]);
                run_silent("net", &["stop", "bits"]);
                add(windir.join(r"SoftwareDistribution\Download"));
                run_silent("net", &["start", "bits"]);
                run_silent("net", &["start", "wuauserv"]);
            }
            3 => {
                ps("Delete-DeliveryOptimizationCache -Force -EA SilentlyContinue");
                freed = None;
            }
            4 => {
                for d in ["ReportArchive", "ReportQueue", "Temp"] {
                    add(progdata.join(r"Microsoft\Windows\WER").join(d));
                }
                add(local.join("CrashDumps"));
            }
            5 => {
                let dir = local.join(r"Microsoft\Windows\Explorer");
                let mut b = 0;
                if let Ok(es) = std::fs::read_dir(&dir) {
                    for e in es.flatten() {
                        let name = e.file_name().to_string_lossy().to_lowercase();
                        if name.starts_with("thumbcache_") && name.ends_with(".db") {
                            let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                            if std::fs::remove_file(e.path()).is_ok() {
                                b += len;
                            }
                        }
                    }
                }
                freed = Some(b);
            }
            6 => {
                for d in browser_cache_dirs(&local) {
                    add(d);
                }
            }
            7 => {
                ps("Clear-RecycleBin -Force -EA SilentlyContinue");
                freed = None;
            }
            8 => {
                run_silent("ipconfig", &["/flushdns"]);
                freed = None;
            }
            9 => {
                for s in INTEL_SERVICES {
                    run_silent("sc", &["stop", s]);
                    run_silent("sc", &["config", s, "start=", "disabled"]);
                }
                freed = None;
            }
            10 => {
                let (ok, _) = ps("Optimize-Volume -DriveLetter C -ReTrim");
                r.say(Kind::Dim, if ok { "      TRIM enviado al SSD" } else { "      No aplica (¿disco HDD?)" });
                freed = None;
            }
            12 => add(windir.join("Logs")),
            13 => {
                let home = env("USERPROFILE", r"C:\Users\Default");
                // OJO: no borrar .cargo\registry\src ni .cargo\git\checkouts:
                // los proyectos ya compilados los usan y cargo no los re-extrae solo.
                for d in [
                    home.join(r".cargo\registry\cache"),
                    home.join(r".bun\install\cache"),
                    local.join("npm-cache"),
                    local.join(r"pip\cache"),
                    local.join(r"uv\cache"),
                    local.join("go-build"),
                ] {
                    add(d);
                }
            }
            14 => {
                for d in [r"C:\NVIDIA", r"C:\AMD", r"C:\Intel"] {
                    add(PathBuf::from(d));
                }
            }
            11 => {
                r.say(Kind::Dim, "      Esto puede tardar, no cierres la app...");
                let (_, out) = run_status("Dism.exe", &["/Online", "/Cleanup-Image", "/StartComponentCleanup"]);
                r.tail(Kind::Dim, &out, 1);
                freed = None;
            }
            _ => freed = None,
        }
        if let Some(b) = freed {
            r.say(Kind::Dim, format!("      {}", mb(b)));
            total += b;
        }
    }

    r.say(Kind::Info, "");
    r.say(Kind::Ok, format!("Liberados {}", mb(total)));
    r.stat("LIBERADO", mb(total));
    if let (Some(a), Some(d)) = (antes, free_gb()) {
        r.say(Kind::Ok, format!("Espacio libre: {a:.2} GB -> {d:.2} GB"));
        r.stat("LIBRE ANTES", format!("{a:.1} GB"));
        r.stat("LIBRE AHORA", format!("{d:.1} GB"));
    }
}

fn red_rapida(r: &Rep) {
    const N: u32 = 5;
    r.step(1, N, "Limpiando DNS");
    run_silent("ipconfig", &["/flushdns"]);
    r.step(2, N, "Liberando IP");
    run_silent("ipconfig", &["/release"]);
    r.step(3, N, "Reiniciando adaptadores físicos");
    ps("Get-NetAdapter -Physical | Where-Object Status -ne 'Disabled' | Restart-NetAdapter -Confirm:$false");
    thread::sleep(Duration::from_secs(5));
    r.step(4, N, "Renovando IP");
    run_silent("ipconfig", &["/renew"]);
    r.step(5, N, "Probando conexión (8.8.8.8)");
    let out = run_output("ping", &["-n", "4", "8.8.8.8"]);
    let ok = out.iter().filter(|l| l.contains("TTL=")).count();
    r.tail(Kind::Dim, &out, out.len());
    r.say(Kind::Info, "");
    if ok > 0 {
        r.say(Kind::Ok, format!("Conexión OK ({ok}/4 respuestas)"));
        r.stat("CONEXIÓN", "OK");
    } else {
        r.say(Kind::Err, "Sin respuesta. Revisa el cable / WiFi o prueba la red completa.");
        r.stat("CONEXIÓN", "SIN RED");
    }
    r.stat("PING", format!("{ok}/4"));
}

fn red_completa(r: &Rep) {
    let cmds: [(&str, &[&str]); 3] = [
        ("netsh", &["winsock", "reset"]),
        ("netsh", &["int", "ip", "reset"]),
        ("ipconfig", &["/flushdns"]),
    ];
    let n = cmds.len() as u32;
    for (i, (cmd, args)) in cmds.iter().enumerate() {
        r.step(i as u32 + 1, n, &format!("{cmd} {}", args.join(" ")));
        r.tail(Kind::Dim, &run_output(cmd, args), 3);
    }
    r.say(Kind::Ok, "Reset completado.");
    let _ = r.0.send(Msg::AskRestart);
}

fn set_dns(r: &Rep, servers: Option<[&str; 2]>) {
    r.step(1, 2, "Cambiando DNS");
    let script = match servers {
        Some([a, b]) => format!(
            "Get-NetAdapter -Physical | ? Status -eq 'Up' | Set-DnsClientServerAddress -ServerAddresses ('{a}','{b}')"
        ),
        None => "Get-NetAdapter -Physical | ? Status -eq 'Up' | Set-DnsClientServerAddress -ResetServerAddresses".into(),
    };
    let (ok, out) = ps(&script);
    r.step(2, 2, "Limpiando caché DNS");
    run_silent("ipconfig", &["/flushdns"]);
    if ok {
        let txt = servers.map(|s| s.join(", ")).unwrap_or_else(|| "automático (DHCP)".into());
        r.say(Kind::Ok, format!("DNS: {txt}"));
        r.stat("DNS", servers.map(|s| s[0].to_string()).unwrap_or_else(|| "AUTO".into()));
    } else {
        r.say(Kind::Err, "No se pudo cambiar el DNS:");
        r.tail(Kind::Err, &out, 2);
    }
}

/// Aplica y deshace tweaks en una sola tarea (solo lo que cambia).
fn run_changes(r: &Rep, apply: &[&'static Tweak], undo: &[&'static Tweak]) {
    let n = (apply.len() + undo.len() + usize::from(!apply.is_empty())) as u32;
    let mut k = 0;
    let mut step = |s: &str| {
        k += 1;
        r.step(k, n, s);
    };
    if !apply.is_empty() {
        step("Punto de restauración");
        match ps(RESTORE_POINT) {
            (true, _) => r.say(Kind::Ok, "      Creado"),
            (false, _) => r.say(Kind::Err, "      No se pudo crear (¿Protección del sistema apagada?)"),
        }
    }
    let mut good = 0;
    for (t, is_undo) in apply.iter().map(|t| (t, false)).chain(undo.iter().map(|t| (t, true))) {
        step(&format!("{}: {}", if is_undo { "Deshacer" } else { "Aplicar" }, t.name));
        let (ok, out) = ps(if is_undo { t.undo } else { t.apply });
        if ok {
            good += 1;
            r.say(Kind::Ok, "      OK");
        } else {
            r.tail(Kind::Err, &out, 2);
        }
    }
    let total = apply.len() + undo.len();
    r.say(Kind::Info, "");
    r.say(Kind::Ok, format!("{good} de {total} cambios hechos."));
    r.say(Kind::Dim, "Algunos cambios se ven al cerrar sesión o reiniciar.");
    if !apply.is_empty() {
        r.stat("APLICADOS", apply.len().to_string());
    }
    if !undo.is_empty() {
        r.stat("DESHECHOS", undo.len().to_string());
    }
}

#[allow(dead_code)]
fn run_tweaks(r: &Rep, list: &[(&'static Tweak, Option<bool>)], undo: bool) {
    // ya aplicado + aplicar, o no aplicado + deshacer => no hace falta
    let skip = |st: Option<bool>| st == Some(!undo);
    let pending = list.iter().filter(|(_, st)| !skip(*st)).count();
    if pending == 0 {
        r.step(1, 1, "Comprobando");
        r.say(Kind::Ok, if undo { "Nada que deshacer: ninguno estaba aplicado." } else { "Todo ya estaba aplicado. Nada que hacer." });
        r.stat("OMITIDOS", format!("{}", list.len()));
        return;
    }
    let extra = if undo { 0 } else { 1 };
    let n = (list.len() + extra) as u32;
    if !undo {
        r.step(1, n, "Punto de restauración");
        match ps(RESTORE_POINT) {
            (true, _) => r.say(Kind::Ok, "      Creado"),
            (false, _) => r.say(Kind::Err, "      No se pudo crear (¿Protección del sistema apagada?)"),
        }
    }
    let mut good = 0;
    let mut skipped = 0;
    for (i, &(t, st)) in list.iter().enumerate() {
        let verb = if undo { "Deshacer" } else { "Aplicar" };
        r.step((i + 1 + extra) as u32, n, &format!("{verb}: {}", t.name));
        if skip(st) {
            skipped += 1;
            r.say(Kind::Dim, if undo { "      No estaba aplicado, se omite" } else { "      Ya estaba aplicado, se omite" });
            continue;
        }
        let (ok, out) = ps(if undo { t.undo } else { t.apply });
        if ok {
            good += 1;
            r.say(Kind::Ok, "      OK");
        } else {
            r.tail(Kind::Err, &out, 2);
        }
    }
    r.say(Kind::Info, "");
    r.say(Kind::Ok, format!("{good} de {pending} cambios {}.", if undo { "deshechos" } else { "aplicados" }));
    if skipped > 0 {
        r.say(Kind::Dim, format!("{skipped} omitidos porque no hacía falta."));
    }
    r.say(Kind::Dim, "Algunos cambios se ven al cerrar sesión o reiniciar.");
    r.stat(if undo { "DESHECHOS" } else { "APLICADOS" }, format!("{good}/{pending}"));
    if skipped > 0 {
        r.stat("OMITIDOS", skipped.to_string());
    }
}

fn run_repairs(r: &Rep, list: &[&'static Tweak]) {
    let n = list.len() as u32;
    let mut good = 0;
    for (i, t) in list.iter().enumerate() {
        r.step(i as u32 + 1, n, t.name);
        let (ok, out) = ps(t.apply);
        if ok {
            good += 1;
        }
        r.tail(if ok { Kind::Dim } else { Kind::Err }, &out, 3);
        r.say(if ok { Kind::Ok } else { Kind::Err }, if ok { "      OK" } else { "      Terminó con errores" });
    }
    r.stat("COMPLETADAS", format!("{good}/{n}"));
}

const WINGET_FLAGS: [&str; 4] = [
    "--silent",
    "--accept-package-agreements",
    "--accept-source-agreements",
    "--disable-interactivity",
];

fn install_apps(r: &Rep, list: &[&'static AppPkg]) {
    if run_status("winget", &["--version"]).0 == false {
        r.say(Kind::Err, "winget no está disponible. Instala 'App Installer' desde la Microsoft Store.");
        return;
    }
    let n = list.len() as u32;
    let mut good = 0;
    for (i, &&(_, name, id, _)) in list.iter().enumerate() {
        r.step(i as u32 + 1, n, name);
        if let Some(url) = id.strip_prefix("web:") {
            r.say(Kind::Dim, "      No está en winget: se abre la web oficial en el navegador.");
            run_silent("cmd", &["/c", "start", "", url]);
            r.say(Kind::Ok, "      Web abierta");
            good += 1;
            continue;
        }
        if id.starts_with("vscode:") || id.starts_with("custom:") {
            let (ok, out) = install_special(r, id);
            if ok {
                good += 1;
                r.say(Kind::Ok, "      Instalado");
            } else {
                r.tail(Kind::Err, &out, 2);
            }
            continue;
        }
        let mut args = vec!["install", "--id", id, "-e"];
        args.extend(WINGET_FLAGS);
        if !id.contains('.') {
            args.extend(["--source", "msstore"]); // ids de la Microsoft Store no llevan punto
        }
        if id == "Microsoft.VisualStudio.2022.BuildTools" {
            // sin esto se instala vacío: hace falta la carga de trabajo de C++
            r.say(Kind::Dim, "      Con compilador C++ (MSVC). Tarda bastante...");
            args.extend([
                "--override",
                "--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended",
            ]);
        }
        let (ok, out) = run_status("winget", &args);
        if ok {
            good += 1;
            r.say(Kind::Ok, "      Instalado");
        } else {
            r.tail(Kind::Err, &out, 1);
        }
    }
    r.stat("INSTALADAS", format!("{good}/{n}"));
}

fn upgrade_all(r: &Rep) {
    r.step(1, 1, "winget upgrade --all");
    r.say(Kind::Dim, "      Puede tardar varios minutos...");
    let mut args = vec!["upgrade", "--all"];
    args.extend(WINGET_FLAGS);
    let (ok, out) = run_status("winget", &args);
    r.tail(Kind::Dim, &out, 8);
    r.say(if ok { Kind::Ok } else { Kind::Err }, if ok { "Actualización terminada." } else { "Algunas apps no se pudieron actualizar." });
}

// =====================================================================
//  UTILIDADES
// =====================================================================

/// Funciones auxiliares que tienen disponibles todos los scripts.
const PS_PRELUDE: &str = r"$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding=[Text.Encoding]::UTF8; function SetReg($p,$n,$v,$t='DWord'){ if(!(Test-Path $p)){ New-Item -Path $p -Force | Out-Null }; Set-ItemProperty -Path $p -Name $n -Value $v -Type $t -Force }; function DelReg($p,$n){ Remove-ItemProperty -Path $p -Name $n -Force -EA SilentlyContinue }; function GetReg($p,$n){ try { (Get-ItemProperty -Path $p -Name $n -EA Stop).$n } catch { $null } }; ";

/// Ejecuta un script de PowerShell. Devuelve (éxito, líneas de salida).
fn ps(script: &str) -> (bool, Vec<String>) {
    run_status(
        "powershell",
        &["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &format!("{PS_PRELUDE}{script}")],
    )
}

/// Carpetas de caché de Chrome, Edge, Brave (todos los perfiles) y Firefox.
fn browser_cache_dirs(local: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for base in [r"Google\Chrome\User Data", r"Microsoft\Edge\User Data", r"BraveSoftware\Brave-Browser\User Data"] {
        let Ok(es) = std::fs::read_dir(local.join(base)) else { continue };
        for e in es.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name == "Default" || name.starts_with("Profile ") {
                for c in ["Cache", "Code Cache", "GPUCache"] {
                    v.push(e.path().join(c));
                }
            }
        }
    }
    if let Ok(es) = std::fs::read_dir(local.join(r"Mozilla\Firefox\Profiles")) {
        for e in es.flatten() {
            v.push(e.path().join("cache2"));
        }
    }
    v
}

fn mb(b: u64) -> String {
    format!("{:.1} MB", b as f64 / 1_048_576.0)
}

fn is_admin() -> bool {
    Command::new("net")
        .arg("session")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_silent(cmd: &str, args: &[&str]) {
    let _ = Command::new(cmd)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

/// Ejecuta un comando y devuelve (éxito, líneas útiles de stdout+stderr).
/// Quita los NUL de salidas UTF-16 (sfc) y las barras de progreso (winget, dism).
fn run_status(cmd: &str, args: &[&str]) -> (bool, Vec<String>) {
    match Command::new(cmd)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push('\n');
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            let lines = s
                .replace('\0', "")
                .split(['\n', '\r'])
                .map(|l| l.trim().to_string())
                .filter(|l| {
                    !l.is_empty()
                        && l.chars().any(|c| c.is_alphanumeric())
                        && !l.contains(['█', '▒'])
                        && l.matches('=').count() < 5 // barra [=====   ] de DISM
                })
                .collect();
            (o.status.success(), lines)
        }
        Err(e) => (false, vec![format!("Error: {e}")]),
    }
}

fn run_output(cmd: &str, args: &[&str]) -> Vec<String> {
    run_status(cmd, args).1
}

fn free_gb() -> Option<f64> {
    let out = run_output("powershell", &["-NoProfile", "-Command", "(Get-PSDrive C).Free"]);
    out.first()?.trim().parse::<f64>().ok().map(|b| b / 1_073_741_824.0)
}

/// Borra el contenido de `dir` (no la carpeta). Devuelve bytes liberados.
fn clean_dir(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let mut freed = 0;
    for e in entries.flatten() {
        let p = e.path();
        let Ok(meta) = std::fs::symlink_metadata(&p) else { continue };
        if meta.file_type().is_symlink() {
            // no seguir enlaces: borrar solo el enlace
            let _ = std::fs::remove_dir(&p).or_else(|_| std::fs::remove_file(&p));
        } else if meta.is_dir() {
            freed += clean_dir(&p);
            let _ = std::fs::remove_dir(&p);
        } else if std::fs::remove_file(&p).is_ok() {
            freed += meta.len();
        }
    }
    freed
}
