//! Busqueda de paquetes en vivo usando las APIs oficiales de Arch Linux.
//!
//! - Repositorios oficiales: https://archlinux.org/packages/search/json/
//! - AUR (RPC v5):           https://aur.archlinux.org/rpc/v5/search/
//!
//! Ambas son APIs publicas y estables. Las peticiones llevan timeout para no
//! colgar la interfaz si la red va lenta.

use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::model::Source;

/// Un paquete encontrado en una busqueda.
pub struct Found {
    pub name: String,
    pub description: String,
}

const TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!("arch-postinstall/", env!("CARGO_PKG_VERSION"));

/// Codifica de forma minima un termino para usarlo en una URL.
fn url_encode(term: &str) -> String {
    let mut out = String::with_capacity(term.len());
    for b in term.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn get_json(url: &str) -> Result<String> {
    let body = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .timeout(TIMEOUT)
        .call()
        .context("No se pudo conectar (¿hay internet?)")?
        .into_string()
        .context("Respuesta ilegible del servidor")?;
    Ok(body)
}

// ----------------------------- AUR -----------------------------

#[derive(Deserialize)]
struct AurResponse {
    #[serde(default)]
    results: Vec<AurPackage>,
}

#[derive(Deserialize)]
struct AurPackage {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Description")]
    description: Option<String>,
}

/// Busca en el AUR por nombre y descripcion.
pub fn search_aur(term: &str) -> Result<Vec<Found>> {
    let url = format!(
        "https://aur.archlinux.org/rpc/v5/search/{}?by=name-desc",
        url_encode(term)
    );
    let body = get_json(&url)?;
    let resp: AurResponse = serde_json::from_str(&body).context("JSON invalido del AUR")?;
    let mut out: Vec<Found> = resp
        .results
        .into_iter()
        .map(|p| Found {
            name: p.name,
            description: p.description.unwrap_or_default(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.truncate(50);
    Ok(out)
}

// -------------------------- Oficiales --------------------------

#[derive(Deserialize)]
struct OfficialResponse {
    #[serde(default)]
    results: Vec<OfficialPackage>,
}

#[derive(Deserialize)]
struct OfficialPackage {
    pkgname: String,
    pkgdesc: Option<String>,
}

/// Busca en los repositorios oficiales (core/extra/multilib).
pub fn search_official(term: &str) -> Result<Vec<Found>> {
    let url = format!(
        "https://archlinux.org/packages/search/json/?q={}",
        url_encode(term)
    );
    let body = get_json(&url)?;
    let resp: OfficialResponse =
        serde_json::from_str(&body).context("JSON invalido de archlinux.org")?;

    // El mismo paquete puede venir repetido por arquitectura: deduplicamos.
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<Found> = Vec::new();
    for p in resp.results {
        if seen.insert(p.pkgname.clone()) {
            out.push(Found {
                name: p.pkgname,
                description: p.pkgdesc.unwrap_or_default(),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.truncate(50);
    Ok(out)
}

/// Busca segun el origen indicado.
pub fn search(source: Source, term: &str) -> Result<Vec<Found>> {
    match source {
        Source::Official => search_official(term),
        Source::Aur => search_aur(term),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encode_keeps_safe_chars() {
        assert_eq!(
            url_encode("visual-studio-code-bin"),
            "visual-studio-code-bin"
        );
        assert_eq!(url_encode("python3.12_x"), "python3.12_x");
    }

    #[test]
    fn url_encode_escapes_special_chars() {
        assert_eq!(url_encode("a b"), "a%20b");
        assert_eq!(url_encode("c++"), "c%2B%2B");
        assert_eq!(url_encode("ñoño"), "%C3%B1o%C3%B1o");
    }

    #[test]
    fn aur_response_parses_real_shape() {
        let body = r#"{"resultcount":1,"results":[{"Name":"spotify","Description":"A streaming service"}],"type":"search","version":5}"#;
        let resp: AurResponse = serde_json::from_str(body).unwrap();
        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].name, "spotify");
    }

    #[test]
    fn official_response_parses_real_shape() {
        let body = r#"{"version":2,"results":[{"pkgname":"firefox","pkgdesc":"Browser","repo":"extra","arch":"x86_64"}]}"#;
        let resp: OfficialResponse = serde_json::from_str(body).unwrap();
        assert_eq!(resp.results.len(), 1);
        assert_eq!(resp.results[0].pkgname, "firefox");
    }
}
