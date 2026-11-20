# Optimizador de Windows (Rust)

App de escritorio en Rust + egui/eframe 0.29 (un solo `.exe`, arranque instantáneo). Nació juntando `Optimizar.bat` y `Reiniciar Red.bat`; hoy es un "WinUtil" propio con estilo **pixel art**.
- Código: `src/main.rs` (datos, App, tareas), `src/ui.rs` (paleta, sprites, widgets), `src/fondo.rs` (config, bandeja, programador), `src/sistema.rs` (pestaña SISTEMA), `src/actualizar.rs` (APPS → ACTUALIZAR y exportar/importar), `src/energia.rs` (RENDIMIENTO → ENERGÍA).

## Compilar y probar
- Toolchain `stable-x86_64-pc-windows-gnu` (Rust 1.100, rustup). Necesita **WinLibs** (MinGW 64 bits, UCRT, POSIX) extraído en `C:\mingw64` (para `dlltool` y `windres`). No usar el MinGW de mingw.org (32 bits).
- Compilar: PATH = `C:\mingw64\bin;%USERPROFILE%\.cargo\bin`, luego `cargo build --release`.
- `Escritorio\Instalar Optimizador.bat` hace lo mismo y copia `Optimizador.exe` al escritorio; se cierra solo si todo sale bien (deja la ventana abierta si falla la compilación o la copia).
- Flujo acordado: tras cada cambio, Claude compila, cierra el `KevWinOptimizer` abierto, copia el `.exe` al escritorio y lo abre.
- Icono: `assets/icon.ico` (monitor pixel art 16x16, varios tamaños) incrustado con `build.rs` + `embed-resource`; el icono de ventana se genera del mismo sprite (`SPR_ICON`).
- Se auto-eleva a administrador al abrir con la API de Windows (`IsUserAnAdmin` + `ShellExecuteW` "runas"), así que cada apertura pide UAC. El exe lleva VERSIONINFO (assets/app.rc).
- Versión en `Cargo.toml` (`VERSION` = `CARGO_PKG_VERSION`, se muestra en la cabecera). Actual: **1.2.0**.

## GitHub y releases
- Repo: https://github.com/KevshuppDev/KevWinOptimizer (licencia MIT, README en español). Git instalado en `C:\Program Files\Git`; credenciales con Git Credential Manager.
- `.github/workflows/release.yml`: al subir un tag `v*` compila en `windows-latest` (toolchain **MSVC**, no GNU) y publica `KevWinOptimizer.exe` en la Release.
- Para sacar versión: subir `version` en `Cargo.toml`, commit, `git tag vX.Y.Z` y `git push origin vX.Y.Z`.
- **Actualizador**: al abrir, consulta `releases/latest` con `ureq` (en Rust, sin PowerShell); si hay versión mayor muestra el botón `NUEVA VERSIÓN vX` que abre la página de la release en el navegador.

## Diseño
- Paleta retro de 16 colores, fuente monoespaciada, sin esquinas redondeadas, bordes gruesos y sombras duras.
- Widgets propios: `pixel_button`, `pixel_small`, `pixel_tab`, `pixel_check` (con etiqueta e información al pasar el cursor), `app_chip` (fichas compactas de apps), `pixel_progress` (barra de celdas de 4 px con brillo y porcentaje).
- Cabecera con sprite de monitor (parpadea mientras trabaja) + pestañas. Barra de estado abajo con progreso, tiempo y resumen.
- **Registro**: panel inferior plegable y redimensionable (`TopBottomPanel::bottom("log")`, encima de la barra de estado; botón `REGISTRO ▴` con líneas sin leer). No cambia el tamaño de la ventana (antes era lateral y la ensanchaba 380 px: fallaba en pantalla partida). Se abre solo al empezar una tarea, muestra una línea viva (spinner + segundos) mientras trabaja y, si está cerrado, la barra de estado enseña la última línea.

## Pestañas
- **Modo avanzado = 7 pestañas** (`TABS`): LIMPIAR (TAREAS / ESPACIO EN DISCO / ARCHIVOS GRANDES), AJUSTES (`Tab::Rendimiento`, subpestañas `PERF_VIEWS`: PRIVACIDAD=4 / TWEAKS=0 / INICIO / SERVICIOS / ENERGÍA), APPS, RED, REPARAR (HERRAMIENTAS / DIAGNÓSTICO / DRIVERS), SISTEMA (EQUIPO / BATERÍA / RESPALDO), CONFIG (GENERAL / HISTORIAL). `sistema::State::ui(ui, ctx, views)` pinta solo las subpestañas que se le pasan (vacío = sin barra; LIMPIAR y REPARAR fijan `sys.view` y la llaman así). El modo simple (MI EQUIPO) usa `SYS_VIEWS_SIMPLE` con todo. Acciones de diagnóstico/drivers/respaldo: `App::sys_actions`.
- **Buscador global** en la cabecera (solo modo avanzado, `search_hits`): busca en tareas, tweaks, servicios, reparaciones, paneles, apps y secciones fijas; IR (o Enter) lleva a la pestaña/subpestaña (en apps rellena el buscador de APPS).
- **Un clic**: "Un clic: recomendada" en LIMPIAR (defaults sin mirar casillas) y "Un clic: recomendados (N)" en PRIVACIDAD/TWEAKS (aplica los `default` que falten, no deshace nada).
- **Historial** (`%APPDATA%\KevWinOptimizer\historial.txt`, máx. 200 líneas `fecha|tarea|resumen|+a;b/-c`): se escribe al terminar cada tarea; las de tweaks tienen DESHACER (`tweak_revert`).
- **Bandeja**: al pasar el ratón muestra CPU % (hilo con `GetSystemTimes` cada 2 s) y RAM (`GlobalMemoryStatusEx`). Sin temperatura (necesitaría WMI/PowerShell).
- **ARCHIVOS GRANDES** (sistema view 6): los 80 archivos > 100 MB en Descargas / usuario / C:, botón MOSTRAR (`explorer /select,`). No borra nada.
- **Nombre**: KevWinOptimizer en todas partes (paquete `kevwinoptimizer`, exe `KevWinOptimizer.exe`). Si la tarea de inicio apunta a otro exe, se rehace sola al abrir (`fix_autostart_path`).
- **Modo simple** (`src/simple.rs`, `simple=1` en config, por defecto activado): pestañas MI PC, APPS (completa), MI EQUIPO y CONFIG. MI PC tiene tarjetas seguras: limpiar (`SAFE_CLEAN`), privacidad (`SAFE_PRIVACY`), acelerar (`SAFE_PERF`), arreglar internet, reparar (punto de restauración + SFC + DISM). Botón MODO AVANZADO/SIMPLE en la cabecera.
- **LIMPIEZA**: 20 tareas (17 = Windows.old vía `cleanmgr /sagerun:77` con StateFlags0077; 18 = caché de fuentes parando `FontCache`; 19 = puntos de restauración viejos, deja el último; las tres desmarcadas por defecto). Antes: con casillas (temporales, caché de Windows Update, optimización de entrega, WER, miniaturas, navegadores, papelera, DNS, servicios de Intel, TRIM, DISM, logs de Windows, cachés de programación, instaladores de drivers, Prefetch (desmarcada por defecto), caché de shaders (D3DSCache, NVIDIA/AMD DXCache-GLCache-VkCache, Intel ShaderCache)). Muestra MB liberados por tarea y espacio libre antes/después.
- **RED**: red rápida (flushdns, release, reinicio de adaptadores, renew, ping) y completa (winsock + TCP/IP, pide reiniciar). DNS: Cloudflare / Google / automático.
  - **CONEXIONES Y VPN** (`src/red.rs`): lista los adaptadores (`Get-NetAdapter`, `AdminStatus`) con ACTIVAR/DESACTIVAR (`netsh interface set interface ... admin=`), VPN primero. Las VPN arrancan/paran su servicio (`VPN_SVC`: Radmin = `RvControlSvc`). Cloudflare WARP va aparte: inicia el servicio `CloudflareWARP` y usa `warp-cli connect/disconnect`.
- **PRIVACIDAD** y **RENDIMIENTO → TWEAKS**: tweaks tipo WinUtil (telemetría, Copilot/Recall, Bing, anuncios, Ultimate Performance, hibernación, Game DVR, efectos visuales, extensiones, ocultos, modo oscuro, finalizar tarea, Modo Juego, HAGS, ratón 1:1, menú clásico, Widgets).
  - Cada tweak tiene script de aplicar, deshacer y una **comprobación** (`PRIVACY_CHECK` / `PERF_CHECK`, en el mismo orden que las listas).
  - La casilla = estado deseado. Etiquetas: ACTIVO / APLICAR / DESHACER. Un solo botón "Aplicar cambios (N)"; crea punto de restauración antes de aplicar.
- **RENDIMIENTO → INICIO**: programas que arrancan con Windows (Run de HKCU/HKLM/WOW64 y carpetas de Inicio); se activan/desactivan con las claves `StartupApproved`, igual que el Administrador de tareas.
- **RENDIMIENTO → SERVICIOS**: packs de servicios (innecesarios de Windows, actualizadores, Intel, Xbox, impresión, Oracle, SQL Server, MySQL/MariaDB, PostgreSQL, MongoDB, Apache, Docker, VMs, VPNs, launchers). Iniciar / Detener / "Arrancar con Windows". Detener = parar + Manual (nunca Desactivado). Solo se muestran packs con servicios presentes.
- **REPARAR**: punto de restauración, SFC, DISM RestoreHealth, reiniciar Windows Update, chkdsk /scan, reparar Microsoft Store, reparar winget, reconstruir caché de iconos. Debajo, PANELES CLÁSICOS (`PANELS`: Panel de control, Red, Energía, Sonido, Sistema, Usuarios/netplwiz, Programas, Características, Servicios, Discos).
- **Presets de tweaks** (PRIVACIDAD y RENDIMIENTO → TWEAKS): MÍNIMO (`PRIV_MIN`, `PERF_MIN`) / ESTÁNDAR (los `default: true`; incluye Modo oscuro) / MÁXIMO (marca todos), NINGUNO y COMO ESTÁ. El preset deja exactamente su selección (lo activo que no esté sale como DESHACER). LIMPIEZA tiene RÁPIDA (`CLEAN_FAST`) / ESTÁNDAR / PROFUNDA (todo) / NINGUNA. En esas listas `check_list(..., buttons=false)` oculta los botones Todas/Ninguna/Recomendadas/Sin cambios. Tweak nuevo en PRIVACIDAD: "Domar Microsoft Edge" (directivas en `HKLM\SOFTWARE\Policies\Microsoft\Edge`). También "Domar Brave" (directivas en `HKLM\SOFTWARE\Policies\BraveSoftware\Brave`) y "Bloquear telemetría de Adobe" (líneas `# KWO-ADOBE` en hosts; solo dominios de estadísticas, **no** los de licencias/activación). En TWEAKS: "Hora UTC (dual boot con Linux)" (`RealTimeIsUniversal`). Desinstalar Edge no se hizo (Microsoft lo bloquea fuera de la UE y rompe WebView2).
- **APPS → INSTALAR**: ~120 apps por categorías (IA y agentes, programación base/lenguajes/herramientas, bases de datos, navegadores, comunicación, multimedia, oficina, utilidades, juegos, runtimes para juegos, drivers, hardware). Todos los IDs se verificaron con `winget show`.
  - Prefijos de id: normal = winget; sin punto = Microsoft Store (`--source msstore`); `vscode:` = extensión de VS Code; `custom:` = instalador propio (Oracle XE 11g desde Google Drive, no oficial); `web:` = abre la web (AMD).
  - Portables con interfaz (`PORTABLE_GUI`, hoy solo `Oracle.SQLDeveloper`): winget descomprime el zip en `%LOCALAPPDATA%\Microsoft\WinGet\Packages` sin acceso directo; `portable_shortcuts` lo crea en el menú Inicio y el escritorio tras instalar y al detectar apps (si falta el del menú Inicio).
  - Barra de progreso: winget no emite progreso sin consola, así que dentro de cada paso la barra avanza sola por tiempo (`step_at`, curva que frena hacia el 92 % del paso).
  - Packs (alternan marcar/quitar): Personal, Rust, Web, IA, Oracle, Gamer.
  - **Buscador**: filtra por nombre, categoría, id o descripción (varias palabras, sin importar tildes ni mayúsculas); "todas" de cada categoría solo afecta a lo visible.
  - Detecta lo instalado: `winget export` + ejecutables en el PATH y carpetas típicas (portables como `C:\mingw64`, `C:\xampp`) + paquetes AppX + extensiones de VS Code. Lo instalado sale con ✔ verde.
  - **Desinstalar**: clic en una instalada la marca en naranja con ✖ (`sel` = acción pendiente: instalar si no está, desinstalar si está). El botón pasa a `Desinstalar (N)` / `Aplicar (+a / -d)` y `apply_apps` primero quita (`winget uninstall`, `code --uninstall-extension`) y luego instala. `custom:` (Oracle XE) no se quita solo: abre `appwiz.cpl`. Packs, "todas" e importar no tocan las instaladas.
  - Lo instalado se vuelve a detectar con el botón RECARGAR (junto al buscador) y al volver el foco a la ventana estando en APPS (como mucho cada 15 s, `apps_at` / `was_focused`).
  - Detecta GPU/CPU y marca con ★ el driver recomendado (este PC: Intel HD 620 / i5-7300U → Intel DSA).
- **APPS → QUITAR BLOATWARE**: 34 apps preinstaladas conocidas; solo muestra las presentes, con etiqueta RECOMENDADO para las seguras.

- **APPS → ACTUALIZAR**: lee la tabla de `winget upgrade --include-unknown` (columnas por posición de la cabecera, vale en cualquier idioma; los ids recortados con `…` se completan con `winget export`). Se actualizan las marcadas o todo.
- **APPS → exportar/importar**: `Documentos\KevWinOptimizer-apps.txt` (un id por línea, instaladas + marcadas). Importar = botón o arrastrar el archivo a la ventana.
- **RENDIMIENTO → ENERGÍA**: planes de energía (`powercfg /list`, salida en CP850), archivo de paginación (automático o fijo RAM / 2×RAM, pide reiniciar) y almacenamiento reservado (`Set-WindowsReservedStorageState`). Los planes también salen en un submenú de la bandeja.
- **SISTEMA**: EQUIPO (modelo, Windows, CPU, GPU, RAM, discos con salud/desgaste/temperatura, volúmenes, tiempo encendido), ESPACIO EN DISCO (analizador por carpetas, guarda 5 niveles; más hondo vuelve a analizar; no sigue uniones) y BATERÍA (salud, ciclos, `powercfg /batteryreport`).
- **SISTEMA → DIAGNÓSTICO** (`src/diagnostico.rs`, subpestañas 3 y 4 de `sistema::State::view`; también en MI EQUIPO del modo simple): 10 comprobaciones (`CHECKS`: internet/DNS, dispositivos con error, cola de impresión, audio, Windows Update desactivado, servicio de hora, Defender en tiempo real, búsqueda, espacio < 10 %, reinicio pendiente). Cada una tiene test (`OK` / `FAIL|detalle`) y arreglo; botón ARREGLAR por fila y ARREGLAR TODO.
- **SISTEMA → DRIVERS** (mismo archivo): dispositivos con error (Get-PnpDevice), BUSCAR DRIVERS (`pnputil /scan-devices` + Windows Update → actualizaciones opcionales), impresoras instaladas, MIS DRIVERS (`MY_DRIVERS`, botón INSTALAR que descarga con `ureq` a `Documentos\KevWinOptimizer-drivers`: Epson L3210 = `L3210_Lite_LA.exe` de ftp.epson.com, se abre visible; Xerox Phaser 3320 = zip del Global Print Driver PCL6, busca la versión más nueva en la web de Xerox, se descomprime con `tar` y se instala con `pnputil /add-driver` + `Add-PrinterDriver`) y copia de seguridad con `pnputil /export-driver` a `Documentos\KevWinOptimizer-drivers`. Impresoras: ★ predeterminada, PRUEBA (`PrintTestPage`), PREDETERMINADA (`SetDefaultPrinter` + `LegacyDefaultPrinterMode=1`) y QUITAR COPIAS (las "(Copia N)" cuya original existe).
- **SISTEMA → RESPALDO** (subpestaña 5, "antes de formatear"): crea `Documentos\KevWinOptimizer-respaldo-FECHA` con apps.txt (`actualizar::list_text`), wifi (`netsh wlan export profile key=clear`), windows.txt (clave OA3 de la BIOS), impresoras.txt y drivers. RESTAURAR: añade las Wi-Fi, instala drivers con pnputil y marca las apps en APPS.
- **CONFIG** (`src/fondo.rs`): config en `%APPDATA%\KevWinOptimizer\config.txt`.
  - Icono de bandeja (`tray-icon`) con el menú Abrir / Limpiar ahora / Salir. Si "Cerrar a la bandeja" está activo, la X oculta la ventana (Win32 `ShowWindow`).
  - "Abrir al encender": tarea `KevWinOptimizer` del Programador de tareas (AtLogOn, RunLevel Highest, sin UAC) que arranca con `--tray`, oculta en la bandeja.
  - Limpieza programada: cada 1/3/7/14/30 días a partir de una hora, con tareas propias. La corre un hilo de fondo (aunque la ventana esté oculta) y su salida se vuelca al registro.

## Lecciones aprendidas (bugs ya resueltos)
- Los scripts de PowerShell se pasan por línea de comandos: **no usar comillas dobles** dentro (se pierden). Solo comillas simples, `-f` y `Join-Path`.
- Dentro de `if (...)` de PowerShell no puede ir `;` → las comprobaciones se envuelven en `& { ... }`.
- No borrar `.cargo\registry\src` ni `.cargo\git\checkouts`: rompe proyectos ya compilados (cargo no re-extrae). Solo `registry\cache`. Si pasa: `cargo fetch`.
- Quitar AppX: primero para el usuario actual, luego `-AllUsers` (suele fallar con 0x80070002 si no) y luego `Remove-AppxProvisionedPackage`. Para detectar, contar solo `InstallState = Installed` (no `Staged`).
- `winget list` recorta IDs; usar `winget export` a un JSON en `%TEMP%`.
- La salida de `sfc` es UTF-16: se quitan los `\0`. Las barras de progreso de winget/DISM se filtran.
- Intel DSA choca con la limpieza "Servicios de Intel" y el pack de servicios de Intel (los desactivan).
- **Defender** marcó el exe como `Behavior:Win32/DefenseEvasion.A!ml` (falso positivo por IA). Lo agravó un auto-actualizador que descargaba el exe con PowerShell oculto y se reemplazaba solo: **no volver a hacer eso** ni lanzar PowerShell oculto con `-ExecutionPolicy Bypass` para ir a internet. El exe nuevo aún lo detecta al compilar; pendiente reportarlo en https://www.microsoft.com/wdsi/filesubmission.

## Cambios ya aplicados al sistema (fuera de la app)
- Servicios de Intel desactivados: `DSAService`, `ESRV_SVC_QUEENCREEK`, `SystemUsageReportSvc_QUEENCREEK`, `USER_ESRV_SVC_QUEENCREEK`.
- Inicio automático desactivado (StartupApproved) para Adobe Acrobat Synchronizer y Edge AutoLaunch.
- Quitados: Outlook (nuevo), Teams (personal) y Obtener ayuda.

## Pendiente / ideas
- Borrar la Release v1.0.0 vieja (exe con el actualizador peligroso) y relanzar el workflow; hacer público el repo.
- Comprobar que Contactos y Wallet se quitan con el nuevo orden de borrado.
- Probar en la práctica la instalación de Oracle XE 11g y de las apps de la Store.
- Servicios: pack "A manual (estilo WinUtil)" añadido (no recomendado por defecto).
- Ideas sin hacer: archivos duplicados, temperatura en la bandeja.
- Probar en la app: batería (solo se lee como admin), analizador en C:, cambio de paginación.
- 15-11-2026: Defender marcó `Trojan:Win32/Bearfoos.A/B!ml` al compilar (archivo, no comportamiento). Se arregló quitando la autoelevación con PowerShell (`Start-Process -Verb RunAs`), `net session` y `-ExecutionPolicy Bypass`, y añadiendo VERSIONINFO; el exe quedó limpio con `MpCmdRun -Scan -ScanType 3 -File`. No usar manifiesto propio: choca con el `asInvoker` que mete rustc (".rsrc merge failure").
