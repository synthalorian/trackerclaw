use crate::store::{Entry, Store};
use anyhow::Result;
use std::path::Path;

pub async fn send_webhook(db: &str, entry: &Entry) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    if let Some((url, enabled, headers)) = store.get_webhook()? {
        if !enabled {
            return Ok(());
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let mut req = client.post(&url).json(&entry);

        if let Some(h) = headers {
            for line in h.lines() {
                if let Some((key, value)) = line.split_once(':') {
                    req = req.header(key.trim(), value.trim());
                }
            }
        }

        req.send().await?;
    }
    Ok(())
}

pub fn set_webhook(db: &str, url: &str, enabled: bool, headers: Option<&str>) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    store.set_webhook(url, enabled, headers)?;
    println!("Webhook configured: {}", url);
    Ok(())
}

pub fn show_webhook(db: &str) -> Result<()> {
    let store = Store::open(Path::new(db))?;
    match store.get_webhook()? {
        Some((url, enabled, _)) => {
            let status = if enabled { "enabled" } else { "disabled" };
            println!("Webhook URL: {} ({})", url, status);
        }
        None => println!("No webhook configured."),
    }
    Ok(())
}
