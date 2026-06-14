# AGENTS.md - Notas para retomar el proyecto

> **Lee esto primero** al empezar una sesion de trabajo. Resume el estado
> del proyecto, la arquitectura, las convenciones que hemos usado y el
> trabajo pendiente.

## Que es este proyecto

`arch-postinstall` es un asistente TUI (Rust + ratatui) para
**automatizar la post-instalacion de Arch Linux**. Esta pensado para
alguien que YA instalo Arch (con `archinstall` o a mano) y esta dentro
del sistema, sin entorno grafico todavia. Le ayuda a:

- Elegir un entorno de escritorio (KDE, GNOME, Hyprland, Qtile, ninguno)
- Elegir drivers (GPU, microcódigo de CPU)
- Marcar paquetes oficiales y del AUR con checklist
- Buscar paquetes en vivo en repos oficiales + AUR
- Configurar locale, zona horaria, teclado, hostname, multilib, huerfanos
- Guardar/cargar perfiles TOML reutilizables
- Instalar y dejar el sistema listo para reiniciar al escritorio

NO es un instalador de Arch (eso es `archinstall`). Es la
post-instalacion: el equivalente a "ahora que ya tengo Arch, dejemelo
bonito".

## Estado actual

- **Version**: `0.8.0` (en `Cargo.toml`)
- **Rama**: `main`, limpia, sin cambios sin commitear
- **Ultimo commit**: `5e9877e` (Flags de GRUB funcionales en el form del sistema)
- **Tests**: 66/66 pasan
- **Linters**: `cargo clippy --all-targets -- -D warnings` limpio,
  `cargo fmt --check` limpio
- **Binario**: `cargo build --release` produce
  `./target/release/arch-postinstall` (~16-18 s)
- **Repo remoto**: `https://github.com/Khinmmad/Script-de-instalacion`
  (el remoto local ya apunta a la URL canonica con `S` mayuscula)

## Estructura del proyecto

```
script-de-instalacion-new/
├── Cargo.toml
├── README.md
├── AGENTS.md                    <-- este archivo
└── src/
    ├── main.rs       # CLI (parse_args, run_plan), entry point, panic hook
    ├── tui.rs        # TUI: modos, draw_*, handle_*, pickers
    ├── installer.rs  # ejecucion: pacman, yay, sudo_copy_file, ensure_yay
    ├── preflight.rs  # checks pre-instalacion (red, sudo, disco, etc.)
    ├── estimate.rs   # estimacion de espacio (pacman -Si, df, AUR=unknown)
    ├── detect.rs     # SystemStatus: que hay instalado
    ├── options.rs    # listas para pickers (locales, zonas, keymaps)
    ├── update.rs     # check de actualizacion via GitHub
    ├── catalog.rs    # catalogo curado de paquetes y entornos
    ├── model.rs      # InstallPlan, Profile, format_system_settings
    ├── profile.rs    # save/load TOML
    ├── repo_api.rs   # busqueda en vivo (APIs oficiales + AUR)
    └── validate.rs   # validadores de input (hostname, locale, etc.)
```

## Convenciones del proyecto

- **Sin comentarios innecesarios**. Solo doc-comments (`///`) en
  funciones publicas, y comentarios donde la logica no es obvia.
- **Estilo**: idiomatico, `cargo fmt`, `cargo clippy -D warnings`.
- **Mensajes al usuario**: en espanol, claros, con sugerencias cuando
  algo falla.
- **Tests**: junto al codigo, en `#[cfg(test)] mod tests`. Helpers
  privados se duplican dentro del test si los del modulo superior
  chocarian con la visibilidad.
- **Logger**: el `Logger` de `installer.rs` es la fuente de verdad para
  mensajes al usuario durante la instalacion. La TUI usa
  `app.status` para mensajes cortos.
- **Seguridad**: NUNCA pasar input del usuario por una shell. Usar
  `Command::new(...).args(...)` con args como `&[&str]`. Los archivos
  temporales del instalador van via `sudo install` (helper
  `sudo_copy_file` en installer.rs).
- **Orden de instalacion** (en `installer::execute`):
  0. Pre-flight checks
  1. Habilitar [multilib] si se pidio
  2. `pacman -Syu`
  3. Setup AUR (git, base-devel, yay) - **solo si hay AUR en el plan**
  4. Paquetes AUR (yay) - **antes que los oficiales**
  5. Paquetes oficiales (pacman)
  6. Config sistema (locale, zona, teclado, hostname)
  7. Boot (microcodigo, NVIDIA)
  8. Servicios sistema
  9. Servicios usuario (audio)
  10. Limpieza de huerfanos (opt-in)

## Como trabaja el usuario

1. Lanza `./target/release/arch-postinstall`
2. Pantalla de bienvenida -> Enter -> menu principal
3. Configura cada seccion (entorno, drivers, paquetes, sistema,
   busqueda, perfiles)
4. "Instalar ahora" -> pantalla de revision
5. Revisa el plan + pre-flight
6. Enter para instalar (o `s` para salir sin hacer nada,
   `p` para re-ejecutar pre-flight, `Esc` para volver)

## Comandos utiles

```bash
# Build
cargo build
cargo build --release

# Test
cargo test
cargo test preflight       # solo los tests de un modulo
cargo test options

# Lint
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo fmt                  # arreglar formato

# Probar
./target/release/arch-postinstall                    # TUI
./target/release/arch-postinstall --help
./target/release/arch-postinstall --list-profiles
./target/release/arch-postinstall -p nombre -n -y   # dry-run con perfil

# Git
git log --oneline -20
git status
git push origin main
```

## Perfil de prueba (para smoke tests)

```bash
mkdir -p ~/.config/arch-postinstall/profiles
cat > ~/.config/arch-postinstall/profiles/test.toml <<'EOF'
name = "test"
desktop_environment = "hyprland"
display_manager = "sddm"
official_packages = ["firefox", "kitty", "paquete-inexistente-xyz"]
aur_packages = ["paquete-aur-inexistente-abc"]
EOF
./target/release/arch-postinstall --profile test --dry-run -y
```

Esto ejercita: pre-flight, AUR setup, AUR install, official install,
skip de paquetes ya presentes, sugerencias de error.

## Trabajo hecho en esta sesion

Todas las mejoras se hicieron en commits separados para que cada una
fuera revisable:

- **Refactors base** (commits previos): `sudo_copy_file`,
  `merge_profile_into`, `format_system_settings`, `format_list_or_none`,
  `validate.rs` helpers, etc.
- **Modulo `detect.rs`** (SystemStatus: paquetes, servicios, configs)
- **Modulo `preflight.rs`** (7 checks + 3 AUR)
- **Modulo `update.rs`** (GitHub releases, silencioso si falla)
- **Modulo `options.rs`** (listas para pickers desde el sistema)
- **Modulo `estimate.rs`** (download/install size con `pacman -Si` + `df`)
- **TUI**:
  - Paquetes/drivers ya instalados marcados con `✓` verde
  - Formulario de sistema pre-rellenado con valores actuales
  - Pantalla de revision: pre-flight, "Por instalar / Ya instalado",
    "Servicios a habilitar / ya activos", updates disponibles,
    "Espacio: descargar X, instalar Y, libre Z (ok/no-cabe)"
  - Pickers buscables para locale/zona/teclado/mirror (Enter abre, `(Personalizado...)` para valor fuera de la lista)
  - Menu de mirrors: ~60 paises, `reflector --country` se aplica antes de `-Syu`
  - Form del sistema: GRUB_TIMEOUT, GRUB_DEFAULT=saved, GRUB_GFXMODE=auto
  - `s` en revision = salir sin hacer nada
  - `p` en revision = re-ejecutar pre-flight
  - `u` global = descartar aviso de actualizacion
  - `q` en menu principal con plan no vacio = confirmacion (doble pulsacion)
  - Resumen en menu: "X por instalar, Y ya en sistema"
- **Instalador**:
  - Orden AUR-primero
  - Skip de paquetes ya instalados (con log explicito)
  - Captura stderr y sugerencias contextuales por programa
  - Panic hook con mensaje claro
  - Backup de /etc antes de cada modificacion (`*.arch-postinstall.bak`)
  - Aplicar mirror selection via reflector (instala reflector si falta)
  - Ajustes funcionales de GRUB (GRUB_TIMEOUT / saved / gfxmode)
  - Snapshot BTRFS+snapper pre-instalacion (rollback)
  - Post-install hooks del perfil (`sh -c <cmd>`)
- **CLI**:
  - `show_plan` muestra estimacion, mirrors, post-install, grub en texto plano
  - `--validate-profile <PATH>` para CI (exit 0/1 segun validez)

## Trabajo pendiente / ideas

- **Per-package notes** en perfiles (overkill?)
- **Mouse support** en la TUI (overkill?)
- **`--quiet` / JSON output** para CI
- **Wallpaper / tema por DE** (overkill)

## Notas personales del agente

- El usuario habla espanol, responde en espanol. Mensajes del
  programa tambien en espanol.
- El usuario es detallista: le importa el orden de los pasos, los
  mensajes de error, y los edge cases. Antes de commitear, verificar
  que el codigo maneja los casos raros.
- Despues de cada cambio: `cargo test` + `cargo clippy -D warnings` +
  `cargo fmt --check`. El usuario espera CI limpio.
- Si hay un fallo raro, probar end-to-end con un perfil y
  `--dry-run -y` para ver el output completo.
