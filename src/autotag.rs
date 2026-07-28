use anyhow::Result;
use regex::Regex;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct TagRule {
    pub pattern: String,
    pub tags: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct TagConfig {
    #[serde(default)]
    pub rules: Vec<TagRule>,
}

/// Load tag rules from config file
pub fn load_config() -> TagConfig {
    let config_path = shellexpand::tilde("~/.config/trackerclaw/autotag.toml").to_string();
    let path = Path::new(&config_path);

    if !path.exists() {
        // Return default config with some useful examples
        return default_config();
    }

    match std::fs::read_to_string(path) {
        Ok(content) => match toml::from_str::<TagConfig>(&content) {
            Ok(config) => config,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to parse autotag config: {}. Using defaults.",
                    e
                );
                default_config()
            }
        },
        Err(e) => {
            eprintln!(
                "Warning: Failed to read autotag config: {}. Using defaults.",
                e
            );
            default_config()
        }
    }
}

fn default_config() -> TagConfig {
    TagConfig {
        rules: vec![
            TagRule {
                pattern: r"(?i)firefox|chrome|chromium|brave|safari".to_string(),
                tags: "web,browsing".to_string(),
            },
            TagRule {
                pattern: r"(?i)code\.exe|visual studio code|vscodium|cursor".to_string(),
                tags: "coding,dev".to_string(),
            },
            TagRule {
                pattern: r"(?i)terminal|alacritty|kitty|ghostty|wezterm|tmux".to_string(),
                tags: "terminal,cli".to_string(),
            },
            TagRule {
                pattern: r"(?i)discord|slack|teams|zoom|meet".to_string(),
                tags: "communication".to_string(),
            },
            TagRule {
                pattern: r"(?i)gimp|photoshop|figma|inkscape|blender".to_string(),
                tags: "design".to_string(),
            },
            TagRule {
                pattern: r"(?i)rust|cargo\.toml|\.rs".to_string(),
                tags: "rust".to_string(),
            },
            TagRule {
                pattern: r"(?i)go\.mod|\.go".to_string(),
                tags: "golang".to_string(),
            },
        ],
    }
}

/// Get the active window title using available methods
pub fn get_active_window_title() -> Option<String> {
    // Try Hyprland first (synth uses Hyprland)
    if let Some(title) = try_hyprland() {
        return Some(title);
    }

    // Fallback to X11
    if let Some(title) = try_x11() {
        return Some(title);
    }

    None
}

fn try_hyprland() -> Option<String> {
    let output = std::process::Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    json.get("title")?.as_str().map(|s| s.to_string())
}

fn try_x11() -> Option<String> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};
    use x11rb::rust_connection::RustConnection;

    let (conn, screen_num) = RustConnection::connect(None).ok()?;
    let screen = &conn.setup().roots[screen_num];

    // Get _NET_ACTIVE_WINDOW property
    let active_window_prop = conn
        .intern_atom(false, b"_NET_ACTIVE_WINDOW")
        .ok()?
        .reply()
        .ok()?
        .atom;
    let reply = conn
        .get_property(
            false,
            screen.root,
            active_window_prop,
            AtomEnum::WINDOW,
            0,
            1,
        )
        .ok()?
        .reply()
        .ok()?;

    if reply.value_len == 0 {
        return None;
    }

    let window = reply.value32()?.next()?;

    // Get WM_NAME property
    let wm_name = conn.intern_atom(false, b"WM_NAME").ok()?.reply().ok()?.atom;
    let name_reply = conn
        .get_property(false, window, wm_name, AtomEnum::STRING, 0, 1024)
        .ok()?
        .reply()
        .ok()?;

    if name_reply.value_len > 0 {
        String::from_utf8(name_reply.value).ok()
    } else {
        None
    }
}

/// Match text against all rules and return combined matching tags
pub fn match_tags(text: &str, config: &TagConfig) -> Option<String> {
    let mut all_tags = Vec::new();
    for rule in &config.rules {
        if let Ok(re) = Regex::new(&rule.pattern) {
            if re.is_match(text) {
                for tag in rule.tags.split(',') {
                    let t = tag.trim();
                    if !t.is_empty() && !all_tags.contains(&t) {
                        all_tags.push(t);
                    }
                }
            }
        }
    }
    if all_tags.is_empty() {
        None
    } else {
        Some(all_tags.join(", "))
    }
}

/// Auto-detect tags from current window title
pub fn auto_detect_tags() -> Option<String> {
    let title = get_active_window_title()?;
    let config = load_config();
    match_tags(&title, &config)
}

/// Infer tags from a task name using keyword rules
pub fn infer_tags_from_task_name(name: &str) -> Option<String> {
    let config = load_config();
    match_tags(name, &config)
}

/// Create default config file if it doesn't exist
pub fn ensure_config_exists() -> Result<()> {
    let config_dir = shellexpand::tilde("~/.config/trackerclaw").to_string();
    std::fs::create_dir_all(&config_dir)?;

    let config_path = format!("{}/autotag.toml", config_dir);
    if !Path::new(&config_path).exists() {
        let default_toml = r#"# Auto-tagging rules for TrackerClaw
# Match window titles against regex patterns to automatically assign tags
# Each rule: pattern = regex, tags = comma-separated tags

[[rules]]
pattern = "(?i)firefox|chrome|chromium|brave|safari"
tags = "web,browsing"

[[rules]]
pattern = "(?i)code\\.exe|visual studio code|vscodium|cursor"
tags = "coding,dev"

[[rules]]
pattern = "(?i)terminal|alacritty|kitty|ghostty|wezterm|tmux"
tags = "terminal,cli"

[[rules]]
pattern = "(?i)discord|slack|teams|zoom|meet"
tags = "communication"

[[rules]]
pattern = "(?i)gimp|photoshop|figma|inkscape|blender"
tags = "design"

[[rules]]
pattern = "(?i)rust|cargo\\.toml|\\.rs"
tags = "rust"

[[rules]]
pattern = "(?i)go\\.mod|\\.go"
tags = "golang"
"#;
        std::fs::write(&config_path, default_toml)?;
        println!("Created default auto-tag config at {}", config_path);
    }
    Ok(())
}
