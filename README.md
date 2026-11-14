# KevWinOptimizer

Optimizador de Windows todo en uno, escrito en **Rust + egui/eframe**, con estética **pixel art**. Un solo `.exe`, arranque instantáneo, sin instalación.

## Funciones

- **Limpieza**: 15 tareas (temporales, caché de Windows Update, navegadores, papelera, DNS, logs, cachés de programación, DISM, TRIM...). Muestra los MB liberados por tarea y el espacio libre antes y después.
- **Red**: reinicio rápido (flushdns, release/renew, adaptadores) y completo (winsock + TCP/IP). Cambio de DNS: Cloudflare, Google o automático.
- **Privacidad**: telemetría, Copilot/Recall, Bing en el menú Inicio, anuncios...
- **Rendimiento**
  - *Tweaks*: Ultimate Performance, Game DVR, efectos visuales, HAGS, ratón 1:1, menú contextual clásico, modo oscuro... Cada tweak detecta su estado actual y se puede deshacer.
  - *Inicio*: activar/desactivar los programas que arrancan con Windows (igual que el Administrador de tareas).
  - *Servicios*: packs de servicios (Xbox, Intel, actualizadores, bases de datos, VMs, VPNs...) para iniciarlos, detenerlos o decidir si arrancan con Windows.
- **Reparar**: punto de restauración, SFC, DISM RestoreHealth, reinicio de Windows Update, chkdsk.
- **Apps**
  - *Instalar*: ~120 apps por categorías con **winget**, Microsoft Store y extensiones de VS Code, con packs predefinidos (Personal, Rust, Web, IA, Oracle, Gamer). Detecta lo que ya tienes instalado y te recomienda el driver según tu GPU/CPU.
  - *Quitar bloatware*: elimina apps preinstaladas y marca cuáles son seguras de quitar.

Antes de aplicar tweaks se crea un punto de restauración.

## Requisitos

- Windows 10/11
- Permisos de administrador (la app los pide al abrirse)
- [winget](https://learn.microsoft.com/windows/package-manager/winget/) para instalar apps

## Compilar

Toolchain `stable-x86_64-pc-windows-gnu` + [WinLibs](https://winlibs.com/) (MinGW-w64 UCRT, POSIX) extraído en `C:\mingw64`.

```powershell
$env:Path = "C:\mingw64\bin;$env:USERPROFILE\.cargo\bin;" + $env:Path
cargo build --release
```

El ejecutable queda en `target\release\Optimizador.exe`.

## Aviso

Esta herramienta modifica el registro, los servicios y las apps del sistema. Úsala bajo tu propia responsabilidad y crea un punto de restauración antes de hacer cambios grandes.

## Licencia

[MIT](LICENSE) © 2026 KevshuppDev
