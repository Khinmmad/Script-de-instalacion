# Arch Post-Install

Asistente **TUI** (interfaz de terminal) escrito en **Rust** para automatizar la
post-instalación de Arch Linux: elige tu entorno de escritorio y los paquetes
(oficiales y del AUR) con un menú interactivo, guarda perfiles reutilizables y
deja un log de todo lo que se instaló.

> Este proyecto era originalmente un script de Python incompleto. Se reescribió
> en Rust para distribuirse como un **único binario sin dependencias** (ideal
> para una instalación recién hecha donde puede que ni siquiera tengas Python).

## Características

- 🖥️ **Asistente TUI interactivo** (ratatui): navega con flechas, marca paquetes
  con la barra espaciadora.
- 📦 **Selección interactiva de paquetes**: oficiales y del AUR, no listas fijas.
- 🎨 **Entornos de escritorio**: KDE Plasma, GNOME, Hyprland, Qtile o ninguno.
- 🤖 **Instalación automática de `yay`**: lo compila desde el AUR si no está.
- 🛡️ **Manejo robusto de errores**: los paquetes se instalan uno por uno, así un
  fallo no aborta el resto. Al final ves un resumen de éxitos y fallos.
- 📝 **Log con marca de tiempo** en `~/.local/state/arch-postinstall/`.
- 💾 **Perfiles guardables/cargables** para reproducir tu setup en otra máquina.

## Instalación rápida (sin compilar) ⚡

El repo ya incluye un **binario precompilado estático** (no depende de Python
ni de librerías del sistema). No necesitas Rust ni compilar nada:

```bash
git clone https://github.com/Khinmmad/Script-de-instalacion
cd Script-de-instalacion

# Opción A: ejecutarlo directamente
./dist/arch-postinstall-x86_64-linux

# Opción B: instalarlo en tu PATH y luego llamarlo solo por su nombre
./install.sh            # lo copia a ~/.local/bin
arch-postinstall        # ¡listo!
```

> `./install.sh --system` lo instala en `/usr/local/bin` (con sudo) para todos
> los usuarios.

También puedes descargar el binario de la sección
[Releases](https://github.com/Khinmmad/Script-de-instalacion/releases) (se
publica automáticamente al subir un tag `vX.Y.Z`).

## Requisitos

- Arch Linux (o derivado con `pacman`).
- Conexión a internet.
- **Para usar el binario precompilado:** nada más (es estático).
- **Solo si quieres compilarlo tú:** toolchain de Rust (`rustup`/`cargo`).

## Compilación (opcional, solo si modificas el código)

```bash
git clone https://github.com/Khinmmad/Script-de-instalacion
cd Script-de-instalacion
cargo build --release
# El binario queda en ./target/release/arch-postinstall
```

## Uso

### Asistente interactivo (por defecto)

```bash
./target/release/arch-postinstall
```

Te guía por 4 pasos: entorno de escritorio → paquetes oficiales → paquetes AUR →
revisión. En la revisión puedes marcar **guardar como perfil**.

> No lo corras como `root`. El programa usa `sudo` cuando hace falta.

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

| Tecla            | Acción                                  |
| ---------------- | --------------------------------------- |
| `↑`/`↓` o `k`/`j`| Mover el cursor                         |
| `Espacio`        | Marcar/desmarcar paquete o entorno      |
| `Enter`          | Siguiente paso / confirmar              |
| `q` o `Esc`      | Retroceder un paso / salir              |
| `s`              | (Revisión) activar guardar perfil       |
| `n`              | (Revisión) editar el nombre del perfil  |

## Estructura del proyecto

```
Script-de-instalacion/
├── Cargo.toml
└── src/
    ├── main.rs       # CLI, orquestación y argumentos
    ├── tui.rs        # Asistente interactivo (ratatui)
    ├── catalog.rs    # Catálogo de paquetes y entornos
    ├── model.rs      # Estructuras de datos y perfiles
    ├── profile.rs    # Guardar/cargar perfiles (TOML)
    └── installer.rs  # Ejecución (pacman/yay), logging y resumen
```

## Personalización

Edita `src/catalog.rs` para añadir o quitar paquetes y entornos, y recompila.
Los perfiles guardados se almacenan como TOML en
`~/.config/arch-postinstall/profiles/` y también puedes editarlos a mano.
