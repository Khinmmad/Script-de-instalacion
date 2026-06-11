# Arch Post-Install

Asistente **TUI** (interfaz de terminal) escrito en **Rust** para automatizar la
post-instalación de Arch Linux: elige tu entorno de escritorio y los paquetes
(oficiales y del AUR) con un menú interactivo, guarda perfiles reutilizables y
deja un log de todo lo que se instaló.

> Este proyecto era originalmente un script de Python incompleto. Se reescribió
> en Rust para distribuirse como un **único binario sin dependencias** (ideal
> para una instalación recién hecha donde puede que ni siquiera tengas Python).

## Características

- 🖥️ **Menú estilo `archinstall`**: una pantalla principal navegable por teclado
  desde la que configuras cada sección y luego pulsas "Instalar ahora".
- 🔎 **Buscador en vivo de cualquier paquete**: busca en los **repositorios
  oficiales** y en el **AUR** usando sus APIs oficiales y añade lo que quieras,
  no estás limitado a una lista fija (como hacen `yay`/`paru`).
- 📦 **Selección interactiva de paquetes** oficiales y del AUR con checklist.
- 🎨 **Entornos de escritorio**: KDE Plasma, GNOME, Hyprland, Qtile o ninguno.
- 🤖 **Instalación automática de `yay`**: lo compila desde el AUR si no está.
- 🛡️ **Manejo robusto de errores**: los paquetes se instalan uno por uno, así un
  fallo no aborta el resto. Al final ves un resumen de éxitos y fallos.
- 📝 **Log con marca de tiempo** en `~/.local/state/arch-postinstall/`.
- 💾 **Perfiles guardables/cargables** desde el propio menú, para reproducir tu
  setup en otra máquina.

> El buscador necesita conexión a internet (consulta `archlinux.org` y
> `aur.archlinux.org`). El resto del catálogo curado funciona sin buscar.

## Pasos para usarlo (sin compilar) ⚡

El repo ya incluye un **binario precompilado estático** (no depende de Python
ni de librerías del sistema). No necesitas Rust ni compilar nada.

**1. Clona el repositorio:**

```bash
git clone https://github.com/Khinmmad/Script-de-instalacion
```

**2. Entra a la carpeta:**

```bash
cd Script-de-instalacion
```

**3. Instálalo en tu PATH:**

```bash
./install.sh
```

> Esto copia el binario a `~/.local/bin`. Para instalarlo para todos los
> usuarios en `/usr/local/bin`, usa `./install.sh --system` (pide sudo).

**4. Ejecútalo escribiendo su nombre:**

```bash
arch-postinstall
```

Se abrirá el asistente: sigue los 4 pasos en pantalla (entorno → paquetes
oficiales → paquetes AUR → revisión) y confirma para instalar.

> No lo corras como `root`. El programa usa `sudo` cuando hace falta.

### Alternativa: ejecutarlo sin instalar

Si prefieres no copiarlo al PATH, ejecútalo directamente desde la carpeta:

```bash
./dist/arch-postinstall-x86_64-linux
```

### Alternativa: descargar de Releases

También puedes bajar el binario desde la sección
[Releases](https://github.com/Khinmmad/Script-de-instalacion/releases) (se
publica automáticamente al subir un tag `vX.Y.Z`), darle permisos de ejecución
con `chmod +x` y correrlo.

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
arch-postinstall
```

Se abre un **menú principal** (estilo `archinstall`). Desde ahí entras a cada
sección, la configuras y vuelves al menú:

- **Entorno de escritorio** — elige uno (KDE, GNOME, Hyprland, Qtile o ninguno).
- **Paquetes oficiales / Paquetes AUR** — marca/desmarca con la barra espaciadora.
- **Buscar y añadir paquetes** — busca en vivo en los repos oficiales o el AUR
  (Tab cambia la fuente) y añade cualquier paquete a tu selección.
- **Cargar / Guardar perfil** — gestiona tus perfiles sin salir del programa.
- **Instalar ahora** — aplica todo.

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

| Tecla             | Acción                                            |
| ----------------- | ------------------------------------------------- |
| `↑`/`↓` o `k`/`j` | Mover el cursor                                   |
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
