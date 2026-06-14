//! Interfaz TUI estilo archinstall: un menu principal navegable por teclado
//! desde el que se configura cada seccion (entorno, paquetes, busqueda en vivo,
//! perfiles) y finalmente se lanza la instalacion.

use anyhow::Result;
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::catalog::{BASE_PACKAGES, DESKTOP_ENVIRONMENTS, DRIVERS, EXTRA_PACKAGES};
use crate::detect::SystemStatus;
use crate::model::{
    format_list_or_none, format_system_settings, InstallPlan, Profile, Source, SystemLabelStyle,
};
use crate::{profile, repo_api, validate};

/// Un paquete seleccionable (curado o anadido por busqueda).
#[derive(Clone)]
struct PkgItem {
    name: String,
    description: String,
    selected: bool,
    /// `true` si el paquete ya esta instalado en el sistema. Solo afecta a
    /// como se muestra; el instalador filtra los ya instalados para no
    /// reinstalarlos.
    installed: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Welcome,
    Main,
    Desktop,
    Drivers,
    Official,
    Aur,
    System,
    Search,
    LoadProfile,
    SaveProfile,
    Review,
}

/// Resultado del asistente.
pub enum Outcome {
    Cancelled,
    Confirmed {
        plan: Box<InstallPlan>,
        save_as: Option<String>,
    },
}

/// Entradas del menu principal. Los indices se usan en `handle_main`.
const MENU: &[&str] = &[
    "Entorno de escritorio",
    "Controladores (drivers GPU + microcodigo)",
    "Paquetes oficiales",
    "Paquetes AUR",
    "Configuracion del sistema (locale, zona, teclado...)",
    "Buscar y anadir paquetes (oficial/AUR)",
    "Cargar perfil",
    "Guardar perfil",
    "Instalar ahora",
    "Salir",
];

// Indices de las entradas del menu principal (deben coincidir con MENU).
const MENU_DESKTOP: usize = 0;
const MENU_DRIVERS: usize = 1;
const MENU_OFFICIAL: usize = 2;
const MENU_AUR: usize = 3;
const MENU_SYSTEM: usize = 4;
const MENU_SEARCH: usize = 5;
const MENU_LOAD: usize = 6;
const MENU_SAVE: usize = 7;
const MENU_INSTALL: usize = 8;
const MENU_QUIT: usize = 9;

/// Numero de campos en el formulario de configuracion del sistema.
const SYS_FIELDS: usize = 7;
const SYS_MULTILIB: usize = 4;
const SYS_REBOOT: usize = 5;
const SYS_CLEANUP: usize = 6;

struct App {
    mode: Mode,
    main_cursor: usize,
    de_index: usize,
    /// Marcado/no marcado, paralelo a `catalog::DRIVERS`.
    drivers: Vec<bool>,
    /// `installed` paralelo a `catalog::DRIVERS` (todos los paquetes del
    /// driver ya estan en el sistema).
    drivers_installed: Vec<bool>,
    official: Vec<PkgItem>,
    aur: Vec<PkgItem>,
    list_cursor: usize,
    status: String,

    // Estado del sistema (paquetes instalados, servicios, configs). Se
    // detecta una sola vez al iniciar la TUI.
    sys_state: SystemStatus,

    // Busqueda
    search_source: Source,
    search_input: String,
    search_results: Vec<PkgItem>,
    typing: bool, // true: el texto va al campo de entrada

    // Perfiles
    profiles: Vec<String>,
    name_input: String,

    // Configuracion del sistema (formulario)
    sys_cursor: usize,
    sys_locale: String,
    sys_timezone: String,
    sys_keymap: String,
    sys_hostname: String,
    sys_multilib: bool,
    sys_reboot: bool,
    sys_cleanup_orphans: bool,
}

impl App {
    fn new() -> Self {
        let sys_state = SystemStatus::detect();

        let official = EXTRA_PACKAGES
            .iter()
            .filter(|p| p.source == Source::Official)
            .map(|p| {
                let installed = matches!(
                    sys_state.package_status(p.name, Source::Official),
                    crate::detect::PackageStatus::Installed
                );
                PkgItem {
                    name: p.name.to_string(),
                    description: p.description.to_string(),
                    selected: p.default_on,
                    installed,
                }
            })
            .collect();
        let aur = EXTRA_PACKAGES
            .iter()
            .filter(|p| p.source == Source::Aur)
            .map(|p| {
                let installed = matches!(
                    sys_state.package_status(p.name, Source::Aur),
                    crate::detect::PackageStatus::Installed
                );
                PkgItem {
                    name: p.name.to_string(),
                    description: p.description.to_string(),
                    selected: p.default_on,
                    installed,
                }
            })
            .collect();

        let drivers = DRIVERS.iter().map(|d| d.default_on).collect();
        let drivers_installed: Vec<bool> = DRIVERS
            .iter()
            .map(|d| {
                // Un driver se considera "ya instalado" si todos sus
                // paquetes oficiales estan en el sistema.
                d.packages.iter().all(|p| {
                    matches!(
                        sys_state.package_status(p, Source::Official),
                        crate::detect::PackageStatus::Installed
                    )
                })
            })
            .collect();

        // Pre-rellena el formulario con la configuracion actual del sistema
        // para que el usuario solo tenga que tocar lo que quiera cambiar.
        // Los campos vacios quedan vacios (que ya significa "no tocar").
        let sys_locale = sys_state.locale.clone().unwrap_or_default();
        let sys_timezone = sys_state.timezone.clone().unwrap_or_default();
        let sys_keymap = sys_state.keymap.clone().unwrap_or_default();
        let sys_hostname = sys_state.hostname.clone().unwrap_or_default();
        let sys_multilib = sys_state.multilib_enabled;
        let sys_reboot = false;
        // Empezamos con la limpieza apagada: es algo destructivo y no
        // queremos borrar nada sin que el usuario lo haya pedido.
        let sys_cleanup_orphans = false;

        App {
            mode: Mode::Welcome,
            main_cursor: 0,
            de_index: 0,
            drivers,
            drivers_installed,
            official,
            aur,
            list_cursor: 0,
            status: "Usa ↑/↓ y Enter. En esta pantalla 'Instalar ahora' lanza todo.".into(),
            sys_state,
            search_source: Source::Official,
            search_input: String::new(),
            search_results: Vec::new(),
            typing: false,
            profiles: Vec::new(),
            name_input: String::new(),
            sys_cursor: 0,
            sys_locale,
            sys_timezone,
            sys_keymap,
            sys_hostname,
            sys_multilib,
            sys_reboot,
            sys_cleanup_orphans,
        }
    }

    /// Referencia mutable al campo de texto del formulario bajo el cursor.
    fn sys_field_mut(&mut self) -> Option<&mut String> {
        match self.sys_cursor {
            0 => Some(&mut self.sys_locale),
            1 => Some(&mut self.sys_timezone),
            2 => Some(&mut self.sys_keymap),
            3 => Some(&mut self.sys_hostname),
            _ => None,
        }
    }

    fn count_selected(items: &[PkgItem]) -> usize {
        items.iter().filter(|p| p.selected).count()
    }

    fn build_plan(&self) -> InstallPlan {
        let de = &DESKTOP_ENVIRONMENTS[self.de_index];
        let mut official: Vec<String> = Vec::new();
        let mut aur: Vec<String> = Vec::new();

        if de.id != "ninguno" {
            official.extend(BASE_PACKAGES.iter().map(|s| s.to_string()));
            official.extend(de.packages.iter().map(|s| s.to_string()));
            if let Some(dm) = de.display_manager {
                official.push(dm.to_string());
            }
        }
        // Controladores seleccionados (todos oficiales).
        for (i, driver) in DRIVERS.iter().enumerate() {
            if self.drivers.get(i).copied().unwrap_or(false) {
                official.extend(driver.packages.iter().map(|s| s.to_string()));
            }
        }
        for p in self.official.iter().filter(|p| p.selected) {
            official.push(p.name.clone());
        }
        for p in self.aur.iter().filter(|p| p.selected) {
            aur.push(p.name.clone());
        }

        official.sort();
        official.dedup();
        aur.sort();
        aur.dedup();

        let de_id = (de.id != "ninguno").then(|| de.id.to_string());
        let dm = if de.id == "ninguno" {
            None
        } else {
            de.display_manager.map(|s| s.to_string())
        };
        let mut plan = InstallPlan::new(de_id, dm, official, aur);
        plan.locale = nonempty(&self.sys_locale).filter(|s| validate::is_valid_locale(s));
        plan.timezone = nonempty(&self.sys_timezone).filter(|s| validate::is_valid_timezone(s));
        plan.keymap = nonempty(&self.sys_keymap).filter(|s| validate::is_valid_keymap(s));
        plan.hostname = nonempty(&self.sys_hostname).filter(|s| validate::is_valid_hostname(s));
        plan.enable_multilib = self.sys_multilib;
        plan.reboot_after = self.sys_reboot;
        plan.cleanup_orphans = self.sys_cleanup_orphans;
        plan
    }

    fn count_drivers(&self) -> usize {
        self.drivers.iter().filter(|&&d| d).count()
    }

    /// Resumen corto de los ajustes del sistema para el menu principal.
    /// Solo considera los campos que tengan un valor (aunque sea invalido),
    /// para que el usuario vea "esto es lo que rellene".
    fn system_summary(&self) -> String {
        let locale = nonempty(&self.sys_locale);
        let timezone = nonempty(&self.sys_timezone);
        let keymap = nonempty(&self.sys_keymap);
        let hostname = nonempty(&self.sys_hostname);
        let mut parts = format_system_settings(
            locale.as_deref(),
            timezone.as_deref(),
            keymap.as_deref(),
            hostname.as_deref(),
            self.sys_multilib,
            self.sys_reboot,
            SystemLabelStyle::Short,
        );
        if self.sys_cleanup_orphans {
            parts.push("limpiar huerfanos".into());
        }
        parts.join(", ")
    }

    /// Aplica un perfil cargado a la seleccion actual.
    fn apply_profile(&mut self, p: Profile) {
        if let Some(id) = &p.desktop_environment {
            if let Some(idx) = DESKTOP_ENVIRONMENTS.iter().position(|d| d.id == *id) {
                self.de_index = idx;
            }
        } else {
            self.de_index = 0;
        }
        merge_profile_into(
            &mut self.official,
            &p.official_packages,
            Source::Official,
            &self.sys_state,
        );
        merge_profile_into(&mut self.aur, &p.aur_packages, Source::Aur, &self.sys_state);
        self.status = format!("Perfil '{}' cargado.", p.name);
    }
}

/// Sincroniza la seleccion de `items` con los nombres de `names`:
/// los del catalogo quedan marcados segun aparezcan o no en `names`, y los
/// que estan en `names` pero no en el catalogo se anaden marcados.
/// `src` se usa para consultar el estado del sistema y marcar como
/// instalados los nuevos que ya esten en la maquina.
fn merge_profile_into(items: &mut Vec<PkgItem>, names: &[String], src: Source, sys: &SystemStatus) {
    for it in items.iter_mut() {
        it.selected = names.contains(&it.name);
    }
    let to_add: Vec<String> = names
        .iter()
        .filter(|n| !items.iter().any(|i| &i.name == *n))
        .cloned()
        .collect();
    for name in to_add {
        let installed = matches!(
            sys.package_status(&name, src),
            crate::detect::PackageStatus::Installed
        );
        items.push(PkgItem {
            name,
            description: "(del perfil)".into(),
            selected: true,
            installed,
        });
    }
}

/// Navegacion comun de listas: flechas, j/k, Home/End y PageUp/PageDown.
fn move_cursor(cursor: &mut usize, len: usize, code: KeyCode) {
    if len == 0 {
        return;
    }
    match code {
        KeyCode::Up | KeyCode::Char('k') => *cursor = (*cursor + len - 1) % len,
        KeyCode::Down | KeyCode::Char('j') => *cursor = (*cursor + 1) % len,
        KeyCode::Home => *cursor = 0,
        KeyCode::End => *cursor = len - 1,
        KeyCode::PageUp => *cursor = cursor.saturating_sub(10),
        KeyCode::PageDown => *cursor = (*cursor + 10).min(len - 1),
        _ => {}
    }
}

/// Convierte un campo de texto en Option, recortando espacios.
fn nonempty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Anade o alterna un paquete en una lista por nombre. Devuelve si quedo activo.
fn toggle_into(vec: &mut Vec<PkgItem>, name: &str, desc: &str, installed: bool) -> bool {
    if let Some(it) = vec.iter_mut().find(|p| p.name == name) {
        it.selected = !it.selected;
        it.selected
    } else {
        vec.push(PkgItem {
            name: name.to_string(),
            description: desc.to_string(),
            selected: true,
            installed,
        });
        true
    }
}

/// Lanza el asistente TUI y devuelve el resultado.
pub fn run() -> Result<Outcome> {
    let mut terminal = ratatui::init();
    let mut app = App::new();

    let outcome = loop {
        terminal.draw(|f| draw(f, &app))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        // Entrada de texto activa (busqueda o nombre de perfil).
        if app.typing {
            match key.code {
                KeyCode::Esc => {
                    app.typing = false;
                }
                KeyCode::Enter => {
                    handle_text_submit(&mut app);
                }
                KeyCode::Backspace => match app.mode {
                    Mode::Search => {
                        app.search_input.pop();
                    }
                    Mode::System => {
                        if let Some(f) = app.sys_field_mut() {
                            f.pop();
                        }
                    }
                    _ => {
                        app.name_input.pop();
                    }
                },
                KeyCode::Char(c) => match app.mode {
                    Mode::Search => app.search_input.push(c),
                    Mode::System => {
                        if let Some(f) = app.sys_field_mut() {
                            f.push(c);
                        }
                    }
                    _ => app.name_input.push(c),
                },
                _ => {}
            }
            continue;
        }

        match app.mode {
            Mode::Welcome => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => app.mode = Mode::Main,
                KeyCode::Esc | KeyCode::Char('q') => break Outcome::Cancelled,
                _ => {}
            },
            Mode::Main => {
                if let Some(o) = handle_main(&mut app, key.code) {
                    break o;
                }
            }
            Mode::Desktop => handle_desktop(&mut app, key.code),
            Mode::Drivers => handle_drivers(&mut app, key.code),
            Mode::Official => handle_packages(&mut app, key.code, Source::Official),
            Mode::Aur => handle_packages(&mut app, key.code, Source::Aur),
            Mode::System => handle_system(&mut app, key.code),
            Mode::Search => handle_search(&mut app, key.code),
            Mode::LoadProfile => handle_load_profile(&mut app, key.code),
            Mode::SaveProfile => {} // se maneja via typing
            Mode::Review => match key.code {
                KeyCode::Enter => {
                    break Outcome::Confirmed {
                        plan: Box::new(app.build_plan()),
                        save_as: None,
                    };
                }
                KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::Main,
                KeyCode::Char('s') => break Outcome::Cancelled,
                _ => {}
            },
        }
    };

    ratatui::restore();
    Ok(outcome)
}

fn handle_text_submit(app: &mut App) {
    match app.mode {
        Mode::Search => {
            let term = app.search_input.trim().to_string();
            if term.is_empty() {
                app.status = "Escribe algo para buscar.".into();
                return;
            }
            app.status = "Buscando...".into();
            match repo_api::search(app.search_source, &term) {
                Ok(found) => {
                    app.search_results = found
                        .into_iter()
                        .map(|f| {
                            let installed = matches!(
                                app.sys_state.package_status(&f.name, app.search_source),
                                crate::detect::PackageStatus::Installed
                            );
                            PkgItem {
                                name: f.name,
                                description: f.description,
                                selected: false,
                                installed,
                            }
                        })
                        .collect();
                    app.list_cursor = 0;
                    app.typing = false;
                    app.status = format!(
                        "{} resultados. Space para anadir, Enter vuelve al menu.",
                        app.search_results.len()
                    );
                }
                Err(e) => {
                    app.status = format!("Error de busqueda: {e}");
                    app.typing = false;
                }
            }
        }
        Mode::SaveProfile => {
            let name = app.name_input.trim().to_string();
            if name.is_empty() {
                app.status = "El nombre no puede estar vacio.".into();
                return;
            }
            let plan = app.build_plan();
            let prof = Profile::from_plan(&name, &plan);
            match profile::save(&prof) {
                Ok(path) => app.status = format!("Perfil guardado: {}", path.display()),
                Err(e) => app.status = format!("No se pudo guardar: {e}"),
            }
            app.typing = false;
            app.mode = Mode::Main;
        }
        Mode::System => {
            // Confirmar la edicion de un campo: salimos del modo texto y, si
            // el valor quedo invalido, avisamos al usuario.
            app.typing = false;
            let (label, valid) = match app.sys_cursor {
                0 => ("locale", validate::is_valid_locale(&app.sys_locale)),
                1 => (
                    "zona horaria",
                    validate::is_valid_timezone(&app.sys_timezone),
                ),
                2 => ("teclado", validate::is_valid_keymap(&app.sys_keymap)),
                3 => ("hostname", validate::is_valid_hostname(&app.sys_hostname)),
                _ => ("", true),
            };
            let value = match app.sys_cursor {
                0 => &app.sys_locale,
                1 => &app.sys_timezone,
                2 => &app.sys_keymap,
                3 => &app.sys_hostname,
                _ => "",
            };
            app.status = if value.is_empty() {
                "Campo vacio: no se modificara ese ajuste.".into()
            } else if !valid {
                format!("'{value}' no es un {label} valido; se omitira al instalar.")
            } else {
                "Campo guardado.".into()
            };
        }
        _ => {}
    }
}

fn handle_main(app: &mut App, code: KeyCode) -> Option<Outcome> {
    move_cursor(&mut app.main_cursor, MENU.len(), code);
    match code {
        KeyCode::Char('q') | KeyCode::Esc => return Some(Outcome::Cancelled),
        KeyCode::Enter => match app.main_cursor {
            MENU_DESKTOP => {
                app.mode = Mode::Desktop;
                app.list_cursor = app.de_index;
            }
            MENU_DRIVERS => {
                app.mode = Mode::Drivers;
                app.list_cursor = 0;
                app.status = "Marca tu GPU y el microcodigo de tu CPU.".into();
            }
            MENU_OFFICIAL => {
                app.mode = Mode::Official;
                app.list_cursor = 0;
            }
            MENU_AUR => {
                app.mode = Mode::Aur;
                app.list_cursor = 0;
            }
            MENU_SYSTEM => {
                app.mode = Mode::System;
                app.sys_cursor = 0;
                app.status = "Rellena lo que quieras configurar (vacio = no tocar).".into();
            }
            MENU_SEARCH => {
                app.mode = Mode::Search;
                app.typing = true;
                app.search_results.clear();
                app.status = "Tab cambia oficial/AUR. Escribe y Enter para buscar.".into();
            }
            MENU_LOAD => {
                app.profiles = profile::list().unwrap_or_default();
                app.mode = Mode::LoadProfile;
                app.list_cursor = 0;
                if app.profiles.is_empty() {
                    app.status = "No hay perfiles guardados.".into();
                }
            }
            MENU_SAVE => {
                app.mode = Mode::SaveProfile;
                app.typing = true;
                app.name_input.clear();
                app.status = "Escribe un nombre y Enter para guardar.".into();
            }
            MENU_INSTALL => {
                let plan = app.build_plan();
                if plan.is_empty() {
                    app.status = "Nada que instalar: elige un entorno o marca paquetes.".into();
                } else {
                    app.mode = Mode::Review;
                    app.status = "Revisa el plan. Enter confirma, Esc vuelve al menu.".into();
                }
            }
            MENU_QUIT => return Some(Outcome::Cancelled),
            _ => {}
        },
        _ => {}
    }
    None
}

fn handle_desktop(app: &mut App, code: KeyCode) {
    move_cursor(&mut app.list_cursor, DESKTOP_ENVIRONMENTS.len(), code);
    match code {
        KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::Main,
        KeyCode::Char(' ') | KeyCode::Enter => {
            app.de_index = app.list_cursor;
            let de = &DESKTOP_ENVIRONMENTS[app.de_index];
            app.status = format!("Entorno: {}", de.label);
            app.mode = Mode::Main;
        }
        _ => {}
    }
}

fn handle_system(app: &mut App, code: KeyCode) {
    move_cursor(&mut app.sys_cursor, SYS_FIELDS, code);
    match code {
        KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::Main,
        KeyCode::Char(' ') => match app.sys_cursor {
            SYS_MULTILIB => app.sys_multilib = !app.sys_multilib,
            SYS_REBOOT => app.sys_reboot = !app.sys_reboot,
            SYS_CLEANUP => app.sys_cleanup_orphans = !app.sys_cleanup_orphans,
            _ => {}
        },
        KeyCode::Enter | KeyCode::Char('i') if app.sys_cursor <= 3 => {
            app.typing = true;
            app.status = "Editando campo. Enter confirma, Esc cancela.".into();
        }
        _ => {}
    }
}

fn handle_drivers(app: &mut App, code: KeyCode) {
    move_cursor(&mut app.list_cursor, DRIVERS.len(), code);
    match code {
        KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::Main,
        KeyCode::Char(' ') => {
            if let Some(sel) = app.drivers.get_mut(app.list_cursor) {
                *sel = !*sel;
            }
        }
        _ => {}
    }
}

fn handle_packages(app: &mut App, code: KeyCode, source: Source) {
    let list = match source {
        Source::Official => &mut app.official,
        Source::Aur => &mut app.aur,
    };
    move_cursor(&mut app.list_cursor, list.len(), code);
    match code {
        KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::Main,
        KeyCode::Char(' ') => {
            if let Some(it) = list.get_mut(app.list_cursor) {
                it.selected = !it.selected;
            }
        }
        _ => {}
    }
}

fn handle_search(app: &mut App, code: KeyCode) {
    move_cursor(&mut app.list_cursor, app.search_results.len(), code);
    match code {
        KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::Main,
        KeyCode::Enter => app.mode = Mode::Main,
        KeyCode::Char('/') | KeyCode::Char('i') => {
            app.typing = true;
            app.status = "Editando busqueda...".into();
        }
        KeyCode::Tab => {
            app.search_source = match app.search_source {
                Source::Official => Source::Aur,
                Source::Aur => Source::Official,
            };
            app.typing = true;
            app.status = format!(
                "Fuente: {}. Escribe y Enter para buscar.",
                source_label(app.search_source)
            );
        }
        KeyCode::Char(' ') => {
            if let Some(res) = app.search_results.get(app.list_cursor).cloned() {
                let target = match app.search_source {
                    Source::Official => &mut app.official,
                    Source::Aur => &mut app.aur,
                };
                let now_on = toggle_into(target, &res.name, &res.description, res.installed);
                if let Some(r) = app.search_results.get_mut(app.list_cursor) {
                    r.selected = now_on;
                }
                app.status = if now_on {
                    format!("Anadido: {}", res.name)
                } else {
                    format!("Quitado: {}", res.name)
                };
            }
        }
        _ => {}
    }
}

fn handle_load_profile(app: &mut App, code: KeyCode) {
    move_cursor(&mut app.list_cursor, app.profiles.len(), code);
    match code {
        KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::Main,
        KeyCode::Enter => {
            if let Some(name) = app.profiles.get(app.list_cursor).cloned() {
                match profile::load(&name) {
                    Ok(p) => app.apply_profile(p),
                    Err(e) => app.status = format!("No se pudo cargar: {e}"),
                }
            }
            app.mode = Mode::Main;
        }
        _ => {}
    }
}

fn source_label(s: Source) -> &'static str {
    match s {
        Source::Official => "Oficial (pacman)",
        Source::Aur => "AUR (yay)",
    }
}

// ----------------------------- Renderizado -----------------------------

fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(3),
    ])
    .split(f.area());

    draw_title(f, chunks[0]);
    match app.mode {
        Mode::Welcome => draw_welcome(f, chunks[1]),
        Mode::Main => draw_main(f, chunks[1], app),
        Mode::Drivers => draw_drivers(f, chunks[1], app),
        Mode::Desktop => draw_desktop(f, chunks[1], app),
        Mode::Official => draw_packages(f, chunks[1], app, Source::Official),
        Mode::Aur => draw_packages(f, chunks[1], app, Source::Aur),
        Mode::System => draw_system(f, chunks[1], app),
        Mode::Search => draw_search(f, chunks[1], app),
        Mode::LoadProfile => draw_load_profile(f, chunks[1], app),
        Mode::SaveProfile => draw_save_profile(f, chunks[1], app),
        Mode::Review => draw_review(f, chunks[1], app),
    }
    draw_status(f, chunks[2], app);
}

fn draw_title(f: &mut Frame, area: Rect) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            "  Arch Post-Install  ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  asistente de post-instalacion (estilo archinstall)"),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(title, area);
}

fn draw_welcome(f: &mut Frame, area: Rect) {
    let cyan = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);
    let green = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);

    let banner = [
        r"    _             _      ____           _   ",
        r"   / \   _ __ ___| |__  |  _ \ ___  ___| |_ ",
        r"  / _ \ | '__/ __| '_ \ | |_) / _ \/ __| __|",
        r" / ___ \| | | (__| | | ||  __/ (_) \__ \ |_ ",
        r"/_/   \_\_|  \___|_| |_||_|   \___/|___/\__|",
    ];

    let mut lines: Vec<Line> = vec![Line::from("")];
    for b in banner {
        lines.push(Line::from(Span::styled(b, cyan)).alignment(Alignment::Center));
    }
    lines.push(Line::from(""));
    lines.push(
        Line::from(vec![
            Span::raw("Asistente de post-instalacion para Arch Linux  "),
            Span::styled(format!("v{}", env!("CARGO_PKG_VERSION")), dim),
        ])
        .alignment(Alignment::Center),
    );
    lines.push(Line::from(""));
    for feat in [
        "🎨  Elige tu entorno de escritorio (KDE, GNOME, Hyprland, Qtile…)",
        "📦  Marca paquetes oficiales y del AUR con checklists",
        "🔎  Busca en vivo cualquier paquete (repos oficiales + AUR)",
        "💾  Guarda y reutiliza tu configuracion como perfil",
    ] {
        lines.push(Line::from(Span::raw(feat)).alignment(Alignment::Center));
    }
    lines.push(Line::from(""));
    lines.push(
        Line::from(vec![
            Span::styled("▶ Pulsa ", green),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" para comenzar", green),
            Span::raw("   ·   "),
            Span::styled(
                "q",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" para salir"),
        ])
        .alignment(Alignment::Center),
    );

    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn draw_main(f: &mut Frame, area: Rect, app: &App) {
    let de = &DESKTOP_ENVIRONMENTS[app.de_index];
    let off = App::count_selected(&app.official);
    let aur = App::count_selected(&app.aur);
    let drv = app.count_drivers();

    let mut summaries = vec![String::new(); MENU.len()];
    summaries[MENU_DESKTOP] = format!("[ {} ]", de.label);
    summaries[MENU_DRIVERS] = format!("[ {drv} seleccionados ]");
    summaries[MENU_OFFICIAL] = format!("[ {off} seleccionados ]");
    summaries[MENU_AUR] = format!("[ {aur} seleccionados ]");
    let sys_summary = app.system_summary();
    if !sys_summary.is_empty() {
        summaries[MENU_SYSTEM] = format!("[ {sys_summary} ]");
    }

    // Resumen del plan: cuenta cuantos paquetes iran al pacman/yay y cuantos
    // ya estan en el sistema, usando la deteccion. Asi el usuario ve de un
    // vistazo cuanto trabajo queda sin tener que entrar a la revision.
    let plan = app.build_plan();
    let to_install = plan
        .official
        .iter()
        .filter(|p| !app.sys_state.official.contains(*p) && !app.sys_state.aur.contains(*p))
        .count()
        + plan
            .aur
            .iter()
            .filter(|p| !app.sys_state.official.contains(*p) && !app.sys_state.aur.contains(*p))
            .count();
    let already = plan.official.len() + plan.aur.len() - to_install;
    summaries[MENU_INSTALL] = format!("[ {to_install} por instalar, {already} ya en sistema ]");

    let items: Vec<ListItem> = MENU
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let mut spans = vec![Span::styled(
                format!("{label:<44}"),
                Style::default().add_modifier(Modifier::BOLD),
            )];
            if !summaries[i].is_empty() {
                spans.push(Span::styled(
                    summaries[i].clone(),
                    Style::default().fg(Color::Green),
                ));
            }
            if i == MENU_INSTALL {
                spans = vec![Span::styled(
                    format!("▶ {label}"),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )];
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    render_list(f, area, items, app.main_cursor, " Menu principal ");
}

fn draw_system(f: &mut Frame, area: Rect, app: &App) {
    // Cada campo de texto: (etiqueta, valor, ejemplo, ya configurado).
    let fields = [
        (
            "Locale",
            &app.sys_locale,
            "es_MX.UTF-8",
            app.sys_state.locale.is_some(),
        ),
        (
            "Zona horaria",
            &app.sys_timezone,
            "America/Mexico_City",
            app.sys_state.timezone.is_some(),
        ),
        (
            "Teclado (consola)",
            &app.sys_keymap,
            "la-latin1",
            app.sys_state.keymap.is_some(),
        ),
        (
            "Hostname",
            &app.sys_hostname,
            "mi-arch",
            app.sys_state.hostname.is_some(),
        ),
    ];

    let mut lines = vec![Line::from("")];
    for (i, (label, value, example, already_set)) in fields.iter().enumerate() {
        let focused = app.sys_cursor == i;
        let editing = focused && app.typing;
        let prefix = if focused { "➤ " } else { "  " };
        let shown = if value.is_empty() {
            Span::styled(format!("<{example}>"), Style::default().fg(Color::DarkGray))
        } else {
            Span::styled(
                (*value).clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
        };
        let value_style = if editing {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default()
        };
        let cursor = if editing { "_" } else { "" };
        let already = if *already_set && !editing {
            Span::styled("  (actual)", Style::default().fg(Color::DarkGray))
        } else {
            Span::raw("")
        };
        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{label:<20}"),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            shown,
            Span::styled(cursor.to_string(), value_style),
            already,
        ]));
    }

    // Toggles.
    let toggle_line = |idx: usize, label: &str, on: bool, cursor: usize, note: &str| {
        let prefix = if cursor == idx { "➤ " } else { "  " };
        let box_ = if on { "[x]" } else { "[ ]" };
        let note_span = if on && !note.is_empty() {
            Span::styled(format!("  ({note})"), Style::default().fg(Color::DarkGray))
        } else {
            Span::raw("")
        };
        Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{box_} "),
                Style::default().fg(if on { Color::Green } else { Color::DarkGray }),
            ),
            Span::styled(
                label.to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            note_span,
        ])
    };
    lines.push(Line::from(""));
    lines.push(toggle_line(
        SYS_MULTILIB,
        "Habilitar repositorio multilib (Steam, libs 32-bit)",
        app.sys_multilib,
        app.sys_cursor,
        if app.sys_state.multilib_enabled {
            "ya habilitado"
        } else {
            ""
        },
    ));
    lines.push(toggle_line(
        SYS_REBOOT,
        "Reiniciar automaticamente al terminar",
        app.sys_reboot,
        app.sys_cursor,
        "",
    ));
    lines.push(toggle_line(
        SYS_CLEANUP,
        "Limpiar paquetes huerfanos al terminar (pacman -Rns)",
        app.sys_cleanup_orphans,
        app.sys_cursor,
        "",
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Deja un campo vacio para no tocar ese ajuste del sistema.",
        Style::default().fg(Color::DarkGray),
    )));

    let p = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Configuracion del sistema "),
    );
    f.render_widget(p, area);
}

fn draw_drivers(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = DRIVERS
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let on = app.drivers.get(i).copied().unwrap_or(false);
            let installed = app.drivers_installed.get(i).copied().unwrap_or(false);
            let checkbox = if on { "[x] " } else { "[ ] " };
            let cb_style = if on {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let installed_span = if installed {
                Span::styled(
                    "✓ ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw("  ")
            };
            let label_style = if installed {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            let pkg_color = if installed {
                Color::DarkGray
            } else {
                Color::Gray
            };
            ListItem::new(Line::from(vec![
                Span::styled(checkbox, cb_style),
                installed_span,
                Span::styled(format!("{:<40}", d.label), label_style),
                Span::styled(d.packages.join(" "), Style::default().fg(pkg_color)),
            ]))
        })
        .collect();
    render_list(
        f,
        area,
        items,
        app.list_cursor,
        " Controladores · Space marca (puedes elegir varios) ",
    );
}

fn draw_desktop(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = DESKTOP_ENVIRONMENTS
        .iter()
        .enumerate()
        .map(|(i, de)| {
            let marker = if i == app.de_index { "(•) " } else { "( ) " };
            ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(Color::Green)),
                Span::styled(de.label, Style::default().add_modifier(Modifier::BOLD)),
            ]))
        })
        .collect();
    render_list(
        f,
        area,
        items,
        app.list_cursor,
        " Entorno de escritorio (Space/Enter elige) ",
    );
}

fn draw_packages(f: &mut Frame, area: Rect, app: &App, source: Source) {
    let list = match source {
        Source::Official => &app.official,
        Source::Aur => &app.aur,
    };
    let items: Vec<ListItem> = list.iter().map(|p| package_item(p)).collect();
    let title = match source {
        Source::Official => " Paquetes oficiales (Space marca) ",
        Source::Aur => " Paquetes AUR (Space marca) ",
    };
    render_list(f, area, items, app.list_cursor, title);
}

fn package_item(p: &PkgItem) -> ListItem<'_> {
    let checkbox = if p.selected { "[x] " } else { "[ ] " };
    let cb_style = if p.selected {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let installed_span = if p.installed {
        Span::styled(
            "✓ ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("  ")
    };
    let name_style = if p.installed {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    let desc_color = if p.installed {
        Color::DarkGray
    } else {
        Color::Gray
    };
    ListItem::new(Line::from(vec![
        Span::styled(checkbox, cb_style),
        installed_span,
        Span::styled(format!("{:<28}", p.name), name_style),
        Span::styled(
            truncate(&p.description, 60),
            Style::default().fg(desc_color),
        ),
    ]))
}

fn draw_search(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).split(area);

    let cursor = if app.typing { "_" } else { "" };
    let input = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" [{}] ", source_label(app.search_source)),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("Buscar: "),
        Span::styled(
            format!("{}{cursor}", app.search_input),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Buscador en vivo (Tab cambia fuente) "),
    );
    f.render_widget(input, rows[0]);

    let items: Vec<ListItem> = app.search_results.iter().map(|p| package_item(p)).collect();
    if items.is_empty() {
        let hint = Paragraph::new("Sin resultados todavia. Escribe un termino y pulsa Enter.")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(hint, rows[1]);
    } else {
        render_list(
            f,
            rows[1],
            items,
            app.list_cursor,
            " Resultados (Space anade/quita) ",
        );
    }
}

fn draw_load_profile(f: &mut Frame, area: Rect, app: &App) {
    if app.profiles.is_empty() {
        let p = Paragraph::new(
            "No hay perfiles guardados.\n\nUsa 'Guardar perfil' en el menu para crear uno.",
        )
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Cargar perfil "),
        );
        f.render_widget(p, area);
        return;
    }
    let items: Vec<ListItem> = app
        .profiles
        .iter()
        .map(|n| {
            ListItem::new(Line::from(Span::styled(
                n.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            )))
        })
        .collect();
    render_list(
        f,
        area,
        items,
        app.list_cursor,
        " Cargar perfil (Enter aplica) ",
    );
}

fn draw_save_profile(f: &mut Frame, area: Rect, app: &App) {
    let p = Paragraph::new(vec![
        Line::from(""),
        Line::from("Nombre del perfil:").bold(),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{}_", app.name_input),
                Style::default().fg(Color::Black).bg(Color::Yellow),
            ),
        ]),
        Line::from(""),
        Line::from("Enter para guardar · Esc para cancelar").fg(Color::DarkGray),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Guardar perfil "),
    );
    f.render_widget(p, area);
}

fn draw_review(f: &mut Frame, area: Rect, app: &App) {
    let plan = app.build_plan();

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    lines.extend(review_env_section(&plan));
    lines.push(Line::from(""));
    lines.extend(review_packages_section(&plan, &app.sys_state));
    lines.extend(review_system_section(&plan));
    lines.push(Line::from(""));
    lines.push(review_footer());

    let p = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Revision del plan "),
    );
    f.render_widget(p, area);
}

/// Lineas con el entorno elegido y su display manager.
fn review_env_section(plan: &InstallPlan) -> Vec<Line<'static>> {
    let de_label = plan
        .desktop_env_id
        .as_deref()
        .map(lookup_de_label)
        .unwrap_or("(ninguno)");
    let dm = plan
        .display_manager
        .clone()
        .unwrap_or_else(|| "ninguno".into());
    vec![
        Line::from(vec![
            Span::raw("  Entorno de escritorio:  "),
            Span::styled(
                de_label.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("  Display manager:        "),
            Span::styled(dm, Style::default().fg(Color::Cyan)),
        ]),
    ]
}

/// Paquetes y servicios, separados en "por instalar" y "ya instalado"
/// usando la deteccion del sistema. El resumen indica cuantos hay en cada
/// lado para que el usuario vea de un vistazo cuanto trabajo queda.
fn review_packages_section(plan: &InstallPlan, sys: &SystemStatus) -> Vec<Line<'static>> {
    let (off_to_install, off_have): (Vec<String>, Vec<String>) = plan
        .official
        .iter()
        .cloned()
        .partition(|p| !sys.official.contains(p) && !sys.aur.contains(p));
    let (aur_to_install, aur_have): (Vec<String>, Vec<String>) = plan
        .aur
        .iter()
        .cloned()
        .partition(|p| !sys.official.contains(p) && !sys.aur.contains(p));

    let mut out = Vec::new();

    out.push(Line::from(vec![
        Span::styled(
            format!(
                "  Por instalar ({}):  ",
                off_to_install.len() + aur_to_install.len()
            ),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            join_with_source(&off_to_install, &aur_to_install),
            Style::default().fg(Color::Gray),
        ),
    ]));
    if off_to_install.is_empty() && aur_to_install.is_empty() {
        out.push(Line::from(Span::styled(
            "    (todo lo del plan ya esta instalado; pacman -Syu actualizara si hay algo pendiente)",
            Style::default().fg(Color::DarkGray),
        )));
    } else if !sys.updates_available.is_empty() {
        out.push(Line::from(Span::styled(
            format!(
                "    (ademas hay {} paquete(s) con actualizacion disponible en el sistema)",
                sys.updates_available.len()
            ),
            Style::default().fg(Color::DarkGray),
        )));
    }
    out.push(Line::from(""));

    out.push(Line::from(vec![
        Span::styled(
            format!("  Ya instalado ({}):  ", off_have.len() + aur_have.len()),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            join_with_source(&off_have, &aur_have),
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    out.push(Line::from(""));

    let mut svcs = plan.services.clone();
    if !plan.user_services.is_empty() {
        svcs.push("audio (PipeWire, --user)".into());
    }
    let (svcs_to_enable, svcs_already): (Vec<String>, Vec<String>) = svcs
        .iter()
        .cloned()
        .partition(|s| !matches!(sys.service_status(s), crate::detect::ServiceStatus::Enabled));
    out.push(Line::from(vec![
        Span::styled(
            format!("  Servicios a habilitar ({}):  ", svcs_to_enable.len()),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format_list_or_none(&svcs_to_enable),
            Style::default().fg(Color::Gray),
        ),
    ]));
    if !svcs_already.is_empty() {
        out.push(Line::from(vec![
            Span::styled(
                format!("  Servicios ya activos ({}):  ", svcs_already.len()),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format_list_or_none(&svcs_already),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    out
}

/// Junta dos listas (oficial + AUR) en una sola linea con un sufijo `(aur)`
/// en los del AUR, para que el usuario distinga el origen sin perder la
/// lista compacta.
fn join_with_source(official: &[String], aur: &[String]) -> String {
    let mut parts: Vec<String> = official.to_vec();
    for a in aur {
        parts.push(format!("{a} (aur)"));
    }
    format_list_or_none(&parts)
}

/// Lineas con locale/zona/teclado/hostname/multilib/reiniciar, o vacias si
/// el usuario no toco ninguno de esos campos.
fn review_system_section(plan: &InstallPlan) -> Vec<Line<'static>> {
    let sys = format_system_settings(
        plan.locale.as_deref(),
        plan.timezone.as_deref(),
        plan.keymap.as_deref(),
        plan.hostname.as_deref(),
        plan.enable_multilib,
        plan.reboot_after,
        SystemLabelStyle::Detailed,
    );
    if sys.is_empty() {
        return Vec::new();
    }
    vec![Line::from(vec![
        Span::styled(
            "  Sistema:  ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(sys.join(", "), Style::default().fg(Color::Gray)),
    ])]
}

/// Pie con los atajos de teclado de la pantalla de revision.
fn review_footer() -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Enter",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" confirma e instala  ·  "),
        Span::styled(
            "Esc",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" vuelve al menu  ·  "),
        Span::styled(
            "s",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" salir sin hacer nada"),
    ])
}

/// Busca la etiqueta legible de un entorno de escritorio a partir de su id.
/// Si no se encuentra, devuelve el id tal cual.
fn lookup_de_label(id: &str) -> &str {
    DESKTOP_ENVIRONMENTS
        .iter()
        .find(|d| d.id == id)
        .map(|d| d.label)
        .unwrap_or(id)
}

fn render_list(f: &mut Frame, area: Rect, items: Vec<ListItem>, cursor: usize, title: &str) {
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title.to_string()),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("➤ ");
    let mut state = ListState::default();
    state.select(Some(cursor));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_status(f: &mut Frame, area: Rect, app: &App) {
    let help = match app.mode {
        Mode::Welcome => "Enter: comenzar · q: salir",
        Mode::Main => "↑/↓: mover · Enter: abrir · q: salir",
        Mode::Search => "Tab: fuente · i: editar · Space: anadir · Enter: volver · q: menu",
        Mode::System => "↑/↓: campo · Enter/i: editar · Space: marcar · q: menu",
        Mode::LoadProfile => "↑/↓: mover · Enter: cargar · q: menu",
        Mode::SaveProfile => "Escribe el nombre · Enter: guardar · Esc: cancelar",
        Mode::Review => "Enter: instalar · s: salir sin hacer nada · Esc: volver al menu",
        _ => "↑/↓: mover · Space: marcar · q: volver al menu",
    };
    let text = vec![
        Line::from(Span::styled(
            app.status.clone(),
            Style::default().fg(Color::Yellow),
        )),
        Line::from(Span::styled(help, Style::default().fg(Color::DarkGray))),
    ];
    let p = Paragraph::new(text)
        .alignment(Alignment::Left)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_plan_without_env_has_no_base() {
        let mut app = App::new();
        app.de_index = 0; // "ninguno"
        let plan = app.build_plan();
        assert!(plan.desktop_env_id.is_none());
        assert!(plan.display_manager.is_none());
        assert!(!plan.official.iter().any(|p| p == "xorg-server"));
    }

    #[test]
    fn build_plan_with_env_includes_base_and_dm() {
        let mut app = App::new();
        let idx = DESKTOP_ENVIRONMENTS
            .iter()
            .position(|d| d.id == "kde")
            .unwrap();
        app.de_index = idx;
        let plan = app.build_plan();
        assert_eq!(plan.desktop_env_id.as_deref(), Some("kde"));
        assert_eq!(plan.display_manager.as_deref(), Some("sddm"));
        assert!(plan.official.iter().any(|p| p == "xorg-server"));
        assert!(plan.official.iter().any(|p| p == "plasma-meta"));
        assert!(plan.official.iter().any(|p| p == "sddm"));
    }

    #[test]
    fn build_plan_deduplicates() {
        let mut app = App::new();
        let idx = DESKTOP_ENVIRONMENTS
            .iter()
            .position(|d| d.id == "hyprland")
            .unwrap();
        app.de_index = idx;
        // kitty viene del entorno hyprland Y esta marcado por defecto en extras.
        let plan = app.build_plan();
        let count = plan.official.iter().filter(|p| *p == "kitty").count();
        assert_eq!(count, 1, "los paquetes no deben repetirse en el plan");
    }

    #[test]
    fn apply_profile_marks_and_adds_packages() {
        let mut app = App::new();
        let prof = Profile {
            name: "t".into(),
            desktop_environment: Some("gnome".into()),
            display_manager: Some("gdm".into()),
            official_packages: vec!["firefox".into(), "paquete-nuevo".into()],
            aur_packages: vec![],
        };
        app.apply_profile(prof);
        assert_eq!(DESKTOP_ENVIRONMENTS[app.de_index].id, "gnome");
        let firefox = app.official.iter().find(|p| p.name == "firefox").unwrap();
        assert!(firefox.selected);
        // Un paquete del perfil que no estaba en el catalogo se anade marcado.
        let nuevo = app
            .official
            .iter()
            .find(|p| p.name == "paquete-nuevo")
            .unwrap();
        assert!(nuevo.selected);
        // Lo no incluido en el perfil queda desmarcado.
        let vlc = app.official.iter().find(|p| p.name == "vlc").unwrap();
        assert!(!vlc.selected);
    }

    #[test]
    fn move_cursor_wraps_and_jumps() {
        let mut c = 0usize;
        move_cursor(&mut c, 5, KeyCode::Up);
        assert_eq!(c, 4, "subir desde el inicio envuelve al final");
        move_cursor(&mut c, 5, KeyCode::Down);
        assert_eq!(c, 0);
        move_cursor(&mut c, 5, KeyCode::End);
        assert_eq!(c, 4);
        move_cursor(&mut c, 5, KeyCode::Home);
        assert_eq!(c, 0);
        move_cursor(&mut c, 50, KeyCode::PageDown);
        assert_eq!(c, 10);
        move_cursor(&mut c, 0, KeyCode::Down); // lista vacia: no panic
        assert_eq!(c, 10);
    }
}
