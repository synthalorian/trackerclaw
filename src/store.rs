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
    pub project_id: Option<i64>,
    pub user_id: Option<i64>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub client: Option<String>,
    pub hourly_rate: Option<f64>,
    pub color: Option<String>,
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
                project_id INTEGER,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                duration_seconds INTEGER,
                user_id INTEGER DEFAULT 1
            )",
            [],
        )?;
        // Migrate old databases that don't have project_id
        let _ = self.conn.execute(
            "ALTER TABLE entries ADD COLUMN project_id INTEGER",
            [],
        );
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS current (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                entry_id INTEGER
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS projects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                client TEXT,
                hourly_rate REAL,
                color TEXT,
                created_at TEXT NOT NULL
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

    pub fn start_entry(&self, name: &str, tags: Option<&str>, project_id: Option<i64>, user_id: i64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO entries (name, tags, project_id, started_at, user_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, tags, project_id, Utc::now().to_rfc3339(), user_id],
        )?;
        let id = self.conn.last_insert_rowid();
        self.conn.execute(
            "INSERT OR REPLACE INTO current (id, entry_id) VALUES (1, ?1)",
            [id],
        )?;
        Ok(id)
    }

    pub fn stop_current(&self, user_id: i64) -> Result<Option<Entry>> {
        let current = self.get_current(user_id)?;
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

    pub fn get_current(&self, user_id: i64) -> Result<Option<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.name, e.tags, e.project_id, e.user_id, e.started_at, e.ended_at, e.duration_seconds
             FROM entries e
             JOIN current c ON e.id = c.entry_id
             WHERE e.user_id = ?1"
        )?;
        let mut rows = stmt.query([user_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::entry_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    fn user_filter(&self, is_admin: bool) -> &'static str {
        if is_admin { "" } else { " AND user_id = ?2" }
    }

    pub fn list_today(&self, user_id: i64, is_admin: bool) -> Result<Vec<Entry>> {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let sql = format!(
            "SELECT id, name, tags, project_id, user_id, started_at, ended_at, duration_seconds
             FROM entries
             WHERE date(started_at) = date(?1){}
             ORDER BY started_at DESC",
            self.user_filter(is_admin)
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = if is_admin {
            stmt.query_map([&today], Self::entry_from_row)?
        } else {
            stmt.query_map([&today, &user_id.to_string()], Self::entry_from_row)?
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_recent(&self, days: i64, user_id: i64, is_admin: bool) -> Result<Vec<Entry>> {
        let since = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let sql = format!(
            "SELECT id, name, tags, project_id, user_id, started_at, ended_at, duration_seconds
             FROM entries
             WHERE started_at >= ?1{}
             ORDER BY started_at DESC",
            self.user_filter(is_admin)
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = if is_admin {
            stmt.query_map([&since], Self::entry_from_row)?
        } else {
            stmt.query_map([&since, &user_id.to_string()], Self::entry_from_row)?
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_by_tag(&self, tag: &str, days: i64, user_id: i64, is_admin: bool) -> Result<Vec<Entry>> {
        let since = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let pattern = format!("%{}%", tag);
        let sql = format!(
            "SELECT id, name, tags, project_id, user_id, started_at, ended_at, duration_seconds
             FROM entries
             WHERE started_at >= ?1 AND tags LIKE ?2{}
             ORDER BY started_at DESC",
            self.user_filter(is_admin)
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = if is_admin {
            stmt.query_map([&since, &pattern], Self::entry_from_row)?
        } else {
            stmt.query_map([&since, &pattern, &user_id.to_string()], Self::entry_from_row)?
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_completed_entry(&self, name: &str, tags: Option<&str>, project_id: Option<i64>, started: DateTime<Utc>, ended: DateTime<Utc>, duration: i64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO entries (name, tags, project_id, started_at, ended_at, duration_seconds) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![name, tags, project_id, started.to_rfc3339(), ended.to_rfc3339(), duration],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_entry_by_id(&self, id: i64) -> Result<Option<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tags, project_id, user_id, started_at, ended_at, duration_seconds
             FROM entries WHERE id = ?1"
        )?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::entry_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    fn is_current_entry(&self, id: i64) -> Result<bool> {
        let mut stmt = self.conn.prepare("SELECT entry_id FROM current WHERE id = 1")?;
        let mut rows = stmt.query([])?;
        Ok(rows.next()?.and_then(|r| r.get::<_, i64>(0).ok()) == Some(id))
    }

    pub fn update_entry(
        &self,
        id: i64,
        name: Option<&str>,
        tags: Option<&str>,
        started_at: Option<&str>,
        ended_at: Option<&str>,
    ) -> Result<()> {
        if self.is_current_entry(id)? {
            anyhow::bail!("Cannot edit the currently tracking entry. Stop it first.");
        }

        let entry = match self.get_entry_by_id(id)? {
            Some(e) => e,
            None => anyhow::bail!("Entry {} not found", id),
        };

        if entry.ended_at.is_none() {
            anyhow::bail!("Cannot edit an open entry. Stop it first.");
        }

        let final_name = name.unwrap_or(&entry.name);
        let final_tags = tags.or(entry.tags.as_deref());
        let final_started = started_at.unwrap_or(&entry.started_at);
        let final_ended = ended_at.unwrap_or(entry.ended_at.as_deref().unwrap());

        let started_dt = DateTime::parse_from_rfc3339(final_started)?.with_timezone(&chrono::Utc);
        let ended_dt = DateTime::parse_from_rfc3339(final_ended)?.with_timezone(&chrono::Utc);
        let duration = (ended_dt - started_dt).num_seconds();

        self.conn.execute(
            "UPDATE entries SET name = ?1, tags = ?2, started_at = ?3, ended_at = ?4, duration_seconds = ?5 WHERE id = ?6",
            params![final_name, final_tags, started_dt.to_rfc3339(), ended_dt.to_rfc3339(), duration, id],
        )?;
        Ok(())
    }

    pub fn delete_entry(&self, id: i64) -> Result<()> {
        if self.is_current_entry(id)? {
            anyhow::bail!("Cannot delete the currently tracking entry. Stop it first.");
        }
        let deleted = self.conn.execute("DELETE FROM entries WHERE id = ?1", [id])?;
        if deleted == 0 {
            anyhow::bail!("Entry {} not found", id);
        }
        Ok(())
    }

    fn day_stat_from_row(row: &rusqlite::Row) -> rusqlite::Result<(String, i64)> {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    }

    fn project_stat_from_row(row: &rusqlite::Row) -> rusqlite::Result<(String, i64)> {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    }

    pub fn daily_stats(&self, days: i64, user_id: i64, is_admin: bool) -> Result<Vec<(String, i64)>> {
        let since = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let sql = format!(
            "SELECT date(started_at) as day, COALESCE(SUM(duration_seconds), 0) as total
             FROM entries
             WHERE started_at >= ?1 AND duration_seconds IS NOT NULL{}
             GROUP BY day
             ORDER BY day ASC",
            self.user_filter(is_admin)
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = if is_admin {
            stmt.query_map([&since], Self::day_stat_from_row)?
        } else {
            stmt.query_map([&since, &user_id.to_string()], Self::day_stat_from_row)?
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn project_stats(&self, days: i64, user_id: i64, is_admin: bool) -> Result<Vec<(String, i64)>> {
        let since = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let sql = format!(
            "SELECT COALESCE(tags, 'untagged') as project, COALESCE(SUM(duration_seconds), 0) as total
             FROM entries
             WHERE started_at >= ?1 AND duration_seconds IS NOT NULL{}
             GROUP BY project
             ORDER BY total DESC",
            self.user_filter(is_admin)
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = if is_admin {
            stmt.query_map([&since], Self::project_stat_from_row)?
        } else {
            stmt.query_map([&since, &user_id.to_string()], Self::project_stat_from_row)?
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn invoice_entries(&self, days: i64, tag: Option<&str>, user_id: i64, is_admin: bool) -> Result<Vec<Entry>> {
        let since = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let user_sql = self.user_filter(is_admin);

        if let Some(t) = tag {
            let pattern = format!("%{}%", t);
            let sql = format!(
                "SELECT id, name, tags, project_id, user_id, started_at, ended_at, duration_seconds
                 FROM entries
                 WHERE started_at >= ?1 AND duration_seconds IS NOT NULL AND tags LIKE ?2{}
                 ORDER BY started_at DESC",
                user_sql
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = if is_admin {
                stmt.query_map([&since, &pattern], Self::entry_from_row)?
            } else {
                stmt.query_map([&since, &pattern, &user_id.to_string()], Self::entry_from_row)?
            };
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        } else {
            let sql = format!(
                "SELECT id, name, tags, project_id, user_id, started_at, ended_at, duration_seconds
                 FROM entries
                 WHERE started_at >= ?1 AND duration_seconds IS NOT NULL{}
                 ORDER BY started_at DESC",
                user_sql
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = if is_admin {
                stmt.query_map([&since], Self::entry_from_row)?
            } else {
                stmt.query_map([&since, &user_id.to_string()], Self::entry_from_row)?
            };
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

    pub fn add_project(&self, name: &str, client: Option<&str>, hourly_rate: Option<f64>, color: Option<&str>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO projects (name, client, hourly_rate, color, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, client, hourly_rate, color, Utc::now().to_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_project_by_name(&self, name: &str) -> Result<Option<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, client, hourly_rate, color FROM projects WHERE name = ?1"
        )?;
        let mut rows = stmt.query([name])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                client: row.get(2)?,
                hourly_rate: row.get(3)?,
                color: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_project_by_id(&self, id: i64) -> Result<Option<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, client, hourly_rate, color FROM projects WHERE id = ?1"
        )?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                client: row.get(2)?,
                hourly_rate: row.get(3)?,
                color: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, client, hourly_rate, color FROM projects ORDER BY name"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                name: row.get(1)?,
                client: row.get(2)?,
                hourly_rate: row.get(3)?,
                color: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn update_project(&self, id: i64, name: Option<&str>, client: Option<&str>, hourly_rate: Option<f64>, color: Option<&str>) -> Result<()> {
        let existing = self.get_project_by_id(id)?.ok_or_else(|| anyhow::anyhow!("Project {} not found", id))?;
        let final_name = name.unwrap_or(&existing.name);
        let final_client = client.or(existing.client.as_deref());
        let final_rate = hourly_rate.or(existing.hourly_rate);
        let final_color = color.or(existing.color.as_deref());
        self.conn.execute(
            "UPDATE projects SET name = ?1, client = ?2, hourly_rate = ?3, color = ?4 WHERE id = ?5",
            params![final_name, final_client, final_rate, final_color, id],
        )?;
        Ok(())
    }

    pub fn delete_project(&self, id: i64) -> Result<()> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM entries WHERE project_id = ?1",
            [id],
            |row| row.get(0),
        )?;
        if count > 0 {
            anyhow::bail!("Cannot delete project with existing time entries.");
        }
        self.conn.execute("DELETE FROM projects WHERE id = ?1", [id])?;
        Ok(())
    }

    fn entry_from_row(row: &rusqlite::Row) -> rusqlite::Result<Entry> {
        Ok(Entry {
            id: row.get(0)?,
            name: row.get(1)?,
            tags: row.get(2)?,
            project_id: row.get(3)?,
            user_id: row.get(4)?,
            started_at: row.get(5)?,
            ended_at: row.get(6)?,
            duration_seconds: row.get(7)?,
        })
    }

    pub fn list_by_project(&self, project_id: i64, days: i64, user_id: i64, is_admin: bool) -> Result<Vec<Entry>> {
        let since = (Utc::now() - chrono::Duration::days(days)).to_rfc3339();
        let sql = format!(
            "SELECT id, name, tags, project_id, user_id, started_at, ended_at, duration_seconds
             FROM entries
             WHERE started_at >= ?1 AND project_id = ?2{}
             ORDER BY started_at DESC",
            self.user_filter(is_admin)
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = if is_admin {
            stmt.query_map([&since, &project_id.to_string()], Self::entry_from_row)?
        } else {
            stmt.query_map([&since, &project_id.to_string(), &user_id.to_string()], Self::entry_from_row)?
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn entries_for_date_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tags, project_id, user_id, started_at, ended_at, duration_seconds
             FROM entries
             WHERE started_at >= ?1 AND started_at < ?2 AND duration_seconds IS NOT NULL
             ORDER BY started_at ASC"
        )?;
        let rows = stmt.query_map(params![start.to_rfc3339(), end.to_rfc3339()], Self::entry_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static DB_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_db() -> String {
        let n = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = format!("/tmp/trackerclaw_test_{}_{}.db", std::process::id(), n);
        let _ = fs::remove_file(&path);
        path
    }

    #[test]
    fn start_and_stop_entry() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        let id = store.start_entry("test task", Some("rust"), None, 1).unwrap();
        assert!(store.get_current(1).unwrap().is_some());

        std::thread::sleep(std::time::Duration::from_millis(10));
        let entry = store.stop_current(1).unwrap().unwrap();
        assert_eq!(entry.id, id);
        assert!(entry.ended_at.is_some());
        assert!(entry.duration_seconds.unwrap() >= 0);
        assert!(store.get_current(1).unwrap().is_none());

        let _ = fs::remove_file(&db);
    }

    #[test]
    fn get_entry_by_id_and_edit() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        let id = store.start_entry("editable", Some("old"), None, 1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.stop_current(1).unwrap();

        let before = store.get_entry_by_id(id).unwrap().unwrap();
        assert_eq!(before.tags.as_deref(), Some("old"));

        let started = DateTime::parse_from_rfc3339(&before.started_at).unwrap().with_timezone(&Utc);
        let new_ended = (started + Duration::hours(2)).to_rfc3339();
        store.update_entry(id, Some("new name"), Some("new"), None, Some(&new_ended)).unwrap();

        let after = store.get_entry_by_id(id).unwrap().unwrap();
        assert_eq!(after.name, "new name");
        assert_eq!(after.tags.as_deref(), Some("new"));
        assert_eq!(after.duration_seconds, Some(7200));

        let _ = fs::remove_file(&db);
    }

    #[test]
    fn cannot_edit_or_delete_active_entry() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        let id = store.start_entry("active", None, None, 1).unwrap();
        assert!(store.update_entry(id, Some("x"), None, None, None).is_err());
        assert!(store.delete_entry(id).is_err());
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn delete_entry_removes_it() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        let id = store.start_entry("to delete", None, None, 1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.stop_current(1).unwrap();
        store.delete_entry(id).unwrap();
        assert!(store.get_entry_by_id(id).unwrap().is_none());
        assert!(store.delete_entry(id).is_err());
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn budget_round_trip() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        store.set_budget("rust", 3600).unwrap();
        let budgets = store.list_budgets().unwrap();
        assert_eq!(budgets.len(), 1);
        assert_eq!(budgets[0].0, "rust");
        assert_eq!(budgets[0].1, 3600);
        store.delete_budget("rust").unwrap();
        assert!(store.list_budgets().unwrap().is_empty());
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn insert_completed_entry() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        let started = Utc::now();
        let ended = started + Duration::minutes(45);
        let id = store.insert_completed_entry("completed", Some("done"), None, started, ended, 2700).unwrap();
        let entry = store.get_entry_by_id(id).unwrap().unwrap();
        assert_eq!(entry.name, "completed");
        assert_eq!(entry.duration_seconds, Some(2700));
        let _ = fs::remove_file(&db);
    }
}
