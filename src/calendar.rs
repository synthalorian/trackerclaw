use crate::store::{Entry, Store};
use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use std::path::Path;

pub fn week_view(db: &str) -> Result<Vec<(NaiveDate, Vec<Entry>)>> {
    let store = Store::open(Path::new(db))?;
    let today = Local::now().date_naive();
    let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    let mut result = Vec::new();
    for i in 0..7 {
        let date = monday + Duration::days(i);
        let start = date.and_hms_opt(0, 0, 0).unwrap();
        let end = start + Duration::days(1);
        let entries = store.entries_for_date_range(start.and_local_timezone(Utc).unwrap(), end.and_local_timezone(Utc).unwrap())?;
        result.push((date, entries));
    }
    Ok(result)
}

pub fn month_view(db: &str, year: i32, month: u32) -> Result<Vec<(NaiveDate, i64)>> {
    let store = Store::open(Path::new(db))?;
    let mut days_in_month = 28;
    while NaiveDate::from_ymd_opt(year, month, days_in_month + 1).is_some() {
        days_in_month += 1;
    }
    let mut result = Vec::new();
    for day in 1..=days_in_month {
        let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
        let start = date.and_hms_opt(0, 0, 0).unwrap();
        let end = start + Duration::days(1);
        let entries = store.entries_for_date_range(start.and_local_timezone(Utc).unwrap(), end.and_local_timezone(Utc).unwrap())?;
        let total: i64 = entries.iter().map(|e| e.duration_seconds.unwrap_or(0)).sum();
        result.push((date, total));
    }
    Ok(result)
}

pub fn format_duration_short(seconds: i64) -> String {
    if seconds >= 3600 {
        format!("{:.1}h", seconds as f64 / 3600.0)
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}s", seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_seconds() {
        assert_eq!(format_duration_short(45), "45s");
    }

    #[test]
    fn format_minutes() {
        assert_eq!(format_duration_short(300), "5m");
    }

    #[test]
    fn format_hours() {
        assert_eq!(format_duration_short(5400), "1.5h");
    }
}
