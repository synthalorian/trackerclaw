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
                duration_seconds INTEGER,
                user_id INTEGER DEFAULT 1
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
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS budgets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project_tag TEXT NOT NULL UNIQUE,
                budget_seconds INTEGER NOT NULL,
                created_at TEXT NOT NULL
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                role TEXT NOT NULL DEFAULT 'member',
                created_at TEXT NOT NULL
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS webhook_config (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                url TEXT,
                enabled INTEGER NOT NULL DEFAULT 0,
                headers TEXT
            )",
            [],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO users (id, name, role, created_at) VALUES (1, 'default', 'admin', ?1)",
            [Utc::now().to_rfc3339()],
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

    pub fn set_budget(&self, project_tag: &str, budget_seconds: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO budgets (project_tag, budget_seconds, created_at) VALUES (?1, ?2, ?3)",
            params![project_tag, budget_seconds, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn get_budget(&self, project_tag: &str) -> Result<Option<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT project_tag, budget_seconds FROM budgets WHERE project_tag = ?1"
        )?;
        let mut rows = stmt.query([project_tag])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
    }

    pub fn list_budgets(&self) -> Result<Vec<(String, i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT b.project_tag, b.budget_seconds, COALESCE(SUM(e.duration_seconds), 0)
             FROM budgets b
             LEFT JOIN entries e ON e.tags LIKE ('%' || b.project_tag || '%') AND e.duration_seconds IS NOT NULL
             GROUP BY b.project_tag"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn delete_budget(&self, project_tag: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM budgets WHERE project_tag = ?1",
            [project_tag],
        )?;
        Ok(())
    }

    pub fn set_webhook(&self, url: &str, enabled: bool, headers: Option<&str>) -> Result<()> {
        let enabled_i = if enabled { 1 } else { 0 };
        self.conn.execute(
            "INSERT OR REPLACE INTO webhook_config (id, url, enabled, headers) VALUES (1, ?1, ?2, ?3)",
            params![url, enabled_i, headers],
        )?;
        Ok(())
    }

    pub fn get_webhook(&self) -> Result<Option<(String, bool, Option<String>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT url, enabled, headers FROM webhook_config WHERE id = 1"
        )?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let url: String = row.get(0)?;
            let enabled: i64 = row.get(1)?;
            let headers: Option<String> = row.get(2)?;
            Ok(Some((url, enabled != 0, headers)))
        } else {
            Ok(None)
        }
    }

    pub fn add_user(&self, name: &str, role: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO users (name, role, created_at) VALUES (?1, ?2, ?3)",
            params![name, role, Utc::now().to_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_users(&self) -> Result<Vec<(i64, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, role FROM users ORDER BY id"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_user(&self, name: &str) -> Result<Option<(i64, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, role FROM users WHERE name = ?1"
        )?;
        let mut rows = stmt.query([name])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?, row.get(2)?)))
        } else {
            Ok(None)
        }
    }

    pub fn entries_for_date_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tags, started_at, ended_at, duration_seconds
             FROM entries
             WHERE started_at >= ?1 AND started_at < ?2 AND duration_seconds IS NOT NULL
             ORDER BY started_at ASC"
        )?;
        let rows = stmt.query_map(params![start.to_rfc3339(), end.to_rfc3339()], |row| {
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
