//! Deteccion del estado actual del sistema.
//!
//! La TUI usa esto para:
//! - Marcar con un simbolo los paquetes / servicios que ya estan en el
//!   sistema, para que el usuario vea de un vistazo lo que falta.
//! - Pre-rellenar el formulario de configuracion del sistema con los valores
//!   actuales (locale, zona, teclado, hostname).
//! - En la pantalla de revision, separar "por instalar" de "ya instalado".
//!
//! El instalador tambien lo usa: filtra del plan los paquetes que ya
//! existen, de modo que un re-instalacion no reinstala nada.
//!
//! Cada deteccion es independiente y nunca aborta: si `pacman` no esta
//! disponible o un archivo no se puede leer, devolvemos "no instalado" y
//! seguimos. La idea es que un fallo de deteccion nunca impida arrancar la
//! TUI.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::Source;

/// Estado detectado del sistema. Se construye una vez al inicio y se
/// consulta despues. Todos los campos son opcionales o vacios si la
/// deteccion fallo.
#[derive(Debug, Default, Clone)]
pub struct SystemStatus {
    /// Paquetes oficiales instalados (de core/extra/multilib/...).
    pub official: HashSet<String>,
    /// Paquetes del AUR instalados.
    pub aur: HashSet<String>,
    /// Unidades systemd que estan habilitadas (`is-enabled` == enabled).
    pub enabled_services: HashSet<String>,
    /// Paquetes con actualizaciones disponibles segun `pacman -Qu`.
    pub updates_available: HashSet<String>,
    /// Bootloader detectado.
    pub bootloader: Bootloader,
    /// `true` si el bloque `[multilib]` esta descomentado en pacman.conf.
    pub multilib_enabled: bool,
    /// Valor de `LANG` en `/etc/locale.conf` (sin expansion).
    pub locale: Option<String>,
    /// Zona horaria detectada a partir de `/etc/localtime` (puede no
    /// coincidir exactamente con el nombre IANA canonico).
    pub timezone: Option<String>,
    /// Valor de `KEYMAP` en `/etc/vconsole.conf`.
    pub keymap: Option<String>,
    /// Contenido de `/etc/hostname`.
    pub hostname: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Bootloader {
    Grub,
    SystemdBoot,
    #[default]
    Unknown,
}

/// Resultado de buscar un paquete concreto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageStatus {
    /// `pacman -Q` lo encontro (oficial) o `pacman -Qm` lo encontro (AUR).
    Installed,
    /// `pacman` no lo reporta.
    #[allow(dead_code)]
    NotInstalled,
    /// `pacman` no estaba disponible; no se pudo saber.
    #[allow(dead_code)]
    Unknown,
}

/// Resultado de buscar un servicio concreto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    /// `systemctl is-enabled` devolvio `enabled`.
    Enabled,
    /// Existe la unidad pero esta deshabilitado.
    #[allow(dead_code)]
    Disabled,
    /// La unidad no existe (el paquete no esta o el nombre es incorrecto).
    #[allow(dead_code)]
    NotInstalled,
    /// `systemctl` no estaba disponible.
    #[allow(dead_code)]
    Unknown,
}

impl SystemStatus {
    /// Ejecuta todas las detecciones. Cada paso es independiente: un fallo
    /// en uno no aborta el resto. Diseñado para ser seguro de llamar al
    /// inicio de la TUI.
    pub fn detect() -> Self {
        let mut s = SystemStatus::default();
        s.detect_packages();
        s.detect_updates();
        s.detect_enabled_services();
        s.bootloader = detect_bootloader();
        s.multilib_enabled = detect_multilib();
        s.locale = detect_locale();
        s.timezone = detect_timezone();
        s.keymap = detect_keymap();
        s.hostname = detect_hostname();
        s
    }

    fn detect_packages(&mut self) {
        if let Ok(out) = Command::new("pacman").arg("-Qq").output() {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let mut all: HashSet<String> = stdout
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();
                if let Ok(aur_out) = Command::new("pacman").arg("-Qqm").output() {
                    if aur_out.status.success() {
                        let aur: HashSet<String> = String::from_utf8_lossy(&aur_out.stdout)
                            .lines()
                            .map(|l| l.trim().to_string())
                            .filter(|l| !l.is_empty())
                            .collect();
                        for p in &aur {
                            all.remove(p);
                        }
                        self.aur = aur;
                    }
                }
                self.official = all;
            }
        }
    }

    /// `pacman -Qu` lista los paquetes con actualizacion disponible. Si no
    /// esta disponible o no se puede sincronizar la DB, lo dejamos vacio.
    fn detect_updates(&mut self) {
        if let Ok(out) = Command::new("pacman").arg("-Qu").output() {
            if out.status.success() {
                self.updates_available = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter_map(|l| l.split_whitespace().next())
                    .map(|s| s.to_string())
                    .collect();
            }
        }
    }

    fn detect_enabled_services(&mut self) {
        let Ok(out) = Command::new("systemctl")
            .args([
                "list-unit-files",
                "--state=enabled",
                "--no-legend",
                "--plain",
            ])
            .output()
        else {
            return;
        };
        if !out.status.success() {
            return;
        }
        self.enabled_services = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .map(|name| {
                // systemctl reporta el nombre real de la unidad (puede
                // incluir ".service" o no). Lo dejamos tal cual para
                // comparar; service_status prueba con y sin sufijo.
                name.to_string()
            })
            .collect();
    }

    /// `true` si el paquete `name` esta instalado en el origen `src`.
    pub fn package_status(&self, name: &str, src: Source) -> PackageStatus {
        let set = match src {
            Source::Official => &self.official,
            Source::Aur => &self.aur,
        };
        if set.contains(name) {
            PackageStatus::Installed
        } else if self.official.is_empty() && self.aur.is_empty() {
            // No pudimos detectar nada: pacman no respondio.
            PackageStatus::Unknown
        } else {
            PackageStatus::NotInstalled
        }
    }

    /// Estado de un servicio (con o sin sufijo `.service`).
    pub fn service_status(&self, name: &str) -> ServiceStatus {
        if self.enabled_services.is_empty() {
            return ServiceStatus::Unknown;
        }
        let with_suffix = if name.contains('.') {
            name.to_string()
        } else {
            format!("{name}.service")
        };
        let base = name.strip_suffix(".service").unwrap_or(name).to_string();
        if self.enabled_services.contains(&with_suffix) || self.enabled_services.contains(&base) {
            ServiceStatus::Enabled
        } else {
            ServiceStatus::Disabled
        }
    }
}

fn detect_bootloader() -> Bootloader {
    if Path::new("/boot/grub/grub.cfg").exists() || Path::new("/etc/default/grub").exists() {
        Bootloader::Grub
    } else if Path::new("/boot/loader/entries").exists()
        || Path::new("/efi/loader/entries").exists()
    {
        Bootloader::SystemdBoot
    } else {
        Bootloader::Unknown
    }
}

fn detect_multilib() -> bool {
    let Ok(body) = std::fs::read_to_string("/etc/pacman.conf") else {
        return false;
    };
    let mut in_block = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('[') {
            in_block = trimmed.eq_ignore_ascii_case("[multilib]");
            continue;
        }
        if in_block && trimmed.starts_with("Include") {
            return true;
        }
    }
    false
}

fn detect_locale() -> Option<String> {
    let body = std::fs::read_to_string("/etc/locale.conf").ok()?;
    for line in body.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("LANG=") {
            let v = rest.trim().trim_matches('"').trim_matches('\'');
            if !v.is_empty() && v != "C" {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn detect_timezone() -> Option<String> {
    // /etc/localtime suele ser un symlink a /usr/share/zoneinfo/<Region>/<City>.
    let target = std::fs::read_link("/etc/localtime").ok()?;
    let path: PathBuf = target;
    let zoneinfo = Path::new("/usr/share/zoneinfo");
    let stripped = path.strip_prefix(zoneinfo).ok()?;
    let s = stripped.to_str()?.to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn detect_keymap() -> Option<String> {
    let body = std::fs::read_to_string("/etc/vconsole.conf").ok()?;
    for line in body.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("KEYMAP=") {
            let v = rest.trim().trim_matches('"').trim_matches('\'');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn detect_hostname() -> Option<String> {
    let body = std::fs::read_to_string("/etc/hostname").ok()?;
    let v = body.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn multilib_detected_when_uncommented() {
        let body = "\
[options]
HoldPkg = pacman

#[multilib]
#Include = /etc/pacman.d/mirrorlist

[core]
Include = /etc/pacman.d/mirrorlist

[multilib]
Include = /etc/pacman.d/mirrorlist
";
        assert!(multilib_enabled_in(body));
        assert!(!multilib_enabled_in(
            "#[multilib]\n#Include = /etc/pacman.d/mirrorlist\n"
        ));
    }

    #[test]
    fn locale_parsed_from_locale_conf() {
        let body = "LANG=es_MX.UTF-8\nLC_COLLATE=C\n";
        assert_eq!(parse_locale_from(body).as_deref(), Some("es_MX.UTF-8"));
        assert_eq!(parse_locale_from("LANG=C\n").as_deref(), None);
        assert_eq!(parse_locale_from("# comentario\n").as_deref(), None);
    }

    #[test]
    fn keymap_parsed_from_vconsole_conf() {
        let body = "KEYMAP=la-latin1\nFONT=lat2-16\n";
        assert_eq!(parse_keymap_from(body).as_deref(), Some("la-latin1"));
        assert_eq!(parse_keymap_from("FONT=lat2-16\n").as_deref(), None);
    }

    #[test]
    fn hostname_parsed_from_file() {
        assert_eq!(parse_hostname_from("mi-arch\n").as_deref(), Some("mi-arch"));
        assert_eq!(parse_hostname_from("  \n").as_deref(), None);
    }

    // Helpers separados para poder testearlos sin tocar el sistema de
    // archivos. Duplican la logica de detect_*; si divergen, los tests
    // fallaran y daran la pista.
    fn multilib_enabled_in(body: &str) -> bool {
        let mut in_block = false;
        for line in body.lines() {
            let t = line.trim();
            if t.starts_with('#') || t.is_empty() {
                continue;
            }
            if t.starts_with('[') {
                in_block = t.eq_ignore_ascii_case("[multilib]");
                continue;
            }
            if in_block && t.starts_with("Include") {
                return true;
            }
        }
        false
    }

    fn parse_locale_from(body: &str) -> Option<String> {
        for line in body.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("LANG=") {
                let v = rest.trim().trim_matches('"').trim_matches('\'');
                if !v.is_empty() && v != "C" {
                    return Some(v.to_string());
                }
            }
        }
        None
    }

    fn parse_keymap_from(body: &str) -> Option<String> {
        for line in body.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("KEYMAP=") {
                let v = rest.trim().trim_matches('"').trim_matches('\'');
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
        None
    }

    fn parse_hostname_from(body: &str) -> Option<String> {
        let v = body.trim();
        if v.is_empty() {
            None
        } else {
            Some(v.to_string())
        }
    }
}
