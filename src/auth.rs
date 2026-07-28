use crate::store::Store;
use anyhow::{bail, Result};
use std::path::Path;

pub fn user_file_path() -> String {
    shellexpand::tilde("~/.config/trackerclaw/.user").to_string()
}

pub fn current_user_name() -> String {
    let path = user_file_path();
    std::fs::read_to_string(&path)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "default".to_string())
}

pub fn set_current_user(name: &str) -> Result<()> {
    let path = user_file_path();
    if let Some(parent) = Path::new(&path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, name)?;
    Ok(())
}

pub fn resolve_current_user(
    db: &str,
    override_user: Option<&str>,
) -> Result<(i64, String, String)> {
    let name = override_user
        .map(|s| s.to_string())
        .unwrap_or_else(current_user_name);
    let store = Store::open(Path::new(db))?;
    match store.get_user(&name)? {
        Some((id, _, role)) => Ok((id, name, role)),
        None => bail!(
            "User '{}' not found. Add them with 'trackerclaw team add {}'",
            name,
            name
        ),
    }
}

pub fn ensure_default_user(db: &str) -> Result<()> {
    if let Some(parent) = Path::new(db).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let store = Store::open(Path::new(db))?;
    if store.list_users()?.is_empty() {
        store.add_user("default", "admin")?;
    }
    Ok(())
}

pub fn require_admin(role: &str) -> Result<()> {
    if role != "admin" {
        bail!("Admin role required.");
    }
    Ok(())
}
