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
use crate::model::{InstallPlan, Profile, Source};
use crate::{profile, repo_api};

/// Un paquete seleccionable (curado o anadido por busqueda).
#[derive(Clone)]
struct PkgItem {
    name: String,
    description: String,
    selected: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Welcome,
    Main,
    Desktop,
    Drivers,
    Official,
    Aur,
    Search,
    LoadProfile,
    SaveProfile,
    Review,
}

/// Resultado del asistente.
pub enum Outcome {
    Cancelled,
    Confirmed {
        plan: InstallPlan,
        save_as: Option<String>,
    },
}

/// Entradas del menu principal. Los indices se usan en `handle_main`.
const MENU: &[&str] = &[
    "Entorno de escritorio",
    "Controladores (drivers GPU + microcodigo)",
    "Paquetes oficiales",
    "Paquetes AUR",
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
const MENU_SEARCH: usize = 4;
const MENU_LOAD: usize = 5;
const MENU_SAVE: usize = 6;
const MENU_INSTALL: usize = 7;
const MENU_QUIT: usize = 8;

struct App {
    mode: Mode,
    main_cursor: usize,
    de_index: usize,
    /// Marcado/no marcado, paralelo a `catalog::DRIVERS`.
    drivers: Vec<bool>,
    official: Vec<PkgItem>,
    aur: Vec<PkgItem>,
    list_cursor: usize,
    status: String,

    // Busqueda
    search_source: Source,
    search_input: String,
    search_results: Vec<PkgItem>,
    typing: bool, // true: el texto va al campo de entrada

    // Perfiles
    profiles: Vec<String>,
    name_input: String,
}

impl App {
    fn new() -> Self {
        let official = EXTRA_PACKAGES
            .iter()
            .filter(|p| p.source == Source::Official)
            .map(|p| PkgItem {
                name: p.name.to_string(),
                description: p.description.to_string(),
                selected: p.default_on,
            })
            .collect();
        let aur = EXTRA_PACKAGES
            .iter()
            .filter(|p| p.source == Source::Aur)
            .map(|p| PkgItem {
                name: p.name.to_string(),
                description: p.description.to_string(),
                selected: p.default_on,
            })
            .collect();

        let drivers = DRIVERS.iter().map(|d| d.default_on).collect();

        App {
            mode: Mode::Welcome,
            main_cursor: 0,
            de_index: 0,
            drivers,
            official,
            aur,
            list_cursor: 0,
            status: "Usa ↑/↓ y Enter. En esta pantalla 'Instalar ahora' lanza todo.".into(),
            search_source: Source::Official,
            search_input: String::new(),
            search_results: Vec::new(),
            typing: false,
            profiles: Vec::new(),
            name_input: String::new(),
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
        InstallPlan::new(de_id, dm, official, aur)
    }

    fn count_drivers(&self) -> usize {
        self.drivers.iter().filter(|&&d| d).count()
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
        // Desmarca todo y luego marca lo del perfil, anadiendo lo que falte.
        for it in self.official.iter_mut() {
            it.selected = p.official_packages.contains(&it.name);
        }
        for name in &p.official_packages {
            if !self.official.iter().any(|i| &i.name == name) {
                self.official.push(PkgItem {
                    name: name.clone(),
                    description: "(del perfil)".into(),
                    selected: true,
                });
            }
        }
        for it in self.aur.iter_mut() {
            it.selected = p.aur_packages.contains(&it.name);
        }
        for name in &p.aur_packages {
            if !self.aur.iter().any(|i| &i.name == name) {
                self.aur.push(PkgItem {
                    name: name.clone(),
                    description: "(del perfil)".into(),
                    selected: true,
                });
            }
        }
        self.status = format!("Perfil '{}' cargado.", p.name);
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

/// Anade o alterna un paquete en una lista por nombre. Devuelve si quedo activo.
fn toggle_into(vec: &mut Vec<PkgItem>, name: &str, desc: &str) -> bool {
    if let Some(it) = vec.iter_mut().find(|p| p.name == name) {
        it.selected = !it.selected;
        it.selected
    } else {
        vec.push(PkgItem {
            name: name.to_string(),
            description: desc.to_string(),
            selected: true,
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
                KeyCode::Backspace => {
                    if app.mode == Mode::Search {
                        app.search_input.pop();
                    } else {
                        app.name_input.pop();
                    }
                }
                KeyCode::Char(c) => {
                    if app.mode == Mode::Search {
                        app.search_input.push(c);
                    } else {
                        app.name_input.push(c);
                    }
                }
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
            Mode::Search => handle_search(&mut app, key.code),
            Mode::LoadProfile => handle_load_profile(&mut app, key.code),
            Mode::SaveProfile => {} // se maneja via typing
            Mode::Review => match key.code {
                KeyCode::Enter => {
                    break Outcome::Confirmed {
                        plan: app.build_plan(),
                        save_as: None,
                    };
                }
                KeyCode::Esc | KeyCode::Char('q') => app.mode = Mode::Main,
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
                        .map(|f| PkgItem {
                            name: f.name,
                            description: f.description,
                            selected: false,
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
                let now_on = toggle_into(target, &res.name, &res.description);
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

fn draw_drivers(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = DRIVERS
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let on = app.drivers.get(i).copied().unwrap_or(false);
            let checkbox = if on { "[x] " } else { "[ ] " };
            let cb_style = if on {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            ListItem::new(Line::from(vec![
                Span::styled(checkbox, cb_style),
                Span::styled(
                    format!("{:<40}", d.label),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(d.packages.join(" "), Style::default().fg(Color::Gray)),
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
    ListItem::new(Line::from(vec![
        Span::styled(checkbox, cb_style),
        Span::styled(
            format!("{:<28}", p.name),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate(&p.description, 60),
            Style::default().fg(Color::Gray),
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
    let de = &DESKTOP_ENVIRONMENTS[app.de_index];

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  Entorno de escritorio:  "),
            Span::styled(
                de.label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("  Display manager:        "),
            Span::styled(
                plan.display_manager
                    .clone()
                    .unwrap_or_else(|| "ninguno".into()),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("  Oficiales ({}):  ", plan.official.len()),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                join_wrapped(&plan.official),
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("  AUR ({}):  ", plan.aur.len()),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(join_wrapped(&plan.aur), Style::default().fg(Color::Gray)),
        ]),
        Line::from(""),
    ];

    // Servicios que se habilitaran para dejar el sistema listo para usar.
    let mut svcs = plan.services.clone();
    if !plan.user_services.is_empty() {
        svcs.push("audio (PipeWire, --user)".into());
    }
    lines.push(Line::from(vec![
        Span::styled(
            "  Servicios a habilitar:  ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(join_wrapped(&svcs), Style::default().fg(Color::Gray)),
    ]));
    lines.push(Line::from(Span::styled(
        "  (systemctl enable: el equipo arranca listo para usarse)",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
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
        Span::raw(" vuelve al menu"),
    ]));

    let p = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Revision del plan "),
    );
    f.render_widget(p, area);
}

fn join_wrapped(items: &[String]) -> String {
    if items.is_empty() {
        "(ninguno)".to_string()
    } else {
        items.join(", ")
    }
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
        Mode::LoadProfile => "↑/↓: mover · Enter: cargar · q: menu",
        Mode::SaveProfile => "Escribe el nombre · Enter: guardar · Esc: cancelar",
        Mode::Review => "Enter: confirmar e instalar · Esc: volver al menu",
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
