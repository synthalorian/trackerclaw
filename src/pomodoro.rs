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

#[derive(Clone, Copy, PartialEq, Debug)]
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
        Self::with_durations(
            task_name,
            cfg.pomodoro_work_minutes * 60,
            cfg.pomodoro_break_minutes * 60,
        )
    }

    fn with_durations(task_name: String, work_seconds: u64, break_seconds: u64) -> Self {
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
            break_seconds,
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

    /// Pause or resume. On resume, rebase `start` so the paused wall time
    /// does not count against the remaining focus time.
    fn toggle_pause(&mut self) {
        self.running = !self.running;
        if self.running {
            let elapsed = self.total.saturating_sub(self.remaining);
            self.start = Instant::now() - elapsed;
        }
    }

    /// Seconds of actual focus time accumulated in the current work phase.
    fn focused_seconds(&self) -> u64 {
        self.total.saturating_sub(self.remaining).as_secs()
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

pub async fn run(db: &str, task_name: Option<String>, user_id: i64) -> Result<()> {
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
                .style(
                    Style::default()
                        .fg(phase_color)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(title, chunks[0]);

            let task_line = Line::from(vec![
                Span::styled("Task: ", Style::default().fg(Color::Yellow)),
                Span::raw(&state.task_name),
            ]);
            f.render_widget(
                Paragraph::new(task_line).alignment(Alignment::Center),
                chunks[1],
            );

            let timer_text = PomodoroState::fmt_time(state.remaining);
            let timer = Paragraph::new(timer_text)
                .style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
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
                                state.toggle_pause();
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

    // Log the focus time to the database. Completed work phases log the
    // full configured length; quitting early logs actual focused seconds
    // (pauses excluded) if at least a minute accumulated.
    let focused = if state.phase == Phase::Work {
        state.focused_seconds()
    } else {
        state.work_seconds
    };
    if focused >= 60 {
        let store = Store::open(std::path::Path::new(db))?;
        let name = format!("Pomodoro: {}", task);
        let ended = Utc::now();
        let started = ended - chrono::Duration::seconds(focused as i64);
        store.insert_completed_entry(
            &name,
            Some("pomodoro"),
            None,
            started,
            ended,
            focused as i64,
            user_id,
        )?;
        println!("Logged Pomodoro session ({}s)", focused);
    } else {
        println!("Pomodoro too short to log ({}s focused).", focused);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_time_formats_mm_ss() {
        assert_eq!(PomodoroState::fmt_time(Duration::from_secs(0)), "00:00");
        assert_eq!(PomodoroState::fmt_time(Duration::from_secs(65)), "01:05");
        assert_eq!(PomodoroState::fmt_time(Duration::from_secs(1500)), "25:00");
    }

    #[test]
    fn zero_length_work_phase_completes_immediately() {
        let mut s = PomodoroState::with_durations("t".into(), 0, 300);
        s.tick();
        assert!(s.done);
        assert_eq!(s.remaining, Duration::ZERO);
        assert_eq!(s.phase, Phase::Work);
    }

    #[test]
    fn work_to_break_transition_resets_state() {
        let mut s = PomodoroState::with_durations("t".into(), 0, 0);
        s.tick();
        assert!(s.done);
        s.transition_to_break();
        assert_eq!(s.phase, Phase::Break);
        assert!(!s.done);
        assert!(s.running);
        s.tick();
        assert!(s.done, "zero-length break completes immediately");
    }

    #[test]
    fn pause_freezes_remaining_and_resume_rebases() {
        let mut s = PomodoroState::with_durations("t".into(), 60, 60);
        std::thread::sleep(Duration::from_millis(20));
        s.tick();
        let remaining_at_pause = s.remaining;
        s.toggle_pause();
        assert!(!s.running);
        std::thread::sleep(Duration::from_millis(20));
        s.tick(); // no-op while paused
        assert_eq!(
            s.remaining, remaining_at_pause,
            "paused time must not consume remaining"
        );

        s.toggle_pause();
        assert!(s.running);
        std::thread::sleep(Duration::from_millis(20));
        s.tick();
        // After resume, only ~20ms more should have been consumed.
        assert!(s.remaining <= remaining_at_pause);
        assert!(remaining_at_pause - s.remaining < Duration::from_secs(1));
    }

    #[test]
    fn done_state_is_sticky() {
        let mut s = PomodoroState::with_durations("t".into(), 0, 60);
        s.tick();
        let r = s.remaining;
        s.tick();
        assert_eq!(s.remaining, r);
        assert!(s.done);
    }

    #[test]
    fn focused_seconds_counts_down_from_total() {
        let mut s = PomodoroState::with_durations("t".into(), 120, 60);
        assert_eq!(s.focused_seconds(), 0);
        s.remaining = Duration::from_secs(30);
        assert_eq!(s.focused_seconds(), 90);
    }
}
