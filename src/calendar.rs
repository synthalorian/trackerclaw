use crate::store::{Entry, Store};
use anyhow::Result;
use chrono::{Datelike, Duration, Local, NaiveDate, Utc};
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
        let entries = store.entries_for_date_range(
            start.and_local_timezone(Utc).unwrap(),
            end.and_local_timezone(Utc).unwrap(),
        )?;
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
        let entries = store.entries_for_date_range(
            start.and_local_timezone(Utc).unwrap(),
            end.and_local_timezone(Utc).unwrap(),
        )?;
        let total: i64 = entries
            .iter()
            .map(|e| e.duration_seconds.unwrap_or(0))
            .sum();
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
    use chrono::TimeZone;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_db() -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "trackerclaw_cal_test_{}_{}.db",
            std::process::id(),
            n
        ))
    }

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

    #[test]
    fn month_view_sums_each_day() {
        let db = temp_db();
        {
            let store = Store::open(std::path::Path::new(db.to_str().unwrap())).unwrap();
            let day1 = Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap();
            let day1b = Utc.with_ymd_and_hms(2026, 6, 10, 18, 0, 0).unwrap();
            let day2 = Utc.with_ymd_and_hms(2026, 6, 15, 9, 0, 0).unwrap();
            store
                .insert_completed_entry("a", None, None, day1, day1 + Duration::hours(1), 3600, 1)
                .unwrap();
            store
                .insert_completed_entry(
                    "b",
                    None,
                    None,
                    day1b,
                    day1b + Duration::minutes(30),
                    1800,
                    1,
                )
                .unwrap();
            store
                .insert_completed_entry("c", None, None, day2, day2 + Duration::hours(2), 7200, 1)
                .unwrap();
        }
        let days = month_view(db.to_str().unwrap(), 2026, 6).unwrap();
        assert_eq!(days.len(), 30);
        let d10 = days.iter().find(|(d, _)| d.day() == 10).unwrap();
        assert_eq!(d10.1, 5400);
        let d15 = days.iter().find(|(d, _)| d.day() == 15).unwrap();
        assert_eq!(d15.1, 7200);
        let d1 = days.iter().find(|(d, _)| d.day() == 1).unwrap();
        assert_eq!(d1.1, 0);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn month_view_handles_short_months() {
        let db = temp_db();
        // February 2026 (non-leap) has 28 days; must not panic or overshoot.
        let days = month_view(db.to_str().unwrap(), 2026, 2).unwrap();
        assert_eq!(days.len(), 28);
        // February 2024 (leap) has 29.
        let days = month_view(db.to_str().unwrap(), 2024, 2).unwrap();
        assert_eq!(days.len(), 29);
        let _ = std::fs::remove_file(&db);
    }

    #[test]
    fn week_view_returns_seven_days() {
        let db = temp_db();
        let days = week_view(db.to_str().unwrap()).unwrap();
        assert_eq!(days.len(), 7);
        // First day must be a Monday.
        assert_eq!(days[0].0.weekday(), chrono::Weekday::Mon);
        let _ = std::fs::remove_file(&db);
    }
}
