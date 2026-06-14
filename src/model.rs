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
    /// Locale a generar y fijar (ej. "es_MX.UTF-8"). None = no tocar.
    pub locale: Option<String>,
    /// Zona horaria (ej. "America/Mexico_City"). None = no tocar.
    pub timezone: Option<String>,
    /// Distribucion de teclado de consola (ej. "la-latin1"). None = no tocar.
    pub keymap: Option<String>,
    /// Hostname del equipo. None = no tocar.
    pub hostname: Option<String>,
    /// Pais/region para `reflector --country` (ej. "Mexico", "Spain").
    /// None = dejar /etc/pacman.d/mirrorlist como esta.
    pub mirror_region: Option<String>,
    /// Habilitar el repositorio [multilib] (Steam, libs de 32 bits).
    pub enable_multilib: bool,
    /// Reiniciar automaticamente al terminar.
    pub reboot_after: bool,
    /// Limpiar paquetes huerfanos al terminar (`pacman -Rns` sobre los
    /// que ya no son dependencia de nada). Opcional y desactivado por
    /// defecto para no borrar nada sin consentimiento.
    pub cleanup_orphans: bool,
}

/// Une una lista para mostrarla. Si esta vacia devuelve `(ninguno)` para
/// que el usuario vea explicitamente que no hay nada seleccionado.
pub fn format_list_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "(ninguno)".to_string()
    } else {
        items.join(", ")
    }
}

/// Estilo de etiquetas para los ajustes del sistema. Usado por la TUI, la
/// pantalla de revision y la salida CLI para que las tres vistas digan lo
/// mismo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemLabelStyle {
    /// Palabras sueltas: `locale`, `zona`, `teclado`, `hostname`, `multilib`,
    /// `reiniciar`. Pensado para el menu principal donde el espacio es corto.
    Short,
    /// Parejas `clave=valor` para el resto de campos: `locale=es_MX`,
    /// `zona=America/...`, etc. Los flags van igual que en `Short`.
    Detailed,
}

/// Devuelve la lista de etiquetas que describen los ajustes del sistema
/// recibidos. Las opciones vacias no contribuyen; los flags solo aparecen si
/// estan activos. Centraliza el formato para que la CLI, el menu y la
/// pantalla de revision nunca se contradigan.
#[allow(clippy::too_many_arguments)]
pub fn format_system_settings(
    locale: Option<&str>,
    timezone: Option<&str>,
    keymap: Option<&str>,
    hostname: Option<&str>,
    mirror_region: Option<&str>,
    multilib: bool,
    reboot: bool,
    style: SystemLabelStyle,
) -> Vec<String> {
    let mut out = Vec::new();
    match style {
        SystemLabelStyle::Short => {
            if locale.is_some() {
                out.push("locale".into());
            }
            if timezone.is_some() {
                out.push("zona".into());
            }
            if keymap.is_some() {
                out.push("teclado".into());
            }
            if hostname.is_some() {
                out.push("hostname".into());
            }
            if mirror_region.is_some() {
                out.push("mirrors".into());
            }
        }
        SystemLabelStyle::Detailed => {
            if let Some(v) = locale {
                out.push(format!("locale={v}"));
            }
            if let Some(v) = timezone {
                out.push(format!("zona={v}"));
            }
            if let Some(v) = keymap {
                out.push(format!("teclado={v}"));
            }
            if let Some(v) = hostname {
                out.push(format!("hostname={v}"));
            }
            if let Some(v) = mirror_region {
                out.push(format!("mirrors={v}"));
            }
        }
    }
    if multilib {
        out.push("multilib".into());
    }
    if reboot {
        out.push("reiniciar".into());
    }
    out
}

impl InstallPlan {
    /// Construye un plan derivando automaticamente los servicios a habilitar
    /// a partir de los paquetes elegidos y el display manager. Los ajustes del
    /// sistema (locale, zona horaria, etc.) quedan vacios; el llamador los fija.
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
            locale: None,
            timezone: None,
            keymap: None,
            hostname: None,
            mirror_region: None,
            enable_multilib: false,
            reboot_after: false,
            cleanup_orphans: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.official.is_empty() && self.aur.is_empty()
    }

    /// True si hay paquete de microcodigo de CPU en el plan.
    pub fn has_microcode(&self) -> bool {
        self.official
            .iter()
            .any(|p| p == "intel-ucode" || p == "amd-ucode")
    }

    /// True si el plan instala el driver propietario de NVIDIA.
    pub fn has_nvidia(&self) -> bool {
        self.official
            .iter()
            .any(|p| p == "nvidia" || p == "nvidia-open")
    }

    /// Parametros de kernel que requieren los drivers elegidos.
    pub fn kernel_params(&self) -> Vec<String> {
        let mut params = Vec::new();
        if self.has_nvidia() {
            params.push("nvidia-drm.modeset=1".to_string());
        }
        params
    }
}

/// Decide que servicios habilitar segun los paquetes instalados. Esto es lo que
/// deja el sistema "listo para usar" tras la instalacion.
fn derive_services(official: &[String], dm: &Option<String>) -> (Vec<String>, Vec<String>) {
    let has = |name: &str| official.iter().any(|p| p == name);

    let mut system = Vec::new();
    // Un DM vacio no genera un servicio; lo descartamos para no terminar
    // con `systemctl enable .service` en el log.
    if let Some(dm) = dm {
        let trimmed = dm.trim();
        if !trimmed.is_empty() {
            system.push(trimmed.to_string()); // ej. sddm / gdm / lightdm
        }
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
    /// Pais/region para `reflector --country`. Equivale a
    /// `InstallPlan::mirror_region`.
    #[serde(default)]
    pub mirror_region: Option<String>,
}

impl Profile {
    pub fn from_plan(name: &str, plan: &InstallPlan) -> Self {
        Profile {
            name: name.to_string(),
            desktop_environment: plan.desktop_env_id.clone(),
            display_manager: plan.display_manager.clone(),
            official_packages: plan.official.clone(),
            aur_packages: plan.aur.clone(),
            mirror_region: plan.mirror_region.clone(),
        }
    }

    pub fn into_plan(self) -> InstallPlan {
        // Un perfil editado a mano puede tener strings vacios donde
        // esperamos None (ej. `display_manager = ""`). Los normalizamos
        // para no generar servicios basura tipo `systemctl enable .service`.
        let de = self.desktop_environment.and_then(nonempty_string);
        let dm = self.display_manager.and_then(nonempty_string);
        let region = self.mirror_region.and_then(nonempty_string);
        let mut plan = InstallPlan::new(de, dm, self.official_packages, self.aur_packages);
        plan.mirror_region = region;
        plan
    }
}

/// Devuelve `Some(s.to_string())` solo si `s` no esta vacio tras trim.
fn nonempty_string(s: String) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
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

    #[test]
    fn profile_with_empty_strings_normalizes_to_none() {
        // Un perfil editado a mano puede tener `display_manager = ""` o
        // `desktop_environment = ""`. Esos campos no deben generar
        // servicios ni base/packages raros.
        let parsed: Profile = toml::from_str(
            r#"name = "min"
desktop_environment = ""
display_manager = ""
"#,
        )
        .unwrap();
        let plan = parsed.into_plan();
        assert!(plan.desktop_env_id.is_none());
        assert!(plan.display_manager.is_none());
        assert!(plan.services.is_empty());
    }

    #[test]
    fn detects_microcode_and_nvidia() {
        let plan = InstallPlan::new(
            None,
            None,
            vec!["nvidia".into(), "intel-ucode".into()],
            vec![],
        );
        assert!(plan.has_microcode());
        assert!(plan.has_nvidia());
        assert_eq!(
            plan.kernel_params(),
            vec!["nvidia-drm.modeset=1".to_string()]
        );

        let plain = InstallPlan::new(None, None, vec!["firefox".into()], vec![]);
        assert!(!plain.has_microcode());
        assert!(!plain.has_nvidia());
        assert!(plain.kernel_params().is_empty());
    }
}
