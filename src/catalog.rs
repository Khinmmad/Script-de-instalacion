//! Catalogo por defecto de paquetes y entornos de escritorio.
//!
//! Es solo un punto de partida: desde la TUI puedes buscar y anadir cualquier
//! otro paquete (oficial o del AUR) en vivo. Los nombres aqui estan revisados
//! contra los repositorios actuales de Arch Linux.

use crate::model::{DesktopEnvironment, Package, Source};

/// Paquetes base comunes a cualquier entorno grafico (todos oficiales).
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
        // plasma-meta es el meta recomendado; konsole+dolphin para terminal y archivos.
        packages: &["plasma-meta", "konsole", "dolphin"],
        display_manager: Some("sddm"),
    },
    DesktopEnvironment {
        id: "gnome",
        label: "GNOME (Wayland)",
        packages: &["gnome", "gnome-terminal"],
        display_manager: Some("gdm"),
    },
    DesktopEnvironment {
        id: "hyprland",
        label: "Hyprland (Wayland, tiling)",
        // Todos oficiales (repo extra) actualmente.
        packages: &[
            "hyprland",
            "waybar",
            "wofi",
            "xdg-desktop-portal-hyprland",
            "kitty",
        ],
        display_manager: Some("sddm"),
    },
    DesktopEnvironment {
        id: "qtile",
        label: "Qtile (X11, tiling en Python)",
        packages: &["qtile", "alacritty"],
        display_manager: Some("lightdm"),
    },
];

/// Paquetes extra ofrecidos por defecto en la TUI (puedes anadir mas buscando).
#[rustfmt::skip]
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
    Package { name: "alacritty", description: "Emulador de terminal (GPU)", source: Source::Official, default_on: false },
    Package { name: "kitty", description: "Emulador de terminal (GPU)", source: Source::Official, default_on: true },
    Package { name: "thunar", description: "Gestor de archivos ligero", source: Source::Official, default_on: false },
    Package { name: "network-manager-applet", description: "Applet de red en bandeja", source: Source::Official, default_on: true },
    // ---- AUR ----
    Package { name: "visual-studio-code-bin", description: "Editor de codigo (VS Code)", source: Source::Aur, default_on: true },
    Package { name: "spotify", description: "Cliente de musica", source: Source::Aur, default_on: false },
    Package { name: "swww", description: "Daemon de wallpapers para Wayland", source: Source::Aur, default_on: false },
    Package { name: "ags", description: "Aylur's GTK Shell (widgets)", source: Source::Aur, default_on: false },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn no_duplicate_extra_packages() {
        let mut seen = HashSet::new();
        for p in EXTRA_PACKAGES {
            assert!(
                seen.insert(p.name),
                "paquete duplicado en catalogo: {}",
                p.name
            );
        }
    }

    #[test]
    fn desktop_environment_ids_are_unique() {
        let mut seen = HashSet::new();
        for de in DESKTOP_ENVIRONMENTS {
            assert!(seen.insert(de.id), "id de entorno duplicado: {}", de.id);
        }
    }

    #[test]
    fn first_environment_is_none_option() {
        // La TUI asume que el indice 0 es "sin entorno grafico".
        assert_eq!(DESKTOP_ENVIRONMENTS[0].id, "ninguno");
        assert!(DESKTOP_ENVIRONMENTS[0].packages.is_empty());
        assert!(DESKTOP_ENVIRONMENTS[0].display_manager.is_none());
    }

    #[test]
    fn real_environments_have_display_manager() {
        for de in DESKTOP_ENVIRONMENTS.iter().filter(|d| d.id != "ninguno") {
            assert!(
                de.display_manager.is_some(),
                "el entorno {} no tiene display manager",
                de.id
            );
            assert!(
                !de.packages.is_empty(),
                "el entorno {} no tiene paquetes",
                de.id
            );
        }
    }
}
