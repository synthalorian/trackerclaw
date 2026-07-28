use crate::store::Store;
use crate::time_parse::{default_sync_window, parse_date};
use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Utc};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct TogglEntry {
    pub description: String,
    pub start: String,
    pub duration: i64,
    pub tags: Vec<String>,
}

pub struct TogglClient {
    api_token: String,
    client: reqwest::Client,
}

impl TogglClient {
    pub fn new(api_token: String) -> Self {
        Self {
            api_token,
            client: reqwest::Client::new(),
        }
    }

    pub async fn get_time_entries(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<TogglEntry>> {
        let auth = format!("{}:api_token", self.api_token);
        let encoded = BASE64.encode(&auth);
        let url = format!(
            "https://api.track.toggl.com/api/v9/me/time_entries?start_date={}&end_date={}",
            start_date, end_date
        );
        let resp = self
            .client
            .get(&url)
            .header(AUTHORIZATION, format!("Basic {}", encoded))
            .header(CONTENT_TYPE, "application/json")
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("Toggl API error: {}", resp.status());
        }
        let entries: Vec<TogglEntry> = resp.json().await?;
        Ok(entries)
    }

    pub async fn create_time_entry(&self, entry: &TogglEntry, workspace_id: i64) -> Result<()> {
        let auth = format!("{}:api_token", self.api_token);
        let encoded = BASE64.encode(&auth);
        let url = format!(
            "https://api.track.toggl.com/api/v9/workspaces/{}/time_entries",
            workspace_id
        );
        let resp = self
            .client
            .post(&url)
            .header(AUTHORIZATION, format!("Basic {}", encoded))
            .header(CONTENT_TYPE, "application/json")
            .json(&json!({
                "description": entry.description,
                "start": entry.start,
                "duration": entry.duration,
                "tags": entry.tags,
                "created_with": "trackerclaw"
            }))
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("Toggl API error: {}", resp.status());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockifyEntry {
    pub description: String,
    pub start: String,
    pub end: String,
    pub project_id: Option<String>,
    pub tag_ids: Vec<String>,
}

pub struct ClockifyClient {
    api_key: String,
    workspace_id: String,
    client: reqwest::Client,
}

impl ClockifyClient {
    pub fn new(api_key: String, workspace_id: String) -> Self {
        Self {
            api_key,
            workspace_id,
            client: reqwest::Client::new(),
        }
    }

    pub async fn get_time_entries(&self, start: &str, end: &str) -> Result<Vec<ClockifyEntry>> {
        let url = format!(
            "https://api.clockify.me/api/v1/workspaces/{}/user/me/time-entries?start={}&end={}&page-size=5000",
            self.workspace_id, start, end
        );
        let resp = self
            .client
            .get(&url)
            .header("X-Api-Key", &self.api_key)
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("Clockify API error: {}", resp.status());
        }
        let entries: Vec<ClockifyEntry> = resp.json().await?;
        Ok(entries)
    }

    pub async fn get_projects(&self) -> Result<Vec<(String, String)>> {
        let url = format!(
            "https://api.clockify.me/api/v1/workspaces/{}/projects",
            self.workspace_id
        );
        let resp = self
            .client
            .get(&url)
            .header("X-Api-Key", &self.api_key)
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("Clockify API error: {}", resp.status());
        }
        let projects: Vec<serde_json::Value> = resp.json().await?;
        let result = projects
            .iter()
            .filter_map(|p| {
                let id = p.get("id")?.as_str()?;
                let name = p.get("name")?.as_str()?;
                Some((id.to_string(), name.to_string()))
            })
            .collect();
        Ok(result)
    }
}

fn load_existing_keys(
    store: &Store,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<HashSet<(String, String)>> {
    let entries = store.entries_for_date_range(start, end)?;
    Ok(entries
        .into_iter()
        .map(|e| (e.name, e.started_at))
        .collect())
}

pub async fn import_toggl(db: &str, api_token: &str, start: &str, end: &str) -> Result<()> {
    let start_dt = parse_date(start)?;
    let end_dt = parse_date(end)?;
    let (user_id, _, _) = crate::auth::resolve_current_user(db, None)?;
    let client = TogglClient::new(api_token.to_string());
    let entries = client
        .get_time_entries(&start_dt.to_rfc3339(), &end_dt.to_rfc3339())
        .await?;

    let store = Store::open(Path::new(db))?;
    let existing = load_existing_keys(&store, start_dt, end_dt)?;

    let mut imported = 0;
    let mut skipped = 0;
    for e in entries {
        let started = DateTime::parse_from_rfc3339(&e.start)?.with_timezone(&Utc);
        let ended = started + chrono::Duration::seconds(e.duration);
        let tags = if e.tags.is_empty() {
            None
        } else {
            Some(e.tags.join(","))
        };

        if existing.contains(&(e.description.clone(), started.to_rfc3339())) {
            skipped += 1;
            continue;
        }

        store.insert_completed_entry(
            &e.description,
            tags.as_deref(),
            None,
            started,
            ended,
            e.duration,
            user_id,
        )?;
        imported += 1;
    }

    println!(
        "Toggl import complete: fetched {}, imported {}, skipped {} duplicates.",
        imported + skipped,
        imported,
        skipped
    );
    Ok(())
}

pub async fn export_toggl(
    db: &str,
    api_token: &str,
    workspace_id: i64,
    start: Option<&str>,
    end: Option<&str>,
) -> Result<()> {
    let (start_dt, end_dt) = match (start, end) {
        (Some(s), Some(e)) => (parse_date(s)?, parse_date(e)?),
        _ => default_sync_window(7),
    };

    let store = Store::open(Path::new(db))?;
    let entries = store.entries_for_date_range(start_dt, end_dt)?;

    let client = TogglClient::new(api_token.to_string());
    let mut exported = 0;
    for e in entries {
        let started = e.started_at;
        let duration = e.duration_seconds.unwrap_or(0);
        let tags: Vec<String> = e
            .tags
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let toggl_entry = TogglEntry {
            description: e.name,
            start: started,
            duration,
            tags,
        };
        client.create_time_entry(&toggl_entry, workspace_id).await?;
        exported += 1;
    }

    println!(
        "Exported {} entries to Toggl workspace {}.",
        exported, workspace_id
    );
    Ok(())
}

pub async fn import_clockify(
    db: &str,
    api_key: &str,
    workspace_id: &str,
    start: &str,
    end: &str,
) -> Result<()> {
    let start_dt = parse_date(start)?;
    let end_dt = parse_date(end)?;
    let (user_id, _, _) = crate::auth::resolve_current_user(db, None)?;
    let client = ClockifyClient::new(api_key.to_string(), workspace_id.to_string());

    let projects = client.get_projects().await?;
    let project_map: std::collections::HashMap<String, String> = projects.into_iter().collect();

    let entries = client
        .get_time_entries(&start_dt.to_rfc3339(), &end_dt.to_rfc3339())
        .await?;

    let store = Store::open(Path::new(db))?;
    let existing = load_existing_keys(&store, start_dt, end_dt)?;

    let mut imported = 0;
    let mut skipped = 0;
    for e in entries {
        let started = DateTime::parse_from_rfc3339(&e.start)?.with_timezone(&Utc);
        let ended = DateTime::parse_from_rfc3339(&e.end)?.with_timezone(&Utc);
        let duration = (ended - started).num_seconds();

        let mut tags: Vec<String> = Vec::new();
        if let Some(pid) = &e.project_id {
            if let Some(name) = project_map.get(pid) {
                tags.push(name.clone());
            }
        }
        for tid in &e.tag_ids {
            tags.push(tid.clone());
        }
        let tags_str = if tags.is_empty() {
            None
        } else {
            Some(tags.join(","))
        };

        if existing.contains(&(e.description.clone(), started.to_rfc3339())) {
            skipped += 1;
            continue;
        }

        store.insert_completed_entry(
            &e.description,
            tags_str.as_deref(),
            None,
            started,
            ended,
            duration,
            user_id,
        )?;
        imported += 1;
    }

    println!(
        "Clockify import complete: fetched {}, imported {}, skipped {} duplicates.",
        imported + skipped,
        imported,
        skipped
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_db() -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "trackerclaw_int_test_{}_{}.db",
            std::process::id(),
            n
        ))
    }

    #[test]
    fn toggl_entry_deserializes_api_shape() {
        let json = r#"{"description":"Fix bug","start":"2026-06-01T10:00:00+00:00","duration":3600,"tags":["rust","backend"]}"#;
        let e: TogglEntry = serde_json::from_str(json).unwrap();
        assert_eq!(e.description, "Fix bug");
        assert_eq!(e.duration, 3600);
        assert_eq!(e.tags, vec!["rust", "backend"]);
    }

    #[test]
    fn clockify_entry_deserializes_api_shape() {
        let json = r#"{"description":"Design","start":"2026-06-01T10:00:00Z","end":"2026-06-01T11:30:00Z","projectId":"abc123","tagIds":["t1"]}"#;
        let e: ClockifyEntry = serde_json::from_str(json).unwrap();
        assert_eq!(e.description, "Design");
        assert_eq!(e.project_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn existing_keys_detect_duplicates() {
        let db = temp_db();
        let store = Store::open(&db).unwrap();
        let start = Utc::now() - chrono::Duration::days(1);
        let end = Utc::now() + chrono::Duration::days(1);
        let t0 = Utc::now() - chrono::Duration::hours(2);
        store
            .insert_completed_entry(
                "dup",
                None,
                None,
                t0,
                t0 + chrono::Duration::hours(1),
                3600,
                1,
            )
            .unwrap();

        let keys = load_existing_keys(&store, start, end).unwrap();
        assert!(keys.contains(&("dup".to_string(), t0.to_rfc3339())));
        assert!(!keys.contains(&("other".to_string(), t0.to_rfc3339())));
        let _ = std::fs::remove_file(&db);
    }
}
