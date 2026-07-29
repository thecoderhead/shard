//! Shard — PTY-aware CLI proxy that compresses noisy terminal output before it
//! reaches AI coding agents. Phase 1: Core Stream Interceptor & Project
//! Scaffolding.
//!
//! Entry point delegates to [`cli::dispatch`] which routes CLI subcommands to
//! the appropriate subsystem. All heavy lifting lives in the submodules:
//!
//! * [`pty`]       — [`pty::ShardPTYBridge`], dual-stream tee, PTY lifecycle.
//! * [`vte_tok`]   — ANSI-preserving byte tokenizer skeleton.
//! * [`vfs`]       — Raw log VFS cache with rotation.
//! * [`metrics`]   — SQLite-backed run journal at `.shard/metrics.db`.
//! * [`tokens`]    — Approximate token counter (Phase 1 heuristic, upgradable).
//! * [`analytics`] — `shard stats` reporting surface.
//! * [`intent`]    — `SHARD_INTENT` environment parsing.
//! * [`doctor`]    — `shard check` environment sanity checks.
//! * [`hooks`]     — `shard init` hook installer (Phase 3 placeholder).

mod analytics;
mod bench;
mod cli;
mod clean;
mod compact;
mod distill;
mod doctor;
mod error;
mod hooks;
mod hooks_impl;
mod intent;
mod log_cmd;
mod metrics;
mod output_format;
mod pty;
mod read_cmd;
mod redact;
mod tokens;
mod ui;
mod vfs;
mod vte_tok;

use std::process::ExitCode;

fn main() -> ExitCode {
    // Wire tracing early so PTY spawn errors are visible when RUST_LOG is set.
    // Default is WARN so an idle `shard <cmd>` stays silent on the raw stream.
    let env_filter = tracing_subscriber::EnvFilter::try_from_env("SHARD_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .without_time()
        .try_init();

    match cli::dispatch() {
        Ok(code) => code,
        Err(err) => {
            use crossterm::style::Stylize;
            eprintln!(
                "{} {} {:#}",
                "┃".dim(),
                "shard error".red().bold(),
                err,
            );
            ExitCode::from(1)
        }
    }
}
