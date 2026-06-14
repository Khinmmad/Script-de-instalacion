//! Validacion de un perfil TOML sin instalar nada.
//!
//! Pensado para CI y para autores de perfiles: parsea un archivo,
//! comprueba que los campos de sistema tienen formato valido y que
//! cada paquete existe en los repos oficiales o en el AUR.
//!
//! Reporta:
//! - formato de locale / timezone / keymap / hostname / mirror_region
//! - existencia de cada paquete oficial (consulta `pacman -Si`)
//! - existencia de cada paquete AUR (consulta el RPC del AUR)
//!
//! Devuelve `ValidateReport` con todos los resultados. El caller decide
//! como imprimirlo y que exit code usar.

use std::collections::HashSet;
use std::process::Command;

use crate::model::Profile;
use crate::repo_api;
use crate::validate;

/// Resultado de la validacion de un campo del sistema. `Ok` significa
/// "el formato es valido" (no necesariamente que el valor exista en el
/// sistema; eso lo verifica el instalador).
#[derive(Debug, Clone)]
pub struct FieldCheck {
    pub name: &'static str,
    pub value: String,
    pub ok: bool,
}

impl FieldCheck {
    fn ok(name: &'static str, value: impl Into<String>) -> Self {
        Self {
            name,
            value: value.into(),
            ok: true,
        }
    }
    fn bad(name: &'static str, value: impl Into<String>) -> Self {
        Self {
            name,
            value: value.into(),
            ok: false,
        }
    }
}

/// Resultado completo de validar un perfil. `fields_ok` es `true` si
/// todos los `FieldCheck` pasaron. `missing_official` y `missing_aur`
/// son los paquetes que no se encontraron en sus respectivos origenes.
#[derive(Debug, Clone)]
pub struct ValidateReport {
    pub profile_name: String,
    pub fields: Vec<FieldCheck>,
    pub missing_official: Vec<String>,
    pub missing_aur: Vec<String>,
    pub found_official: usize,
    pub found_aur: usize,
    pub api_errors: Vec<String>,
}

impl ValidateReport {
    /// `true` si el perfil es instalable: campos con formato valido y
    /// todos los paquetes existen.
    pub fn is_ok(&self) -> bool {
        self.fields.iter().all(|f| f.ok)
            && self.missing_official.is_empty()
            && self.missing_aur.is_empty()
    }
}

/// Valida un perfil cargado. No aborta: devuelve un report con todo lo
/// que encontro (campos validos, paquetes faltantes, errores de red).
/// El caller decide que hacer con el resultado.
///
/// `validate_packages` se puede poner en `false` para no hacer
/// peticiones de red (chequea solo formato de campos).
pub fn validate(profile: &Profile, validate_packages: bool) -> ValidateReport {
    let mut fields = Vec::new();

    if let Some(loc) = profile.locale.as_deref() {
        if validate::is_valid_locale(loc) {
            fields.push(FieldCheck::ok("locale", loc));
        } else {
            fields.push(FieldCheck::bad("locale", loc));
        }
    }
    if let Some(tz) = profile.timezone.as_deref() {
        if validate::is_valid_timezone(tz) {
            fields.push(FieldCheck::ok("timezone", tz));
        } else {
            fields.push(FieldCheck::bad("timezone", tz));
        }
    }
    if let Some(km) = profile.keymap.as_deref() {
        if validate::is_valid_keymap(km) {
            fields.push(FieldCheck::ok("keymap", km));
        } else {
            fields.push(FieldCheck::bad("keymap", km));
        }
    }
    if let Some(host) = profile.hostname.as_deref() {
        if validate::is_valid_hostname(host) {
            fields.push(FieldCheck::ok("hostname", host));
        } else {
            fields.push(FieldCheck::bad("hostname", host));
        }
    }
    if let Some(region) = profile.mirror_region.as_deref() {
        // El formato de region es libre: reflector acepta casi cualquier
        // cadena. Marcamos ok si no esta vacio tras trim.
        if !region.trim().is_empty() {
            fields.push(FieldCheck::ok("mirror_region", region));
        } else {
            fields.push(FieldCheck::bad("mirror_region", region));
        }
    }

    let mut missing_official = Vec::new();
    let mut missing_aur = Vec::new();
    let mut found_official = 0;
    let mut found_aur = 0;
    let mut api_errors = Vec::new();

    if validate_packages && !profile.official_packages.is_empty() {
        match check_official(&profile.official_packages) {
            Ok(found) => {
                for pkg in &profile.official_packages {
                    if found.contains(pkg) {
                        found_official += 1;
                    } else {
                        missing_official.push(pkg.clone());
                    }
                }
            }
            Err(e) => api_errors.push(format!("oficiales: {e}")),
        }
    }

    if validate_packages && !profile.aur_packages.is_empty() {
        let name_refs: Vec<&str> = profile.aur_packages.iter().map(String::as_str).collect();
        match repo_api::aur_info(&name_refs) {
            Ok(found) => {
                for pkg in &profile.aur_packages {
                    if found.contains(pkg) {
                        found_aur += 1;
                    } else {
                        missing_aur.push(pkg.clone());
                    }
                }
            }
            Err(e) => api_errors.push(format!("AUR: {e}")),
        }
    }

    ValidateReport {
        profile_name: profile.name.clone(),
        fields,
        missing_official,
        missing_aur,
        found_official,
        found_aur,
        api_errors,
    }
}

/// Consulta `pacman -Si` con todos los paquetes en una sola llamada.
/// Devuelve un set con los nombres que pacman conoce. Un fallo de
/// pacman se propaga como Err.
fn check_official(packages: &[String]) -> anyhow::Result<HashSet<String>> {
    if packages.is_empty() {
        return Ok(HashSet::new());
    }
    let pkg_refs: Vec<&str> = packages.iter().map(String::as_str).collect();
    let out = Command::new("pacman").arg("-Si").args(&pkg_refs).output()?;
    if !out.status.success() {
        // pacman puede devolver exito parcial (algunos paquetes no
        // encontrados). Aun asi, parseamos la salida.
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut found = HashSet::new();
    for line in stdout.lines() {
        if let Some(name) = line.strip_prefix("Name            : ") {
            found.insert(name.trim().to_string());
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_profile() -> Profile {
        Profile {
            name: "test".into(),
            desktop_environment: Some("hyprland".into()),
            display_manager: Some("sddm".into()),
            official_packages: vec!["firefox".into(), "vim".into()],
            aur_packages: vec!["spotify".into()],
            mirror_region: Some("Mexico".into()),
            locale: None,
            timezone: None,
            keymap: None,
            hostname: None,
        }
    }

    #[test]
    fn fields_validate_format() {
        let p = sample_profile();
        let r = validate(&p, false);
        // Sin chequear paquetes: solo formato de campos del sistema.
        // sample_profile no tiene locale/timezone/keymap/hostname, asi
        // que fields queda vacio (o casi).
        let all_ok = r.fields.iter().all(|f| f.ok);
        assert!(all_ok);
    }

    #[test]
    fn fields_detect_bad_locale() {
        let mut p = sample_profile();
        // Hack: meter un locale con formato invalido.
        p.locale = Some("BAD LOCALE WITH SPACES".into());
        let r = validate(&p, false);
        let locale_check = r.fields.iter().find(|f| f.name == "locale").unwrap();
        assert!(!locale_check.ok);
    }

    #[test]
    fn fields_detect_bad_timezone() {
        let mut p = sample_profile();
        p.timezone = Some("../etc/passwd".into());
        let r = validate(&p, false);
        let tz = r.fields.iter().find(|f| f.name == "timezone").unwrap();
        assert!(!tz.ok);
    }

    #[test]
    fn fields_detect_bad_hostname() {
        let mut p = sample_profile();
        p.hostname = Some("-invalid-".into());
        let r = validate(&p, false);
        let h = r.fields.iter().find(|f| f.name == "hostname").unwrap();
        assert!(!h.ok);
    }

    #[test]
    fn is_ok_requires_no_missing() {
        let mut r = ValidateReport {
            profile_name: "x".into(),
            fields: vec![],
            missing_official: vec!["foo".into()],
            missing_aur: vec![],
            found_official: 0,
            found_aur: 0,
            api_errors: vec![],
        };
        assert!(!r.is_ok());
        r.missing_official.clear();
        r.missing_aur.push("bar".into());
        assert!(!r.is_ok());
        r.missing_aur.clear();
        assert!(r.is_ok());
    }
}
