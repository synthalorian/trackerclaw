use crate::store::Store;
use anyhow::Result;
use std::path::Path;

pub async fn generate(db: &str, days: Option<i64>, project: Option<String>, user_id: i64, is_admin: bool) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let days = days.unwrap_or(7);

    let entries = if let Some(ref name) = project {
        if let Some(project) = store.get_project_by_name(name)? {
            store.list_by_project(project.id, days, user_id, is_admin)?
        } else {
            // Fallback to tag-based filtering for backwards compatibility
            store.list_by_tag(name, days, user_id, is_admin)?
        }
    } else {
        store.list_recent(days, user_id, is_admin)?
    };

    if entries.is_empty() {
        println!("No entries found.");
        return Ok(());
    }

    let total: i64 = entries.iter().filter_map(|e| e.duration_seconds).sum();
    match project {
        Some(ref tag) => println!("Report for project '{}' (last {} days)", tag, days),
        None => println!("Report (last {} days)", days),
    }
    println!("Total tracked time: {}s (~{:.1}h)", total, total as f64 / 3600.0);
    println!("\n{:<30} {:<10} {}", "NAME", "DURATION", "TAGS");
    for e in entries {
        let dur = e.duration_seconds.unwrap_or(0);
        println!("{:<30} {: <10}s {}", e.name, dur, e.tags.as_deref().unwrap_or("-"));
    }
    Ok(())
}
