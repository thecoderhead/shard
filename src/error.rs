//! Structured error types for Shard subsystems.
//!
//! We use [`thiserror`] for machine-inspectable errors at subsystem boundaries
//! and [`anyhow`] at the CLI dispatch layer where free-form context is
//! preferable. Every public entry point returns `anyhow::Result` so callers
//! never need to import Shard's error enum.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)] // Phase 2/3 error variants; not yet raised from Phase 1 paths.
pub enum ShardError {
    #[error("failed to spawn PTY-hosted command `{argv:?}`: {message}")]
    PtySpawn { argv: Vec<String>, message: String },

    #[error("PTY I/O failure: {0}")]
    PtyIo(#[from] io::Error),

    #[error("terminal is not a TTY and PTY passthrough was forced")]
    NoTty,

    #[error("failed to open metrics database at {path}: {source}")]
    MetricsOpen {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },

    #[error("metrics query failed: {0}")]
    MetricsQuery(#[from] rusqlite::Error),

    #[error("VFS cache write failed at {path}: {source}")]
    VfsWrite {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("unsupported command: {0}")]
    Unsupported(&'static str),
}
