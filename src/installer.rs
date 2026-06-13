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
            Ok((file, path)) => Logger {
                file: Some(file),
                path: Some(path),
            },
            Err(_) => Logger {
                file: None,
                path: None,
            },
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
    if !run(
        log,
        opts,
        "git",
        &["clone", "https://aur.archlinux.org/yay.git", tmp],
    ) {
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

    StepResult {
        label: format!("{manager}: {pkg}"),
        ok,
    }
}

/// Bootloader detectado en el sistema.
enum Bootloader {
    Grub,
    SystemdBoot,
    Unknown,
}

fn detect_bootloader() -> Bootloader {
    use std::path::Path;
    if Path::new("/boot/grub/grub.cfg").exists() || Path::new("/etc/default/grub").exists() {
        Bootloader::Grub
    } else if Path::new("/boot/loader/entries").exists()
        || Path::new("/efi/loader/entries").exists()
    {
        Bootloader::SystemdBoot
    } else {
        Bootloader::Unknown
    }
}

/// Genera un nombre de archivo temporal unico bajo /tmp.
fn temp_path(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("/tmp/arch-postinstall-{prefix}-{}-{n}", std::process::id())
}

/// Escribe `content` en un archivo del sistema (vía sudo). Usa un archivo
/// temporal como intermediario para que el contenido nunca pase por una
/// shell: asi es seguro aunque contenga comillas, `$`, espacios o saltos de
/// linea.
fn write_root_file(log: &mut Logger, opts: &InstallOptions, path: &str, content: &str) -> bool {
    if opts.dry_run {
        log.log(&format!(
            "[dry-run] escribir {} bytes en {path}",
            content.len()
        ));
        return true;
    }

    let tmp = temp_path("write");
    if let Err(e) = fs::write(&tmp, content) {
        log.log(&format!("  ! no se pudo escribir {tmp}: {e}"));
        return false;
    }

    let ok = run(log, opts, "sudo", &["install", "-m", "0644", &tmp, path]);
    let _ = fs::remove_file(&tmp);
    ok
}

/// Descomenta la linea `#<loc> ...` en `/etc/locale.gen` y luego lo deja
/// escrito en disco. Lo hace editando el archivo en Rust y copiandolo con
/// sudo, en vez de pasar el `loc` por `sed` (donde una barra o un caracter
/// especial romperia la expresion).
fn uncomment_locale_gen(log: &mut Logger, opts: &InstallOptions, loc: &str) -> bool {
    if opts.dry_run {
        log.log(&format!("[dry-run] descomentar {loc} en /etc/locale.gen"));
        return true;
    }

    const PATH: &str = "/etc/locale.gen";
    let body = match fs::read_to_string(PATH) {
        Ok(s) => s,
        Err(e) => {
            log.log(&format!("  ! no se pudo leer {PATH}: {e}"));
            return false;
        }
    };

    let target_prefix = format!("#{loc} ");
    let mut changed = false;
    let new_body: String = body
        .lines()
        .map(|l| {
            if !changed && l.starts_with(&target_prefix) {
                changed = true;
                &l[1..]
            } else {
                l
            }
        })
        .collect::<Vec<&str>>()
        .join("\n");
    let new_body = if body.ends_with('\n') && !new_body.ends_with('\n') {
        format!("{new_body}\n")
    } else {
        new_body
    };

    if !changed {
        // La linea no estaba comentada o no existe; en cualquier caso no hay
        // que modificar el archivo (locale-gen es lo que detecta si el locale
        // esta disponible, asi que dejar el archivo como esta es correcto).
        log.log(&format!(
            "  /etc/locale.gen: linea para {loc} no estaba comentada"
        ));
        return true;
    }

    let tmp = temp_path("locale-gen");
    if let Err(e) = fs::write(&tmp, &new_body) {
        log.log(&format!("  ! no se pudo escribir {tmp}: {e}"));
        return false;
    }
    let ok = run(log, opts, "sudo", &["install", "-m", "0644", &tmp, PATH]);
    let _ = fs::remove_file(&tmp);
    ok
}

/// Anade `line` al final de `/etc/hosts` (vía sudo), si no contiene ya el
/// hostname. Lee el archivo en Rust para hacer la comprobacion y usa un
/// archivo temporal para que el contenido nunca pase por una shell.
fn append_to_hosts(log: &mut Logger, opts: &InstallOptions, host: &str, line: &str) -> bool {
    if opts.dry_run {
        log.log(&format!(
            "[dry-run] anadir '{line}' a /etc/hosts si no existe"
        ));
        return true;
    }

    let already = fs::read_to_string("/etc/hosts")
        .map(|s| s.contains(host))
        .unwrap_or(false);
    if already {
        log.log(&format!("  /etc/hosts ya contiene entrada para {host}"));
        return true;
    }

    let tmp = temp_path("hosts");
    if let Err(e) = fs::write(&tmp, line) {
        log.log(&format!("  ! no se pudo escribir {tmp}: {e}"));
        return false;
    }

    // tmp es una ruta controlada por nosotros (no viene del usuario), asi que
    // es seguro expandirla en bash. El contenido va por el archivo, no por
    // la linea de comandos.
    let cmd = format!("cat {tmp} >> /etc/hosts");
    let ok = run(log, opts, "sudo", &["bash", "-c", &cmd]);
    let _ = fs::remove_file(&tmp);
    ok
}

/// Activa el repositorio [multilib] descomentando su bloque en pacman.conf.
fn enable_multilib(log: &mut Logger, opts: &InstallOptions) -> StepResult {
    // Quita el '#' de las dos lineas del bloque [multilib]. Idempotente: si ya
    // estan descomentadas, no hace nada.
    let sed = r"/\[multilib\]/,/Include/ s/^#//";
    let ok = run(log, opts, "sudo", &["sed", "-i", sed, "/etc/pacman.conf"]);
    StepResult {
        label: "habilitar multilib".into(),
        ok,
    }
}

/// Aplica los basicos del sistema que se hayan indicado.
fn configure_system_basics(
    plan: &InstallPlan,
    log: &mut Logger,
    opts: &InstallOptions,
    results: &mut Vec<StepResult>,
) {
    if plan.timezone.is_none()
        && plan.locale.is_none()
        && plan.keymap.is_none()
        && plan.hostname.is_none()
    {
        return;
    }
    log.log("==> Configurando basicos del sistema");

    if let Some(tz) = &plan.timezone {
        let target = format!("/usr/share/zoneinfo/{tz}");
        let ok = run(log, opts, "sudo", &["ln", "-sf", &target, "/etc/localtime"])
            && run(log, opts, "sudo", &["hwclock", "--systohc"]);
        results.push(StepResult {
            label: format!("zona horaria {tz}"),
            ok,
        });
    }

    if let Some(loc) = &plan.locale {
        // Descomenta la linea del locale, lo genera y fija LANG.
        let gen = uncomment_locale_gen(log, opts, loc) && run(log, opts, "sudo", &["locale-gen"]);
        let write = write_root_file(log, opts, "/etc/locale.conf", &format!("LANG={loc}"));
        results.push(StepResult {
            label: format!("locale {loc}"),
            ok: gen && write,
        });
    }

    if let Some(km) = &plan.keymap {
        let ok = write_root_file(log, opts, "/etc/vconsole.conf", &format!("KEYMAP={km}"));
        results.push(StepResult {
            label: format!("teclado {km}"),
            ok,
        });
    }

    if let Some(host) = &plan.hostname {
        let ok = write_root_file(log, opts, "/etc/hostname", host);
        let line = format!("127.0.1.1 {host}.localdomain {host}\n");
        let hosts_ok = append_to_hosts(log, opts, host, &line);
        results.push(StepResult {
            label: format!("hostname {host}"),
            ok: ok && hosts_ok,
        });
    }
}

/// True si /etc/default/grub ya contiene el texto dado.
fn grub_has(text: &str) -> bool {
    std::fs::read_to_string("/etc/default/grub")
        .map(|s| s.contains(text))
        .unwrap_or(false)
}

/// Configura el arranque para microcodigo de CPU y/o NVIDIA. Para GRUB lo hace
/// automaticamente; para systemd-boot deja instrucciones precisas.
fn configure_boot(
    plan: &InstallPlan,
    log: &mut Logger,
    opts: &InstallOptions,
    results: &mut Vec<StepResult>,
) {
    if !plan.has_microcode() && !plan.has_nvidia() {
        return;
    }
    log.log("==> Configurando el arranque (microcodigo de CPU / NVIDIA)");
    let params = plan.kernel_params();

    match detect_bootloader() {
        Bootloader::Grub => {
            for p in &params {
                if grub_has(p) {
                    log.log(&format!("  parametro de kernel ya presente: {p}"));
                    continue;
                }
                let sed = format!(r#"s/\(GRUB_CMDLINE_LINUX_DEFAULT="[^"]*\)"/\1 {p}"/"#);
                let ok = run(log, opts, "sudo", &["sed", "-i", &sed, "/etc/default/grub"]);
                results.push(StepResult {
                    label: format!("grub: +{p}"),
                    ok,
                });
            }
            // grub-mkconfig detecta el microcodigo automaticamente.
            let ok = run(
                log,
                opts,
                "sudo",
                &["grub-mkconfig", "-o", "/boot/grub/grub.cfg"],
            );
            results.push(StepResult {
                label: "grub-mkconfig".into(),
                ok,
            });
        }
        Bootloader::SystemdBoot => {
            log.log("  systemd-boot detectado. Edita /boot/loader/entries/*.conf:");
            if plan.has_microcode() {
                let img = if plan.official.iter().any(|p| p == "intel-ucode") {
                    "intel-ucode.img"
                } else {
                    "amd-ucode.img"
                };
                log.log(&format!(
                    "    - Anade ANTES de la linea 'initrd' existente:  initrd /{img}"
                ));
            }
            for p in &params {
                log.log(&format!("    - Anade a la linea 'options':  {p}"));
            }
            results.push(StepResult {
                label: "systemd-boot (instrucciones en el log)".into(),
                ok: true,
            });
        }
        Bootloader::Unknown => {
            log.log("  ! No se detecto GRUB ni systemd-boot. Configura el arranque a mano:");
            if plan.has_microcode() {
                log.log("    El microcodigo necesita una linea 'initrd' en tu bootloader.");
            }
            for p in &params {
                log.log(&format!("    Anade el parametro de kernel: {p}"));
            }
            results.push(StepResult {
                label: "arranque (configurar manualmente)".into(),
                ok: true,
            });
        }
    }
}

/// Ejecuta el plan completo y devuelve los resultados de cada paso.
pub fn execute(plan: &InstallPlan, opts: &InstallOptions, log: &mut Logger) -> Vec<StepResult> {
    let mut results = Vec::new();

    // Multilib debe activarse antes del -Syu para que pacman vea esos paquetes.
    if plan.enable_multilib {
        log.log("==> Habilitando el repositorio [multilib]");
        results.push(enable_multilib(log, opts));
    }

    log.log("==> Sincronizando bases de datos y sistema (pacman -Syu)");
    let mut syu = vec!["pacman", "-Syu"];
    if opts.noconfirm {
        syu.push("--noconfirm");
    }
    let synced = run(log, opts, "sudo", &syu);
    results.push(StepResult {
        label: "pacman -Syu".into(),
        ok: synced,
    });

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
                results.push(StepResult {
                    label: format!("yay: {pkg}"),
                    ok: false,
                });
            }
        }
    }

    // Basicos del sistema (locale, zona horaria, teclado, hostname).
    configure_system_basics(plan, log, opts, &mut results);

    // Drivers que requieren tocar el arranque: microcodigo de CPU y NVIDIA.
    configure_boot(plan, log, opts, &mut results);

    // Servicios de sistema: display manager (sddm/gdm/lightdm), NetworkManager,
    // bluetooth... Esto deja el equipo listo para arrancar al escritorio.
    if !plan.services.is_empty() {
        log.log("==> Habilitando servicios de sistema (systemctl enable)");
        for svc in &plan.services {
            let unit = unit_name(svc);
            let ok = run(log, opts, "sudo", &["systemctl", "enable", &unit]);
            results.push(StepResult {
                label: format!("enable {svc}"),
                ok,
            });
        }
    }

    // Servicios de usuario: stack de audio PipeWire. Se habilitan para el
    // usuario actual (sin sudo); puede no haber sesion de usuario durante una
    // post-instalacion, en cuyo caso falla sin abortar el resto.
    if !plan.user_services.is_empty() {
        log.log("==> Habilitando audio para el usuario (systemctl --user enable)");
        let units: Vec<String> = plan.user_services.iter().map(|s| unit_name(s)).collect();
        let mut args: Vec<&str> = vec!["--user", "enable"];
        args.extend(units.iter().map(|s| s.as_str()));
        let ok = run(log, opts, "systemctl", &args);
        results.push(StepResult {
            label: "enable audio (--user)".into(),
            ok,
        });
    }

    results
}

/// Normaliza el nombre de una unidad systemd (anade .service si no trae sufijo).
fn unit_name(name: &str) -> String {
    if name.contains('.') {
        name.to_string()
    } else {
        format!("{name}.service")
    }
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
