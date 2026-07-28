use crate::notifications;
use crate::store::Store;
use anyhow::Result;
use std::path::Path;

pub fn check_budget_warnings(db: &str) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let budgets = store.list_budgets()?;
    for (project, budget_sec, used_sec) in budgets {
        if budget_sec <= 0 {
            continue;
        }
        let budget_h = budget_sec as f64 / 3600.0;
        let used_h = used_sec as f64 / 3600.0;
        let ratio = used_h / budget_h;
        if ratio >= 0.8 {
            notifications::notify_budget_warning(&project, used_h, budget_h);
        }
    }
    Ok(())
}

pub fn set_budget(db: &str, project: &str, hours: f64) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    if store.get_project_by_name(project)?.is_none() {
        anyhow::bail!(
            "Project '{}' not found. Create it first with 'trackerclaw project add {}'",
            project,
            project
        );
    }
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
    println!(
        "{: <20} {: <12} {: <12} {: <10}",
        "PROJECT", "BUDGET", "USED", "REMAINING"
    );
    for (project, budget_sec, used_sec) in budgets {
        let budget_h = budget_sec as f64 / 3600.0;
        let used_h = used_sec as f64 / 3600.0;
        let remaining = budget_h - used_h;
        println!(
            "{:<20} {:<12.1} {:<12.1} {:<10.1}",
            project, budget_h, used_h, remaining
        );
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
    let bar: String = std::iter::repeat_n('█', filled)
        .chain(std::iter::repeat_n('░', empty))
        .collect();
    format!("[{:.0}%] {}", ratio * 100.0, bar)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_bar_zero_budget() {
        assert_eq!(render_budget_bar(100, 0, 20), "[No budget]");
    }

    #[test]
    fn budget_bar_partial_fill() {
        let bar = render_budget_bar(1800, 3600, 10);
        assert!(bar.contains("50%"));
        assert_eq!(bar.chars().filter(|c| *c == '█').count(), 5);
        assert_eq!(bar.chars().filter(|c| *c == '░').count(), 5);
    }

    #[test]
    fn budget_bar_clamps_over_100_percent() {
        let bar = render_budget_bar(7200, 3600, 10);
        assert!(bar.contains("100%"));
        assert_eq!(bar.chars().filter(|c| *c == '█').count(), 10);
        assert_eq!(bar.chars().filter(|c| *c == '░').count(), 0);
    }
}
