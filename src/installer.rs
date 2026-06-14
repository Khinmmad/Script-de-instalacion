//! Ejecucion de la instalacion: pacman, yay y gestores de display.
//!
//! Filosofia de robustez (heredada del README original): un paquete que
//! falla NO aborta el resto. Cada paso se registra en un log y al final se
//! muestra un resumen de exitos y fallos.

use std::collections::HashSet;
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
            Err(e) => {
                eprintln!(
                    "Aviso: no se pudo abrir el archivo de log ({e}); \
                     la salida ira solo a la terminal."
                );
                Logger {
                    file: None,
                    path: None,
                }
            }
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
#[derive(Default)]
pub struct InstallOptions {
    /// No ejecuta nada; solo muestra los comandos que correria.
    pub dry_run: bool,
    /// Evita preguntas de pacman/yay (--noconfirm).
    pub noconfirm: bool,
    /// Paquetes oficiales que ya estan en el sistema: el instalador los
    /// salta sin invocar pacman, en vez de confiar en `--needed`.
    pub skip_official: HashSet<String>,
    /// Paquetes del AUR que ya estan en el sistema: el instalador los
    /// salta sin invocar yay.
    pub skip_aur: HashSet<String>,
}

impl InstallOptions {
    /// `true` si el paquete `pkg` (del gestor `manager`) ya esta instalado
    /// y debe saltarse.
    pub fn should_skip(&self, manager: &str, pkg: &str) -> bool {
        if manager == "pacman" {
            self.skip_official.contains(pkg)
        } else {
            self.skip_aur.contains(pkg)
        }
    }
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
fn yay_present() -> bool {
    Command::new("yay")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Ejecuta un comando registrandolo. Devuelve true si tuvo exito. En caso
/// de fallo captura `stderr` y muestra las primeras lineas junto a una
/// sugerencia contextual segun el programa, para que el usuario sepa que
/// paso y por donde investigar.
fn run(log: &mut Logger, opts: &InstallOptions, program: &str, args: &[&str]) -> bool {
    let pretty = format!("{program} {}", args.join(" "));
    if opts.dry_run {
        log.log(&format!("[dry-run] {pretty}"));
        return true;
    }
    log.log(&format!("$ {pretty}"));
    match Command::new(program).args(args).output() {
        Ok(out) if out.status.success() => true,
        Ok(out) => {
            log_command_failure(log, program, &pretty, &out.stderr, out.status.code());
            false
        }
        Err(e) => {
            log.log(&format!("  ! no se pudo ejecutar '{program}': {e}"));
            log.log(&format!(
                "      Sugerencia: comprueba que '{program}' este instalado y en el PATH."
            ));
            false
        }
    }
}

/// Imprime un fallo de comando con contexto suficiente para depurar.
/// Extrae hasta 5 lineas no vacias de `stderr` (descartando ruido de
/// barras de progreso / espacios en blanco) y, si el programa es uno de
/// los conocidos, anade una sugerencia concreta.
fn log_command_failure(
    log: &mut Logger,
    program: &str,
    pretty: &str,
    stderr: &[u8],
    code: Option<i32>,
) {
    let stderr_text = String::from_utf8_lossy(stderr);
    let preview: Vec<&str> = stderr_text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(5)
        .collect();
    log.log(&format!("  ! fallo (codigo {code:?}): {pretty}"));
    for line in &preview {
        log.log(&format!("      | {line}"));
    }
    if let Some(hint) = hint_for_failure(program, &stderr_text) {
        log.log(&format!("      Sugerencia: {hint}"));
    }
}

/// Sugerencia especifica segun que comando fallo. Si `stderr` contiene
/// pistas reconocibles (paquete no encontrado, base-devel faltante,
/// sudo pidiendo password) afinamos el mensaje.
fn hint_for_failure(program: &str, stderr: &str) -> Option<String> {
    let lower = stderr.to_lowercase();
    let pkg_not_found = lower.contains("target not found") || lower.contains("package not found");
    let lock_held =
        lower.contains("another program holds the lock") || lower.contains("database is locked");
    let keyring =
        lower.contains("signature") || lower.contains("keyring") || lower.contains("trust");
    match program {
        "sudo" => Some(
            "comprueba que tu usuario tenga sudo y que la sesion no haya expirado (vuelve a intentarlo)."
                .into(),
        ),
        "pacman" => {
            if pkg_not_found {
                Some("el paquete no existe en los repos activos; revisa el nombre, los repos habilitados en /etc/pacman.conf o ejecuta 'pacman -Ss <nombre>'.".into())
            } else if lock_held {
                Some("otra instancia de pacman esta corriendo o el lock quedo colgado; ejecuta 'sudo rm /var/lib/pacman/db.lck' si es seguro.".into())
            } else if keyring {
                Some("problema con la firma de la base de datos; ejecuta 'sudo pacman-key --refresh-keys' y vuelve a intentar.".into())
            } else {
                Some("ejecuta 'sudo pacman -Syu' manualmente; si el problema persiste, revisa /etc/pacman.conf y los mirrors.".into())
            }
        }
        "yay" | "paru" => {
            if pkg_not_found {
                Some("el paquete no existe en el AUR; revisa el nombre en https://aur.archlinux.org.".into())
            } else {
                Some("los paquetes AUR se compilan localmente; revisa tu toolchain (gcc, make, base-devel) y la salida completa arriba.".into())
            }
        }
        "makepkg" => Some(
            "fallo la compilacion del paquete; revisa las dependencias de compilacion (makedeps) y la salida completa arriba."
                .into(),
        ),
        "git" => Some("comprueba tu conexion a internet y que 'git' este instalado (ya deberia estarlo tras el setup AUR).".into()),
        "systemctl" => Some("comprueba que systemd este corriendo y que la unidad exista ('systemctl list-unit-files | grep <nombre>').".into()),
        "locale-gen" => Some("el locale puede no ser valido; revisa /etc/locale.gen.".into()),
        "grub-mkconfig" => Some("comprueba que /boot este montado y que GRUB este instalado.".into()),
        "ln" => Some("el destino puede no existir; revisa la ruta del archivo o enlace simbolico.".into()),
        _ => None,
    }
}

/// Asegura que yay este instalado, compilandolo desde el AUR si hace falta.
/// Devuelve `true` si yay esta listo para usarse al final.
fn ensure_yay(log: &mut Logger, opts: &InstallOptions) -> bool {
    if yay_present() {
        log.log("  ✓ yay ya esta instalado en el sistema.");
        return true;
    }
    log.log("  yay no encontrado: instalando dependencias y compilando desde AUR...");
    // git + base-devel son las dependencias minimas para clonar y compilar
    // un paquete del AUR. `base-devel` incluye gcc, make, fakeroot y el
    // resto de herramientas de build; `git` lo necesita makepkg para
    // resolver sources.
    let mut conf: Vec<&str> = vec!["pacman", "-S", "--needed"];
    if opts.noconfirm {
        conf.push("--noconfirm");
    }
    conf.extend(["git", "base-devel"]);
    if !run(log, opts, "sudo", &conf) {
        log.log("  ! fallo la instalacion de git/base-devel; no se puede continuar con AUR.");
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
        log.log("  ! fallo el clone del repositorio de yay; no se puede continuar con AUR.");
        return false;
    }
    let mut mk: Vec<&str> = vec!["-si"];
    if opts.noconfirm {
        mk.push("--noconfirm");
    }
    // makepkg debe correr dentro del directorio clonado.
    if opts.dry_run {
        log.log(&format!("[dry-run] (cd {tmp} && makepkg {})", mk.join(" ")));
        log.log("  ✓ yay quedaria compilado (dry-run).");
        return true;
    }
    log.log(&format!("$ (cd {tmp} && makepkg {})", mk.join(" ")));
    let ok = match Command::new("makepkg").args(&mk).current_dir(tmp).output() {
        Ok(out) if out.status.success() => true,
        Ok(out) => {
            log_command_failure(
                log,
                "makepkg",
                &format!("(cd {tmp} && makepkg {})", mk.join(" ")),
                &out.stderr,
                out.status.code(),
            );
            false
        }
        Err(e) => {
            log.log(&format!("  ! no se pudo ejecutar 'makepkg': {e}"));
            log.log("      Sugerencia: makepkg viene con 'pacman' (paquete pacman); revisa que no estes corriendo como root.");
            false
        }
    };
    if ok {
        log.log("  ✓ yay compilado e instalado.");
    } else {
        log.log("  ! makepkg fallo al compilar yay.");
    }
    ok
}

/// Instala un solo paquete con el gestor indicado. Devuelve StepResult.
fn install_one(log: &mut Logger, opts: &InstallOptions, manager: &str, pkg: &str) -> StepResult {
    // Si el sistema ya tiene el paquete, lo saltamos sin invocar al gestor.
    // Asi evitamos ruido en el log y el (pequeno) coste de pacman/yay
    // haciendo un no-op. Tambien hace explicito en el resumen lo que se
    // ahorro: el usuario ve "saltado" en vez de un exito rapido.
    if opts.should_skip(manager, pkg) {
        log.log(&format!("  ↪ {pkg} ya esta instalado, se omite"));
        return StepResult {
            label: format!("{manager}: {pkg} (ya instalado)"),
            ok: true,
        };
    }

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

/// Escribe `content` en un archivo temporal y lo copia como root a `dst`
/// con `sudo install -m <mode>`. El contenido nunca pasa por una shell, asi
/// que es seguro aunque tenga comillas, `$`, espacios o saltos de linea.
/// Devuelve `true` si se completo.
fn sudo_copy_file(
    log: &mut Logger,
    opts: &InstallOptions,
    content: &str,
    dst: &str,
    mode: &str,
    prefix: &str,
) -> bool {
    if opts.dry_run {
        log.log(&format!(
            "[dry-run] escribir {} bytes en {dst} (modo {mode})",
            content.len()
        ));
        return true;
    }

    let tmp = temp_path(prefix);
    if let Err(e) = fs::write(&tmp, content) {
        log.log(&format!("  ! no se pudo escribir {tmp}: {e}"));
        return false;
    }

    let ok = run(log, opts, "sudo", &["install", "-m", mode, &tmp, dst]);
    let _ = fs::remove_file(&tmp);
    ok
}

/// Escribe `content` en un archivo del sistema (vía sudo). Usa un archivo
/// temporal como intermediario para que el contenido nunca pase por una
/// shell: asi es seguro aunque contenga comillas, `$`, espacios o saltos de
/// linea.
fn write_root_file(log: &mut Logger, opts: &InstallOptions, path: &str, content: &str) -> bool {
    sudo_copy_file(log, opts, content, path, "0644", "write")
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

    // Creamos un archivo temporal con el contenido previo de /etc/hosts +
    // la linea nueva. Asi el contenido nunca pasa por una shell: el comando
    // sudo solo concatena archivos con `cat origen >> destino`, no expande
    // variables ni interpreta nada. `host` y `line` ya fueron validados
    // antes de llegar aqui (validate::is_valid_hostname).
    let body = match fs::read_to_string("/etc/hosts") {
        Ok(s) => s,
        Err(e) => {
            log.log(&format!("  ! no se pudo leer /etc/hosts: {e}"));
            return false;
        }
    };
    let merged = format!("{body}{line}");
    sudo_copy_file(log, opts, &merged, "/etc/hosts", "0644", "hosts")
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
///
/// Orden de operaciones (importante: AUR **antes** que los oficiales, para
/// que `git`, `base-devel` y `yay` esten listos antes de cualquier
/// instalacion):
///
/// 1. Habilitar `[multilib]` (si se pidio) — antes del `-Syu` para que
///    pacman vea los paquetes de multilib al sincronizar.
/// 2. `pacman -Syu` — sincronizar DB y actualizar el sistema base.
/// 3. Preparar AUR (solo si hay paquetes AUR): instalar `git` y
///    `base-devel` si faltan, clonar y compilar `yay` si no esta.
/// 4. Instalar paquetes AUR con `yay`.
/// 5. Instalar paquetes oficiales con `pacman`.
/// 6. Configurar basicos del sistema (locale, zona, teclado, hostname).
/// 7. Cablear microcódigo / NVIDIA al bootloader.
/// 8. Habilitar servicios de sistema (`systemctl enable`).
/// 9. Habilitar servicios de usuario (audio PipeWire).
///
/// Un paso que falla NO aborta el resto: queda registrado en `results`
/// con `ok: false` y seguimos con el siguiente.
pub fn execute(plan: &InstallPlan, opts: &InstallOptions, log: &mut Logger) -> Vec<StepResult> {
    let mut results = Vec::new();

    // 1. Multilib antes del -Syu.
    if plan.enable_multilib {
        log.log("==> Habilitando el repositorio [multilib]");
        results.push(enable_multilib(log, opts));
    }

    // 2. Sync. Si falla seguimos, pero los pasos posteriores pueden fallar
    //    tambien; el resumen final lo reflejara.
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

    // 3. Setup AUR ANTES de instalar cualquier paquete: si un paquete
    //    oficial tiene un wrapper o build-step que use git/yay, ya
    //    estara disponible, y los AUR iran inmediatamente despues.
    let aur_ready = if !plan.aur.is_empty() {
        log.log("==> Preparando herramientas para AUR (git, base-devel, yay)");
        ensure_yay(log, opts)
    } else {
        log.log("==> Sin paquetes AUR en el plan; se omite la preparacion de yay.");
        true
    };

    // 4. Paquetes AUR primero.
    if !plan.aur.is_empty() {
        if aur_ready {
            log.log("==> Instalando paquetes AUR (uno por uno para robustez)");
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

    // 5. Paquetes oficiales.
    if !plan.official.is_empty() {
        log.log("==> Instalando paquetes oficiales (uno por uno para robustez)");
        for pkg in &plan.official {
            results.push(install_one(log, opts, "pacman", pkg));
        }
    }

    // 6. Basicos del sistema (locale, zona horaria, teclado, hostname).
    configure_system_basics(plan, log, opts, &mut results);

    // 7. Drivers que requieren tocar el arranque: microcodigo de CPU y NVIDIA.
    configure_boot(plan, log, opts, &mut results);

    // 8. Servicios de sistema: display manager (sddm/gdm/lightdm),
    //    NetworkManager, bluetooth... Esto deja el equipo listo para
    //    arrancar al escritorio. Filtramos vacios por si un perfil mal
    //    escrito contiene un nombre de servicio en blanco.
    let system_services: Vec<&String> = plan
        .services
        .iter()
        .filter(|s| !s.trim().is_empty())
        .collect();
    if !system_services.is_empty() {
        log.log("==> Habilitando servicios de sistema (systemctl enable)");
        for svc in system_services {
            let unit = unit_name(svc);
            let ok = run(log, opts, "sudo", &["systemctl", "enable", &unit]);
            results.push(StepResult {
                label: format!("enable {svc}"),
                ok,
            });
        }
    }

    // 9. Servicios de usuario: stack de audio PipeWire. Se habilitan para
    //    el usuario actual (sin sudo); puede no haber sesion de usuario
    //    durante una post-instalacion, en cuyo caso falla sin abortar el
    //    resto.
    let user_services: Vec<&String> = plan
        .user_services
        .iter()
        .filter(|s| !s.trim().is_empty())
        .collect();
    if !user_services.is_empty() {
        log.log("==> Habilitando audio para el usuario (systemctl --user enable)");
        let units: Vec<String> = user_services.iter().map(|s| unit_name(s)).collect();
        let mut args: Vec<&str> = vec!["--user", "enable"];
        args.extend(units.iter().map(|s| s.as_str()));
        let ok = run(log, opts, "systemctl", &args);
        results.push(StepResult {
            label: "enable audio (--user)".into(),
            ok,
        });
    }

    // 10. Limpieza de huerfanos (opt-in). Solo se ejecuta si el usuario
    //     marco el toggle. Identifica los paquetes que ninguna instalacion
    //     actual necesita y los borra con sus dependencias.
    if plan.cleanup_orphans {
        log.log("==> Limpiando paquetes huerfanos (pacman -Rns)");
        // Primero listamos los candidatos: pacman -Qtdq devuelve los
        // huerfanos y los instalados como dependencia. Si no hay ninguno
        // la salida es vacia y el comando -Rns fallaria, asi que lo
        // verificamos antes.
        let orphans = match Command::new("pacman").arg("-Qtdq").output() {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>(),
            Ok(_) => Vec::new(),
            Err(e) => {
                log.log(&format!("  ! no se pudo listar huerfanos: {e}"));
                Vec::new()
            }
        };
        if orphans.is_empty() {
            log.log("    No hay paquetes huerfanos. Nada que limpiar.");
            results.push(StepResult {
                label: "limpiar huerfanos (sin candidatos)".into(),
                ok: true,
            });
        } else {
            log.log(&format!("    {} huerfano(s) detectado(s)", orphans.len()));
            let mut args: Vec<&str> = vec!["pacman", "-Rns", "--noconfirm"];
            if opts.dry_run {
                args.push("--print");
            }
            // Cada nombre de paquete: ya viene de pacman, no es input del
            // usuario, asi que se pasa como argumento (no por shell).
            let pkg_refs: Vec<&str> = orphans.iter().map(String::as_str).collect();
            args.extend(pkg_refs.iter().copied());
            let ok = run(log, opts, "sudo", &args);
            results.push(StepResult {
                label: format!("limpiar huerfanos ({} paquetes)", orphans.len()),
                ok,
            });
        }
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

/// Imprime un resumen final legible. Si hubo fallos los lista uno por uno
/// y, al final, recuerda donde esta el log completo.
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
        log.log(&format!("    {} paso(s) fallaron:", failed.len()));
        for f in &failed {
            log.log(&format!("      - {}", f.label));
        }
        log.log("    Cada fallo se registro en el log con su 'stderr' y una sugerencia.");
    }
    if let Some(path) = log.path.clone() {
        log.log(&format!("    Log completo: {}", path.display()));
    } else {
        log.log(
            "    (No se pudo crear el archivo de log; revisa los permisos de ~/.local/state/.)",
        );
    }
    if !failed.is_empty() {
        log.log("    Puedes volver a ejecutar el programa: los pasos ya completados se saltaran.");
    }
}
