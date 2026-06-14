//! Arch Post-Install: asistente TUI para instalar paquetes y entornos de
//! escritorio tras una instalacion limpia de Arch Linux.

mod catalog;
mod detect;
mod installer;
mod model;
mod options;
mod preflight;
mod profile;
mod repo_api;
mod tui;
mod update;
mod validate;

use std::process::ExitCode;

use anyhow::Result;

use installer::{InstallOptions, Logger};
use model::{format_list_or_none, format_system_settings, InstallPlan, Profile, SystemLabelStyle};

use crate::detect::SystemStatus;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct Cli {
    dry_run: bool,
    yes: bool,
    profile: Option<String>,
    list_profiles: bool,
    help: bool,
    version: bool,
}

fn parse_args() -> Cli {
    let mut cli = Cli {
        dry_run: false,
        yes: false,
        profile: None,
        list_profiles: false,
        help: false,
        version: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dry-run" | "-n" => cli.dry_run = true,
            "--yes" | "-y" => cli.yes = true,
            "--list-profiles" | "-l" => cli.list_profiles = true,
            "--help" | "-h" => cli.help = true,
            "--version" | "-V" => cli.version = true,
            "--profile" | "-p" => cli.profile = args.next(),
            other => {
                eprintln!("Argumento desconocido: {other}\n");
                cli.help = true;
            }
        }
    }
    cli
}

fn print_help() {
    println!(
        "arch-postinstall {VERSION}
Asistente de post-instalacion para Arch Linux (TUI + perfiles).

USO:
    arch-postinstall [OPCIONES]

OPCIONES:
    (sin opciones)        Lanza el asistente TUI interactivo.
    -p, --profile <NOMBRE>  Instala directamente desde un perfil guardado.
    -l, --list-profiles   Lista los perfiles guardados y sale.
    -n, --dry-run         Muestra lo que haria sin ejecutar nada.
    -y, --yes             No preguntar confirmacion (usa --noconfirm).
    -h, --help            Muestra esta ayuda.
    -V, --version         Muestra la version.

Los perfiles se guardan en:
    ~/.config/arch-postinstall/profiles/
"
    );
}

/// Pide confirmacion por stdin (a menos que --yes).
fn confirm(prompt: &str) -> bool {
    use std::io::{self, Write};
    print!("{prompt} [s/N]: ");
    let _ = io::stdout().flush();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }
    matches!(
        input.trim().to_lowercase().as_str(),
        "s" | "si" | "sí" | "y" | "yes"
    )
}

/// Imprime un resumen del plan en texto plano.
fn show_plan(plan: &InstallPlan, sys: &SystemStatus) {
    println!("\n── Plan de instalacion ──");
    if let Some(de) = &plan.desktop_env_id {
        println!("  Entorno        : {de}");
    }
    if let Some(dm) = &plan.display_manager {
        println!("  Display manager: {dm}");
    }
    let off_to_install: Vec<String> = plan
        .official
        .iter()
        .filter(|p| !sys.official.contains(*p) && !sys.aur.contains(*p))
        .cloned()
        .collect();
    let off_have: Vec<String> = plan
        .official
        .iter()
        .filter(|p| sys.official.contains(*p) || sys.aur.contains(*p))
        .cloned()
        .collect();
    let aur_to_install: Vec<String> = plan
        .aur
        .iter()
        .filter(|p| !sys.official.contains(*p) && !sys.aur.contains(*p))
        .cloned()
        .collect();
    let aur_have: Vec<String> = plan
        .aur
        .iter()
        .filter(|p| sys.official.contains(*p) || sys.aur.contains(*p))
        .cloned()
        .collect();
    println!(
        "  Por instalar   : {} oficial(es), {} AUR",
        off_to_install.len(),
        aur_to_install.len()
    );
    println!(
        "  Ya instalado   : {} oficial(es), {} AUR",
        off_have.len(),
        aur_have.len()
    );
    if !sys.updates_available.is_empty() {
        println!(
            "  Actualizaciones: {} disponibles (las aplicara pacman -Syu)",
            sys.updates_available.len()
        );
    }
    let mut svcs = plan.services.clone();
    if !plan.user_services.is_empty() {
        svcs.push("audio (PipeWire, --user)".into());
    }
    println!("  Servicios      : {}", format_list_or_none(&svcs));

    let sys_settings = format_system_settings(
        plan.locale.as_deref(),
        plan.timezone.as_deref(),
        plan.keymap.as_deref(),
        plan.hostname.as_deref(),
        plan.enable_multilib,
        plan.reboot_after,
        SystemLabelStyle::Detailed,
    );
    if !sys_settings.is_empty() {
        println!("  Sistema        : {}", sys_settings.join(", "));
    }
    println!();
}

/// Construye las opciones del instalador, incluyendo los paquetes que ya
/// estan en el sistema (se omiten en vez de reinstalarse).
fn make_options(cli: &Cli, sys: &SystemStatus) -> InstallOptions {
    InstallOptions {
        dry_run: cli.dry_run,
        noconfirm: cli.yes,
        skip_official: sys.official.clone(),
        skip_aur: sys.aur.clone(),
    }
}

/// Ejecuta un plan: confirma (si hace falta), guarda log y muestra resumen.
/// `already_confirmed` evita preguntar dos veces cuando el plan ya fue
/// revisado y confirmado en la pantalla de revision de la TUI.
fn run_plan(plan: InstallPlan, cli: &Cli, already_confirmed: bool) -> Result<()> {
    if plan.is_empty() {
        println!("No hay nada que instalar. Saliendo.");
        return Ok(());
    }

    let sys = SystemStatus::detect();
    show_plan(&plan, &sys);

    // Pre-flight en CLI: imprime el informe y aborta si hay fallos
    // criticos, salvo que el usuario use --yes.
    let report = preflight::PreflightReport::run(!plan.aur.is_empty());
    if report.has_failures() && !cli.yes && !cli.dry_run {
        eprintln!("\nPre-flight checks con fallos:");
        for c in &report.checks {
            if c.status == preflight::CheckStatus::Fail {
                eprintln!("  [FAIL] {:<22} {}", c.name, c.detail);
            }
        }
        eprintln!("\nUsa --yes para continuar de todas formas.");
        return Ok(());
    }

    if installer::is_root() {
        eprintln!("Advertencia: estas corriendo como root. makepkg/yay no deben usarse como root.");
    }

    if !already_confirmed && !cli.yes && !cli.dry_run && !confirm("¿Proceder con la instalacion?")
    {
        println!("Cancelado por el usuario.");
        return Ok(());
    }

    let opts = make_options(cli, &sys);
    let mut log = Logger::new();
    log.log(&format!("== arch-postinstall {VERSION} =="));
    let results = installer::execute(&plan, &opts, &mut log);
    installer::print_summary(&results, &mut log);

    if plan.reboot_after {
        if cli.dry_run {
            println!("[dry-run] sudo systemctl reboot");
        } else if cli.yes || confirm("Instalacion terminada. ¿Reiniciar ahora?") {
            println!("Reiniciando...");
            let _ = std::process::Command::new("sudo")
                .args(["systemctl", "reboot"])
                .status();
        } else {
            println!("Recuerda reiniciar para aplicar todos los cambios.");
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    // Si la app entra en panico con la TUI activa, restaura la terminal
    // antes de imprimir el error; si no, la consola queda inutilizable.
    // Ademas dejamos un mensaje claro para que el usuario sepa que hacer.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        eprintln!("\n[!] La aplicacion se cerro por un error inesperado.");
        eprintln!("    Por favor abre un issue con el mensaje siguiente:");
        eprintln!();
        default_hook(info);
    }));

    let cli = parse_args();

    // Aviso de actualizacion: silencioso si falla. Solo lo mostramos en
    // los modos donde el usuario va a hacer algo (no en -h / -V).
    if !cli.help && !cli.version {
        if let Some(new_ver) = update::check_for_update() {
            eprintln!(
                "Nueva version v{new_ver} disponible (actual: v{VERSION}). \
                 Descargala de: https://github.com/Khinmmad/Script-de-instalacion/releases/latest"
            );
            eprintln!();
        }
    }

    if cli.help {
        print_help();
        return ExitCode::SUCCESS;
    }
    if cli.version {
        println!("arch-postinstall {VERSION}");
        return ExitCode::SUCCESS;
    }
    if cli.list_profiles {
        match profile::list() {
            Ok(names) if names.is_empty() => println!("No hay perfiles guardados."),
            Ok(names) => {
                println!("Perfiles guardados:");
                for n in names {
                    println!("  - {n}");
                }
            }
            Err(e) => {
                eprintln!("Error al listar perfiles: {e}");
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }

    // Modo perfil: instalar directamente desde un perfil guardado.
    if let Some(name) = &cli.profile {
        match profile::load(name) {
            Ok(p) => {
                println!("Perfil cargado: {}", p.name);
                if let Err(e) = run_plan(p.into_plan(), &cli, false) {
                    eprintln!("Error: {e}");
                    return ExitCode::FAILURE;
                }
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("No se pudo cargar el perfil '{name}': {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    // Modo por defecto: asistente TUI.
    match tui::run() {
        Ok(tui::Outcome::Cancelled) => {
            println!("Asistente cancelado. No se instalo nada.");
            ExitCode::SUCCESS
        }
        Ok(tui::Outcome::Confirmed { plan, save_as }) => {
            if let Some(name) = save_as {
                let prof = Profile::from_plan(&name, &plan);
                match profile::save(&prof) {
                    Ok(path) => println!("Perfil guardado en: {}", path.display()),
                    Err(e) => eprintln!("No se pudo guardar el perfil: {e}"),
                }
            }
            if let Err(e) = run_plan(*plan, &cli, true) {
                eprintln!("Error: {e}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            // Asegura restaurar la terminal aunque algo falle.
            ratatui::restore();
            eprintln!("Error en la TUI: {e}");
            ExitCode::FAILURE
        }
    }
}
