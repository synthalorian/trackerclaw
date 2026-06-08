use crate::store::Store;
use anyhow::Result;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tokio::time::interval;

const CHECK_INTERVAL_SECS: u64 = 30;
pub const IDLE_THRESHOLD_MS: u64 = 5 * 60 * 1000; // 5 minutes

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

    // Strategy 3: Check /dev/input event files for last access time
    if let Some(ms) = try_input_devices() {
        return Some(ms);
    }

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

fn try_input_devices() -> Option<u64> {
    // Check the most recently modified /dev/input/event* file
    let mut newest = None;
    if let Ok(entries) = std::fs::read_dir("/dev/input") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("event") {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(modified) = meta.modified() {
                        let age = modified.elapsed().unwrap_or(Duration::MAX);
                        if let Some((_, best_age)) = newest {
                            if age < best_age {
                                newest = Some((name_str.to_string(), age));
                            }
                        } else {
                            newest = Some((name_str.to_string(), age));
                        }
                    }
                }
            }
        }
    }

    newest.map(|(_, age)| age.as_millis() as u64)
}

/// Run idle detection loop. Auto-pauses tracking when user is idle.
pub async fn run_idle_monitor(db_path: &str) -> Result<()> {
    println!("Idle monitor started. Threshold: {} min. Press Ctrl+C to stop.", IDLE_THRESHOLD_MS / 60000);

    let mut check_interval = interval(Duration::from_secs(CHECK_INTERVAL_SECS));
    let mut was_idle = false;
    let mut idle_start: Option<Instant> = None;

    loop {
        check_interval.tick().await;

        let idle_ms = get_idle_time_ms();
        match idle_ms {
            Some(ms) => {
                let is_idle = ms >= IDLE_THRESHOLD_MS;

                if is_idle && !was_idle {
                    // Just became idle - stop tracking
                    println!("User idle for {}s. Auto-pausing tracking...", ms / 1000);
                    if let Ok(store) = Store::open(Path::new(db_path)) {
                        match store.stop_current() {
                            Ok(Some(entry)) => {
                                let dur = entry.duration_seconds.unwrap_or(0);
                                println!("Auto-paused: {} — {}s", entry.name, dur);
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
            if ms >= IDLE_THRESHOLD_MS {
                format!("Idle: {}m {}s (threshold: {}m)", mins, secs, IDLE_THRESHOLD_MS / 60000)
            } else {
                format!("Active: {}m {}s since last input", mins, secs)
            }
        }
        None => "Idle detection unavailable. Install xprintidle or ensure X11 is running.".to_string(),
    }
}
