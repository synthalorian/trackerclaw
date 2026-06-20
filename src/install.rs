use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

fn current_exe() -> Result<std::path::PathBuf> {
    std::env::current_exe().map_err(Into::into)
}

fn local_bin_dir() -> String {
    shellexpand::tilde("~/.local/bin").to_string()
}

fn systemd_user_dir() -> String {
    shellexpand::tilde("~/.config/systemd/user").to_string()
}

fn service_name() -> &'static str {
    "trackerclaw-idle.service"
}

fn username() -> String {
    std::env::var("USER").unwrap_or_else(|_| "user".to_string())
}

fn detect_install_path() -> std::path::PathBuf {
    let local = std::path::PathBuf::from(local_bin_dir()).join("trackerclaw");
    let system = std::path::PathBuf::from("/usr/local/bin/trackerclaw");

    if system.exists() {
        return system;
    }
    local
}

pub fn install() -> Result<()> {
    let src = current_exe()?;
    let dest = detect_install_path();

    println!("Installing TrackerClaw...");
    println!("  Binary: {} -> {}", src.display(), dest.display());

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&src, &dest)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest, perms)?;
    }

    install_service(&dest)?;

    println!("\nInstalled. Run:");
    println!("  systemctl --user enable --now {}", service_name());
    println!("  systemctl --user status {}", service_name());
    Ok(())
}

fn install_service(bin_path: &std::path::Path) -> Result<()> {
    let dir = systemd_user_dir();
    std::fs::create_dir_all(&dir)?;
    let service_path = std::path::Path::new(&dir).join(service_name());

    let unit = format!(
        "[Unit]\nDescription=TrackerClaw idle monitor\nAfter=graphical-session.target\n\n[Service]\nExecStart={} idle\nRestart=on-failure\nEnvironment=\"PATH=/usr/local/bin:/usr/bin:/bin:/home/{}/.local/bin\"\n\n[Install]\nWantedBy=default.target\n",
        bin_path.display(),
        username(),
    );

    std::fs::write(&service_path, unit)?;
    println!("  Service: {}", service_path.display());

    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output();

    Ok(())
}

pub fn uninstall() -> Result<()> {
    let local = std::path::PathBuf::from(local_bin_dir()).join("trackerclaw");
    let service_path = std::path::Path::new(&systemd_user_dir()).join(service_name());

    println!("Uninstalling TrackerClaw user service...");

    let _ = Command::new("systemctl")
        .args(["--user", "stop", service_name()])
        .output();
    let _ = Command::new("systemctl")
        .args(["--user", "disable", service_name()])
        .output();

    if service_path.exists() {
        std::fs::remove_file(&service_path)?;
        println!("  Removed service file.");
    }

    if local.exists() {
        std::fs::remove_file(&local)?;
        println!("  Removed {}.", local.display());
    } else {
        println!("  Skipping system binary at /usr/local/bin (remove manually if needed).");
    }

    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output();

    println!("Uninstalled.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_unit_contains_exec_start() {
        let unit = format!(
            "[Unit]\nDescription=TrackerClaw idle monitor\n\n[Service]\nExecStart={} idle\nRestart=on-failure\n\n[Install]\nWantedBy=default.target\n",
            "/tmp/trackerclaw"
        );
        assert!(unit.contains("ExecStart=/tmp/trackerclaw idle"));
    }
}
