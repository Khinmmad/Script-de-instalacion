//! Guardado y carga de perfiles de instalacion en disco (TOML).

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::model::Profile;

/// Directorio donde viven los perfiles: ~/.config/arch-postinstall/profiles/
pub fn profiles_dir() -> Result<PathBuf> {
    let base = dirs::config_dir()
        .context("No se pudo determinar el directorio de configuracion del usuario")?;
    Ok(base.join("arch-postinstall").join("profiles"))
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Guarda un perfil como `<nombre>.toml`. Devuelve la ruta escrita.
pub fn save(profile: &Profile) -> Result<PathBuf> {
    let dir = profiles_dir()?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("No se pudo crear el directorio {}", dir.display()))?;

    let file = dir.join(format!("{}.toml", sanitize_name(&profile.name)));
    let body = toml::to_string_pretty(profile).context("No se pudo serializar el perfil a TOML")?;
    fs::write(&file, body).with_context(|| format!("No se pudo escribir {}", file.display()))?;
    Ok(file)
}

/// Carga un perfil por nombre (sin extension).
pub fn load(name: &str) -> Result<Profile> {
    let dir = profiles_dir()?;
    let file = dir.join(format!("{}.toml", sanitize_name(name)));
    let body = fs::read_to_string(&file)
        .with_context(|| format!("No se pudo leer el perfil {}", file.display()))?;
    let profile: Profile =
        toml::from_str(&body).with_context(|| format!("Perfil invalido: {}", file.display()))?;
    Ok(profile)
}

/// Lista los nombres de los perfiles guardados.
pub fn list() -> Result<Vec<String>> {
    let dir = profiles_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}
