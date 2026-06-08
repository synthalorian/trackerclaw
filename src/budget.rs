use crate::store::Store;
use anyhow::Result;
use std::path::Path;

pub fn set_budget(db: &str, project: &str, hours: f64) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let seconds = (hours * 3600.0) as i64;
    store.set_budget(project, seconds)?;
    println!("Budget set for '{}': {:.1} hours", project, hours);
    Ok(())
}

pub fn list_budgets(db: &str) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let budgets = store.list_budgets()?;
    if budgets.is_empty() {
        println!("No budgets set. Use 'tracker budget set <project> <hours>' to create one.");
        return Ok(());
    }
    println!("{: <20} {: <12} {: <12} {: <10}", "PROJECT", "BUDGET", "USED", "REMAINING");
    for (project, budget_sec, used_sec) in budgets {
        let budget_h = budget_sec as f64 / 3600.0;
        let used_h = used_sec as f64 / 3600.0;
        let remaining = budget_h - used_h;
        println!("{:<20} {:<12.1} {:<12.1} {:<10.1}", project, budget_h, used_h, remaining);
    }
    Ok(())
}

pub fn delete_budget(db: &str, project: &str) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    store.delete_budget(project)?;
    println!("Budget removed for '{}'.", project);
    Ok(())
}

pub fn render_budget_bar(used_seconds: i64, budget_seconds: i64, width: usize) -> String {
    if budget_seconds <= 0 {
        return "[No budget]".to_string();
    }
    let ratio = (used_seconds as f64 / budget_seconds as f64).min(1.0);
    let filled = (ratio * width as f64) as usize;
    let empty = width - filled;
    let bar: String = std::iter::repeat('█').take(filled)
        .chain(std::iter::repeat('░').take(empty))
        .collect();
    format!("[{:.0}%] {}", ratio * 100.0, bar)
}
