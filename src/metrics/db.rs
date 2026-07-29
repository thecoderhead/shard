//! SQLite backend for the `runs` journal.
//!
//! Thread-model: the connection is opened on demand per operation. Metrics
//! writes happen exactly once per run (after the child exits), so pooling would
//! be wasted complexity. Reads (`shard gain`) are read-only and equally
//! infrequent. `rusqlite` with `bundled` avoids a runtime SQLite dependency on
//! Windows.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags, params};
use serde::Serialize;
use uuid::Uuid;

/// Wrapper around a `rusqlite` [`Connection`] scoped to a single metrics DB.
pub struct MetricsDb {
    conn: Connection,
    #[allow(dead_code)] // exposed via `path()` for tooling / diagnostics.
    path: PathBuf,
}

/// One row in `runs`. Populated by [`crate::pty::ShardPTYBridge`] after the
/// child exits.
#[derive(Debug, Clone)]
pub struct RunRecord {
    pub id: Uuid,
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub wall_ms: u64,
    pub exit_code: i32,
    pub raw_bytes: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub savings_pct: f64,
    pub log_path: PathBuf,
    pub intent: Option<String>,
    pub is_tty: bool,
}

/// Aggregate view over the entire `runs` table, backing `shard gain`.
#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub total_commands: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub tokens_saved: u64,
    pub savings_pct: f64,
    pub total_exec_ms: u64,
    pub avg_exec_ms: u64,
}

impl MetricsDb {
    /// Open (or create + migrate) the metrics database at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags)
            .with_context(|| format!("open metrics DB at {}", path.display()))?;
        // WAL: writers don't block readers; matters when `shard gain` runs
        // concurrently with a long-lived AI agent session.
        conn.pragma_update(None, "journal_mode", "WAL")
            .context("set journal_mode=WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .context("set synchronous=NORMAL")?;
        let db = Self {
            conn,
            path: path.to_path_buf(),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS runs (
                id            TEXT PRIMARY KEY,
                argv          TEXT NOT NULL,
                cwd           TEXT NOT NULL,
                started_at    TEXT NOT NULL,
                finished_at   TEXT NOT NULL,
                wall_ms       INTEGER NOT NULL,
                exit_code     INTEGER NOT NULL,
                raw_bytes     INTEGER NOT NULL,
                tokens_in     INTEGER NOT NULL,
                tokens_out    INTEGER NOT NULL,
                savings_pct   REAL NOT NULL,
                log_path      TEXT NOT NULL,
                intent        TEXT,
                is_tty        INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_runs_started_at ON runs(started_at);
            CREATE INDEX IF NOT EXISTS idx_runs_argv0     ON runs(argv);

            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY
            );
            INSERT OR IGNORE INTO schema_version(version) VALUES (1);
            "#,
        )?;
        Ok(())
    }

    /// Insert one record. Row-per-run so contention is negligible.
    pub fn insert_run(&self, r: &RunRecord) -> Result<()> {
        let argv_json = serde_json::to_string(&r.argv)?;
        self.conn.execute(
            r#"
            INSERT INTO runs (
                id, argv, cwd, started_at, finished_at, wall_ms,
                exit_code, raw_bytes, tokens_in, tokens_out,
                savings_pct, log_path, intent, is_tty
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                r.id.to_string(),
                argv_json,
                r.cwd.to_string_lossy(),
                r.started_at.to_rfc3339(),
                r.finished_at.to_rfc3339(),
                r.wall_ms as i64,
                r.exit_code,
                r.raw_bytes as i64,
                r.tokens_in as i64,
                r.tokens_out as i64,
                r.savings_pct,
                r.log_path.to_string_lossy(),
                r.intent,
                r.is_tty as i64,
            ],
        )?;
        Ok(())
    }

    /// Aggregate summary across all rows (Phase 1 `shard gain`).
    pub fn summary(&self) -> Result<RunSummary> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                COUNT(*)                     AS total,
                COALESCE(SUM(tokens_in),  0) AS in_,
                COALESCE(SUM(tokens_out), 0) AS out_,
                COALESCE(SUM(wall_ms),    0) AS wall
            FROM runs
            "#,
        )?;
        let (total, tokens_in, tokens_out, total_exec_ms): (i64, i64, i64, i64) =
            stmt.query_row([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?;

        let tokens_in = tokens_in.max(0) as u64;
        let tokens_out = tokens_out.max(0) as u64;
        let tokens_saved = tokens_in.saturating_sub(tokens_out);
        let savings_pct = if tokens_in == 0 {
            0.0
        } else {
            (tokens_saved as f64 / tokens_in as f64) * 100.0
        };
        let total = total.max(0) as u64;
        let total_exec_ms = total_exec_ms.max(0) as u64;
        let avg_exec_ms = if total == 0 { 0 } else { total_exec_ms / total };

        Ok(RunSummary {
            total_commands: total,
            tokens_in,
            tokens_out,
            tokens_saved,
            savings_pct,
            total_exec_ms,
            avg_exec_ms,
        })
    }

    /// Most-recent `limit` runs, newest first. Backs `shard gain --history`.
    pub fn recent(&self, limit: u32) -> Result<Vec<HistoryRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, started_at, argv, savings_pct,
                   tokens_in, tokens_out, wall_ms, exit_code, log_path
            FROM runs
            ORDER BY started_at DESC
            LIMIT ?
            "#,
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let id: String = row.get(0)?;
            let started_at: String = row.get(1)?;
            let argv_json: String = row.get(2)?;
            let savings_pct: f64 = row.get(3)?;
            let tokens_in: i64 = row.get(4)?;
            let tokens_out: i64 = row.get(5)?;
            let wall_ms: i64 = row.get(6)?;
            let exit_code: i32 = row.get(7)?;
            let log_path: String = row.get(8)?;
            Ok(HistoryRow {
                id,
                started_at,
                argv_json,
                savings_pct,
                tokens_saved: tokens_in.saturating_sub(tokens_out).max(0) as u64,
                tokens_in: tokens_in.max(0) as u64,
                tokens_out: tokens_out.max(0) as u64,
                wall_ms: wall_ms.max(0) as u64,
                exit_code,
                log_path,
            })
        })?;
        let mut out = Vec::with_capacity(limit as usize);
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Daily token savings for the last `days` days, oldest first. Backs
    /// `shard gain --graph`.
    pub fn daily(&self, days: u32) -> Result<Vec<DailyRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT DATE(started_at)         AS day,
                   COUNT(*)                 AS runs,
                   COALESCE(SUM(tokens_in),  0),
                   COALESCE(SUM(tokens_out), 0)
            FROM runs
            WHERE DATE(started_at) >= DATE('now', ?)
            GROUP BY day
            ORDER BY day ASC
            "#,
        )?;
        let offset = format!("-{days} day");
        let rows = stmt.query_map(params![offset], |row| {
            let day: String = row.get(0)?;
            let runs: i64 = row.get(1)?;
            let tokens_in: i64 = row.get(2)?;
            let tokens_out: i64 = row.get(3)?;
            Ok(DailyRow {
                day,
                runs: runs.max(0) as u64,
                tokens_in: tokens_in.max(0) as u64,
                tokens_out: tokens_out.max(0) as u64,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// VACUUM the database to reclaim disk space after log deletion.
    pub fn vacuum(&self) -> Result<()> {
        self.conn.execute_batch("VACUUM;")?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct HistoryRow {
    pub id: String,
    pub started_at: String,
    pub argv_json: String,
    pub savings_pct: f64,
    pub tokens_saved: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub wall_ms: u64,
    pub exit_code: i32,
    pub log_path: String,
}

#[derive(Debug, Clone)]
pub struct DailyRow {
    pub day: String,
    pub runs: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample(id: Uuid) -> RunRecord {
        RunRecord {
            id,
            argv: vec!["echo".into(), "hello".into()],
            cwd: PathBuf::from("/tmp"),
            started_at: Utc::now(),
            finished_at: Utc::now(),
            wall_ms: 42,
            exit_code: 0,
            raw_bytes: 128,
            tokens_in: 32,
            tokens_out: 6,
            savings_pct: 81.25,
            log_path: PathBuf::from("/tmp/log"),
            intent: None,
            is_tty: false,
        }
    }

    #[test]
    fn round_trip_run() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("metrics.db");
        let db = MetricsDb::open(&db_path).unwrap();
        db.insert_run(&sample(Uuid::new_v4())).unwrap();
        db.insert_run(&sample(Uuid::new_v4())).unwrap();

        let sum = db.summary().unwrap();
        assert_eq!(sum.total_commands, 2);
        assert_eq!(sum.tokens_in, 64);
        assert_eq!(sum.tokens_out, 12);
        assert_eq!(sum.tokens_saved, 52);
        assert!((sum.savings_pct - 81.25).abs() < 1e-9);
    }
}
