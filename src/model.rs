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

/// Un conjunto de controladores (GPU o microcodigo) seleccionable.
#[derive(Debug, Clone)]
pub struct DriverBundle {
    /// Clave estable (ej. "nvidia"). Usada como identificador unico en el
    /// catalogo y sus tests; reservada para futura persistencia en perfiles.
    #[allow(dead_code)]
    pub id: &'static str,
    /// Nombre legible mostrado en la TUI.
    pub label: &'static str,
    /// Paquetes oficiales que instala este controlador.
    pub packages: &'static [&'static str],
    /// Si viene marcado por defecto.
    pub default_on: bool,
}

/// Plan concreto que el usuario confirmo y que el instalador ejecutara.
#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub desktop_env_id: Option<String>,
    pub display_manager: Option<String>,
    /// Paquetes oficiales a instalar (incluye base + entorno + drivers + extras).
    pub official: Vec<String>,
    /// Paquetes del AUR a instalar.
    pub aur: Vec<String>,
    /// Servicios de sistema a habilitar (NetworkManager, bluetooth, el DM...).
    pub services: Vec<String>,
    /// Servicios de usuario a habilitar (stack de audio PipeWire).
    pub user_services: Vec<String>,
}

impl InstallPlan {
    /// Construye un plan derivando automaticamente los servicios a habilitar
    /// a partir de los paquetes elegidos y el display manager.
    pub fn new(
        desktop_env_id: Option<String>,
        display_manager: Option<String>,
        official: Vec<String>,
        aur: Vec<String>,
    ) -> Self {
        let (services, user_services) = derive_services(&official, &display_manager);
        InstallPlan {
            desktop_env_id,
            display_manager,
            official,
            aur,
            services,
            user_services,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.official.is_empty() && self.aur.is_empty()
    }
}

/// Decide que servicios habilitar segun los paquetes instalados. Esto es lo que
/// deja el sistema "listo para usar" tras la instalacion.
fn derive_services(official: &[String], dm: &Option<String>) -> (Vec<String>, Vec<String>) {
    let has = |name: &str| official.iter().any(|p| p == name);

    let mut system = Vec::new();
    if let Some(dm) = dm {
        system.push(dm.clone()); // ej. sddm / gdm / lightdm
    }
    if has("networkmanager") {
        system.push("NetworkManager".into());
    }
    if has("bluez") || has("bluez-utils") {
        system.push("bluetooth".into());
    }

    // El stack de audio de PipeWire corre como servicios de usuario.
    let mut user = Vec::new();
    if has("pipewire") {
        user.push("pipewire".into());
        if has("pipewire-pulse") {
            user.push("pipewire-pulse".into());
        }
        if has("wireplumber") {
            user.push("wireplumber".into());
        }
    }

    (system, user)
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
        InstallPlan::new(
            self.desktop_environment,
            self.display_manager,
            self.official_packages,
            self.aur_packages,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_plan() -> InstallPlan {
        InstallPlan::new(
            Some("hyprland".into()),
            Some("sddm".into()),
            vec!["firefox".into(), "kitty".into()],
            vec!["spotify".into()],
        )
    }

    #[test]
    fn plan_roundtrip_through_profile() {
        let plan = sample_plan();
        let profile = Profile::from_plan("mi-setup", &plan);
        let back = profile.into_plan();
        assert_eq!(back.desktop_env_id, plan.desktop_env_id);
        assert_eq!(back.display_manager, plan.display_manager);
        assert_eq!(back.official, plan.official);
        assert_eq!(back.aur, plan.aur);
    }

    #[test]
    fn profile_roundtrip_through_toml() {
        let profile = Profile::from_plan("mi-setup", &sample_plan());
        let body = toml::to_string_pretty(&profile).unwrap();
        let parsed: Profile = toml::from_str(&body).unwrap();
        assert_eq!(parsed.name, "mi-setup");
        assert_eq!(parsed.official_packages, profile.official_packages);
        assert_eq!(parsed.aur_packages, profile.aur_packages);
    }

    #[test]
    fn profile_with_missing_fields_uses_defaults() {
        // Un perfil editado a mano puede omitir campos opcionales.
        let parsed: Profile = toml::from_str("name = \"minimo\"").unwrap();
        assert!(parsed.desktop_environment.is_none());
        assert!(parsed.official_packages.is_empty());
        assert!(parsed.into_plan().is_empty());
    }

    #[test]
    fn empty_plan_is_empty() {
        let plan = InstallPlan::new(None, None, vec![], vec![]);
        assert!(plan.is_empty());
        assert!(!sample_plan().is_empty());
    }

    #[test]
    fn services_are_derived_from_packages() {
        let plan = InstallPlan::new(
            Some("kde".into()),
            Some("sddm".into()),
            vec![
                "networkmanager".into(),
                "bluez".into(),
                "pipewire".into(),
                "pipewire-pulse".into(),
                "wireplumber".into(),
            ],
            vec![],
        );
        assert!(plan.services.contains(&"sddm".to_string()));
        assert!(plan.services.contains(&"NetworkManager".to_string()));
        assert!(plan.services.contains(&"bluetooth".to_string()));
        assert_eq!(
            plan.user_services,
            vec!["pipewire", "pipewire-pulse", "wireplumber"]
        );
    }

    #[test]
    fn no_services_without_relevant_packages() {
        let plan = InstallPlan::new(None, None, vec!["firefox".into()], vec![]);
        assert!(plan.services.is_empty());
        assert!(plan.user_services.is_empty());
    }
}
