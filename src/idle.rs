use crate::auth;
use crate::budget;
use crate::config;
use crate::notifications;
use crate::store::Store;
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tokio::time::interval;

const CHECK_INTERVAL_SECS: u64 = 30;

pub fn idle_threshold_ms() -> u64 {
    config::load_config().idle_threshold_ms
}

/// Try to get idle time in milliseconds using multiple strategies
pub fn get_idle_time_ms() -> Option<u64> {
    // Strategy 1: xprintidle command (works on X11 and many Wayland compositors with XWayland)
    if let Some(ms) = try_xprintidle() {
        return Some(ms);
    }

    // Strategy 2: X11 screensaver extension via x11rb
    if let Some(ms) = try_x11_screensaver() {
        return Some(ms);
    }

    // Note: there is intentionally no /dev/input fallback — device file
    // mtimes do not track input events, so it produced garbage idle times
    // and false auto-pauses.
    None
}

fn try_xprintidle() -> Option<u64> {
    let output = Command::new("xprintidle")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        text.trim().parse::<u64>().ok()
    } else {
        None
    }
}

fn try_x11_screensaver() -> Option<u64> {
    use x11rb::connection::Connection;
    use x11rb::protocol::screensaver::ConnectionExt;
    use x11rb::rust_connection::RustConnection;

    let (conn, screen_num) = RustConnection::connect(None).ok()?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;

    // Query the screensaver info to get idle time
    let reply = conn.screensaver_query_info(root).ok()?.reply().ok()?;
    Some(reply.ms_since_user_input as u64)
}

/// Run idle detection loop. Auto-pauses tracking when user is idle.
pub async fn run_idle_monitor(db_path: &str) -> Result<()> {
    let (user_id, _, _) = auth::resolve_current_user(db_path, None)?;
    let threshold = idle_threshold_ms();
    println!(
        "Idle monitor started. Threshold: {} min. Press Ctrl+C to stop.",
        threshold / 60000
    );

    let mut check_interval = interval(Duration::from_secs(CHECK_INTERVAL_SECS));
    let mut was_idle = false;
    let mut idle_start: Option<Instant> = None;
    let mut notified_milestones: HashSet<(i64, i64)> = HashSet::new();

    loop {
        check_interval.tick().await;

        let idle_ms = get_idle_time_ms();
        match idle_ms {
            Some(ms) => {
                let is_idle = ms >= threshold;

                if is_idle && !was_idle {
                    // Just became idle - stop tracking
                    println!("User idle for {}s. Auto-pausing tracking...", ms / 1000);
                    if let Ok(store) = Store::open(Path::new(db_path)) {
                        match store.stop_current(user_id) {
                            Ok(Some(entry)) => {
                                let dur = entry.duration_seconds.unwrap_or(0);
                                println!("Auto-paused: {} — {}s", entry.name, dur);
                                notifications::notify_idle_resume(&entry.name);
                                let _ = budget::check_budget_warnings(db_path);
                                notified_milestones.clear();
                            }
                            Ok(None) => {}
                            Err(e) => eprintln!("Error stopping entry: {}", e),
                        }
                    }
                    idle_start = Some(Instant::now());
                    was_idle = true;
                } else if !is_idle && was_idle {
                    // User is back
                    if let Some(start) = idle_start {
                        let idle_duration = start.elapsed().as_secs() / 60;
                        println!("User back after {} min idle.", idle_duration);
                    }
                    was_idle = false;
                    idle_start = None;
                }

                // Milestone notifications while actively tracking
                if !is_idle {
                    if let Ok(store) = Store::open(Path::new(db_path)) {
                        if let Ok(Some(entry)) = store.get_current(user_id) {
                            if let Ok(started) = DateTime::parse_from_rfc3339(&entry.started_at) {
                                let elapsed_minutes =
                                    (Utc::now() - started.with_timezone(&Utc)).num_minutes();
                                if elapsed_minutes > 0
                                    && elapsed_minutes % 60 == 0
                                    && notified_milestones.insert((entry.id, elapsed_minutes))
                                {
                                    notifications::notify_milestone(&entry.name, elapsed_minutes);
                                }
                            }
                        } else {
                            notified_milestones.clear();
                        }
                    }
                }

                // Verbose status every check
                if is_idle {
                    println!("[idle] {}s since last input", ms / 1000);
                }
            }
            None => {
                eprintln!("Could not determine idle time. Is X11/XWayland running? Try installing xprintidle.");
            }
        }
    }
}

/// Check idle status once and return formatted string
pub fn check_idle_status() -> String {
    match get_idle_time_ms() {
        Some(ms) => {
            let mins = ms / 60000;
            let secs = (ms % 60000) / 1000;
            let threshold = idle_threshold_ms();
            if ms >= threshold {
                format!(
                    "Idle: {}m {}s (threshold: {}m)",
                    mins,
                    secs,
                    threshold / 60000
                )
            } else {
                format!("Active: {}m {}s since last input", mins, secs)
            }
        }
        None => {
            "Idle detection unavailable. Install xprintidle or ensure X11 is running.".to_string()
        }
    }
}
