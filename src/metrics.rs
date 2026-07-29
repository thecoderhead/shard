//! SQLite metrics store rooted at `.shard/metrics.db`.
//!
//! One row per `shard <cmd>` invocation. Schema is intentionally denormalised
//! for query speed — analytics dashboards scan `runs` directly with simple
//! `GROUP BY DATE(started_at)` aggregates. Migrations are additive and idempotent.
//!
//! Provides a graceful degradation wrapper so a locked or corrupted DB never
//! blocks a command from completing.

pub mod db;

pub use db::{MetricsDb, RunRecord};

/// Open the metrics database at `path`, returning `None` on failure so the
/// command can proceed without metrics.
pub fn open_or_degrade(path: &std::path::Path) -> Option<MetricsDb> {
    match MetricsDb::open(path) {
        Ok(db) => Some(db),
        Err(e) => {
            tracing::warn!(target: "shard::metrics", %e, "metrics DB unavailable, degrading gracefully");
            None
        }
    }
}
