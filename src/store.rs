use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: i64,
    pub name: String,
    pub tags: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: Option<i64>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Store { conn };
        store.init()?;
        Ok(store)
    }

    pub fn init(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                tags TEXT,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                duration_seconds INTEGER
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS current (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                entry_id INTEGER
            )",
            [],
        )?;
        Ok(())
    }

    pub fn start_entry(&self, name: &str, tags: Option<&str>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO entries (name, tags, started_at) VALUES (?1, ?2, ?3)",
            params![name, tags, Utc::now().to_rfc3339()],
        )?;
        let id = self.conn.last_insert_rowid();
        self.conn.execute(
            "INSERT OR REPLACE INTO current (id, entry_id) VALUES (1, ?1)",
            [id],
        )?;
        Ok(id)
    }

    pub fn stop_current(&self) -> Result<Option<Entry>> {
        let current = self.get_current()?;
        if let Some(entry) = current {
            let started = DateTime::parse_from_rfc3339(&entry.started_at)?.with_timezone::<chrono::Utc>(&chrono::Utc);
            let ended = Utc::now();
            let duration = (ended - started).num_seconds();
            self.conn.execute(
                "UPDATE entries SET ended_at = ?1, duration_seconds = ?2 WHERE id = ?3",
                params![ended.to_rfc3339(), duration, entry.id],
            )?;
            self.conn.execute("DELETE FROM current WHERE id = 1", [])?;
            Ok(Some(Entry { ended_at: Some(ended.to_rfc3339()), duration_seconds: Some(duration), ..entry }))
        } else {
            Ok(None)
        }
    }

    pub fn get_current(&self) -> Result<Option<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.name, e.tags, e.started_at, e.ended_at, e.duration_seconds
             FROM entries e
             JOIN current c ON e.id = c.entry_id"
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Entry {
                id: row.get(0)?,
                name: row.get(1)?,
                tags: row.get(2)?,
                started_at: row.get(3)?,
                ended_at: row.get(4)?,
                duration_seconds: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_today(&self) -> Result<Vec<Entry>> {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tags, started_at, ended_at, duration_seconds
             FROM entries
             WHERE date(started_at) = date(?1)
             ORDER BY started_at DESC"
        )?;
        let rows = stmt.query_map([&today], |row| {
            Ok(Entry {
                id: row.get(0)?,
                name: row.get(1)?,
                tags: row.get(2)?,
                started_at: row.get(3)?,
                ended_at: row.get(4)?,
                duration_seconds: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_recent(&self, days: i64) -> Result<Vec<Entry>> {
        let since = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tags, started_at, ended_at, duration_seconds
             FROM entries
             WHERE started_at >= ?1
             ORDER BY started_at DESC"
        )?;
        let rows = stmt.query_map([&since], |row| {
            Ok(Entry {
                id: row.get(0)?,
                name: row.get(1)?,
                tags: row.get(2)?,
                started_at: row.get(3)?,
                ended_at: row.get(4)?,
                duration_seconds: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_by_tag(&self, tag: &str, days: i64) -> Result<Vec<Entry>> {
        let since = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let pattern = format!("%{}%", tag);
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tags, started_at, ended_at, duration_seconds
             FROM entries
             WHERE started_at >= ?1 AND tags LIKE ?2
             ORDER BY started_at DESC"
        )?;
        let rows = stmt.query_map([&since, &pattern], |row| {
            Ok(Entry {
                id: row.get(0)?,
                name: row.get(1)?,
                tags: row.get(2)?,
                started_at: row.get(3)?,
                ended_at: row.get(4)?,
                duration_seconds: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_completed_entry(&self, name: &str, tags: Option<&str>, started: DateTime<Utc>, ended: DateTime<Utc>, duration: i64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO entries (name, tags, started_at, ended_at, duration_seconds) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, tags, started.to_rfc3339(), ended.to_rfc3339(), duration],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn daily_stats(&self, days: i64) -> Result<Vec<(String, i64)>> {
        let since = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let mut stmt = self.conn.prepare(
            "SELECT date(started_at) as day, COALESCE(SUM(duration_seconds), 0) as total
             FROM entries
             WHERE started_at >= ?1 AND duration_seconds IS NOT NULL
             GROUP BY day
             ORDER BY day ASC"
        )?;
        let rows = stmt.query_map([&since], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn project_stats(&self, days: i64) -> Result<Vec<(String, i64)>> {
        let since = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(tags, 'untagged') as project, COALESCE(SUM(duration_seconds), 0) as total
             FROM entries
             WHERE started_at >= ?1 AND duration_seconds IS NOT NULL
             GROUP BY project
             ORDER BY total DESC"
        )?;
        let rows = stmt.query_map([&since], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn invoice_entries(&self, days: i64, tag: Option<&str>) -> Result<Vec<Entry>> {
        let since = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();

        if let Some(t) = tag {
            let pattern = format!("%{}%", t);
            let mut stmt = self.conn.prepare(
                "SELECT id, name, tags, started_at, ended_at, duration_seconds
                 FROM entries
                 WHERE started_at >= ?1 AND duration_seconds IS NOT NULL AND tags LIKE ?2
                 ORDER BY started_at DESC"
            )?;
            let rows = stmt.query_map([&since, &pattern], |row| {
                Ok(Entry {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    tags: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    duration_seconds: row.get(5)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, name, tags, started_at, ended_at, duration_seconds
                 FROM entries
                 WHERE started_at >= ?1 AND duration_seconds IS NOT NULL
                 ORDER BY started_at DESC"
            )?;
            let rows = stmt.query_map([&since], |row| {
                Ok(Entry {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    tags: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    duration_seconds: row.get(5)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        }
    }
}
