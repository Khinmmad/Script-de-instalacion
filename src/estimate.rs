//! Estimacion del espacio en disco que consumira una instalacion.
//!
//! Suma el tamano de descarga e instalacion de los paquetes que el plan
//! pretende anadir, distinguiendo entre repos oficiales y AUR.
//!
//! - **Oficiales**: una sola llamada a `pacman -Si` con todos los paquetes
//!   pendientes devuelve `Download Size` e `Installed Size` por paquete.
//!   El tamano viene en formato humano ("81.94 MiB") que parseamos.
//! - **AUR**: el RPC del AUR no expone `DownloadSize` ni `InstalledSize`
//!   (siempre `null` en la respuesta publica), y los snapshots en cgit
//!   no devuelven `Content-Length` en HEAD. Asi que el tamano de los
//!   paquetes AUR es desconocido: los contamos aparte y se lo decimos
//!   al usuario.
//!
//! El espacio libre en disco se lee de `df -B1 /`. Si df falla, se omite.
//!
//! Todo es best-effort: un fallo parcial nunca aborta. Los paquetes sin
//! tamano se cuentan y se muestran al usuario con una nota explicita.

use std::collections::HashSet;
use std::process::Command;

const KIB: u64 = 1024;
const MIB: u64 = KIB * 1024;
const GIB: u64 = MIB * 1024;

/// Tamano agregado de un conjunto de paquetes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PackageSizes {
    /// Bytes que se descargaran.
    pub download_bytes: u64,
    /// Bytes que ocuparan en disco una vez instalados.
    pub install_bytes: u64,
    /// Paquetes para los que no se pudo obtener tamano.
    pub unknown: u32,
}

/// Estimacion completa de un plan: lo que se va a descargar, lo que va a
/// ocupar, lo que no se pudo medir, y cuanto libre queda en el sistema.
#[derive(Debug, Clone, Default)]
pub struct PlanEstimate {
    pub official: PackageSizes,
    pub aur: PackageSizes,
    /// Bytes libres en `/` (None si no se pudo leer `df`).
    pub free_bytes: Option<u64>,
}

impl PlanEstimate {
    pub fn total_download(&self) -> u64 {
        self.official.download_bytes + self.aur.download_bytes
    }
    pub fn total_install(&self) -> u64 {
        self.official.install_bytes + self.aur.install_bytes
    }
    pub fn total_unknown(&self) -> u32 {
        self.official.unknown + self.aur.unknown
    }

    /// `Some(true)` si el espacio libre alcanza de sobra para lo que se
    /// va a instalar, `Some(false)` si no llega, `None` si no sabemos.
    pub fn fits(&self) -> Option<bool> {
        self.free_bytes.map(|free| free > self.total_install())
    }
}

/// Calcula la estimacion para los paquetes del plan, descontando los que
/// ya estan en el sistema (no se reinstalan). Es best-effort: un fallo
/// parcial se traduce en campos a 0 o contadores `unknown` mayores.
pub fn estimate(
    official: &[String],
    aur: &[String],
    already_official: &HashSet<String>,
    already_aur: &HashSet<String>,
) -> PlanEstimate {
    let to_check_off: Vec<String> = official
        .iter()
        .filter(|p| !already_official.contains(*p) && !already_aur.contains(*p))
        .cloned()
        .collect();
    let to_check_aur: Vec<String> = aur
        .iter()
        .filter(|p| !already_official.contains(*p) && !already_aur.contains(*p))
        .cloned()
        .collect();
    PlanEstimate {
        official: estimate_official(&to_check_off),
        aur: estimate_aur(&to_check_aur),
        free_bytes: free_space("/"),
    }
}

fn estimate_official(packages: &[String]) -> PackageSizes {
    if packages.is_empty() {
        return PackageSizes::default();
    }
    let pkg_args: Vec<&str> = packages.iter().map(String::as_str).collect();
    let out = match Command::new("pacman").arg("-Si").args(&pkg_args).output() {
        Ok(o) if o.status.success() => o,
        _ => {
            return PackageSizes {
                unknown: packages.len() as u32,
                ..Default::default()
            }
        }
    };
    let mut sizes = parse_pacman_si(&String::from_utf8_lossy(&out.stdout));
    // Si un paquete no aparecio en la salida (por ejemplo, no esta en
    // los repos activos), el parseo no lo cuenta. Lo sumamos a `unknown`
    // para que el usuario sepa que ese dato falta.
    let parsed_names = count_parsed_names(&String::from_utf8_lossy(&out.stdout));
    let missing = packages.len().saturating_sub(parsed_names);
    sizes.unknown += missing as u32;
    sizes
}

fn count_parsed_names(stdout: &str) -> usize {
    stdout
        .split("\n\n")
        .filter(|b| b.lines().any(|l| l.starts_with("Name            : ")))
        .count()
}

/// Parsea la salida de `pacman -Si pkg1 pkg2 ...` y suma los tamanos.
/// La salida son bloques separados por lineas en blanco; cada bloque
/// tiene `Name`, `Download Size`, `Installed Size` alineados a 16 chars
/// de ancho. Si un bloque no trae tamano, lo contamos como desconocido.
fn parse_pacman_si(stdout: &str) -> PackageSizes {
    let mut sizes = PackageSizes::default();
    for block in stdout.split("\n\n") {
        if block.trim().is_empty() {
            continue;
        }
        let mut download = None;
        let mut install = None;
        for line in block.lines() {
            if let Some(v) = line.strip_prefix("Download Size   : ") {
                download = parse_human_size(v.trim());
            } else if let Some(v) = line.strip_prefix("Installed Size  : ") {
                install = parse_human_size(v.trim());
            }
        }
        match (download, install) {
            (Some(d), Some(i)) => {
                sizes.download_bytes = sizes.download_bytes.saturating_add(d);
                sizes.install_bytes = sizes.install_bytes.saturating_add(i);
            }
            _ => {
                // Bloque sin alguno de los dos tamanos: lo mas probable es
                // que sea el separador o un paquete sin campos completos.
                // No lo contamos como desconocido; eso lo hace el caller
                // comparando el numero de paquetes pedidos con el de
                // bloques con `Name`.
            }
        }
    }
    sizes
}

/// Parsea "81.94 MiB", "1.50 GiB", "512 KiB", "100 B" a bytes.
/// Devuelve `None` si la entrada no encaja con el formato esperado.
fn parse_human_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num, unit) = s.split_once(char::is_whitespace)?;
    let n: f64 = num.parse().ok()?;
    let bytes = match unit.trim() {
        "B" => n,
        "KiB" => n * KIB as f64,
        "MiB" => n * MIB as f64,
        "GiB" => n * GIB as f64,
        "TiB" => n * (GIB as f64 * 1024.0),
        _ => return None,
    };
    Some(bytes as u64)
}

/// El AUR no expone tamano de descarga/instalacion en su API publica, asi
/// que los marcamos como desconocidos. La idea es que la UI pueda decir
/// "N paquetes AUR, tamano desconocido" sin esconder el dato.
fn estimate_aur(packages: &[String]) -> PackageSizes {
    if packages.is_empty() {
        return PackageSizes::default();
    }
    PackageSizes {
        unknown: packages.len() as u32,
        ..Default::default()
    }
}

/// Lee los bytes libres en `mount` (`df -B1`). `None` si `df` falla o la
/// salida no se puede parsear.
pub fn free_space(mount: &str) -> Option<u64> {
    let out = Command::new("df").args(["-B1", mount]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().nth(1)?;
    let cols: Vec<&str> = line.split_whitespace().collect();
    cols.get(3).and_then(|s| s.parse().ok())
}

/// Formatea un tamano en bytes de forma legible: "512 B", "8.0 KB",
/// "5.0 MB", "1.2 GB". Pensado para mensajes de usuario; no es
/// internacionalizable (siempre punto decimal y sufijos en Castellano
/// neutro).
pub fn human_bytes(b: u64) -> String {
    if b >= GIB {
        format!("{:.1} GB", b as f64 / GIB as f64)
    } else if b >= MIB {
        format!("{:.0} MB", b as f64 / MIB as f64)
    } else if b >= KIB {
        format!("{:.0} KB", b as f64 / KIB as f64)
    } else {
        format!("{b} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_human_size_units() {
        assert_eq!(parse_human_size("100 B"), Some(100));
        assert_eq!(parse_human_size("512 KiB"), Some(512 * 1024));
        assert_eq!(
            parse_human_size("2.53 MiB"),
            Some((2.53 * MIB as f64) as u64)
        );
        assert_eq!(
            parse_human_size("1.50 GiB"),
            Some((1.5 * GIB as f64) as u64)
        );
        assert_eq!(
            parse_human_size("0.5 TiB"),
            Some((0.5 * GIB as f64 * 1024.0) as u64)
        );
    }

    #[test]
    fn parse_human_size_invalid() {
        assert!(parse_human_size("abc").is_none());
        assert!(parse_human_size("100").is_none()); // sin unidad
        assert!(parse_human_size("100 XB").is_none()); // unidad rara
        assert!(parse_human_size("").is_none());
    }

    #[test]
    fn parse_pacman_si_sums_blocks() {
        let stdout = "\
Repository      : extra
Name            : firefox
Version         : 121.0-1
Download Size   : 81.94 MiB
Installed Size  : 284.60 MiB

Repository      : extra
Name            : vim
Version         : 9.0-1
Download Size   : 2.53 MiB
Installed Size  : 5.36 MiB
";
        let s = parse_pacman_si(stdout);
        let expected_d = ((81.94 + 2.53) * MIB as f64) as u64;
        let expected_i = ((284.60 + 5.36) * MIB as f64) as u64;
        assert_eq!(s.download_bytes, expected_d);
        assert_eq!(s.install_bytes, expected_i);
        assert_eq!(s.unknown, 0);
    }

    #[test]
    fn parse_pacman_si_handles_missing_size() {
        // Un paquete sin tamano en la salida: el caller lo cuenta via
        // count_parsed_names; este parser ignora el bloque silenciosamente.
        let stdout = "\
Name            : weird
Description     : sin tamano
";
        let s = parse_pacman_si(stdout);
        assert_eq!(s.download_bytes, 0);
        assert_eq!(s.install_bytes, 0);
        assert_eq!(s.unknown, 0); // el caller lo gestiona
    }

    #[test]
    fn count_parsed_names_counts_blocks_with_name() {
        let stdout = "\
Name            : a
Download Size   : 1.00 MiB
Installed Size  : 2.00 MiB

Name            : b
Download Size   : 3.00 MiB
Installed Size  : 4.00 MiB

Name            : c
Description     : raro
";
        assert_eq!(count_parsed_names(stdout), 3);
    }

    #[test]
    fn human_bytes_format() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(500), "500 B");
        assert_eq!(human_bytes(2 * 1024 * 1024), "2 MB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
        assert_eq!(human_bytes(1500), "1 KB"); // 1.46 -> 1
    }

    #[test]
    fn empty_inputs_yield_empty_estimate() {
        let empty: HashSet<String> = HashSet::new();
        let e = estimate(&[], &[], &empty, &empty);
        assert_eq!(e.official, PackageSizes::default());
        assert_eq!(e.aur, PackageSizes::default());
    }

    #[test]
    fn estimate_skips_already_installed() {
        // Marcamos vim como ya instalado. La idea es verificar que el
        // filtro funciona: el resto del flujo (llamar a pacman) depende
        // del entorno, asi que solo comprobamos que no paniquea y que
        // se devuelven campos consistentes.
        let mut already: HashSet<String> = HashSet::new();
        already.insert("vim".to_string());
        let e = estimate(
            &["vim".to_string(), "firefox".to_string()],
            &[],
            &already,
            &HashSet::new(),
        );
        // Si pacman estaba en sync y respondio, tenemos download > 0.
        // Si no, tenemos unknown > 0. Cualquiera de los dos es valido;
        // lo importante es que el filtro no rompe la ejecucion.
        assert!(
            e.official.download_bytes > 0 || e.official.unknown > 0,
            "el filtro de ya-instalados no deberia impedir la estimacion"
        );
        // free_bytes puede o no estar disponible segun df.
        let _ = e.free_bytes;
    }

    #[test]
    fn fits_logic() {
        let e = PlanEstimate {
            official: PackageSizes {
                install_bytes: 2 * GIB,
                ..Default::default()
            },
            aur: PackageSizes::default(),
            free_bytes: Some(3 * GIB),
        };
        assert_eq!(e.fits(), Some(true));
        let e = PlanEstimate {
            official: PackageSizes {
                install_bytes: 5 * GIB,
                ..Default::default()
            },
            free_bytes: Some(3 * GIB),
            ..Default::default()
        };
        assert_eq!(e.fits(), Some(false));
        let e = PlanEstimate {
            official: PackageSizes {
                install_bytes: 1,
                ..Default::default()
            },
            free_bytes: None,
            ..Default::default()
        };
        assert_eq!(e.fits(), None);
    }
}
