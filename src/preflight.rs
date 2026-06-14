//! Pre-flight checks: verifican que el sistema esta listo para instalar
//! antes de empezar. Pensado para el caso tipico de alguien que acaba
//! de instalar Arch (con `archinstall` o a mano) y todavia puede tener
//! cosas sin configurar: sudo sin password, sin internet, pacman con
//! la DB bloqueada, sin espacio, /etc no escribible, o -si eligio AUR-
//! sin `git`/`base-devel`.
//!
//! Cada check es independiente. Si uno falla, los demas siguen. El
//! `PreflightReport` resultante se muestra en la pantalla de revision
//! de la TUI y se registra en el log al empezar `installer::execute`.

use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::installer::Logger;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    /// Paso dado, todo bien.
    Ok,
    /// Aviso: no bloquea la instalacion, pero el usuario deberia saberlo.
    Warn,
    /// Algo critico que probablemente hara fallar la instalacion.
    Fail,
}

#[derive(Debug, Clone)]
pub struct PreflightCheck {
    pub name: &'static str,
    pub status: CheckStatus,
    pub detail: String,
}

impl PreflightCheck {
    fn ok(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Ok,
            detail: detail.into(),
        }
    }
    fn warn(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Warn,
            detail: detail.into(),
        }
    }
    fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Fail,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PreflightReport {
    pub checks: Vec<PreflightCheck>,
}

impl PreflightReport {
    /// Ejecuta todos los checks. `plan_needs_aur` activa las verificaciones
    /// extra de `git` y `base-devel` (que son obligatorios para compilar
    /// paquetes del AUR).
    pub fn run(plan_needs_aur: bool) -> Self {
        let mut checks = vec![
            check_not_root(),
            check_sudo_installed(),
            check_sudo_nopasswd(),
            check_internet(),
            check_pacman_db(),
            check_disk_space(),
            check_etc_writable(),
        ];
        if plan_needs_aur {
            checks.push(check_aur_tool("git", "git"));
            checks.push(check_aur_tool("make", "make"));
            checks.push(check_aur_tool("gcc", "gcc"));
        }
        Self { checks }
    }

    /// `true` si ningun check dio `Fail`. Los `Warn` no cuentan.
    pub fn all_ok(&self) -> bool {
        !self.checks.iter().any(|c| c.status == CheckStatus::Fail)
    }

    pub fn has_warnings(&self) -> bool {
        self.checks.iter().any(|c| c.status == CheckStatus::Warn)
    }

    pub fn has_failures(&self) -> bool {
        self.checks.iter().any(|c| c.status == CheckStatus::Fail)
    }

    /// Imprime cada check con su marcador. Devuelve `true` si no hay fallos.
    pub fn log(&self, log: &mut Logger) -> bool {
        log.log("==> Pre-flight checks");
        for c in &self.checks {
            let marker = match c.status {
                CheckStatus::Ok => "  OK  ",
                CheckStatus::Warn => " WARN ",
                CheckStatus::Fail => " FAIL ",
            };
            log.log(&format!("  [{marker}] {:<22} {}", c.name, c.detail));
        }
        self.all_ok()
    }
}

// ---------------------- checks individuales ----------------------

fn check_not_root() -> PreflightCheck {
    if crate::installer::is_root() {
        PreflightCheck::warn(
            "ejecutando como root",
            "makepkg/yay no deben correr como root; la instalacion de AUR puede fallar",
        )
    } else {
        PreflightCheck::ok("ejecutando como root", "ok (usuario normal)")
    }
}

fn check_sudo_installed() -> PreflightCheck {
    if command_exists("sudo") {
        PreflightCheck::ok("sudo instalado", "ok")
    } else {
        PreflightCheck::fail(
            "sudo instalado",
            "sudo no esta en el PATH; instala 'sudo' y aniade tu usuario al grupo wheel",
        )
    }
}

fn check_sudo_nopasswd() -> PreflightCheck {
    // `sudo -n true` falla con codigo 1 si requiere password. Si funciona,
    // el usuario tiene NOPASSWD y la instalacion no se interrumpira.
    match Command::new("sudo").args(["-n", "true"]).status() {
        Ok(s) if s.success() => {
            PreflightCheck::ok("sudo sin password", "ok (NOPASSWD configurado)")
        }
        Ok(_) => PreflightCheck::warn(
            "sudo sin password",
            "sudo pedira contrasena durante la instalacion; manten la terminal cerca",
        ),
        Err(_) => PreflightCheck::warn(
            "sudo sin password",
            "no se pudo comprobar sudo; la instalacion podria fallar",
        ),
    }
}

fn check_internet() -> PreflightCheck {
    // Conexion TCP a un host conocido. No usa curl/ping (pueden no estar)
    // y evita depender del DNS del sistema (resuelve con ToSocketAddrs).
    let targets = [
        ("archlinux.org", 443),
        ("aur.archlinux.org", 443),
        ("1.1.1.1", 443),
    ];
    for (host, port) in targets {
        if let Ok(mut addrs) = (host, port).to_socket_addrs() {
            if let Some(addr) = addrs.next() {
                if TcpStream::connect_timeout(&addr, Duration::from_secs(5)).is_ok() {
                    return PreflightCheck::ok("conexion a internet", format!("ok ({host})"));
                }
            }
        }
    }
    PreflightCheck::fail(
        "conexion a internet",
        "no se pudo conectar a archlinux.org ni a 1.1.1.1; revisa tu red/DNS",
    )
}

fn check_pacman_db() -> PreflightCheck {
    let lock = Path::new("/var/lib/pacman/db.lck");
    if lock.exists() {
        PreflightCheck::fail(
            "DB de pacman",
            "existe /var/lib/pacman/db.lck; otra instancia corre o quedo colgado",
        )
    } else {
        PreflightCheck::ok("DB de pacman", "ok (sin lock)")
    }
}

fn check_disk_space() -> PreflightCheck {
    // 2 GB es el minimo absoluto; 5 GB es comodo. Avisamos si < 2 GB,
    // bloqueamos solo si < 500 MB.
    let avail = free_bytes("/");
    match avail {
        None => PreflightCheck::warn("espacio en disco", "no se pudo leer df"),
        Some(b) if b < 500 * 1024 * 1024 => PreflightCheck::fail(
            "espacio en disco",
            format!(
                "quedan {} libres; la instalacion probablemente no quepa",
                human_bytes(b)
            ),
        ),
        Some(b) if b < 2 * 1024 * 1024 * 1024 => PreflightCheck::warn(
            "espacio en disco",
            format!(
                "quedan {} libres; recomendable tener al menos 5 GB",
                human_bytes(b)
            ),
        ),
        Some(b) => PreflightCheck::ok("espacio en disco", format!("{} libres", human_bytes(b))),
    }
}

fn check_etc_writable() -> PreflightCheck {
    // Si somos root, /etc es escribible sin mas.
    if crate::installer::is_root() {
        return PreflightCheck::ok("/etc escribible", "ok (root)");
    }
    // Como usuario normal, intentamos crear un archivo temporal. Si
    // funciona, es escribible (modo raro pero posible). Si falla con
    // EACCES u otro, dejamos que sudo se encargue; marcamos warn.
    let path = "/etc/.arch-postinstall-preflight";
    let res = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .and_then(|_| fs::remove_file(path));
    match res {
        Ok(()) => PreflightCheck::ok("/etc escribible", "ok"),
        Err(_) => PreflightCheck::warn(
            "/etc escribible",
            "no se puede escribir directamente; el instalador usara sudo",
        ),
    }
}

fn check_aur_tool(name: &'static str, pkg: &str) -> PreflightCheck {
    if command_exists(name) {
        PreflightCheck::ok("herramienta AUR", format!("{pkg} instalado (ok)"))
    } else {
        PreflightCheck::warn(
            "herramienta AUR",
            format!("{pkg} no esta instalado; el instalador lo agregara al setup"),
        )
    }
}

// ---------------------- helpers ----------------------

fn command_exists(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn free_bytes(mount: &str) -> Option<u64> {
    let out = Command::new("df").args(["-B1", mount]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().nth(1)?;
    let cols: Vec<&str> = line.split_whitespace().collect();
    cols.get(3).and_then(|s| s.parse().ok())
}

fn human_bytes(b: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if b >= GB {
        format!("{:.1} GB", b as f64 / GB as f64)
    } else if b >= MB {
        format!("{:.0} MB", b as f64 / MB as f64)
    } else {
        format!("{:.0} KB", b as f64 / KB as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_format() {
        assert_eq!(human_bytes(500), "0 KB");
        assert_eq!(human_bytes(2 * 1024 * 1024), "2 MB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
    }

    #[test]
    fn report_runs_all_checks() {
        // Sin AUR: 7 checks; con AUR: 10.
        let lite = PreflightReport::run(false);
        assert_eq!(lite.checks.len(), 7);
        let aur = PreflightReport::run(true);
        assert_eq!(aur.checks.len(), 10);
    }

    #[test]
    fn helpers_handle_empty_status() {
        // all_ok / has_warnings / has_failures se evaluan correctamente.
        let empty = PreflightReport { checks: vec![] };
        assert!(empty.all_ok());
        assert!(!empty.has_warnings());
        assert!(!empty.has_failures());
    }
}
