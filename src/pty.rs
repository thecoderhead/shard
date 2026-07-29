//! PTY-hosted command execution — the heart of Shard.
//!
//! [`ShardPTYBridge`] spawns a command inside a pseudo-terminal (via
//! `portable-pty`, which uses ConPTY on Windows and Unix98 PTYs on
//! Linux/macOS), tees its raw output on two paths:
//!
//! * **Path A — Zero-copy passthrough:** raw bytes stream to the developer's
//!   real stdout with sub-millisecond latency. ANSI escapes, cursor movements,
//!   and interactive prompts all reach the terminal unchanged.
//! * **Path B — Analysis fan-out:** the same bytes are cloned into a
//!   [`tokio::sync::mpsc`] channel per subscriber (VFS raw-log cache today;
//!   compaction engines in Phase 2).
//!
//! The bridge also propagates the child's exit code, forwards user stdin into
//! the PTY master, and handles terminal resize where the platform supports it.
//!
//! Interactive TTY detection: if the parent's stdout is itself a TTY, we still
//! do the raw tee (Path A is always on) but flag the run as `passthrough_only`
//! so Phase 2 compaction can skip work for `vim`/`less`/wizard sessions.

pub mod bridge;

#[allow(unused_imports)]
pub use bridge::{RunOutcome, ShardPTYBridge};
