use anyhow::Result;
use serde::Deserialize;
use std::path::Path;

pub const DEFAULT_IDLE_THRESHOLD_MS: u64 = 5 * 60 * 1000;
pub const DEFAULT_POMODORO_WORK_MINUTES: u64 = 25;
pub const DEFAULT_POMODORO_BREAK_MINUTES: u64 = 5;
pub const DEFAULT_RATE: f64 = 150.0;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(default = "default_idle_threshold_ms")]
    pub idle_threshold_ms: u64,
    #[serde(default = "default_pomodoro_work_minutes")]
    pub pomodoro_work_minutes: u64,
    #[serde(default = "default_pomodoro_break_minutes")]
    pub pomodoro_break_minutes: u64,
    #[serde(default = "default_rate")]
    pub default_rate: f64,
    /// Reserved for future UI theming.
    #[allow(dead_code)]
    #[serde(default = "default_theme")]
    pub theme: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            idle_threshold_ms: default_idle_threshold_ms(),
            pomodoro_work_minutes: default_pomodoro_work_minutes(),
            pomodoro_break_minutes: default_pomodoro_break_minutes(),
            default_rate: default_rate(),
            theme: default_theme(),
        }
    }
}

fn default_idle_threshold_ms() -> u64 {
    DEFAULT_IDLE_THRESHOLD_MS
}
fn default_pomodoro_work_minutes() -> u64 {
    DEFAULT_POMODORO_WORK_MINUTES
}
fn default_pomodoro_break_minutes() -> u64 {
    DEFAULT_POMODORO_BREAK_MINUTES
}
fn default_rate() -> f64 {
    DEFAULT_RATE
}
fn default_theme() -> String {
    "synthwave".to_string()
}

pub fn config_path() -> String {
    shellexpand::tilde("~/.config/trackerclaw/config.toml").to_string()
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    let path = Path::new(&path);

    if !path.exists() {
        return AppConfig::default();
    }

    match std::fs::read_to_string(path) {
        Ok(content) => match toml::from_str::<AppConfig>(&content) {
            Ok(config) => config,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to parse config.toml: {}. Using defaults.",
                    e
                );
                AppConfig::default()
            }
        },
        Err(e) => {
            eprintln!(
                "Warning: Failed to read config.toml: {}. Using defaults.",
                e
            );
            AppConfig::default()
        }
    }
}

pub fn ensure_config_exists() -> Result<()> {
    let config_dir = shellexpand::tilde("~/.config/trackerclaw").to_string();
    std::fs::create_dir_all(&config_dir)?;

    let config_path = format!("{}/config.toml", config_dir);
    if !Path::new(&config_path).exists() {
        let default_toml = r#"# TrackerClaw configuration
# All values are optional; missing keys use the defaults shown below.

# Idle auto-pause threshold in milliseconds
idle_threshold_ms = 300000

# Pomodoro timer lengths
pomodoro_work_minutes = 25
pomodoro_break_minutes = 5

# Default hourly rate for invoice generation
default_rate = 150.0

# Theme placeholder for future UI theming
theme = "synthwave"
"#;
        std::fs::write(&config_path, default_toml)?;
        println!("Created default config at {}", config_path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.idle_threshold_ms, 300000);
        assert_eq!(cfg.pomodoro_work_minutes, 25);
        assert_eq!(cfg.pomodoro_break_minutes, 5);
        assert_eq!(cfg.default_rate, 150.0);
        assert_eq!(cfg.theme, "synthwave");
    }
}
