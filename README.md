# Arch Post-Install

Asistente **TUI** (interfaz de terminal) escrito en **Rust** para automatizar la
post-instalación de Arch Linux: elige tu entorno de escritorio y los paquetes
(oficiales y del AUR) con un menú interactivo, guarda perfiles reutilizables y
deja un log de todo lo que se instaló.

> Este proyecto era originalmente un script de Python incompleto. Se reescribió
> en Rust: se compila a un **único binario sin dependencias de runtime** (no
> necesita Python ni librerías externas para ejecutarse).

## Características

- 🖥️ **Menú estilo `archinstall`**: una pantalla principal navegable por teclado
  desde la que configuras cada sección y luego pulsas "Instalar ahora".
- 🔎 **Buscador en vivo de cualquier paquete**: busca en los **repositorios
  oficiales** y en el **AUR** usando sus APIs oficiales y añade lo que quieras,
  no estás limitado a una lista fija (como hacen `yay`/`paru`).
- 📦 **Selección interactiva de paquetes** oficiales y del AUR con checklist.
- 🎨 **Entornos de escritorio**: KDE Plasma, GNOME, Hyprland, Qtile o ninguno.
- 🎮 **Controladores (drivers)**: elige tu GPU (NVIDIA propietario/open, AMD,
  Intel, nouveau, máquina virtual) y el microcódigo de tu CPU (Intel/AMD).
- ⚙️ **Deja el sistema listo para usar**: según lo que elijas, habilita
  automáticamente los servicios — el **display manager** correcto (sddm/gdm/
  lightdm), **NetworkManager**, **Bluetooth** y el **audio** (PipeWire). Arrancas
  y ya estás en tu escritorio.
- 🤖 **Instalación automática de `yay`**: lo compila desde el AUR si no está.
- 🛡️ **Manejo robusto de errores**: los paquetes se instalan uno por uno, así un
  fallo no aborta el resto. Al final ves un resumen de éxitos y fallos.
- 📝 **Log con marca de tiempo** en `~/.local/state/arch-postinstall/`.
- 💾 **Perfiles guardables/cargables** desde el propio menú, para reproducir tu
  setup en otra máquina.

> El buscador necesita conexión a internet (consulta `archlinux.org` y
> `aur.archlinux.org`). El resto del catálogo curado funciona sin buscar.

## Requisitos

- Arch Linux (o derivado con `pacman`).
- Conexión a internet.
- Toolchain de Rust para compilar: `rustup`/`cargo`
  (`sudo pacman -S rustup && rustup default stable`).

## Instalación

**1. Clona el repositorio:**

```bash
git clone https://github.com/Khinmmad/Script-de-instalacion
cd Script-de-instalacion
```

**2. Compila en modo release:**

```bash
cargo build --release
```

El binario queda en `./target/release/arch-postinstall`.

**3. Ejecútalo:**

```bash
./target/release/arch-postinstall
```

> Opcional: para llamarlo por su nombre desde cualquier sitio, copia el binario
> a tu PATH, p. ej. `cp target/release/arch-postinstall ~/.local/bin/`.

> No lo corras como `root`. El programa usa `sudo` cuando hace falta.

## Uso

### Asistente interactivo (por defecto)

```bash
./target/release/arch-postinstall
```

Primero verás una **pantalla de bienvenida**; pulsa Enter para entrar al
**menú principal** (estilo `archinstall`). Desde ahí entras a cada sección, la
configuras y vuelves al menú:

- **Entorno de escritorio** — elige uno (KDE, GNOME, Hyprland, Qtile o ninguno).
- **Controladores (drivers)** — marca tu GPU y el microcódigo de tu CPU; puedes
  elegir varios (p. ej. Intel + NVIDIA en portátiles híbridos).
- **Paquetes oficiales / Paquetes AUR** — marca/desmarca con la barra espaciadora.
- **Buscar y añadir paquetes** — busca en vivo en los repos oficiales o el AUR
  (Tab cambia la fuente) y añade cualquier paquete a tu selección.
- **Cargar / Guardar perfil** — gestiona tus perfiles sin salir del programa.
- **Instalar ahora** — muestra una **pantalla de revisión** con el plan completo
  (entorno, display manager, paquetes y **servicios que se habilitarán**);
  confirmas con Enter y se instala.

Tras instalar los paquetes, el programa habilita los servicios necesarios con
`systemctl` para que el equipo arranque listo: el display manager del entorno
elegido, NetworkManager, Bluetooth (si instalaste `bluez`) y el audio de
PipeWire. Un paso que falle queda registrado pero no aborta el resto.

### Línea de comandos

```bash
# Instalar directamente desde un perfil guardado
arch-postinstall --profile mi-setup

# Ver qué haría sin ejecutar nada (seguro para probar)
arch-postinstall --profile mi-setup --dry-run

# Sin preguntas (usa --noconfirm en pacman/yay)
arch-postinstall --profile mi-setup --yes

# Listar perfiles guardados
arch-postinstall --list-profiles

# Ayuda
arch-postinstall --help
```

### Controles de la TUI

| Tecla             | Acción                                            |
| ----------------- | ------------------------------------------------- |
| `↑`/`↓` o `k`/`j` | Mover el cursor                                   |
| `Home`/`End`      | Saltar al inicio / final de la lista              |
| `PgUp`/`PgDn`     | Saltar 10 elementos                               |
| `Enter`           | Abrir sección / confirmar / "Instalar ahora"      |
| `Espacio`         | Marcar/desmarcar paquete o entorno                |
| `q` o `Esc`       | Volver al menú (o salir desde el menú)            |
| `Tab`             | (Buscador) cambiar entre repos oficiales y AUR    |
| `i` o `/`         | (Buscador) editar el término de búsqueda          |

## Estructura del proyecto

```
Script-de-instalacion/
├── Cargo.toml
└── src/
    ├── main.rs       # CLI, orquestación y argumentos
    ├── tui.rs        # Menú interactivo estilo archinstall (ratatui)
    ├── repo_api.rs   # Buscador en vivo (APIs oficiales + AUR)
    ├── catalog.rs    # Catálogo curado de paquetes y entornos
    ├── model.rs      # Estructuras de datos y perfiles
    ├── profile.rs    # Guardar/cargar perfiles (TOML)
    └── installer.rs  # Ejecución (pacman/yay), logging y resumen
```

## Personalización

Edita `src/catalog.rs` para añadir o quitar paquetes y entornos, y recompila.
Los perfiles guardados se almacenan como TOML en
`~/.config/arch-postinstall/profiles/` y también puedes editarlos a mano.
