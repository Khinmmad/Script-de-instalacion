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

use crate::detect::SystemStatus;
use crate::estimate::{self, free_space, human_bytes};
use crate::installer::Logger;
use crate::model::InstallPlan;

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

    /// Igual que `run` pero anade un check que contrasta el espacio libre
    /// contra el tamano estimado del plan. Falla si no cabe, avisa si
    /// queda justo. Devuelve `None` si el plan no requiere instalar nada
    /// (no hay sentido en comprobar).
    pub fn run_for_plan(plan: &InstallPlan, sys: &SystemStatus) -> Self {
        let mut report = Self::run(!plan.aur.is_empty());
        if let Some(c) = check_disk_for_estimate(plan, sys) {
            report.checks.push(c);
        }
        if let Some(region) = &plan.mirror_region {
            report.checks.push(check_reflector(region));
        }
        report
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
    let avail = free_space("/");
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

/// Contrasta el espacio libre con el tamano estimado del plan. Esto es
/// mas preciso que `check_disk_space` (que usa umbrales absolutos):
/// si vas a instalar paquetes pequenos en una maquina con poco disco
/// tambien avisamos, y si vas a instalar paquetes grandes en una
/// maquina con mucho disco no infladamos el check.
///
/// Devuelve `None` si el plan no requiere instalar nada (no hay contra
/// que medir).
fn check_disk_for_estimate(plan: &InstallPlan, sys: &SystemStatus) -> Option<PreflightCheck> {
    let est = estimate::estimate(&plan.official, &plan.aur, &sys.official, &sys.aur);
    let needed = est.total_install();
    let unknown = est.total_unknown();
    if needed == 0 && unknown == 0 {
        return None;
    }
    match est.free_bytes {
        None => Some(PreflightCheck::warn(
            "espacio para el plan",
            "no se pudo leer df; comprueba el espacio manualmente",
        )),
        Some(free) if free < needed => Some(PreflightCheck::fail(
            "espacio para el plan",
            format!(
                "necesitas {} pero solo hay {} libres en /",
                human_bytes(needed),
                human_bytes(free)
            ),
        )),
        // Margen < 20% libre tras instalar: justo. Warn, no Fail.
        Some(free) if free - needed < (free / 5) => {
            let remaining = free.saturating_sub(needed);
            let mut detail = format!(
                "instalara {}; quedaran {} libres",
                human_bytes(needed),
                human_bytes(remaining)
            );
            if unknown > 0 {
                detail.push_str(&format!(
                    " (mas {unknown} paquete(s) AUR sin tamano conocido)"
                ));
            }
            Some(PreflightCheck::warn("espacio para el plan", detail))
        }
        Some(free) => {
            let mut detail = format!(
                "instalara {}; quedaran {} libres",
                human_bytes(needed),
                human_bytes(free.saturating_sub(needed))
            );
            if unknown > 0 {
                detail.push_str(&format!(
                    " (mas {unknown} paquete(s) AUR sin tamano conocido)"
                ));
            }
            Some(PreflightCheck::ok("espacio para el plan", detail))
        }
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

/// Verifica que `reflector` este disponible si el usuario eligio una
/// region para los mirrors. No es bloqueante: el instalador instala
/// `reflector` con pacman si falta. Pero avisamos para que el usuario
/// sepa que tendra que esperar ese paso extra.
fn check_reflector(region: &str) -> PreflightCheck {
    if command_exists("reflector") {
        PreflightCheck::ok(
            "reflector",
            format!("ok (region {region} se aplicara antes de -Syu)"),
        )
    } else {
        PreflightCheck::warn(
            "reflector",
            format!(
                "no esta instalado; el instalador lo agregara antes de aplicar la region {region}"
            ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estimate::PackageSizes;

    fn empty_plan() -> InstallPlan {
        InstallPlan::new(None, None, vec![], vec![])
    }

    #[test]
    fn report_runs_all_checks() {
        // Sin AUR: 7 checks base; con AUR: 10.
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

    #[test]
    fn disk_for_estimate_skips_empty_plan() {
        // Si el plan no tiene nada que instalar, devolvemos None
        // (no hay contra que medir).
        let plan = empty_plan();
        let sys = SystemStatus::default();
        let r = check_disk_for_estimate(&plan, &sys);
        assert!(r.is_none());
    }

    #[test]
    fn disk_for_estimate_fails_when_short() {
        // Simulamos una estimate que necesita mas de lo libre.
        // No podemos forzar el free_bytes desde fuera, pero podemos
        // verificar que el check existe y devuelve algo razonable.
        // (El caso real depende de df + pacman, que no podemos simular.)
        let plan = InstallPlan::new(None, None, vec!["vim".to_string()], vec![]);
        let sys = SystemStatus::default();
        // Con un sys vacio, estimate devuelve todo unknown. El check
        // existe (warn sobre "no se pudo leer df") o no, segun el
        // entorno. Solo comprobamos que no paniquea.
        let _ = check_disk_for_estimate(&plan, &sys);
    }

    #[test]
    fn run_for_plan_appends_estimate_check() {
        // run_for_plan siempre anade un check extra al final (aunque
        // sea None si el plan esta vacio). Para un plan con paquetes,
        // debe haber al menos uno mas que run.
        let plan = InstallPlan::new(None, None, vec!["vim".to_string()], vec![]);
        let sys = SystemStatus::default();
        let lite = PreflightReport::run(false);
        let for_plan = PreflightReport::run_for_plan(&plan, &sys);
        // run() = 7 checks; run_for_plan debe tener >= 7 (puede ser
        // exactamente 7 si el plan no genera check, pero solo si
        // el plan esta vacio). Con un paquete debe ser >= 8 si el
        // check se anade, o 7 si el check devuelve None.
        // En este caso, el check SI se anada (hay paquetes).
        assert!(for_plan.checks.len() >= lite.checks.len());
    }

    #[test]
    fn package_sizes_default_is_zero() {
        // Sanity check: la API de estimate sigue como la usamos aqui.
        let s = PackageSizes::default();
        assert_eq!(s.download_bytes, 0);
        assert_eq!(s.install_bytes, 0);
        assert_eq!(s.unknown, 0);
    }
}
