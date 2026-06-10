//! Interfaz TUI (asistente) construida con ratatui.
//!
//! Flujo: Bienvenida -> Entorno de escritorio -> Paquetes oficiales ->
//! Paquetes AUR -> Revision (guardar perfil) -> confirmar.

use anyhow::Result;
use ratatui::{
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::catalog::{BASE_PACKAGES, DESKTOP_ENVIRONMENTS, EXTRA_PACKAGES};
use crate::model::{InstallPlan, Source};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Welcome,
    DesktopEnv,
    Official,
    Aur,
    Review,
}

/// Resultado del asistente.
pub enum Outcome {
    Cancelled,
    Confirmed { plan: InstallPlan, save_as: Option<String> },
}

struct App {
    screen: Screen,
    de_index: usize,
    /// Marcado/no marcado, paralelo a EXTRA_PACKAGES.
    selected: Vec<bool>,
    cursor: usize,
    save_profile: bool,
    editing_name: bool,
    profile_name: String,
}

impl App {
    fn new() -> Self {
        let selected = EXTRA_PACKAGES.iter().map(|p| p.default_on).collect();
        App {
            screen: Screen::Welcome,
            de_index: 0,
            selected,
            cursor: 0,
            save_profile: false,
            editing_name: false,
            profile_name: String::new(),
        }
    }

    /// Indices de EXTRA_PACKAGES que pertenecen a un origen dado.
    fn indices_for(&self, source: Source) -> Vec<usize> {
        EXTRA_PACKAGES
            .iter()
            .enumerate()
            .filter(|(_, p)| p.source == source)
            .map(|(i, _)| i)
            .collect()
    }

    fn current_list_len(&self) -> usize {
        match self.screen {
            Screen::DesktopEnv => DESKTOP_ENVIRONMENTS.len(),
            Screen::Official => self.indices_for(Source::Official).len(),
            Screen::Aur => self.indices_for(Source::Aur).len(),
            _ => 0,
        }
    }

    fn move_cursor(&mut self, delta: isize) {
        let len = self.current_list_len();
        if len == 0 {
            return;
        }
        let cur = self.cursor as isize + delta;
        self.cursor = cur.rem_euclid(len as isize) as usize;
    }

    fn toggle_current(&mut self) {
        match self.screen {
            Screen::DesktopEnv => self.de_index = self.cursor,
            Screen::Official => {
                let idxs = self.indices_for(Source::Official);
                if let Some(&i) = idxs.get(self.cursor) {
                    self.selected[i] = !self.selected[i];
                }
            }
            Screen::Aur => {
                let idxs = self.indices_for(Source::Aur);
                if let Some(&i) = idxs.get(self.cursor) {
                    self.selected[i] = !self.selected[i];
                }
            }
            _ => {}
        }
    }

    fn build_plan(&self) -> InstallPlan {
        let de = &DESKTOP_ENVIRONMENTS[self.de_index];

        let mut official: Vec<String> = Vec::new();
        let mut aur: Vec<String> = Vec::new();

        // Base + entorno solo si se eligio un entorno real.
        if de.id != "ninguno" {
            for p in BASE_PACKAGES {
                official.push((*p).to_string());
            }
            for p in de.packages {
                official.push((*p).to_string());
            }
            if let Some(dm) = de.display_manager {
                official.push(dm.to_string());
            }
        }

        for (i, pkg) in EXTRA_PACKAGES.iter().enumerate() {
            if !self.selected[i] {
                continue;
            }
            match pkg.source {
                Source::Official => official.push(pkg.name.to_string()),
                Source::Aur => aur.push(pkg.name.to_string()),
            }
        }

        official.sort();
        official.dedup();
        aur.sort();
        aur.dedup();

        InstallPlan {
            desktop_env_id: if de.id == "ninguno" {
                None
            } else {
                Some(de.id.to_string())
            },
            display_manager: if de.id == "ninguno" {
                None
            } else {
                de.display_manager.map(|s| s.to_string())
            },
            official,
            aur,
        }
    }
}

/// Lanza el asistente TUI y devuelve el resultado.
pub fn run() -> Result<Outcome> {
    let mut terminal = ratatui::init();
    let mut app = App::new();
    let result = loop {
        terminal.draw(|f| draw(f, &app))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        // Modo edicion del nombre del perfil (solo en Review).
        if app.editing_name {
            match key.code {
                KeyCode::Enter | KeyCode::Esc => app.editing_name = false,
                KeyCode::Backspace => {
                    app.profile_name.pop();
                }
                KeyCode::Char(c) => app.profile_name.push(c),
                _ => {}
            }
            continue;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if app.screen == Screen::Welcome {
                    break Outcome::Cancelled;
                }
                // Retroceder una pantalla.
                app.screen = prev_screen(app.screen);
                app.cursor = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => app.move_cursor(-1),
            KeyCode::Down | KeyCode::Char('j') => app.move_cursor(1),
            KeyCode::Char(' ') => app.toggle_current(),
            KeyCode::Char('s') if app.screen == Screen::Review => {
                app.save_profile = !app.save_profile;
            }
            KeyCode::Char('n') if app.screen == Screen::Review && app.save_profile => {
                app.editing_name = true;
            }
            KeyCode::Enter => {
                if app.screen == Screen::Review {
                    let plan = app.build_plan();
                    let save_as = if app.save_profile && !app.profile_name.trim().is_empty() {
                        Some(app.profile_name.trim().to_string())
                    } else {
                        None
                    };
                    break Outcome::Confirmed { plan, save_as };
                }
                // En DesktopEnv, Enter ademas fija la seleccion bajo el cursor.
                if app.screen == Screen::DesktopEnv {
                    app.de_index = app.cursor;
                }
                app.screen = next_screen(app.screen);
                app.cursor = 0;
            }
            _ => {}
        }
    };

    ratatui::restore();
    Ok(result)
}

fn next_screen(s: Screen) -> Screen {
    match s {
        Screen::Welcome => Screen::DesktopEnv,
        Screen::DesktopEnv => Screen::Official,
        Screen::Official => Screen::Aur,
        Screen::Aur => Screen::Review,
        Screen::Review => Screen::Review,
    }
}

fn prev_screen(s: Screen) -> Screen {
    match s {
        Screen::Welcome => Screen::Welcome,
        Screen::DesktopEnv => Screen::Welcome,
        Screen::Official => Screen::DesktopEnv,
        Screen::Aur => Screen::Official,
        Screen::Review => Screen::Aur,
    }
}

// ----------------------------- Renderizado -----------------------------

fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(3), // titulo
        Constraint::Min(5),    // cuerpo
        Constraint::Length(3), // ayuda
    ])
    .split(f.area());

    draw_title(f, chunks[0], app);
    match app.screen {
        Screen::Welcome => draw_welcome(f, chunks[1]),
        Screen::DesktopEnv => draw_desktop_env(f, chunks[1], app),
        Screen::Official => draw_packages(f, chunks[1], app, Source::Official, "Paquetes oficiales (pacman)"),
        Screen::Aur => draw_packages(f, chunks[1], app, Source::Aur, "Paquetes del AUR (yay)"),
        Screen::Review => draw_review(f, chunks[1], app),
    }
    draw_help(f, chunks[2], app);
}

fn draw_title(f: &mut Frame, area: Rect, app: &App) {
    let step = match app.screen {
        Screen::Welcome => "Bienvenida",
        Screen::DesktopEnv => "Paso 1/4 · Entorno de escritorio",
        Screen::Official => "Paso 2/4 · Paquetes oficiales",
        Screen::Aur => "Paso 3/4 · Paquetes AUR",
        Screen::Review => "Paso 4/4 · Revision",
    };
    let title = Paragraph::new(Line::from(vec![
        Span::styled("  Arch Post-Install  ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(step, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ]))
    .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
    f.render_widget(title, area);
}

fn draw_welcome(f: &mut Frame, area: Rect) {
    let text = vec![
        Line::from(""),
        Line::from("Asistente de post-instalacion para Arch Linux.").bold(),
        Line::from(""),
        Line::from("Te guiare para elegir tu entorno de escritorio y los paquetes"),
        Line::from("que quieras instalar (oficiales y del AUR). Podras guardar tu"),
        Line::from("seleccion como un perfil reutilizable."),
        Line::from(""),
        Line::from(vec![
            Span::raw("No corras este programa como "),
            Span::styled("root", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("; se pedira contrasena con sudo cuando haga falta."),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw(" para comenzar  ·  "),
            Span::styled("q", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw(" para salir"),
        ]),
    ];
    let p = Paragraph::new(text)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}

fn draw_desktop_env(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = DESKTOP_ENVIRONMENTS
        .iter()
        .enumerate()
        .map(|(i, de)| {
            let marker = if i == app.de_index { "(•) " } else { "( ) " };
            let line = Line::from(vec![
                Span::styled(marker, Style::default().fg(Color::Green)),
                Span::styled(de.label, Style::default().add_modifier(Modifier::BOLD)),
            ]);
            ListItem::new(line)
        })
        .collect();

    render_list(f, area, items, app.cursor, "Elige UN entorno (Space/Enter para fijar)");
}

fn draw_packages(f: &mut Frame, area: Rect, app: &App, source: Source, title: &str) {
    let idxs = app.indices_for(source);
    let items: Vec<ListItem> = idxs
        .iter()
        .map(|&i| {
            let pkg = &EXTRA_PACKAGES[i];
            let checkbox = if app.selected[i] { "[x] " } else { "[ ] " };
            let cb_style = if app.selected[i] {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let line = Line::from(vec![
                Span::styled(checkbox, cb_style),
                Span::styled(format!("{:<26}", pkg.name), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(pkg.description, Style::default().fg(Color::Gray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    render_list(f, area, items, app.cursor, title);
}

fn render_list(f: &mut Frame, area: Rect, items: Vec<ListItem>, cursor: usize, title: &str) {
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!(" {title} ")))
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

fn draw_review(f: &mut Frame, area: Rect, app: &App) {
    let plan = app.build_plan();
    let de = &DESKTOP_ENVIRONMENTS[app.de_index];

    let mut lines = vec![
        Line::from(vec![
            Span::raw("Entorno: "),
            Span::styled(de.label, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::raw("Display manager: "),
            Span::styled(
                plan.display_manager.clone().unwrap_or_else(|| "ninguno".into()),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("Oficiales ({}): ", plan.official.len()),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(truncate_list(&plan.official), Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("AUR ({}): ", plan.aur.len()),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(truncate_list(&plan.aur), Style::default().fg(Color::Gray)),
        ]),
        Line::from(""),
    ];

    // Linea de guardado de perfil.
    let save_box = if app.save_profile { "[x]" } else { "[ ]" };
    lines.push(Line::from(vec![
        Span::styled(format!("{save_box} "), Style::default().fg(Color::Green)),
        Span::raw("Guardar como perfil  "),
        Span::styled("(s)", Style::default().fg(Color::DarkGray)),
    ]));
    if app.save_profile {
        let name_display = if app.profile_name.is_empty() {
            "<sin nombre>".to_string()
        } else {
            app.profile_name.clone()
        };
        let name_style = if app.editing_name {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default().fg(Color::Yellow)
        };
        lines.push(Line::from(vec![
            Span::raw("    Nombre: "),
            Span::styled(name_display, name_style),
            Span::raw("  "),
            Span::styled("(n para editar)", Style::default().fg(Color::DarkGray)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::raw(" para confirmar e instalar."),
    ]));

    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title(" Revision "));
    f.render_widget(p, area);
}

fn truncate_list(items: &[String]) -> String {
    if items.is_empty() {
        return "(ninguno)".to_string();
    }
    let joined = items.join(", ");
    if joined.len() > 200 {
        format!("{}…", &joined[..200])
    } else {
        joined
    }
}

fn draw_help(f: &mut Frame, area: Rect, app: &App) {
    let help = match app.screen {
        Screen::Welcome => "Enter: comenzar · q: salir",
        Screen::DesktopEnv => "↑/↓: mover · Space/Enter: elegir · q: atras",
        Screen::Official | Screen::Aur => "↑/↓: mover · Space: marcar · Enter: siguiente · q: atras",
        Screen::Review => "s: guardar perfil · n: nombre · Enter: instalar · q: atras",
    };
    let p = Paragraph::new(Line::from(Span::styled(
        help,
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}
