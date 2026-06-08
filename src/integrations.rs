use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::json;

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

    pub async fn get_time_entries(&self, start_date: &str, end_date: &str) -> Result<Vec<TogglEntry>> {
        let auth = format!("{}:api_token", self.api_token);
        let encoded = BASE64.encode(&auth);
        let url = format!(
            "https://api.track.toggl.com/api/v9/me/time_entries?start_date={}&end_date={}",
            start_date, end_date
        );
        let resp = self.client
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
        let url = format!("https://api.track.toggl.com/api/v9/workspaces/{}/time_entries", workspace_id);
        let resp = self.client
            .post(&url)
            .header(AUTHORIZATION, format!("Basic {}", encoded))
            .header(CONTENT_TYPE, "application/json")
            .json(&json!({
                "description": entry.description,
                "start": entry.start,
                "duration": entry.duration,
                "tags": entry.tags,
                "created_with": "opentracker"
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
            "https://api.clockify.me/api/v1/workspaces/{}/user/{}/time-entries?start={}&end={}&page-size=5000",
            self.workspace_id, "", start, end
        );
        let resp = self.client
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
        let url = format!("https://api.clockify.me/api/v1/workspaces/{}/projects", self.workspace_id);
        let resp = self.client
            .get(&url)
            .header("X-Api-Key", &self.api_key)
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("Clockify API error: {}", resp.status());
        }
        let projects: Vec<serde_json::Value> = resp.json().await?;
        let result = projects.iter()
            .filter_map(|p| {
                let id = p.get("id")?.as_str()?;
                let name = p.get("name")?.as_str()?;
                Some((id.to_string(), name.to_string()))
            })
            .collect();
        Ok(result)
    }
}

pub async fn import_toggl(_db: &str, api_token: &str, start: &str, end: &str) -> Result<()> {
    let client = TogglClient::new(api_token.to_string());
    let entries = client.get_time_entries(start, end).await?;
    println!("Fetched {} entries from Toggl.", entries.len());
    println!("Use 'tracker toggl sync' to write them to the local database. (Feature stub - implement sync logic)");
    Ok(())
}

pub async fn export_toggl(_db: &str, _api_token: &str, workspace_id: i64) -> Result<()> {
    println!("Exporting entries to Toggl workspace {}...", workspace_id);
    println!("(Feature stub - implement export logic with date range selection)");
    Ok(())
}

pub async fn import_clockify(_db: &str, api_key: &str, workspace_id: &str) -> Result<()> {
    let client = ClockifyClient::new(api_key.to_string(), workspace_id.to_string());
    let projects = client.get_projects().await?;
    println!("Fetched {} projects from Clockify.", projects.len());
    for (id, name) in &projects {
        println!("  {} - {}", id, name);
    }
    println!("Use 'tracker clockify sync' to write time entries to the local database. (Feature stub)");
    Ok(())
}
