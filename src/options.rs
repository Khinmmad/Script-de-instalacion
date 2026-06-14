//! Listas de opciones para los pickers buscables de la TUI.
//!
//! Cada funcion intenta leer la lista "real" del sistema (que es la
//! mejor fuente en un Arch recien instalado) y, si falla, devuelve una
//! lista corta de fallback. Nunca aborta: si no hay datos, devuelve
//! algo razonable para que el picker siga siendo util.

use std::path::Path;

/// Locales disponibles. Lee `/usr/share/i18n/SUPPORTED` (un locale por
/// linea, con el codeset como segunda columna). Si no existe, devuelve
/// una lista corta de los mas comunes.
pub fn locales() -> Vec<String> {
    if let Ok(body) = std::fs::read_to_string("/usr/share/i18n/SUPPORTED") {
        let mut out: Vec<String> = body
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .map(|s| s.to_string())
            .collect();
        out.sort();
        out.dedup();
        if !out.is_empty() {
            return out;
        }
    }
    fallback_locales()
}

/// Zonas horarias IANA. Recorre `/usr/share/zoneinfo/` recursivamente,
/// saltando las bases alternativas (`posix/`, `right/`).
pub fn timezones() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(()) = walk_zoneinfo(Path::new("/usr/share/zoneinfo"), &mut out) {
        out.sort();
        out.dedup();
        if !out.is_empty() {
            return out;
        }
    }
    fallback_timezones()
}

/// Keymaps de consola. Primero intenta `localectl list-keymaps` (la
/// fuente canonica en systemd). Si no, recorre
/// `/usr/share/kbd/keymaps/` y deriva el nombre quitando prefijos y
/// extensiones (`.gz`, `.map`).
pub fn keymaps() -> Vec<String> {
    if let Ok(out) = std::process::Command::new("localectl")
        .arg("list-keymaps")
        .output()
    {
        if out.status.success() {
            let keys: Vec<String> = String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            if !keys.is_empty() {
                let mut sorted = keys;
                sorted.sort();
                return sorted;
            }
        }
    }
    if let Ok(mut keys) = walk_keymaps(Path::new("/usr/share/kbd/keymaps")) {
        keys.sort();
        keys.dedup();
        if !keys.is_empty() {
            return keys;
        }
    }
    fallback_keymaps()
}

fn walk_zoneinfo(base: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    if !base.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no es directorio",
        ));
    }
    for entry in std::fs::read_dir(base)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Saltamos las bases alternativas; no son lo que el usuario quiere.
        if name == "posix" || name == "right" {
            continue;
        }
        if path.is_dir() {
            walk_zoneinfo(&path, out)?;
        } else {
            // Saltamos archivos que no son zonas (p.ej. zone.tab).
            if name.ends_with(".tab") || name.starts_with(".") {
                continue;
            }
            let rel = path.strip_prefix(base).unwrap();
            out.push(rel.to_string_lossy().to_string());
        }
    }
    Ok(())
}

fn walk_keymaps(base: &Path) -> std::io::Result<Vec<String>> {
    let mut out = Vec::new();
    if !base.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no es directorio",
        ));
    }
    recurse_keymaps(base, base, &mut out)?;
    Ok(out)
}

fn recurse_keymaps(p: &Path, base: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(p)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            recurse_keymaps(&path, base, out)?;
        } else {
            let rel = path.strip_prefix(base).unwrap();
            let s = rel.to_string_lossy();
            // Quitamos extensiones y el ".gz" de los packs.
            let s = s.trim_end_matches(".gz");
            let s = s.trim_end_matches(".map");
            let s = s.trim_end_matches(".kmap");
            out.push(s.to_string());
        }
    }
    Ok(())
}

// ---------------------- fallbacks ----------------------

fn fallback_locales() -> Vec<String> {
    let mut v = vec![
        "C",
        "POSIX",
        "en_US.UTF-8",
        "en_GB.UTF-8",
        "es_ES.UTF-8",
        "es_MX.UTF-8",
        "es_AR.UTF-8",
        "es_CL.UTF-8",
        "fr_FR.UTF-8",
        "de_DE.UTF-8",
        "it_IT.UTF-8",
        "pt_BR.UTF-8",
        "pt_PT.UTF-8",
        "ru_RU.UTF-8",
        "ja_JP.UTF-8",
        "zh_CN.UTF-8",
    ];
    v.sort();
    v.into_iter().map(String::from).collect()
}

fn fallback_timezones() -> Vec<String> {
    let mut v = vec![
        "UTC",
        "America/Mexico_City",
        "America/New_York",
        "America/Chicago",
        "America/Denver",
        "America/Los_Angeles",
        "America/Argentina/Buenos_Aires",
        "America/Santiago",
        "America/Bogota",
        "America/Lima",
        "Europe/Madrid",
        "Europe/London",
        "Europe/Paris",
        "Europe/Berlin",
        "Europe/Rome",
        "Europe/Lisbon",
    ];
    v.sort();
    v.into_iter().map(String::from).collect()
}

fn fallback_keymaps() -> Vec<String> {
    let mut v = vec![
        "us",
        "es",
        "la-latin1",
        "uk",
        "de",
        "de-latin1",
        "fr",
        "fr-latin1",
        "it",
        "pt",
    ];
    v.sort();
    v.into_iter().map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallbacks_are_non_empty_and_sorted() {
        let l = fallback_locales();
        assert!(l.len() >= 5);
        assert!(l.windows(2).all(|w| w[0] <= w[1]));

        let t = fallback_timezones();
        assert!(t.len() >= 5);
        assert!(t.windows(2).all(|w| w[0] <= w[1]));

        let k = fallback_keymaps();
        assert!(k.len() >= 5);
        assert!(k.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn lists_return_something_even_on_minimal_systems() {
        // Aunque no haya /usr/share/i18n/SUPPORTED ni zoneinfo, las
        // funciones deben devolver los fallbacks.
        let l = locales();
        assert!(!l.is_empty());
        let t = timezones();
        assert!(!t.is_empty());
        let k = keymaps();
        assert!(!k.is_empty());
    }
}
