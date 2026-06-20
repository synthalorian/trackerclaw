use crate::config;
use crate::store::Store;
use anyhow::Result;
use chrono::Utc;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Terminal,
};
use std::io::{self, stdout};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Work,
    Break,
}

struct PomodoroState {
    phase: Phase,
    remaining: Duration,
    total: Duration,
    start: Instant,
    task_name: String,
    running: bool,
    done: bool,
    work_seconds: u64,
    break_seconds: u64,
}

impl PomodoroState {
    fn new(task_name: String) -> Self {
        let cfg = config::load_config();
        let work_seconds = cfg.pomodoro_work_minutes * 60;
        let total = Duration::from_secs(work_seconds);
        Self {
            phase: Phase::Work,
            remaining: total,
            total,
            start: Instant::now(),
            task_name,
            running: true,
            done: false,
            work_seconds,
            break_seconds: cfg.pomodoro_break_minutes * 60,
        }
    }

    fn tick(&mut self) {
        if !self.running || self.done {
            return;
        }
        let elapsed = self.start.elapsed();
        if elapsed >= self.total {
            self.remaining = Duration::ZERO;
            self.done = true;
        } else {
            self.remaining = self.total - elapsed;
        }
    }

    fn transition_to_break(&mut self) {
        self.phase = Phase::Break;
        self.total = Duration::from_secs(self.break_seconds);
        self.remaining = self.total;
        self.start = Instant::now();
        self.running = true;
        self.done = false;
    }

    fn fmt_time(d: Duration) -> String {
        let secs = d.as_secs();
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

pub async fn run(db: &str, task_name: Option<String>) -> Result<()> {
    let task = task_name.unwrap_or_else(|| "Pomodoro session".to_string());
    let mut state = PomodoroState::new(task.clone());

    enable_raw_mode()?;
    let mut stdout = stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(1),
                    Constraint::Length(5),
                    Constraint::Length(3),
                    Constraint::Min(1),
                ])
                .split(f.size());

            let phase_text = match state.phase {
                Phase::Work => "🍅 FOCUS",
                Phase::Break => "☕ BREAK",
            };
            let phase_color = match state.phase {
                Phase::Work => Color::Red,
                Phase::Break => Color::Green,
            };

            let title = Paragraph::new(phase_text)
                .style(Style::default().fg(phase_color).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(title, chunks[0]);

            let task_line = Line::from(vec![
                Span::styled("Task: ", Style::default().fg(Color::Yellow)),
                Span::raw(&state.task_name),
            ]);
            f.render_widget(Paragraph::new(task_line).alignment(Alignment::Center), chunks[1]);

            let timer_text = PomodoroState::fmt_time(state.remaining);
            let timer = Paragraph::new(timer_text)
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(timer, chunks[2]);

            let elapsed = state.total.saturating_sub(state.remaining);
            let ratio = if state.total.as_secs() > 0 {
                elapsed.as_secs() as f64 / state.total.as_secs() as f64
            } else {
                1.0
            };
            let gauge = Gauge::default()
                .block(Block::default().borders(Borders::ALL))
                .gauge_style(Style::default().fg(phase_color).bg(Color::DarkGray))
                .ratio(ratio.clamp(0.0, 1.0));
            f.render_widget(gauge, chunks[3]);

            let hint = if state.done && state.phase == Phase::Work {
                "Press [Space] to start break, [q] to quit"
            } else if state.done && state.phase == Phase::Break {
                "Break done! Press [q] to quit"
            } else if state.running {
                "[Space] pause/resume  [q] quit"
            } else {
                "[Space] resume  [q] quit"
            };
            let hint_para = Paragraph::new(hint)
                .style(Style::default().fg(Color::Gray))
                .alignment(Alignment::Center);
            f.render_widget(hint_para, chunks[4]);
        })?;

        // Handle tick every second
        if last_tick.elapsed() >= Duration::from_secs(1) {
            state.tick();
            last_tick = Instant::now();
        }

        // Poll events with timeout so we can update the timer
        if crossterm::event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char(' ') => {
                            if state.done && state.phase == Phase::Work {
                                state.transition_to_break();
                            } else {
                                state.running = !state.running;
                                if state.running {
                                    // Adjust start so remaining is correct
                                    let elapsed = state.total.saturating_sub(state.remaining);
                                    state.start = Instant::now() - elapsed;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    // Log the pomodoro work session to the database if work phase completed
    if state.phase == Phase::Break || (state.phase == Phase::Work && state.done) {
        let store = Store::open(std::path::Path::new(db))?;
        let name = format!("Pomodoro: {}", task);
        let started = Utc::now() - chrono::Duration::seconds(state.work_seconds as i64);
        let ended = Utc::now();
        let duration = (ended - started).num_seconds();
        store.insert_completed_entry(&name, Some("pomodoro"), None, started, ended, duration)?;
        println!("Logged Pomodoro session ({}s)", duration);
    }

    Ok(())
}
