use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
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
        // WAL allows the TUI and web dashboard to share the DB without
        // readers blocking the writer; busy_timeout absorbs short locks.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
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
        // Migrate old databases that predate these columns. Errors are
        // expected (column already exists) and intentionally ignored.
        let _ = self
            .conn
            .execute("ALTER TABLE entries ADD COLUMN project_id INTEGER", []);
        let _ = self.conn.execute(
            "ALTER TABLE entries ADD COLUMN user_id INTEGER DEFAULT 1",
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

    /// Close any still-open entries for this user (e.g. orphaned when the
    /// `current` pointer was overwritten by another session). Returns the
    /// names of the entries that were auto-stopped. Negative durations
    /// (clock skew) are clamped to zero.
    pub fn close_open_entries(&self, user_id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, started_at FROM entries WHERE user_id = ?1 AND ended_at IS NULL",
        )?;
        let open: Vec<(i64, String, String)> = stmt
            .query_map([user_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut closed = Vec::new();
        let now = Utc::now();
        for (id, name, started_at) in open {
            let started = DateTime::parse_from_rfc3339(&started_at)?
                .with_timezone::<chrono::Utc>(&chrono::Utc);
            let duration = (now - started).num_seconds().max(0);
            self.conn.execute(
                "UPDATE entries SET ended_at = ?1, duration_seconds = ?2 WHERE id = ?3",
                params![now.to_rfc3339(), duration, id],
            )?;
            closed.push(name);
        }
        if !closed.is_empty() {
            // Drop the current pointer if it referenced an entry we just closed.
            self.conn.execute(
                "DELETE FROM current WHERE id = 1 AND entry_id NOT IN (SELECT id FROM entries WHERE ended_at IS NULL)",
                [],
            )?;
        }
        Ok(closed)
    }

    pub fn start_entry(
        &self,
        name: &str,
        tags: Option<&str>,
        project_id: Option<i64>,
        user_id: i64,
    ) -> Result<i64> {
        // Starting a new task implicitly stops anything still running for
        // this user; otherwise the old entry would be orphaned forever.
        self.close_open_entries(user_id)?;
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
        // Fall back to any orphaned open entry if the current pointer is
        // stale or was overwritten by another user/session.
        let current = match self.get_current(user_id)? {
            Some(e) => Some(e),
            None => self.get_open_entry(user_id)?,
        };
        if let Some(entry) = current {
            let started = DateTime::parse_from_rfc3339(&entry.started_at)?
                .with_timezone::<chrono::Utc>(&chrono::Utc);
            let ended = Utc::now();
            // Clamp against clock skew: a negative duration would corrupt
            // every aggregate that sums durations.
            let duration = (ended - started).num_seconds().max(0);
            self.conn.execute(
                "UPDATE entries SET ended_at = ?1, duration_seconds = ?2 WHERE id = ?3",
                params![ended.to_rfc3339(), duration, entry.id],
            )?;
            self.conn.execute("DELETE FROM current WHERE id = 1", [])?;
            Ok(Some(Entry {
                ended_at: Some(ended.to_rfc3339()),
                duration_seconds: Some(duration),
                ..entry
            }))
        } else {
            Ok(None)
        }
    }

    fn get_open_entry(&self, user_id: i64) -> Result<Option<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tags, project_id, user_id, started_at, ended_at, duration_seconds
             FROM entries
             WHERE user_id = ?1 AND ended_at IS NULL
             ORDER BY started_at DESC
             LIMIT 1",
        )?;
        let mut rows = stmt.query([user_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::entry_from_row(row)?))
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
        if is_admin {
            ""
        } else {
            " AND user_id = ?2"
        }
    }

    /// User filter for queries where ?1 and ?2 are already a start/end range.
    fn user_filter_range(&self, is_admin: bool) -> &'static str {
        if is_admin {
            ""
        } else {
            " AND user_id = ?3"
        }
    }

    pub fn list_today(&self, user_id: i64, is_admin: bool) -> Result<Vec<Entry>> {
        // "Today" means the local calendar day, not the UTC day: convert
        // local midnight boundaries to UTC and filter by range.
        let naive_start = chrono::Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let start = match naive_start.and_local_timezone(chrono::Local) {
            chrono::LocalResult::Single(d) => d.with_timezone(&Utc),
            chrono::LocalResult::Ambiguous(d, _) => d.with_timezone(&Utc),
            chrono::LocalResult::None => Utc::now() - chrono::Duration::hours(24),
        };
        let end = start + chrono::Duration::days(1);
        let sql = format!(
            "SELECT id, name, tags, project_id, user_id, started_at, ended_at, duration_seconds
             FROM entries
             WHERE started_at >= ?1 AND started_at < ?2{}
             ORDER BY started_at DESC",
            self.user_filter_range(is_admin)
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let start_s = start.to_rfc3339();
        let end_s = end.to_rfc3339();
        let rows = if is_admin {
            stmt.query_map([&start_s, &end_s], Self::entry_from_row)?
        } else {
            stmt.query_map(params![start_s, end_s, user_id], Self::entry_from_row)?
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

    pub fn list_by_tag(
        &self,
        tag: &str,
        days: i64,
        user_id: i64,
        is_admin: bool,
    ) -> Result<Vec<Entry>> {
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
            stmt.query_map(
                [&since, &pattern, &user_id.to_string()],
                Self::entry_from_row,
            )?
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_completed_entry(
        &self,
        name: &str,
        tags: Option<&str>,
        project_id: Option<i64>,
        started: DateTime<Utc>,
        ended: DateTime<Utc>,
        duration: i64,
        user_id: i64,
    ) -> Result<i64> {
        let duration = duration.max(0);
        self.conn.execute(
            "INSERT INTO entries (name, tags, project_id, started_at, ended_at, duration_seconds, user_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![name, tags, project_id, started.to_rfc3339(), ended.to_rfc3339(), duration, user_id],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_entry_by_id(&self, id: i64) -> Result<Option<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tags, project_id, user_id, started_at, ended_at, duration_seconds
             FROM entries WHERE id = ?1",
        )?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::entry_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    fn is_current_entry(&self, id: i64) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare("SELECT entry_id FROM current WHERE id = 1")?;
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
        if ended_dt < started_dt {
            anyhow::bail!("ended_at must not be before started_at");
        }
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
        let deleted = self
            .conn
            .execute("DELETE FROM entries WHERE id = ?1", [id])?;
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

    pub fn daily_stats(
        &self,
        days: i64,
        user_id: i64,
        is_admin: bool,
    ) -> Result<Vec<(String, i64)>> {
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

    pub fn project_stats(
        &self,
        days: i64,
        user_id: i64,
        is_admin: bool,
    ) -> Result<Vec<(String, i64)>> {
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

    pub fn invoice_entries(
        &self,
        days: i64,
        tag: Option<&str>,
        user_id: i64,
        is_admin: bool,
    ) -> Result<Vec<Entry>> {
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
                stmt.query_map(
                    [&since, &pattern, &user_id.to_string()],
                    Self::entry_from_row,
                )?
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

    pub fn list_budgets(&self) -> Result<Vec<(String, i64, i64)>> {
        // A budget keyed by project name matches entries that either belong
        // to the project row itself or carry the name in their tags.
        let mut stmt = self.conn.prepare(
            "SELECT b.project_tag, b.budget_seconds, COALESCE(SUM(e.duration_seconds), 0)
             FROM budgets b
             LEFT JOIN projects p ON p.name = b.project_tag
             LEFT JOIN entries e ON e.duration_seconds IS NOT NULL
                 AND (e.project_id = p.id OR e.tags LIKE ('%' || b.project_tag || '%'))
             GROUP BY b.project_tag",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn delete_budget(&self, project_tag: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM budgets WHERE project_tag = ?1", [project_tag])?;
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
        let mut stmt = self
            .conn
            .prepare("SELECT url, enabled, headers FROM webhook_config WHERE id = 1")?;
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
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, role FROM users ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_user(&self, name: &str) -> Result<Option<(i64, String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, role FROM users WHERE name = ?1")?;
        let mut rows = stmt.query([name])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?, row.get(2)?)))
        } else {
            Ok(None)
        }
    }

    pub fn add_project(
        &self,
        name: &str,
        client: Option<&str>,
        hourly_rate: Option<f64>,
        color: Option<&str>,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO projects (name, client, hourly_rate, color, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, client, hourly_rate, color, Utc::now().to_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_project_by_name(&self, name: &str) -> Result<Option<Project>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, client, hourly_rate, color FROM projects WHERE name = ?1")?;
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
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, client, hourly_rate, color FROM projects WHERE id = ?1")?;
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
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, client, hourly_rate, color FROM projects ORDER BY name")?;
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

    pub fn update_project(
        &self,
        id: i64,
        name: Option<&str>,
        client: Option<&str>,
        hourly_rate: Option<f64>,
        color: Option<&str>,
    ) -> Result<()> {
        let existing = self
            .get_project_by_id(id)?
            .ok_or_else(|| anyhow::anyhow!("Project {} not found", id))?;
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
        self.conn
            .execute("DELETE FROM projects WHERE id = ?1", [id])?;
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

    pub fn list_by_project(
        &self,
        project_id: i64,
        days: i64,
        user_id: i64,
        is_admin: bool,
    ) -> Result<Vec<Entry>> {
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
            stmt.query_map(
                [&since, &project_id.to_string(), &user_id.to_string()],
                Self::entry_from_row,
            )?
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn entries_for_date_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tags, project_id, user_id, started_at, ended_at, duration_seconds
             FROM entries
             WHERE started_at >= ?1 AND started_at < ?2 AND duration_seconds IS NOT NULL
             ORDER BY started_at ASC",
        )?;
        let rows = stmt.query_map(
            params![start.to_rfc3339(), end.to_rfc3339()],
            Self::entry_from_row,
        )?;
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
        let id = store
            .start_entry("test task", Some("rust"), None, 1)
            .unwrap();
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

        let started = DateTime::parse_from_rfc3339(&before.started_at)
            .unwrap()
            .with_timezone(&Utc);
        let new_ended = (started + Duration::hours(2)).to_rfc3339();
        store
            .update_entry(id, Some("new name"), Some("new"), None, Some(&new_ended))
            .unwrap();

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
        let id = store
            .insert_completed_entry("completed", Some("done"), None, started, ended, 2700, 1)
            .unwrap();
        let entry = store.get_entry_by_id(id).unwrap().unwrap();
        assert_eq!(entry.name, "completed");
        assert_eq!(entry.duration_seconds, Some(2700));
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn start_auto_stops_previous_entry() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        let first = store.start_entry("first", None, None, 1).unwrap();
        let second = store.start_entry("second", None, None, 1).unwrap();

        // First entry must have been closed, not orphaned.
        let first_entry = store.get_entry_by_id(first).unwrap().unwrap();
        assert!(first_entry.ended_at.is_some());
        assert!(first_entry.duration_seconds.unwrap() >= 0);

        // Current points at the second entry; stopping works.
        let stopped = store.stop_current(1).unwrap().unwrap();
        assert_eq!(stopped.id, second);
        assert!(store.stop_current(1).unwrap().is_none());
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn stop_recovers_orphaned_open_entry() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        // Simulate an orphan: entry with ended_at NULL but no current row
        // (e.g. current pointer was clobbered by another user/session).
        store.start_entry("orphan", None, None, 1).unwrap();
        store
            .conn
            .execute("DELETE FROM current WHERE id = 1", [])
            .unwrap();

        let stopped = store
            .stop_current(1)
            .unwrap()
            .expect("orphan should be recoverable");
        assert_eq!(stopped.name, "orphan");
        assert!(stopped.ended_at.is_some());
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn update_entry_rejects_end_before_start() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        let id = store.start_entry("timed", None, None, 1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.stop_current(1).unwrap();

        let entry = store.get_entry_by_id(id).unwrap().unwrap();
        let started = DateTime::parse_from_rfc3339(&entry.started_at)
            .unwrap()
            .with_timezone(&Utc);
        let bad_end = (started - Duration::hours(1)).to_rfc3339();
        assert!(store
            .update_entry(id, None, None, None, Some(&bad_end))
            .is_err());

        // And the entry must be unchanged.
        let after = store.get_entry_by_id(id).unwrap().unwrap();
        assert!(after.duration_seconds.unwrap() >= 0);
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn negative_insert_duration_is_clamped() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        let now = Utc::now();
        let id = store
            .insert_completed_entry("skew", None, None, now, now, -500, 1)
            .unwrap();
        let entry = store.get_entry_by_id(id).unwrap().unwrap();
        assert_eq!(entry.duration_seconds, Some(0));
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn list_today_uses_local_day() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        let now = Utc::now();
        store
            .insert_completed_entry(
                "now-ish",
                None,
                None,
                now - Duration::minutes(5),
                now,
                300,
                1,
            )
            .unwrap();
        let entries = store.list_today(1, true).unwrap();
        assert_eq!(entries.len(), 1);
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn member_sees_only_own_entries() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        let now = Utc::now();
        store
            .insert_completed_entry(
                "alice task",
                None,
                None,
                now - Duration::hours(1),
                now,
                3600,
                1,
            )
            .unwrap();
        store
            .insert_completed_entry(
                "bob task",
                None,
                None,
                now - Duration::hours(1),
                now,
                3600,
                2,
            )
            .unwrap();

        let admin_view = store.list_recent(7, 1, true).unwrap();
        assert_eq!(admin_view.len(), 2);
        let bob_view = store.list_recent(7, 2, false).unwrap();
        assert_eq!(bob_view.len(), 1);
        assert_eq!(bob_view[0].name, "bob task");
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn daily_and_project_stats_aggregate() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        let now = Utc::now();
        store
            .insert_completed_entry(
                "a",
                Some("rust"),
                None,
                now - Duration::hours(2),
                now - Duration::hours(1),
                3600,
                1,
            )
            .unwrap();
        store
            .insert_completed_entry(
                "b",
                Some("rust"),
                None,
                now - Duration::hours(1),
                now,
                1800,
                1,
            )
            .unwrap();
        store
            .insert_completed_entry(
                "c",
                Some("docs"),
                None,
                now - Duration::hours(1),
                now,
                600,
                1,
            )
            .unwrap();

        let daily = store.daily_stats(7, 1, true).unwrap();
        let total: i64 = daily.iter().map(|(_, s)| s).sum();
        assert_eq!(total, 6000);

        let projects = store.project_stats(7, 1, true).unwrap();
        let rust = projects.iter().find(|(t, _)| t == "rust").unwrap();
        assert_eq!(rust.1, 5400);
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn budget_usage_matches_project_id_and_tags() {
        let db = temp_db();
        let store = Store::open(Path::new(&db)).unwrap();
        let pid = store.add_project("vhs", None, None, None).unwrap();
        store.set_budget("vhs", 7200).unwrap();

        let now = Utc::now();
        // Entry linked via project_id (no tags) must count.
        store
            .insert_completed_entry(
                "linked",
                None,
                Some(pid),
                now - Duration::hours(2),
                now - Duration::hours(1),
                3600,
                1,
            )
            .unwrap();
        // Entry linked via tag must count.
        store
            .insert_completed_entry(
                "tagged",
                Some("vhs,ui"),
                None,
                now - Duration::hours(1),
                now,
                1800,
                1,
            )
            .unwrap();

        let budgets = store.list_budgets().unwrap();
        assert_eq!(budgets.len(), 1);
        assert_eq!(budgets[0].2, 5400);
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn migrates_legacy_schema() {
        let db = temp_db();
        // Build a pre-project_id / pre-user_id database by hand.
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute(
                "CREATE TABLE entries (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    tags TEXT,
                    started_at TEXT NOT NULL,
                    ended_at TEXT,
                    duration_seconds INTEGER
                )",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO entries (name, tags, started_at, ended_at, duration_seconds)
                 VALUES ('legacy', 'old', '2026-01-01T10:00:00+00:00', '2026-01-01T11:00:00+00:00', 3600)",
                [],
            ).unwrap();
        }
        let store = Store::open(Path::new(&db)).unwrap();
        let entries = store.list_recent(36500, 1, true).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "legacy");
        assert_eq!(entries[0].user_id, Some(1));
        let _ = fs::remove_file(&db);
    }

    #[test]
    fn concurrent_connections_can_read_and_write() {
        let db = temp_db();
        let store_a = Store::open(Path::new(&db)).unwrap();
        let store_b = Store::open(Path::new(&db)).unwrap();
        store_a.start_entry("from A", None, None, 1).unwrap();
        // Second connection sees the write and can stop it (WAL + busy_timeout).
        let stopped = store_b.stop_current(1).unwrap();
        assert!(stopped.is_some());
        assert!(store_a.list_recent(1, 1, true).unwrap().len() == 1);
        let _ = fs::remove_file(&db);
    }
}
