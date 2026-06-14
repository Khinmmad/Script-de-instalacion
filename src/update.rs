//! Aviso de actualizaciones del propio programa.
//!
//! Consulta la API de GitHub para ver si hay un release mas nuevo que la
//! version compilada (`CARGO_PKG_VERSION`). Si lo hay, devuelve el tag
//! correspondiente. Cualquier fallo (sin internet, rate-limit, etc.) se
//! trata como "sin aviso": el programa funciona igual y el usuario no se
//! entera.

use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;

const API_URL: &str = "https://api.github.com/repos/Khinmmad/Script-de-instalacion/releases/latest";
const TIMEOUT: Duration = Duration::from_secs(5);
const USER_AGENT: &str = concat!("arch-postinstall/", env!("CARGO_PKG_VERSION"));

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

/// Devuelve el `tag_name` del ultimo release, o `Err` si no se pudo
/// consultar. Usado internamente por `check_for_update`.
fn latest_release() -> Result<String> {
    let body = ureq::get(API_URL)
        .set("User-Agent", USER_AGENT)
        .timeout(TIMEOUT)
        .call()
        .map_err(|e| anyhow::anyhow!("consulta de actualizaciones: {e}"))?
        .into_string()
        .map_err(|e| anyhow::anyhow!("respuesta ilegible: {e}"))?;
    let r: Release =
        serde_json::from_str(&body).map_err(|e| anyhow::anyhow!("JSON invalido de GitHub: {e}"))?;
    Ok(r.tag_name)
}

/// Si hay un release mas nuevo que la version actual, devuelve su tag
/// (sin prefijo `v`); si no, devuelve `None`. Falla silenciosamente
/// (devuelve `None`) si no se puede consultar.
pub fn check_for_update() -> Option<String> {
    let current = env!("CARGO_PKG_VERSION");
    let latest = latest_release().ok()?;
    if is_newer(&latest, current) {
        Some(latest.trim_start_matches('v').to_string())
    } else {
        None
    }
}

/// Compara dos versiones semver `X.Y.Z` (con prefijo `v` opcional).
/// Devuelve `true` si `latest` es estrictamente mayor que `current`.
/// Componente a componente: si una version tiene menos partes, las
/// faltantes se cuentan como 0 (`0.8` == `0.8.0`).
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .split('.')
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let l = parse(latest);
    let c = parse(current);
    for i in 0..l.len().max(c.len()) {
        let lv = l.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if lv > cv {
            return true;
        }
        if lv < cv {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_comparison() {
        // Casos claros.
        assert!(is_newer("0.8.0", "0.7.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.7.1", "0.7.0"));
        // Iguales: no es "nuevo".
        assert!(!is_newer("0.7.0", "0.7.0"));
        // Mas viejo.
        assert!(!is_newer("0.6.0", "0.7.0"));
        // Con prefijo v.
        assert!(is_newer("v0.8.0", "v0.7.0"));
        assert!(is_newer("v0.8.0", "0.7.0"));
        // Distinto numero de partes.
        assert!(is_newer("0.8", "0.7.9"));
        assert!(!is_newer("0.7.9", "0.8"));
    }
}
