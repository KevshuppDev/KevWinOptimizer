# Optimizador de Windows (Rust)

App de escritorio en Rust + egui/eframe 0.29 (un solo `.exe`, arranque instantáneo). Nació juntando `Optimizar.bat` y `Reiniciar Red.bat`; hoy es un "WinUtil" propio con estilo **pixel art**. Todo el código está en `src/main.rs`.

## Compilar y probar
- Toolchain `stable-x86_64-pc-windows-gnu` (Rust 1.100, rustup). Necesita **WinLibs** (MinGW 64 bits, UCRT, POSIX) extraído en `C:\mingw64` (para `dlltool` y `windres`). No usar el MinGW de mingw.org (32 bits).
- Compilar: PATH = `C:\mingw64\bin;%USERPROFILE%\.cargo\bin`, luego `cargo build --release`.
- `Escritorio\Instalar Optimizador.bat` hace lo mismo y copia `Optimizador.exe` al escritorio; se cierra solo si todo sale bien (deja la ventana abierta si falla la compilación o la copia).
- Flujo acordado: tras cada cambio, Claude compila, cierra el `Optimizador` abierto, copia el `.exe` al escritorio y lo abre.
- Icono: `assets/icon.ico` (monitor pixel art 16x16, varios tamaños) incrustado con `build.rs` + `embed-resource`; el icono de ventana se genera del mismo sprite (`SPR_ICON`).
- Se auto-eleva a administrador al abrir (`net session` + `Start-Process -Verb RunAs`), así que cada apertura pide UAC.
- Versión en `Cargo.toml` (`VERSION` = `CARGO_PKG_VERSION`, se muestra en la cabecera). Actual: **1.0.1**.

## GitHub y releases
- Repo: https://github.com/KevshuppDev/KevWinOptimizer (licencia MIT, README en español). Git instalado en `C:\Program Files\Git`; credenciales con Git Credential Manager.
- `.github/workflows/release.yml`: al subir un tag `v*` compila en `windows-latest` (toolchain **MSVC**, no GNU) y publica `KevWinOptimizer.exe` en la Release.
- Para sacar versión: subir `version` en `Cargo.toml`, commit, `git tag vX.Y.Z` y `git push origin vX.Y.Z`.
- **Actualizador**: al abrir, consulta `releases/latest` con `ureq` (en Rust, sin PowerShell); si hay versión mayor muestra el botón `NUEVA VERSIÓN vX` que abre la página de la release en el navegador.

## Diseño
- Paleta retro de 16 colores, fuente monoespaciada, sin esquinas redondeadas, bordes gruesos y sombras duras.
- Widgets propios: `pixel_button`, `pixel_small`, `pixel_tab`, `pixel_check` (con etiqueta e información al pasar el cursor), `app_chip` (fichas compactas de apps), `pixel_progress` (barra de celdas de 4 px con brillo y porcentaje).
- Cabecera con sprite de monitor (parpadea mientras trabaja) + pestañas. Barra de estado abajo con progreso, tiempo y resumen.
- **Registro** plegable a la derecha (botón `REGISTRO ▸`, muestra líneas nuevas sin leer; la ventana crece/encoge 380 px).

## Pestañas
- **LIMPIEZA**: 15 tareas con casillas (temporales, caché de Windows Update, optimización de entrega, WER, miniaturas, navegadores, papelera, DNS, servicios de Intel, TRIM, DISM, logs de Windows, cachés de programación, instaladores de drivers). Muestra MB liberados por tarea y espacio libre antes/después.
- **RED**: red rápida (flushdns, release, reinicio de adaptadores, renew, ping) y completa (winsock + TCP/IP, pide reiniciar). DNS: Cloudflare / Google / automático.
- **PRIVACIDAD** y **RENDIMIENTO → TWEAKS**: tweaks tipo WinUtil (telemetría, Copilot/Recall, Bing, anuncios, Ultimate Performance, hibernación, Game DVR, efectos visuales, extensiones, ocultos, modo oscuro, finalizar tarea, Modo Juego, HAGS, ratón 1:1, menú clásico, Widgets).
  - Cada tweak tiene script de aplicar, deshacer y una **comprobación** (`PRIVACY_CHECK` / `PERF_CHECK`, en el mismo orden que las listas).
  - La casilla = estado deseado. Etiquetas: ACTIVO / APLICAR / DESHACER. Un solo botón "Aplicar cambios (N)"; crea punto de restauración antes de aplicar.
- **RENDIMIENTO → INICIO**: programas que arrancan con Windows (Run de HKCU/HKLM/WOW64 y carpetas de Inicio); se activan/desactivan con las claves `StartupApproved`, igual que el Administrador de tareas.
- **RENDIMIENTO → SERVICIOS**: packs de servicios (innecesarios de Windows, actualizadores, Intel, Xbox, impresión, Oracle, SQL Server, MySQL/MariaDB, PostgreSQL, MongoDB, Apache, Docker, VMs, VPNs, launchers). Iniciar / Detener / "Arrancar con Windows". Detener = parar + Manual (nunca Desactivado). Solo se muestran packs con servicios presentes.
- **REPARAR**: punto de restauración, SFC, DISM RestoreHealth, reiniciar Windows Update, chkdsk /scan.
- **APPS → INSTALAR**: ~120 apps por categorías (IA y agentes, programación base/lenguajes/herramientas, bases de datos, navegadores, comunicación, multimedia, oficina, utilidades, juegos, runtimes para juegos, drivers, hardware). Todos los IDs se verificaron con `winget show`.
  - Prefijos de id: normal = winget; sin punto = Microsoft Store (`--source msstore`); `vscode:` = extensión de VS Code; `custom:` = instalador propio (Oracle XE 11g desde Google Drive, no oficial); `web:` = abre la web (AMD).
  - Packs (alternan marcar/quitar): Personal, Rust, Web, IA, Oracle, Gamer.
  - **Buscador**: filtra por nombre, categoría, id o descripción (varias palabras, sin importar tildes ni mayúsculas); "todas" de cada categoría solo afecta a lo visible.
  - Detecta lo instalado: `winget export` + ejecutables en el PATH y carpetas típicas (portables como `C:\mingw64`, `C:\xampp`) + paquetes AppX + extensiones de VS Code. Lo instalado sale bloqueado como INSTALADA.
  - Detecta GPU/CPU y marca con ★ el driver recomendado (este PC: Intel HD 620 / i5-7300U → Intel DSA).
- **APPS → QUITAR BLOATWARE**: 34 apps preinstaladas conocidas; solo muestra las presentes, con etiqueta RECOMENDADO para las seguras.

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
- Ideas sin hacer: limpieza de Prefetch, caché de fuentes e iconos, Windows.old, shaders; servicios a manual estilo WinUtil; reserva de almacenamiento; archivo de paginación.
