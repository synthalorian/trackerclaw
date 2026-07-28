use crate::budget;
use crate::store::Store;
use crate::webhook;
use anyhow::Result;
use std::path::Path;

pub async fn start(
    db: &str,
    name: String,
    tags: Option<String>,
    project_id: Option<i64>,
    user_id: i64,
) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    for closed in store.close_open_entries(user_id)? {
        println!("Auto-stopped: {}", closed);
    }
    let id = store.start_entry(&name, tags.as_deref(), project_id, user_id)?;
    println!("Started tracking: {} (id: {})", name, id);
    Ok(())
}

pub async fn stop(db: &str, _user_id: i64) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    match store.stop_current(_user_id)? {
        Some(entry) => {
            let dur = entry.duration_seconds.unwrap_or(0);
            println!("Stopped: {} — {}s", entry.name, dur);
            let _ = webhook::send_webhook(db, &entry).await;
            let _ = budget::check_budget_warnings(db);
        }
        None => println!("Nothing is being tracked."),
    }
    Ok(())
}

pub async fn status(db: &str, user_id: i64) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    match store.get_current(user_id)? {
        Some(entry) => println!(
            "Currently tracking: {} (since {})",
            entry.name, entry.started_at
        ),
        None => println!("Not tracking anything."),
    }
    Ok(())
}

pub async fn today(db: &str, user_id: i64, is_admin: bool) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let entries = store.list_today(user_id, is_admin)?;
    if entries.is_empty() {
        println!("No entries today.");
        return Ok(());
    }
    println!("{:<30} {:<10} TAGS", "NAME", "DURATION");
    for e in entries {
        let dur = format!("{}s", e.duration_seconds.unwrap_or(0));
        println!(
            "{:<30} {: <10} {}",
            e.name,
            dur,
            e.tags.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

pub async fn resume(db: &str, user_id: i64) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let entries = store.list_today(user_id, false)?;
    if let Some(last) = entries.first() {
        if store.get_current(user_id)?.is_none() {
            let id =
                store.start_entry(&last.name, last.tags.as_deref(), last.project_id, user_id)?;
            println!("Resumed: {} (id: {})", last.name, id);
        } else {
            println!("Already tracking something. Stop first.");
        }
    } else {
        println!("No entries to resume today.");
    }
    Ok(())
}

pub async fn show_entry(db: &str, id: i64, user_id: i64, is_admin: bool) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    match store.get_entry_by_id(id)? {
        Some(e) => {
            if !is_admin && e.user_id != Some(user_id) {
                anyhow::bail!("Entry {} does not belong to you.", id);
            }
            println!("Entry {}:", e.id);
            println!("  Name:     {}", e.name);
            println!("  Tags:     {}", e.tags.as_deref().unwrap_or("-"));
            println!("  Started:  {}", e.started_at);
            println!(
                "  Ended:    {}",
                e.ended_at.as_deref().unwrap_or("(running)")
            );
            println!(
                "  Duration: {}s",
                e.duration_seconds
                    .map_or_else(|| "-".to_string(), |d| d.to_string())
            );
        }
        None => println!("Entry {} not found.", id),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn edit_entry(
    db: &str,
    id: i64,
    name: Option<&str>,
    tags: Option<&str>,
    started_at: Option<&str>,
    ended_at: Option<&str>,
    user_id: i64,
    is_admin: bool,
) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    if let Some(e) = store.get_entry_by_id(id)? {
        if !is_admin && e.user_id != Some(user_id) {
            anyhow::bail!("Entry {} does not belong to you.", id);
        }
    }
    store.update_entry(id, name, tags, started_at, ended_at)?;
    println!("Updated entry {}.", id);
    Ok(())
}

pub async fn delete_entry(db: &str, id: i64, user_id: i64, is_admin: bool) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    if let Some(e) = store.get_entry_by_id(id)? {
        if !is_admin && e.user_id != Some(user_id) {
            anyhow::bail!("Entry {} does not belong to you.", id);
        }
    }
    store.delete_entry(id)?;
    println!("Deleted entry {}.", id);
    Ok(())
}

pub async fn calendar_week(db: &str, user_id: i64, is_admin: bool) -> Result<()> {
    let days = crate::calendar::week_view(db)?;
    for (date, entries) in days {
        let visible: Vec<_> = entries
            .into_iter()
            .filter(|e| is_admin || e.user_id == Some(user_id))
            .collect();
        let total: i64 = visible
            .iter()
            .map(|e| e.duration_seconds.unwrap_or(0))
            .sum();
        if visible.is_empty() && !is_admin {
            continue;
        }
        println!(
            "{} — total {}",
            date,
            crate::calendar::format_duration_short(total)
        );
        for e in visible {
            let dur = crate::calendar::format_duration_short(e.duration_seconds.unwrap_or(0));
            println!(
                "  • {} ({}) [{}]",
                e.name,
                dur,
                e.tags.as_deref().unwrap_or("-")
            );
        }
    }
    Ok(())
}

pub async fn export(
    db: &str,
    format: &str,
    output: &std::path::Path,
    user_id: i64,
    is_admin: bool,
) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let entries = store.list_recent(365, user_id, is_admin)?;

    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&entries)?;
            std::fs::write(output, json)?;
        }
        "csv" => {
            let mut wtr = csv::Writer::from_path(output)?;
            wtr.write_record(["name", "started_at", "ended_at", "duration_seconds", "tags"])?;
            for e in entries {
                wtr.write_record([
                    &e.name,
                    &e.started_at,
                    &e.ended_at.unwrap_or_default(),
                    &e.duration_seconds
                        .map_or_else(|| "0".to_string(), |d| d.to_string()),
                    &e.tags.unwrap_or_default(),
                ])?;
            }
            wtr.flush()?;
        }
        "ical" => {
            let mut ical = String::new();
            ical.push_str("BEGIN:VCALENDAR\r\n");
            ical.push_str("VERSION:2.0\r\n");
            ical.push_str("PRODID:-//TrackerClaw//EN\r\n");
            for e in entries {
                let start = ical_datetime(&e.started_at);
                let end = e
                    .ended_at
                    .as_deref()
                    .map(ical_datetime)
                    .unwrap_or_else(|| start.clone());
                let dur = e.duration_seconds.unwrap_or(0);
                let desc = ical_escape(&format!("{} ({}s)", e.name, dur));
                ical.push_str("BEGIN:VEVENT\r\n");
                ical.push_str(&format!("UID:trackerclaw-{}@localhost\r\n", e.id));
                ical.push_str(&format!("SUMMARY:{}\r\n", ical_escape(&e.name)));
                ical.push_str(&format!("DTSTART:{}\r\n", start));
                ical.push_str(&format!("DTEND:{}\r\n", end));
                ical.push_str(&format!("DESCRIPTION:{}\r\n", desc));
                if let Some(ref t) = e.tags {
                    ical.push_str(&format!("CATEGORIES:{}\r\n", ical_escape(t)));
                }
                ical.push_str("END:VEVENT\r\n");
            }
            ical.push_str("END:VCALENDAR\r\n");
            std::fs::write(output, ical)?;
        }
        _ => anyhow::bail!("Unknown format: {}. Use json, csv, or ical.", format),
    }
    println!("Exported to {}", output.display());
    Ok(())
}

/// Format an RFC3339 timestamp as an iCal UTC datetime (YYYYMMDDTHHMMSSZ).
fn ical_datetime(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|d| {
            d.with_timezone(&chrono::Utc)
                .format("%Y%m%dT%H%M%SZ")
                .to_string()
        })
        .unwrap_or_else(|_| rfc3339.to_string())
}

/// Escape text values per RFC 5545.
fn ical_escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_path(ext: &str) -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "trackerclaw_cli_test_{}_{}.{}",
            std::process::id(),
            n,
            ext
        ))
    }

    async fn seeded_db() -> std::path::PathBuf {
        let db = temp_path("db");
        let store = Store::open(&db).unwrap();
        let now = Utc::now();
        store
            .insert_completed_entry(
                "task one, with comma",
                Some("rust"),
                None,
                now - Duration::hours(2),
                now - Duration::hours(1),
                3600,
                1,
            )
            .unwrap();
        store
            .insert_completed_entry(
                "task two",
                Some("docs"),
                None,
                now - Duration::hours(1),
                now,
                1800,
                1,
            )
            .unwrap();
        db
    }

    #[test]
    fn ical_datetime_format() {
        assert_eq!(
            ical_datetime("2026-06-19T09:30:00+00:00"),
            "20260619T093000Z"
        );
        assert_eq!(
            ical_datetime("2026-06-19T09:30:00+02:00"),
            "20260619T073000Z"
        );
    }

    #[test]
    fn ical_escape_specials() {
        assert_eq!(ical_escape("a,b;c\nd"), "a\\,b\\;c\\nd");
    }

    #[tokio::test]
    async fn export_ical_produces_valid_datetimes() {
        let db = seeded_db().await;
        let out = temp_path("ics");
        export(db.to_str().unwrap(), "ical", &out, 1, true)
            .await
            .unwrap();
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("BEGIN:VCALENDAR"));
        assert!(content.contains("DTSTART:2"));
        assert!(content.contains("Z\r\n"));
        assert!(
            !content.contains("+00:00"),
            "raw RFC3339 offset leaked into iCal"
        );
        assert!(content.contains("SUMMARY:task one\\, with comma"));
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(&out);
    }

    #[tokio::test]
    async fn export_csv_and_json() {
        let db = seeded_db().await;
        let csv_out = temp_path("csv");
        export(db.to_str().unwrap(), "csv", &csv_out, 1, true)
            .await
            .unwrap();
        let csv = std::fs::read_to_string(&csv_out).unwrap();
        assert!(csv.contains("task two"));
        assert!(csv.contains("duration_seconds"));

        let json_out = temp_path("json");
        export(db.to_str().unwrap(), "json", &json_out, 1, true)
            .await
            .unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json_out).unwrap()).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 2);

        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(&csv_out);
        let _ = std::fs::remove_file(&json_out);
    }

    #[tokio::test]
    async fn export_member_scope() {
        let db = temp_path("db");
        let store = Store::open(&db).unwrap();
        let now = Utc::now();
        store
            .insert_completed_entry("mine", None, None, now - Duration::hours(1), now, 600, 2)
            .unwrap();
        store
            .insert_completed_entry("theirs", None, None, now - Duration::hours(1), now, 600, 1)
            .unwrap();
        drop(store);
        let out = temp_path("json");
        export(db.to_str().unwrap(), "json", &out, 2, false)
            .await
            .unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "mine");
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(&out);
    }
}
