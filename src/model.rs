//! Estructuras de datos centrales del catalogo y de los planes de instalacion.

use serde::{Deserialize, Serialize};

/// Origen desde el que se instala un paquete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Repositorios oficiales (pacman).
    Official,
    /// Arch User Repository (yay).
    Aur,
}

/// Un paquete individual del catalogo.
#[derive(Debug, Clone)]
pub struct Package {
    /// Nombre exacto del paquete tal como lo conoce pacman/yay.
    pub name: &'static str,
    /// Descripcion corta mostrada en la TUI.
    pub description: &'static str,
    /// De donde se instala.
    pub source: Source,
    /// Si viene marcado por defecto en la seleccion.
    pub default_on: bool,
}

/// Un entorno de escritorio o gestor de ventanas seleccionable.
#[derive(Debug, Clone)]
pub struct DesktopEnvironment {
    /// Clave estable usada en los perfiles (ej. "hyprland").
    pub id: &'static str,
    /// Nombre legible mostrado en la TUI (ej. "Hyprland (Wayland)").
    pub label: &'static str,
    /// Paquetes propios del entorno (sin contar la base comun).
    pub packages: &'static [&'static str],
    /// Display manager recomendado (ej. "sddm"). None = ninguno.
    pub display_manager: Option<&'static str>,
}

/// Plan concreto que el usuario confirmo y que el instalador ejecutara.
#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub desktop_env_id: Option<String>,
    pub display_manager: Option<String>,
    /// Paquetes oficiales a instalar (incluye base + entorno + extras elegidos).
    pub official: Vec<String>,
    /// Paquetes del AUR a instalar.
    pub aur: Vec<String>,
}

impl InstallPlan {
    pub fn is_empty(&self) -> bool {
        self.official.is_empty() && self.aur.is_empty()
    }
}

/// Perfil persistible en disco (TOML) para reproducir una instalacion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub desktop_environment: Option<String>,
    #[serde(default)]
    pub display_manager: Option<String>,
    #[serde(default)]
    pub official_packages: Vec<String>,
    #[serde(default)]
    pub aur_packages: Vec<String>,
}

impl Profile {
    pub fn from_plan(name: &str, plan: &InstallPlan) -> Self {
        Profile {
            name: name.to_string(),
            desktop_environment: plan.desktop_env_id.clone(),
            display_manager: plan.display_manager.clone(),
            official_packages: plan.official.clone(),
            aur_packages: plan.aur.clone(),
        }
    }

    pub fn into_plan(self) -> InstallPlan {
        InstallPlan {
            desktop_env_id: self.desktop_environment,
            display_manager: self.display_manager,
            official: self.official_packages,
            aur: self.aur_packages,
        }
    }
}
