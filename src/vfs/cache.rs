//! Raw-log VFS cache — Path B sink #1.
//!
//! Every PTY run streams its raw child-side bytes (untouched, including ANSI
//! escapes) into `.shard/logs/<uuid>.log`. Compaction Phase 2 can then reference
//! the cached artifact by path in the summary footer so AI agents may `cat` the
//! full log without re-executing state-changing commands.
//!
//! Rotation is coarse and deterministic: after each write completes, if the
//! directory holds more than [`crate::vfs::LOG_RETENTION_COUNT`] files, the
//! oldest by mtime are removed. Racing writers may briefly exceed the cap; that
//! is acceptable given the safety-vs-fidelity tradeoff.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc::UnboundedReceiver;
use uuid::Uuid;

use crate::vfs::LOG_RETENTION_COUNT;

/// Handle owning the log file destination for a single run.
pub struct RawLog {
    pub path: PathBuf,
    #[allow(dead_code)] // surfaced back through RunOutcome for compaction footer.
    pub run_id: Uuid,
}

impl RawLog {
    /// Allocate a new log path under `logs_dir`. Directory must already exist
    /// (see [`crate::vfs::ensure_root`]).
    pub fn allocate(logs_dir: &Path, run_id: Uuid) -> Self {
        let path = logs_dir.join(format!("{run_id}.log"));
        Self { path, run_id }
    }
}

/// Consume raw byte chunks from `rx` and stream them into `path`. Terminates
/// when either the sender is dropped or `cancel` fires. Returns total bytes
/// written. The cancellation path is critical on Windows ConPTY, where the
/// PTY reader thread can occasionally get stuck on a blocking read after
/// child exit — without an external cancel the sink task would hang forever
/// waiting for the last sender clone to drop.
pub async fn drain_to_file(
    path: PathBuf,
    mut rx: UnboundedReceiver<bytes::Bytes>,
    cancel: std::sync::Arc<tokio::sync::Notify>,
) -> Result<u64> {
    let file = File::create(&path)
        .await
        .with_context(|| format!("failed to create raw log at {}", path.display()))?;
    let mut writer = BufWriter::with_capacity(64 * 1024, file);
    let mut total: u64 = 0;
    let cancel_notified = cancel.notified();
    tokio::pin!(cancel_notified);
    loop {
        tokio::select! {
            biased;
            msg = rx.recv() => match msg {
                Some(chunk) => {
                    writer.write_all(&chunk).await
                        .with_context(|| format!("write to {} failed", path.display()))?;
                    total = total.saturating_add(chunk.len() as u64);
                }
                None => break,
            },
            _ = &mut cancel_notified => {
                // Drain any remaining buffered chunks synchronously before
                // exiting — those are bytes that arrived before the reader
                // got stuck and we still want to persist them.
                while let Ok(chunk) = rx.try_recv() {
                    writer.write_all(&chunk).await
                        .with_context(|| format!("write to {} failed", path.display()))?;
                    total = total.saturating_add(chunk.len() as u64);
                }
                break;
            }
        }
    }
    writer
        .flush()
        .await
        .with_context(|| format!("flush of {} failed", path.display()))?;
    Ok(total)
}

/// Enforce the rotating cap on `logs_dir`. Removes oldest `.log` files (by
/// modified time) until the count is within [`LOG_RETENTION_COUNT`]. Errors
/// during eviction are logged but never propagated — rotation is best-effort so
/// a locked file on Windows can't stall a run.
pub fn rotate(logs_dir: &Path) {
    let Ok(entries) = fs::read_dir(logs_dir) else {
        return;
    };
    let mut logs: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("log"))
                .unwrap_or(false)
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let mtime = meta.modified().ok()?;
            Some((mtime, e.path()))
        })
        .collect();

    if logs.len() <= LOG_RETENTION_COUNT {
        return;
    }

    logs.sort_by_key(|(mtime, _)| *mtime);
    let victims = logs.len().saturating_sub(LOG_RETENTION_COUNT);
    for (_, path) in logs.into_iter().take(victims) {
        if let Err(err) = fs::remove_file(&path) {
            tracing::warn!(target: "shard::vfs", ?path, %err, "failed to evict log during rotation");
        }
    }
}
