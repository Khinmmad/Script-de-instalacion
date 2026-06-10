//! Ejecucion de la instalacion: pacman, yay y gestores de display.
//!
//! Filosofia de robustez (heredada del README original): un paquete que
//! falla NO aborta el resto. Cada paso se registra en un log y al final se
//! muestra un resumen de exitos y fallos.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use chrono::Local;

use crate::model::InstallPlan;

/// Resultado de un paso individual de instalacion.
pub struct StepResult {
    pub label: String,
    pub ok: bool,
}

/// Logger sencillo que escribe a stdout y a un archivo de log con timestamp.
pub struct Logger {
    file: Option<File>,
    pub path: Option<PathBuf>,
}

impl Logger {
    pub fn new() -> Self {
        match Self::open_file() {
            Ok((file, path)) => Logger { file: Some(file), path: Some(path) },
            Err(_) => Logger { file: None, path: None },
        }
    }

    fn open_file() -> Result<(File, PathBuf)> {
        let base = dirs::state_dir()
            .or_else(dirs::data_local_dir)
            .context("Sin directorio de estado")?;
        let dir = base.join("arch-postinstall");
        fs::create_dir_all(&dir)?;
        let stamp = Local::now().format("%Y%m%d-%H%M%S");
        let path = dir.join(format!("install-{stamp}.log"));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok((file, path))
    }

    pub fn log(&mut self, line: &str) {
        let stamp = Local::now().format("%H:%M:%S");
        println!("{line}");
        if let Some(f) = self.file.as_mut() {
            let _ = writeln!(f, "[{stamp}] {line}");
        }
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new()
    }
}

/// Opciones de ejecucion del instalador.
pub struct InstallOptions {
    /// No ejecuta nada; solo muestra los comandos que correria.
    pub dry_run: bool,
    /// Evita preguntas de pacman/yay (--noconfirm).
    pub noconfirm: bool,
}

/// Indica si el binario corre como root (no recomendado para makepkg/yay).
pub fn is_root() -> bool {
    // SAFETY: geteuid() de libc no esta disponible sin la crate; usamos el id
    // expuesto por el entorno como heuristica portable.
    std::env::var("EUID")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .map(|id| id == 0)
        .unwrap_or_else(|| {
            // Fallback: preguntar a `id -u`.
            Command::new("id")
                .arg("-u")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim() == "0")
                .unwrap_or(false)
        })
}

/// True si `yay` ya esta instalado en el PATH.
pub fn yay_present() -> bool {
    Command::new("yay")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Ejecuta un comando registrandolo. Devuelve true si tuvo exito.
fn run(log: &mut Logger, opts: &InstallOptions, program: &str, args: &[&str]) -> bool {
    let pretty = format!("{program} {}", args.join(" "));
    if opts.dry_run {
        log.log(&format!("[dry-run] {pretty}"));
        return true;
    }
    log.log(&format!("$ {pretty}"));
    match Command::new(program).args(args).status() {
        Ok(status) if status.success() => true,
        Ok(status) => {
            log.log(&format!("  ! fallo (codigo {:?}): {pretty}", status.code()));
            false
        }
        Err(e) => {
            log.log(&format!("  ! no se pudo ejecutar '{program}': {e}"));
            false
        }
    }
}

/// Asegura que yay este instalado, compilandolo desde el AUR si hace falta.
fn ensure_yay(log: &mut Logger, opts: &InstallOptions) -> bool {
    if yay_present() {
        log.log("yay ya esta instalado.");
        return true;
    }
    log.log("yay no encontrado: instalando desde el AUR...");
    let mut conf: Vec<&str> = vec!["pacman", "-S", "--needed"];
    if opts.noconfirm {
        conf.push("--noconfirm");
    }
    conf.extend(["git", "base-devel"]);
    if !run(log, opts, "sudo", &conf) {
        return false;
    }
    let tmp = "/tmp/yay-postinstall";
    let _ = fs::remove_dir_all(tmp);
    if !run(log, opts, "git", &["clone", "https://aur.archlinux.org/yay.git", tmp]) {
        return false;
    }
    let mut mk: Vec<&str> = vec!["-si"];
    if opts.noconfirm {
        mk.push("--noconfirm");
    }
    // makepkg debe correr dentro del directorio clonado.
    if opts.dry_run {
        log.log(&format!("[dry-run] (cd {tmp} && makepkg {})", mk.join(" ")));
        return true;
    }
    log.log(&format!("$ (cd {tmp} && makepkg {})", mk.join(" ")));
    Command::new("makepkg")
        .args(&mk)
        .current_dir(tmp)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Instala un solo paquete con el gestor indicado. Devuelve StepResult.
fn install_one(log: &mut Logger, opts: &InstallOptions, manager: &str, pkg: &str) -> StepResult {
    let mut args: Vec<&str> = vec!["-S", "--needed"];
    if opts.noconfirm {
        args.push("--noconfirm");
    }
    args.push(pkg);

    let ok = if manager == "pacman" {
        let mut full = vec!["pacman"];
        full.extend(args);
        run(log, opts, "sudo", &full)
    } else {
        run(log, opts, manager, &args)
    };

    StepResult { label: format!("{manager}: {pkg}"), ok }
}

/// Ejecuta el plan completo y devuelve los resultados de cada paso.
pub fn execute(plan: &InstallPlan, opts: &InstallOptions, log: &mut Logger) -> Vec<StepResult> {
    let mut results = Vec::new();

    log.log("==> Sincronizando bases de datos y sistema (pacman -Syu)");
    let mut syu = vec!["pacman", "-Syu"];
    if opts.noconfirm {
        syu.push("--noconfirm");
    }
    let synced = run(log, opts, "sudo", &syu);
    results.push(StepResult { label: "pacman -Syu".into(), ok: synced });

    if !plan.official.is_empty() {
        log.log("==> Instalando paquetes oficiales (uno por uno para robustez)");
        for pkg in &plan.official {
            results.push(install_one(log, opts, "pacman", pkg));
        }
    }

    if !plan.aur.is_empty() {
        log.log("==> Preparando AUR");
        if ensure_yay(log, opts) {
            for pkg in &plan.aur {
                results.push(install_one(log, opts, "yay", pkg));
            }
        } else {
            log.log("  ! No se pudo preparar yay; se omiten los paquetes AUR.");
            for pkg in &plan.aur {
                results.push(StepResult { label: format!("yay: {pkg}"), ok: false });
            }
        }
    }

    if let Some(dm) = &plan.display_manager {
        log.log(&format!("==> Habilitando display manager: {dm}"));
        let ok = run(log, opts, "sudo", &["systemctl", "enable", &format!("{dm}.service")]);
        results.push(StepResult { label: format!("enable {dm}"), ok });
    }

    // NetworkManager casi siempre se quiere activo.
    if plan.official.iter().any(|p| p == "networkmanager") {
        let ok = run(log, opts, "sudo", &["systemctl", "enable", "NetworkManager.service"]);
        results.push(StepResult { label: "enable NetworkManager".into(), ok });
    }

    results
}

/// Imprime un resumen final legible.
pub fn print_summary(results: &[StepResult], log: &mut Logger) {
    let ok = results.iter().filter(|r| r.ok).count();
    let failed: Vec<&StepResult> = results.iter().filter(|r| !r.ok).collect();
    log.log("");
    log.log(&format!(
        "==> Resumen: {ok}/{} pasos correctos.",
        results.len()
    ));
    if failed.is_empty() {
        log.log("    Todo se instalo correctamente.");
    } else {
        log.log(&format!("    {} pasos fallaron:", failed.len()));
        for f in failed {
            log.log(&format!("      - {}", f.label));
        }
    }
    if let Some(path) = log.path.clone() {
        log.log(&format!("    Log completo: {}", path.display()));
    }
}
