use crate::store::Store;
use crate::webhook;
use anyhow::Result;
use std::path::Path;

pub async fn start(db: &str, name: String, tags: Option<String>) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let id = store.start_entry(&name, tags.as_deref())?;
    println!("Started tracking: {} (id: {})", name, id);
    Ok(())
}

pub async fn stop(db: &str) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    match store.stop_current()? {
        Some(entry) => {
            let dur = entry.duration_seconds.unwrap_or(0);
            println!("Stopped: {} — {}s", entry.name, dur);
            let _ = webhook::send_webhook(db, &entry).await;
        }
        None => println!("Nothing is being tracked."),
    }
    Ok(())
}

pub async fn status(db: &str) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    match store.get_current()? {
        Some(entry) => println!("Currently tracking: {} (since {})", entry.name, entry.started_at),
        None => println!("Not tracking anything."),
    }
    Ok(())
}

pub async fn today(db: &str) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let entries = store.list_today()?;
    if entries.is_empty() {
        println!("No entries today.");
        return Ok(());
    }
    println!("{: <30} {: <10} {}", "NAME", "DURATION", "TAGS");
    for e in entries {
        let dur = format!("{}s", e.duration_seconds.unwrap_or(0));
        println!("{:<30} {: <10} {}", e.name, dur, e.tags.as_deref().unwrap_or("-"));
    }
    Ok(())
}

pub async fn resume(db: &str) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let entries = store.list_today()?;
    if let Some(last) = entries.first() {
        if store.get_current()?.is_none() {
            let id = store.start_entry(&last.name, last.tags.as_deref())?;
            println!("Resumed: {} (id: {})", last.name, id);
        } else {
            println!("Already tracking something. Stop first.");
        }
    } else {
        println!("No entries to resume today.");
    }
    Ok(())
}

pub async fn export(db: &str, format: &str, output: &std::path::Path) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    let entries = store.list_recent(365)?;

    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&entries)?;
            std::fs::write(output, json)?;
        }
        "csv" => {
            let mut wtr = csv::Writer::from_path(output)?;
            wtr.write_record(["name", "started_at", "ended_at", "duration_seconds", "tags"])?;
            for e in entries {
                wtr.write_record([&e.name,
                    &e.started_at,
                    &e.ended_at.unwrap_or_default(),
                    &e.duration_seconds.map_or_else(|| "0".to_string(), |d| d.to_string()),
                    &e.tags.unwrap_or_default(),
                ])?;
            }
            wtr.flush()?;
        }
        "ical" => {
            let mut ical = String::new();
            ical.push_str("BEGIN:VCALENDAR\r\n");
            ical.push_str("VERSION:2.0\r\n");
            ical.push_str("PRODID:-//OpenTracker//EN\r\n");
            for e in entries {
                let start = &e.started_at;
                let end = e.ended_at.as_deref().unwrap_or(start);
                let dur = e.duration_seconds.unwrap_or(0);
                let desc = format!("{} ({}s)", e.name, dur);
                ical.push_str("BEGIN:VEVENT\r\n");
                ical.push_str(&format!("SUMMARY:{}\r\n", e.name));
                ical.push_str(&format!("DTSTART:{}\r\n", start.replace('-', "").replace(':', "")));
                ical.push_str(&format!("DTEND:{}\r\n", end.replace('-', "").replace(':', "")));
                ical.push_str(&format!("DESCRIPTION:{}\r\n", desc));
                if let Some(ref t) = e.tags {
                    ical.push_str(&format!("CATEGORIES:{}\r\n", t));
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
