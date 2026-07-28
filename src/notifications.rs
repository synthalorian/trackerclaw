use anyhow::Result;
use notify_rust::Notification;

pub fn notify(title: &str, message: &str) -> Result<()> {
    Notification::new().summary(title).body(message).show()?;
    Ok(())
}

pub fn notify_milestone(name: &str, elapsed_minutes: i64) {
    let title = format!("TrackerClaw: {} Milestone", name);
    let body = format!(
        "You've been working on '{}' for {} minutes.",
        name, elapsed_minutes
    );
    let _ = notify(&title, &body);
}

pub fn notify_idle_resume(name: &str) {
    let title = "TrackerClaw: Idle Detected";
    let body = format!(
        "You were idle. Task '{}' has been paused. Use 'trackerclaw resume' to continue.",
        name
    );
    let _ = notify(title, &body);
}

pub fn notify_budget_warning(project: &str, used_hours: f64, budget_hours: f64) {
    let percentage = (used_hours / budget_hours) * 100.0;
    let title = format!("TrackerClaw: Budget Alert — {}", project);
    let body = format!(
        "{:.1}% of budget used ({:.1}/{:.1} hours).",
        percentage, used_hours, budget_hours
    );
    let _ = notify(&title, &body);
}
