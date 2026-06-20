use anyhow::{bail, Result};
use chrono::{DateTime, NaiveDate, Utc};

/// Parse a user-supplied date/time string into an RFC3339 UTC DateTime.
///
/// Accepts:
/// - `2026-06-19` → start of day UTC
/// - `2026-06-19T09:00:00Z` → used as-is
/// - `2026-06-19T09:00:00+00:00` → used as-is
pub fn parse_date(date_str: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
        return Ok(dt.with_timezone(&Utc));
    }

    if let Ok(naive) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        return Ok(naive.and_hms_opt(0, 0, 0).unwrap().and_utc());
    }

    bail!("Invalid date format: '{}'. Use YYYY-MM-DD or RFC3339.", date_str)
}

/// Default start/end for a sync window of the last N days.
pub fn default_sync_window(days: i64) -> (DateTime<Utc>, DateTime<Utc>) {
    let end = Utc::now();
    let start = end - chrono::Duration::days(days);
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339() {
        let dt = parse_date("2026-06-19T09:30:00Z").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-06-19T09:30:00+00:00");
    }

    #[test]
    fn parses_date_only() {
        let dt = parse_date("2026-06-19").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-06-19T00:00:00+00:00");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_date("not-a-date").is_err());
    }
}
