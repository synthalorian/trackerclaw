use crate::store::Store;
use anyhow::Result;
use std::path::Path;

pub fn add_user(db: &str, name: &str, role: &str) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let id = store.add_user(name, role)?;
    println!("Added user '{}' (id: {}, role: {})", name, id, role);
    Ok(())
}

pub fn list_users(db: &str) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let users = store.list_users()?;
    if users.is_empty() {
        println!("No users found.");
        return Ok(());
    }
    println!("{: <5} {: <20} {: <10}", "ID", "NAME", "ROLE");
    for (id, name, role) in users {
        println!("{:<5} {:<20} {:<10}", id, name, role);
    }
    Ok(())
}

pub fn user_report(db: &str, user_name: &str, days: i64) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let user = match store.get_user(user_name)? {
        Some(u) => u,
        None => {
            println!("User '{}' not found.", user_name);
            return Ok(());
        }
    };
    let entries = store.list_recent(days)?;
    let total: i64 = entries.iter().map(|e| e.duration_seconds.unwrap_or(0)).sum();
    let hours = total as f64 / 3600.0;
    println!("Report for {} (role: {})", user.1, user.2);
    println!("Entries in last {} days: {}", days, entries.len());
    println!("Total time: {:.2} hours", hours);
    Ok(())
}
