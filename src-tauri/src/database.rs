//! SQLite persistence: migrations and typed queries.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::rules::{Rule, RuleDraft};
use crate::settings::Settings;

const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001_initial.sql")];
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

pub struct Db {
    conn: Connection,
}

/// Result of opening the database, including whether a damaged file had to
/// be set aside.
pub struct OpenOutcome {
    pub db: Db,
    pub recovered_from: Option<PathBuf>,
}

impl Db {
    /// Opens (or creates) the database. A corrupt file is renamed aside and a
    /// fresh database is created; a database from a newer app version is
    /// refused so it is never damaged by a downgrade.
    pub fn open(path: &Path) -> AppResult<OpenOutcome> {
        match Self::try_open(path) {
            Ok(db) => Ok(OpenOutcome { db, recovered_from: None }),
            Err(OpenError::TooNew(v)) => Err(AppError::Database(format!(
                "This database was created by a newer version of Tucklane (schema {v}). Please update Tucklane."
            ))),
            Err(OpenError::Corrupt(reason)) => {
                log::error!("database corrupt, setting aside: {reason}");
                let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
                let aside = path.with_extension(format!("db.corrupt-{stamp}"));
                std::fs::rename(path, &aside).map_err(|e| AppError::Database(e.to_string()))?;
                for ext in ["db-wal", "db-shm"] {
                    let _ = std::fs::remove_file(path.with_extension(ext));
                }
                let db = Self::try_open(path).map_err(|e| AppError::Database(e.to_string()))?;
                Ok(OpenOutcome { db, recovered_from: Some(aside) })
            }
        }
    }

    fn try_open(path: &Path) -> Result<Db, OpenError> {
        let conn = Connection::open(path).map_err(|e| OpenError::Corrupt(e.to_string()))?;
        let db = Db { conn };
        db.configure()
            .map_err(|e| OpenError::Corrupt(e.to_string()))?;
        let ok: String = db
            .conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(|e| OpenError::Corrupt(e.to_string()))?;
        if ok != "ok" {
            return Err(OpenError::Corrupt(ok));
        }
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> AppResult<Db> {
        let db = Db {
            conn: Connection::open_in_memory()?,
        };
        db.configure()?;
        db.migrate()
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(db)
    }

    fn configure(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;",
        )
    }

    fn migrate(&self) -> Result<(), OpenError> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| OpenError::Corrupt(e.to_string()))?;
        if version > SCHEMA_VERSION {
            return Err(OpenError::TooNew(version));
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let target = i as i64 + 1;
            self.conn
                .execute_batch(&format!(
                    "BEGIN; {sql}\nPRAGMA user_version = {target}; COMMIT;"
                ))
                .map_err(|e| {
                    let _ = self.conn.execute_batch("ROLLBACK;");
                    OpenError::Corrupt(format!("migration {target} failed: {e}"))
                })?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------- settings

    pub fn load_settings(&self) -> AppResult<Settings> {
        let mut map = match serde_json::to_value(Settings::default())? {
            serde_json::Value::Object(m) => m,
            _ => unreachable!(),
        };
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (k, v) = row?;
            if map.contains_key(&k) {
                if let Ok(val) = serde_json::from_str(&v) {
                    map.insert(k, val);
                }
            }
        }
        // Fall back to defaults for any value that no longer deserializes.
        Ok(serde_json::from_value(serde_json::Value::Object(map)).unwrap_or_default())
    }

    pub fn save_settings(&self, s: &Settings) -> AppResult<()> {
        let now = now_iso();
        let map = match serde_json::to_value(s)? {
            serde_json::Value::Object(m) => m,
            _ => unreachable!(),
        };
        let tx = self.conn.unchecked_transaction()?;
        for (k, v) in map {
            tx.execute(
                "INSERT INTO settings(key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at
                 WHERE settings.value <> excluded.value",
                params![k, v.to_string(), now],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    // ------------------------------------------------------------------- roots

    /// Returns (id, canonical_path, display_path, selected_at).
    pub fn active_root(&self) -> AppResult<Option<(String, String, String, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, canonical_path, display_path, selected_at FROM authorized_roots WHERE status = 'active'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?)
    }

    pub fn set_active_root(&self, canonical: &str, display: &str) -> AppResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_iso();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE authorized_roots SET status = 'revoked', revoked_at = ?1 WHERE status = 'active'",
            params![now],
        )?;
        tx.execute(
            "INSERT INTO authorized_roots(id, canonical_path, display_path, status, selected_at) VALUES (?1, ?2, ?3, 'active', ?4)",
            params![id, canonical, display, now],
        )?;
        tx.commit()?;
        Ok(id)
    }

    pub fn revoke_root(&self) -> AppResult<()> {
        self.conn.execute(
            "UPDATE authorized_roots SET status = 'revoked', revoked_at = ?1 WHERE status = 'active'",
            params![now_iso()],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------------- rules

    fn row_to_rule(r: &Row) -> rusqlite::Result<Rule> {
        let conditions: String = r.get("conditions")?;
        let action: String = r.get("action")?;
        let mode: String = r.get("match_mode")?;
        Ok(Rule {
            id: r.get("id")?,
            name: r.get("name")?,
            description: r.get("description")?,
            priority: r.get("priority")?,
            enabled: r.get::<_, i64>("enabled")? != 0,
            match_mode: serde_json::from_value(serde_json::Value::String(mode)).unwrap_or_default(),
            conditions: serde_json::from_str(&conditions).unwrap_or_default(),
            action: serde_json::from_str(&action).unwrap_or(crate::rules::RuleAction::Review),
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
            last_triggered_at: r.get("last_triggered_at")?,
            match_count: r.get("match_count")?,
        })
    }

    pub fn list_rules(&self) -> AppResult<Vec<Rule>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM rules ORDER BY priority ASC, created_at ASC")?;
        let rules = stmt
            .query_map([], Self::row_to_rule)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rules)
    }

    pub fn get_rule(&self, id: &str) -> AppResult<Rule> {
        self.conn
            .query_row(
                "SELECT * FROM rules WHERE id = ?1",
                params![id],
                Self::row_to_rule,
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound("Rule not found.".into()))
    }

    pub fn insert_rule(&self, d: &RuleDraft) -> AppResult<Rule> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_iso();
        let next: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(priority), 0) + 1 FROM rules",
            [],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO rules(id, name, description, priority, enabled, match_mode, conditions, action, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![
                id,
                d.name,
                d.description,
                next,
                d.enabled as i64,
                match_mode_str(d),
                serde_json::to_string(&d.conditions)?,
                serde_json::to_string(&d.action)?,
                now
            ],
        )?;
        self.get_rule(&id)
    }

    pub fn update_rule(&self, id: &str, d: &RuleDraft) -> AppResult<Rule> {
        let n = self.conn.execute(
            "UPDATE rules SET name = ?2, description = ?3, enabled = ?4, match_mode = ?5, conditions = ?6, action = ?7, updated_at = ?8 WHERE id = ?1",
            params![
                id,
                d.name,
                d.description,
                d.enabled as i64,
                match_mode_str(d),
                serde_json::to_string(&d.conditions)?,
                serde_json::to_string(&d.action)?,
                now_iso()
            ],
        )?;
        if n == 0 {
            return Err(AppError::NotFound("Rule not found.".into()));
        }
        self.get_rule(id)
    }

    pub fn delete_rule(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM rules WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn reorder_rules(&self, ids: &[String]) -> AppResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        for (i, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE rules SET priority = ?2 WHERE id = ?1",
                params![id, i as i64 + 1],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn record_rule_triggered(&self, id: &str, count: i64) -> AppResult<()> {
        self.conn.execute(
            "UPDATE rules SET match_count = match_count + ?2, last_triggered_at = ?3 WHERE id = ?1",
            params![id, count, now_iso()],
        )?;
        Ok(())
    }

    // ------------------------------------------------------- sessions & ops

    pub fn create_session(
        &self,
        root_id: &str,
        kind: SessionKind,
        total: i64,
    ) -> AppResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO sessions(id, root_id, kind, status, started_at, total) VALUES (?1, ?2, ?3, 'running', ?4, ?5)",
            params![id, root_id, kind.as_str(), now_iso(), total],
        )?;
        Ok(id)
    }

    pub fn finish_session(&self, id: &str, status: &str) -> AppResult<SessionRecord> {
        self.conn.execute(
            "UPDATE sessions SET status = ?2, ended_at = ?3,
               completed = (SELECT COUNT(*) FROM operations WHERE session_id = ?1 AND state IN ('completed','undone')),
               skipped   = (SELECT COUNT(*) FROM operations WHERE session_id = ?1 AND state = 'skipped'),
               failed    = (SELECT COUNT(*) FROM operations WHERE session_id = ?1 AND state IN ('failed','recovery_required'))
             WHERE id = ?1",
            params![id, status, now_iso()],
        )?;
        self.get_session(id)
    }

    fn row_to_session(r: &Row) -> rusqlite::Result<SessionRecord> {
        Ok(SessionRecord {
            id: r.get("id")?,
            kind: SessionKind::parse(&r.get::<_, String>("kind")?),
            status: r.get("status")?,
            started_at: r.get("started_at")?,
            ended_at: r.get("ended_at")?,
            total: r.get("total")?,
            completed: r.get("completed")?,
            skipped: r.get("skipped")?,
            failed: r.get("failed")?,
            undone_at: r.get("undone_at")?,
            undoable: r.get("undoable")?,
        })
    }

    const SESSION_SELECT: &'static str = "SELECT s.*, (SELECT COUNT(*) FROM operations o WHERE o.session_id = s.id AND o.kind = 'move' AND o.state = 'completed') AS undoable FROM sessions s";

    pub fn get_session(&self, id: &str) -> AppResult<SessionRecord> {
        self.conn
            .query_row(
                &format!("{} WHERE s.id = ?1", Self::SESSION_SELECT),
                params![id],
                Self::row_to_session,
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound("Session not found.".into()))
    }

    pub fn list_sessions(
        &self,
        root_id: &str,
        limit: u32,
        offset: u32,
    ) -> AppResult<Vec<SessionRecord>> {
        let mut stmt = self.conn.prepare(&format!(
            "{} WHERE s.root_id = ?1 ORDER BY s.started_at DESC LIMIT ?2 OFFSET ?3",
            Self::SESSION_SELECT
        ))?;
        let v = stmt
            .query_map(params![root_id, limit, offset], Self::row_to_session)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    pub fn running_session_ids(&self) -> AppResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM sessions WHERE status = 'running'")?;
        let v = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(v)
    }

    pub fn op_root_id(&self, op_id: &str) -> AppResult<String> {
        Ok(self.conn.query_row(
            "SELECT root_id FROM operations WHERE id = ?1",
            params![op_id],
            |r| r.get(0),
        )?)
    }

    pub fn mark_session_undone(&self, id: &str) -> AppResult<()> {
        self.conn.execute(
            "UPDATE sessions SET undone_at = ?2 WHERE id = ?1",
            params![id, now_iso()],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_planned_op(
        &self,
        session_id: &str,
        root_id: &str,
        kind: OpKind,
        source_rel: &str,
        dest_dir_rel: &str,
        original_name: &str,
        size: Option<i64>,
        mtime_ms: Option<i64>,
        rule_id: Option<&str>,
        rule_name: Option<&str>,
        reason: Option<&str>,
        undo_of: Option<&str>,
    ) -> AppResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_iso();
        self.conn.execute(
            "INSERT INTO operations(id, session_id, root_id, kind, state, source_rel, dest_dir_rel, original_name, size, mtime_ms, rule_id, rule_name, reason, undo_of, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'planned', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
            params![id, session_id, root_id, kind.as_str(), source_rel, dest_dir_rel, original_name, size, mtime_ms, rule_id, rule_name, reason, undo_of, now],
        )?;
        Ok(id)
    }

    pub fn set_op_state(&self, id: &str, state: OpState, error: Option<&str>) -> AppResult<()> {
        self.conn.execute(
            "UPDATE operations SET state = ?2, error = COALESCE(?3, error), updated_at = ?4 WHERE id = ?1",
            params![id, state.as_str(), error, now_iso()],
        )?;
        Ok(())
    }

    /// Records the identity of the file we are about to move, before moving it.
    pub fn mark_op_in_progress(
        &self,
        id: &str,
        file_id: &str,
        size: i64,
        mtime_ms: i64,
    ) -> AppResult<()> {
        self.conn.execute(
            "UPDATE operations SET state = 'in_progress', file_id = ?2, size = ?3, mtime_ms = ?4, updated_at = ?5 WHERE id = ?1",
            params![id, file_id, size, mtime_ms, now_iso()],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_op(
        &self,
        id: &str,
        dest_rel: &str,
        final_name: &str,
        size: i64,
        mtime_ms: i64,
        file_id: &str,
        sha256: Option<&str>,
        method: &str,
    ) -> AppResult<()> {
        let now = now_iso();
        self.conn.execute(
            "UPDATE operations SET state = 'completed', dest_rel = ?2, final_name = ?3, size = ?4, mtime_ms = ?5, file_id = ?6, sha256 = ?7, method = ?8, error = NULL, updated_at = ?9, completed_at = ?9 WHERE id = ?1",
            params![id, dest_rel, final_name, size, mtime_ms, file_id, sha256, method, now],
        )?;
        Ok(())
    }

    fn row_to_op(r: &Row) -> rusqlite::Result<OperationRecord> {
        Ok(OperationRecord {
            id: r.get("id")?,
            session_id: r.get("session_id")?,
            kind: OpKind::parse(&r.get::<_, String>("kind")?),
            state: OpState::parse(&r.get::<_, String>("state")?),
            source_rel: r.get("source_rel")?,
            dest_dir_rel: r.get("dest_dir_rel")?,
            dest_rel: r.get("dest_rel")?,
            original_name: r.get("original_name")?,
            final_name: r.get("final_name")?,
            size: r.get("size")?,
            mtime_ms: r.get("mtime_ms")?,
            file_id: r.get("file_id")?,
            sha256: r.get("sha256")?,
            method: r.get("method")?,
            rule_id: r.get("rule_id")?,
            rule_name: r.get("rule_name")?,
            reason: r.get("reason")?,
            error: r.get("error")?,
            undo_of: r.get("undo_of")?,
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
            completed_at: r.get("completed_at")?,
        })
    }

    pub fn get_op(&self, id: &str) -> AppResult<OperationRecord> {
        self.conn
            .query_row(
                "SELECT * FROM operations WHERE id = ?1",
                params![id],
                Self::row_to_op,
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound("Operation not found.".into()))
    }

    pub fn ops_in_state(&self, state: OpState) -> AppResult<Vec<OperationRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM operations WHERE state = ?1 ORDER BY created_at")?;
        let v = stmt
            .query_map(params![state.as_str()], Self::row_to_op)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    pub fn ops_for_session(&self, session_id: &str) -> AppResult<Vec<OperationRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM operations WHERE session_id = ?1 ORDER BY created_at, rowid")?;
        let v = stmt
            .query_map(params![session_id], Self::row_to_op)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    pub fn query_ops(
        &self,
        root_id: &str,
        q: &HistoryQuery,
    ) -> AppResult<(Vec<OperationRecord>, i64)> {
        let mut where_sql = vec!["root_id = ?1".to_string()];
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(root_id.to_string())];
        type Args = Vec<Box<dyn rusqlite::ToSql>>;
        fn add(where_sql: &mut Vec<String>, args: &mut Args, clause: &str, v: String) {
            args.push(Box::new(v));
            where_sql.push(clause.replace('?', &format!("?{}", args.len())));
        }
        if let Some(s) = q.search.as_deref().filter(|s| !s.trim().is_empty()) {
            let like = format!(
                "%{}%",
                s.trim()
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            );
            args.push(Box::new(like));
            let n = args.len();
            where_sql.push(format!(
                "(original_name LIKE ?{n} ESCAPE '\\' OR source_rel LIKE ?{n} ESCAPE '\\' OR dest_rel LIKE ?{n} ESCAPE '\\' OR final_name LIKE ?{n} ESCAPE '\\')"
            ));
        }
        if let Some(s) = q.state.clone().filter(|s| !s.is_empty()) {
            add(&mut where_sql, &mut args, "state = ?", s);
        }
        if let Some(k) = q.kind.clone().filter(|s| !s.is_empty()) {
            add(&mut where_sql, &mut args, "kind = ?", k);
        }
        if let Some(s) = q.session_id.clone().filter(|s| !s.is_empty()) {
            add(&mut where_sql, &mut args, "session_id = ?", s);
        }
        if let Some(f) = q.from.clone().filter(|s| !s.is_empty()) {
            add(&mut where_sql, &mut args, "created_at >= ?", f);
        }
        if let Some(t) = q.to.clone().filter(|s| !s.is_empty()) {
            add(&mut where_sql, &mut args, "created_at <= ?", t);
        }
        let where_clause = where_sql.join(" AND ");
        let params_ref: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM operations WHERE {where_clause}"),
            params_ref.as_slice(),
            |r| r.get(0),
        )?;
        let limit = q.limit.unwrap_or(100).min(1000);
        let offset = q.offset.unwrap_or(0);
        let mut stmt = self.conn.prepare(&format!(
            "SELECT * FROM operations WHERE {where_clause} ORDER BY created_at DESC, rowid DESC LIMIT {limit} OFFSET {offset}"
        ))?;
        let v = stmt
            .query_map(params_ref.as_slice(), Self::row_to_op)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((v, total))
    }

    /// Operations whose source or destination is `rel` (for per-file history).
    pub fn ops_for_file(&self, root_id: &str, rel: &str) -> AppResult<Vec<OperationRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM operations WHERE root_id = ?1 AND (source_rel = ?2 COLLATE NOCASE OR dest_rel = ?2 COLLATE NOCASE) ORDER BY created_at DESC LIMIT 50",
        )?;
        let v = stmt
            .query_map(params![root_id, rel], Self::row_to_op)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    pub fn op_counts(&self, root_id: &str) -> AppResult<(i64, i64, i64)> {
        Ok(self.conn.query_row(
            "SELECT
               COALESCE(SUM(CASE WHEN kind = 'move' AND state = 'completed' THEN 1 ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN state IN ('failed', 'recovery_required') THEN 1 ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN kind = 'move' AND state IN ('completed','undone') THEN 1 ELSE 0 END), 0)
             FROM operations WHERE root_id = ?1",
            params![root_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    }

    /// Recent failed operations that have not been resolved by a later success.
    pub fn unresolved_failures(&self, root_id: &str) -> AppResult<i64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM operations o WHERE o.root_id = ?1 AND o.state IN ('failed','recovery_required')
               AND NOT EXISTS (SELECT 1 FROM operations p WHERE p.source_rel = o.source_rel AND p.created_at > o.created_at AND p.state IN ('completed','undone'))
               AND o.created_at >= datetime('now', '-7 days')",
            params![root_id],
            |r| r.get(0),
        )?)
    }

    // ------------------------------------------------------------------ events

    pub fn log_event(&self, severity: &str, kind: &str, message: &str, session_id: Option<&str>) {
        if let Err(e) = self.conn.execute(
            "INSERT INTO app_events(severity, kind, message, session_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![severity, kind, message, session_id, now_iso()],
        ) {
            log::error!("failed to record event: {e}");
        }
    }

    pub fn list_events(
        &self,
        limit: u32,
        offset: u32,
        search: Option<&str>,
        severity: Option<&str>,
    ) -> AppResult<Vec<EventRecord>> {
        let like = search.filter(|s| !s.is_empty()).map(|s| format!("%{s}%"));
        let mut stmt = self.conn.prepare(
            "SELECT id, severity, kind, message, session_id, created_at FROM app_events
             WHERE (?3 IS NULL OR message LIKE ?3) AND (?4 IS NULL OR severity = ?4)
             ORDER BY id DESC LIMIT ?1 OFFSET ?2",
        )?;
        let v = stmt
            .query_map(
                params![
                    limit.min(1000),
                    offset,
                    like,
                    severity.filter(|s| !s.is_empty())
                ],
                |r| {
                    Ok(EventRecord {
                        id: r.get(0)?,
                        severity: r.get(1)?,
                        kind: r.get(2)?,
                        message: r.get(3)?,
                        session_id: r.get(4)?,
                        created_at: r.get(5)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    /// Deletes history older than `days`. Operations that are still undoable
    /// are kept so undo keeps working for files that were never touched again.
    pub fn prune_history(&self, days: u32) -> AppResult<()> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days as i64))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        self.conn.execute(
            "DELETE FROM app_events WHERE created_at < ?1",
            params![cutoff],
        )?;
        self.conn.execute(
            "DELETE FROM sessions WHERE started_at < ?1 AND status <> 'running'
               AND NOT EXISTS (SELECT 1 FROM operations o WHERE o.session_id = sessions.id AND o.state IN ('completed','in_progress','recovery_required') AND o.kind = 'move')",
            params![cutoff],
        )?;
        Ok(())
    }

    // -------------------------------------------------------------- file index

    pub fn replace_file_index(
        &self,
        root_id: &str,
        rows: &[(String, u64, i64, Option<String>, &str)],
    ) -> AppResult<()> {
        let now = now_iso();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM file_index WHERE root_id = ?1",
            params![root_id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO file_index(root_id, rel_path, size, mtime_ms, category, status, last_scanned_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for (rel, size, mtime, cat, status) in rows {
                stmt.execute(params![root_id, rel, *size as i64, mtime, cat, status, now])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// (files, bytes, ready, pending, review, last_scanned_at)
    pub fn index_stats(
        &self,
        root_id: &str,
    ) -> AppResult<(i64, i64, i64, i64, i64, Option<String>)> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(size), 0),
                    COALESCE(SUM(status = 'ready'), 0), COALESCE(SUM(status IN ('pending','locked')), 0), COALESCE(SUM(status = 'review'), 0),
                    MAX(last_scanned_at)
             FROM file_index WHERE root_id = ?1",
            params![root_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )?)
    }

    // -------------------------------------------------------------- duplicates

    pub fn replace_duplicate_groups(
        &self,
        root_id: &str,
        groups: &[DuplicateGroup],
    ) -> AppResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM duplicate_groups WHERE root_id = ?1",
            params![root_id],
        )?;
        for g in groups {
            tx.execute(
                "INSERT INTO duplicate_groups(id, root_id, hash, size, members, scanned_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![g.id, root_id, g.hash, g.size as i64, serde_json::to_string(&g.members)?, g.scanned_at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_duplicate_groups(&self, root_id: &str) -> AppResult<Vec<DuplicateGroup>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, hash, size, members, scanned_at FROM duplicate_groups WHERE root_id = ?1 ORDER BY size DESC",
        )?;
        let v = stmt
            .query_map(params![root_id], |r| {
                let members: String = r.get(3)?;
                Ok(DuplicateGroup {
                    id: r.get(0)?,
                    hash: r.get(1)?,
                    size: r.get::<_, i64>(2)? as u64,
                    members: serde_json::from_str(&members).unwrap_or_default(),
                    scanned_at: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    // -------------------------------------------------------------- exclusions

    pub fn add_exclusion(&self, root_id: &str, rel: &str, reason: &str) -> AppResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO exclusions(root_id, rel_path, reason, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![root_id, rel, reason, now_iso()],
        )?;
        Ok(())
    }

    pub fn remove_exclusion(&self, root_id: &str, rel: &str) -> AppResult<()> {
        self.conn.execute(
            "DELETE FROM exclusions WHERE root_id = ?1 AND rel_path = ?2 COLLATE NOCASE",
            params![root_id, rel],
        )?;
        Ok(())
    }

    pub fn list_exclusions(&self, root_id: &str) -> AppResult<Vec<ExclusionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT rel_path, reason, created_at FROM exclusions WHERE root_id = ?1 ORDER BY created_at DESC",
        )?;
        let v = stmt
            .query_map(params![root_id], |r| {
                Ok(ExclusionRecord {
                    rel_path: r.get(0)?,
                    reason: r.get(1)?,
                    created_at: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    // ------------------------------------------------------------- saved plans

    pub fn save_plan(&self, root_id: &str, items_json: &str, count: i64) -> AppResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO saved_plans(id, root_id, created_at, item_count, items) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, root_id, now_iso(), count, items_json],
        )?;
        Ok(id)
    }

    /// Returns (id, created_at, item_count).
    pub fn list_saved_plans(&self, root_id: &str) -> AppResult<Vec<(String, String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, item_count FROM saved_plans WHERE root_id = ?1 ORDER BY created_at DESC LIMIT 20",
        )?;
        let v = stmt
            .query_map(params![root_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(v)
    }

    pub fn get_saved_plan(&self, id: &str) -> AppResult<String> {
        self.conn
            .query_row(
                "SELECT items FROM saved_plans WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::NotFound("Saved plan not found.".into()))
    }

    pub fn delete_saved_plan(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM saved_plans WHERE id = ?1", params![id])?;
        Ok(())
    }
}

fn match_mode_str(d: &RuleDraft) -> &'static str {
    match d.match_mode {
        crate::rules::MatchMode::All => "all",
        crate::rules::MatchMode::Any => "any",
    }
}

enum OpenError {
    Corrupt(String),
    TooNew(i64),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::Corrupt(s) => write!(f, "{s}"),
            OpenError::TooNew(v) => write!(f, "schema version {v} is newer than supported"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_and_roundtrips_settings() {
        let db = Db::open_in_memory().unwrap();
        let mut s = db.load_settings().unwrap();
        assert_eq!(s, Settings::default());
        s.auto_organize = true;
        s.settle_seconds = 30;
        db.save_settings(&s).unwrap();
        assert_eq!(db.load_settings().unwrap(), s);
    }

    #[test]
    fn single_active_root() {
        let db = Db::open_in_memory().unwrap();
        let a = db.set_active_root("\\\\?\\D:\\A", "D:\\A").unwrap();
        let b = db.set_active_root("\\\\?\\D:\\B", "D:\\B").unwrap();
        assert_ne!(a, b);
        assert_eq!(db.active_root().unwrap().unwrap().0, b);
    }

    #[test]
    fn corrupt_database_is_set_aside() {
        let dir = crate::testutil::sandbox();
        let path = dir.path().join("tucklane.db");
        std::fs::write(
            &path,
            b"this is definitely not a sqlite database file at all....",
        )
        .unwrap();
        let out = Db::open(&path).unwrap();
        assert!(out.recovered_from.is_some());
        assert!(out.recovered_from.unwrap().exists());
        assert_eq!(out.db.load_settings().unwrap(), Settings::default());
    }

    #[test]
    fn refuses_newer_schema() {
        let dir = crate::testutil::sandbox();
        let path = dir.path().join("tucklane.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch("PRAGMA user_version = 999;").unwrap();
        }
        assert!(matches!(Db::open(&path), Err(AppError::Database(_))));
    }
}
