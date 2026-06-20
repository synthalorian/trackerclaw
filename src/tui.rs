use crate::auth;
use crate::budget;
use crate::calendar;
use crate::charts;
use crate::idle;
use crate::store::Store;
use chrono::Datelike;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use std::io::{self, stdout};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Entries,
    DailyChart,
    ProjectChart,
    Calendar,
}

#[derive(Clone, Copy, PartialEq)]
enum IdlePrompt {
    None,
    AskResume,
}

struct TuiState {
    tab: Tab,
    selected: usize,
    last_idle_check: Instant,
    was_idle: bool,
    idle_prompt: IdlePrompt,
    last_entry_name: Option<String>,
    last_entry_tags: Option<String>,
    calendar_year: i32,
    calendar_month: u32,
}

impl TuiState {
    fn new() -> Self {
        let now = chrono::Local::now();
        Self {
            tab: Tab::Entries,
            selected: 0,
            last_idle_check: Instant::now(),
            was_idle: false,
            idle_prompt: IdlePrompt::None,
            last_entry_name: None,
            last_entry_tags: None,
            calendar_year: now.year(),
            calendar_month: now.month(),
        }
    }
}

pub async fn run(db: &str) -> Result<()> {
    let (user_id, _user_name, role) = auth::resolve_current_user(db, None)?;
    let is_admin = role == "admin";
    let store = Store::open(std::path::Path::new(db))?;
    enable_raw_mode()?;
    let mut stdout = stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = TuiState::new();
    let mut entries = store.list_today(user_id, is_admin)?;
    let mut daily_stats = store.daily_stats(7, user_id, is_admin)?;
    let mut project_stats = store.project_stats(30, user_id, is_admin)?;
    let mut budgets = store.list_budgets()?;

    let mut last_refresh = Instant::now();

    loop {
        if last_refresh.elapsed() >= Duration::from_secs(10) {
            entries = store.list_today(user_id, is_admin)?;
            daily_stats = store.daily_stats(7, user_id, is_admin)?;
            project_stats = store.project_stats(30, user_id, is_admin)?;
            budgets = store.list_budgets()?;
            last_refresh = Instant::now();
        }

        // Idle detection check
        if state.idle_prompt == IdlePrompt::None && state.last_idle_check.elapsed() >= Duration::from_secs(30) {
            if let Some(ms) = idle::get_idle_time_ms() {
                let is_idle = ms >= idle::idle_threshold_ms();
                if is_idle && !state.was_idle {
                    // Just became idle - remember current entry if any
                    if let Ok(Some(current)) = store.get_current(user_id) {
                        state.last_entry_name = Some(current.name);
                        state.last_entry_tags = current.tags;
                    }
                    state.was_idle = true;
                } else if !is_idle && state.was_idle {
                    // Back from idle - prompt to resume
                    state.idle_prompt = IdlePrompt::AskResume;
                    state.was_idle = false;
                }
            }
            state.last_idle_check = Instant::now();
        }

        terminal.draw(|f| {
            let size = f.size();

            // Main layout with possible idle popup overlay
            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)])
                .split(size);

            let tabs_text = match state.tab {
                Tab::Entries => "[Entries]  Daily  Projects  Calendar",
                Tab::DailyChart => " Entries  [Daily]  Projects  Calendar",
                Tab::ProjectChart => " Entries  Daily  [Projects]  Calendar",
                Tab::Calendar => " Entries  Daily  Projects  [Calendar]",
            };
            let tabs = Paragraph::new(tabs_text)
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title("TrackerClaw"));
            f.render_widget(tabs, main_chunks[0]);

            match state.tab {
                Tab::Entries => render_entries(f, main_chunks[1], &entries, state.selected),
                Tab::DailyChart => render_daily_chart(f, main_chunks[1], &daily_stats),
                Tab::ProjectChart => render_project_chart(f, main_chunks[1], &project_stats, &budgets),
                Tab::Calendar => { let _ = render_calendar(f, main_chunks[1], state.calendar_year, state.calendar_month, &store); }
            }

            // Idle prompt overlay
            if state.idle_prompt == IdlePrompt::AskResume {
                let popup_area = centered_rect(60, 30, size);
                f.render_widget(Clear, popup_area);
                let popup_block = Block::default()
                    .title("Idle Detected")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .style(Style::default().bg(Color::Black));
                f.render_widget(popup_block.clone(), popup_area);

                let inner = popup_block.inner(popup_area);
                let text = Text::from(vec![
                    Line::from("You were idle. Resume previous task?"),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("[y]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                        Span::raw(" Resume  "),
                        Span::styled("[n]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                        Span::raw(" Keep stopped  "),
                        Span::styled("[q]", Style::default().fg(Color::Gray)),
                        Span::raw(" Dismiss"),
                    ]),
                ]);
                let popup = Paragraph::new(text).wrap(Wrap { trim: true });
                f.render_widget(popup, inner);
            }
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                // Handle idle prompt first
                if state.idle_prompt == IdlePrompt::AskResume {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            if let Some(ref name) = state.last_entry_name {
                                let _ = store.start_entry(name, state.last_entry_tags.as_deref(), None, user_id);
                            }
                            state.idle_prompt = IdlePrompt::None;
                            state.last_entry_name = None;
                            state.last_entry_tags = None;
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') | KeyCode::Esc => {
                            state.idle_prompt = IdlePrompt::None;
                            state.last_entry_name = None;
                            state.last_entry_tags = None;
                        }
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('1') => state.tab = Tab::Entries,
                    KeyCode::Char('2') => state.tab = Tab::DailyChart,
                    KeyCode::Char('3') => state.tab = Tab::ProjectChart,
                    KeyCode::Char('4') => state.tab = Tab::Calendar,
                    KeyCode::Right | KeyCode::Char('l') => {
                        state.tab = match state.tab {
                            Tab::Entries => Tab::DailyChart,
                            Tab::DailyChart => Tab::ProjectChart,
                            Tab::ProjectChart => Tab::Calendar,
                            Tab::Calendar => Tab::Entries,
                        };
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        state.tab = match state.tab {
                            Tab::Entries => Tab::Calendar,
                            Tab::DailyChart => Tab::Entries,
                            Tab::ProjectChart => Tab::DailyChart,
                            Tab::Calendar => Tab::ProjectChart,
                        };
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if state.tab == Tab::Entries && state.selected + 1 < entries.len() {
                            state.selected += 1;
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if state.tab == Tab::Entries && state.selected > 0 {
                            state.selected -= 1;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn render_entries(f: &mut ratatui::Frame, area: Rect, entries: &[crate::store::Entry], selected: usize) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let dur_s = e.duration_seconds.unwrap_or(0);
            let dur = if dur_s >= 3600 {
                format!("{:.1}h", dur_s as f64 / 3600.0)
            } else {
                format!("{}m", dur_s / 60)
            };
            let label = format!("{:<25} {:>6}", e.name, dur);
            let style = if i == selected {
                Style::default()
                    .bg(Color::Magenta)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };
            ListItem::new(label).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Today's Entries"));
    f.render_widget(list, chunks[0]);

    let detail = if let Some(e) = entries.get(selected) {
        let dur_s = e.duration_seconds.unwrap_or(0);
        let dur = if dur_s >= 3600 {
            format!("{:.2}h", dur_s as f64 / 3600.0)
        } else {
            format!("{}m {}s", dur_s / 60, dur_s % 60)
        };
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::Yellow)),
                Span::raw(&e.name),
            ]),
            Line::from(vec![
                Span::styled("Started: ", Style::default().fg(Color::Yellow)),
                Span::raw(&e.started_at),
            ]),
            Line::from(vec![
                Span::styled("Duration: ", Style::default().fg(Color::Yellow)),
                Span::raw(dur),
            ]),
        ];
        if let Some(ref t) = e.tags {
            lines.push(Line::from(vec![
                Span::styled("Tags: ", Style::default().fg(Color::Yellow)),
                Span::raw(t),
            ]));
        }
        Paragraph::new(lines)
    } else {
        Paragraph::new("No entries today")
    };

    let detail = detail.block(Block::default().borders(Borders::ALL).title("Detail"));
    f.render_widget(detail, chunks[1]);
}

fn render_daily_chart(f: &mut ratatui::Frame, area: Rect, stats: &[(String, i64)]) {
    let data: Vec<(String, f64)> = stats
        .iter()
        .map(|(day, seconds)| {
            let short = day.split('-').skip(1).collect::<Vec<_>>().join("-");
            (short, charts::format_hours(*seconds))
        })
        .collect();

    let width = area.width as usize;
    let height = area.height as usize;
    let chart_lines = charts::tui_bar_chart(&data, "Daily Hours (Last 7 Days)", width.saturating_sub(2), height.saturating_sub(2));
    let text = Text::from(chart_lines.into_iter().map(Line::from).collect::<Vec<_>>());
    let chart = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Daily Chart"));
    f.render_widget(chart, area);
}

fn render_project_chart(f: &mut ratatui::Frame, area: Rect, stats: &[(String, i64)], budgets: &[(String, i64, i64)]) {
    let filtered: Vec<(String, i64)> = stats.iter()
        .filter(|(_, s)| *s > 0)
        .take(8)
        .cloned()
        .collect();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    let width = chunks[0].width as usize;
    let chart_lines = charts::tui_project_chart(&filtered, "Project Breakdown (Last 30 Days)", width.saturating_sub(2));
    let text = Text::from(chart_lines.into_iter().map(Line::from).collect::<Vec<_>>());
    let chart = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Project Chart"));
    f.render_widget(chart, chunks[0]);

    let budget_items: Vec<Line> = budgets.iter().map(|(project, budget, used)| {
        let bar = budget::render_budget_bar(*used, *budget, 20);
        Line::from(vec![
            Span::raw(format!("{:<15} ", project)),
            Span::styled(bar, Style::default().fg(Color::Cyan)),
        ])
    }).collect();
    let budget_text = Text::from(budget_items);
    let budget_widget = Paragraph::new(budget_text)
        .block(Block::default().borders(Borders::ALL).title("Budgets"));
    f.render_widget(budget_widget, chunks[1]);
}

fn render_calendar(f: &mut ratatui::Frame, area: Rect, year: i32, month: u32, store: &Store) -> Result<()> {
    use chrono::Datelike;
    let first = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let mut days_in_month = 28u32;
    while chrono::NaiveDate::from_ymd_opt(year, month, days_in_month + 1).is_some() {
        days_in_month += 1;
    }

    let start = first.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(chrono::Utc).unwrap();
    let end = start + chrono::Duration::days(days_in_month as i64);
    let entries = store.entries_for_date_range(start, end)?;
    let mut daily: std::collections::HashMap<u32, i64> = std::collections::HashMap::new();
    for e in entries {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&e.started_at) {
            let day = dt.day();
            *daily.entry(day).or_insert(0) += e.duration_seconds.unwrap_or(0);
        }
    }

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("  {}-{}", year, month), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  Su  Mo  Tu  We  Th  Fr  Sa"),
    ];

    let weekday = first.weekday().num_days_from_sunday();
    let mut current_line = String::from("  ");
    for _ in 0..weekday {
        current_line.push_str("    ");
    }

    for day in 1..=days_in_month {
        let seconds = daily.get(&day).copied().unwrap_or(0);
        let style = if seconds > 0 {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let day_str = format!("{:>3} ", day);
        let spans = vec![Span::styled(day_str, style)];
        lines.push(Line::from(spans));
    }

    let text = Text::from(lines);
    let cal = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Calendar"));
    f.render_widget(cal, area);
    Ok(())
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
