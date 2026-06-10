//! Catalogo por defecto de paquetes y entornos de escritorio.
//!
//! Migrado y corregido desde los antiguos `config/paquetes.py`,
//! `config/paquetes_aur.py` y `config/entorno.py`. Aqui se arreglaron
//! varios bugs del original (comas faltantes que fusionaban entradas,
//! el paquete "awww" que en realidad es "swww", etc.).

use crate::model::{DesktopEnvironment, Package, Source};

/// Paquetes base comunes a cualquier entorno grafico.
/// Equivale a la lista `base` del antiguo `entorno.py`.
pub const BASE_PACKAGES: &[&str] = &[
    "xorg-server",
    "xorg-xinit",
    "mesa",
    "networkmanager",
    "bluez",
    "bluez-utils",
    "pipewire",
    "pipewire-pulse",
    "wireplumber",
    "git",
    "base-devel",
];

/// Entornos de escritorio / window managers disponibles.
pub const DESKTOP_ENVIRONMENTS: &[DesktopEnvironment] = &[
    DesktopEnvironment {
        id: "ninguno",
        label: "Ninguno (solo paquetes, sin entorno grafico)",
        packages: &[],
        display_manager: None,
    },
    DesktopEnvironment {
        id: "kde",
        label: "KDE Plasma (Wayland/X11)",
        packages: &["plasma", "kde-applications"],
        display_manager: Some("sddm"),
    },
    DesktopEnvironment {
        id: "gnome",
        label: "GNOME (Wayland)",
        packages: &["gnome"],
        display_manager: Some("gdm"),
    },
    DesktopEnvironment {
        id: "hyprland",
        label: "Hyprland (Wayland, tiling)",
        packages: &[
            "hyprland",
            "waybar",
            "wofi",
            "xdg-desktop-portal-hyprland",
        ],
        display_manager: Some("sddm"),
    },
    DesktopEnvironment {
        id: "qtile",
        label: "Qtile (X11/Wayland, tiling en Python)",
        packages: &["qtile", "python", "python-pywlroots"],
        display_manager: Some("lightdm"),
    },
];

/// Paquetes extra ofrecidos en la TUI (oficiales y AUR).
/// Migrado de `paquetes.py` y `paquetes_aur.py`, con comas corregidas.
pub const EXTRA_PACKAGES: &[Package] = &[
    // ---- Oficiales ----
    Package { name: "firefox", description: "Navegador web", source: Source::Official, default_on: true },
    Package { name: "vlc", description: "Reproductor multimedia", source: Source::Official, default_on: true },
    Package { name: "vim", description: "Editor de texto", source: Source::Official, default_on: true },
    Package { name: "neovim", description: "Editor de texto (fork moderno de vim)", source: Source::Official, default_on: true },
    Package { name: "git", description: "Control de versiones", source: Source::Official, default_on: true },
    Package { name: "htop", description: "Monitor de procesos", source: Source::Official, default_on: true },
    Package { name: "fastfetch", description: "Info del sistema en terminal", source: Source::Official, default_on: true },
    Package { name: "rofi", description: "Lanzador de aplicaciones", source: Source::Official, default_on: false },
    Package { name: "swww", description: "Daemon de wallpapers para Wayland", source: Source::Official, default_on: false },
    Package { name: "alacritty", description: "Emulador de terminal (GPU)", source: Source::Official, default_on: false },
    Package { name: "kitty", description: "Emulador de terminal (GPU)", source: Source::Official, default_on: true },
    Package { name: "thunar", description: "Gestor de archivos ligero", source: Source::Official, default_on: false },
    Package { name: "network-manager-applet", description: "Applet de red en bandeja", source: Source::Official, default_on: true },
    // ---- AUR ----
    Package { name: "visual-studio-code-bin", description: "Editor de codigo (VS Code)", source: Source::Aur, default_on: true },
    Package { name: "spotify", description: "Cliente de musica", source: Source::Aur, default_on: false },
    Package { name: "ags", description: "Aylur's GTK Shell (widgets)", source: Source::Aur, default_on: false },
];
