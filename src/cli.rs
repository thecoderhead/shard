//! Clap-driven CLI router.
//! Handles `shard init`, `shard stats`, verbosity flags, and the external-subcommand
//! catch-all that forwards arbitrary commands through the PTY bridge.

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use crate::analytics;
use crate::analytics::Scope;
use crate::bench;
use crate::clean;
use crate::distill;
use crate::doctor;
use crate::hooks;
use crate::log_cmd;
use crate::pty::ShardPTYBridge;
use crate::read_cmd;

const LONG_ABOUT: &str = "\
╔══════════════════════════════════════════════════════════════════════╗
║  SHARD — TERMINAL NOISE CANCELLATION PROTOCOL                       ║
║  SYSTEM OVERRIDE: ACTIVE  │  NODE: CLI  │  ENCODING: UTF-8          ║
╚══════════════════════════════════════════════════════════════════════╝

Shard deploys a PTY-level interception layer between your shell and
the AI agent's context window. Every command spawns inside a real
pseudo-terminal; raw byte streams tee to your display with <1ms
latency while the analysis pipeline compresses noisy terminal output
into a token-efficient signal.

In piped mode — the AI's native protocol — Shard strips ANSI noise,
classifies output by structural archetype (tabular, linear-log, tree),
and compresses the stream by 60–90% before it reaches the context
window. Result: leaner prompts, faster responses, lower token burn.

SUBCOMMAND MATRIX:
  stats    Decode the metrics database — savings reports, daily graphs,
           and command history in JSON or HUD display. (alias: gain)
  init     Bootstrap the interception mesh: install shell aliases and
           agent hooks that route noisy commands through Shard.
  check    Run system diagnostics — verify PTY availability, hook
           integrity, and metrics DB health. (alias: doctor)
  exec     Direct command injection into the PTY substrate (default).
  clean    Purge cached logs and defragment the metrics database.
  log      Retrieve a raw or re-compacted session log by run ID.
  distill  Compress conversation history for AI consumption —
           reads from stdin or file, splits by paragraph or line window.
  cat      Compact a file for AI ingestion — token-efficient read
           with optional head limit and intent hints.
  bench    Micro-benchmark the compaction engine against a synthetic
           corpus.

ENHANCEMENTS INBOUND:
  • tiktoken-backed exact token accounting (available via `tokens::new_counter()`)
  • Streaming compaction — incremental windowed output during execution
  • Global metrics roll-up across projects (`shard stats --scope global`)
  • Claude Code / Cursor / Windsurf PreToolUse hooks (`shard init` now writes JSON)";

#[derive(Parser)]
#[command(
    name = "shard",
    version,
    about = "◈ SHARD ◈  terminal noise cancellation protocol",
    long_about = LONG_ABOUT,
    disable_help_subcommand = true,
    trailing_var_arg = false,
)]
struct Cli {
    /// Verbosity level (-v, -vv, -vvv). Also honoured via SHARD_LOG.
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Decode the metrics database — token savings and efficiency reports.
    #[command(alias = "gain")]
    Stats(GainArgs),

    /// Bootstrap the interception mesh: shell aliases and agent hooks.
    Init(InitArgs),

    /// Run system diagnostics — verify PTY, hooks, and metrics DB health.
    #[command(alias = "doctor")]
    Check,

    /// Inject a command through the PTY interception substrate.
    #[command(alias = "run")]
    Exec {
        /// The command and its arguments.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true, num_args = 1..)]
        argv: Vec<String>,
    },

    /// Purge cached logs and defragment the metrics database.
    Clean,

    /// Retrieve a session log by UUID prefix — raw or re-compacted.
    Log {
        /// UUID prefix to search for.
        prefix: String,
        /// Print raw bytes instead of re-compacting.
        #[arg(long)]
        raw: bool,
        /// Re-compact with a specific intent (e.g. "debug:test-failure").
        #[arg(long)]
        intent: Option<String>,
    },

    /// Compress conversation history for AI consumption — stdin or file.
    Distill {
        /// Optional file path to distill (reads from stdin if omitted).
        file: Option<String>,
        /// Compaction intent hint (e.g. "debug:test-failure").
        #[arg(long)]
        intent: Option<String>,
        /// Suppress the savings footer.
        #[arg(long)]
        quiet: bool,
        /// Window size in lines (0 = paragraph boundaries).
        #[arg(long, default_value_t = 0)]
        window: usize,
    },

    /// Compact a file for AI ingestion — token-efficient read with intent.
    #[command(name = "cat", alias = "read")]
    Cat {
        /// File path to compact.
        path: String,
        /// Compaction intent hint.
        #[arg(long)]
        intent: Option<String>,
        /// Suppress the savings footer.
        #[arg(long)]
        quiet: bool,
        /// Show only the first N lines.
        #[arg(long)]
        head: Option<usize>,
    },

    /// Micro-benchmark the compaction engine against a synthetic corpus.
    Bench,

    /// External command passthrough — any unrecognised command is proxied.
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Debug, clap::Args)]
struct GainArgs {
    /// Recent command history instead of the aggregate summary.
    #[arg(long)]
    history: bool,

    /// ASCII graph of daily savings.
    #[arg(long)]
    graph: bool,

    /// Scope for aggregation — project-local (default) or global across all projects.
    #[arg(long, value_enum, default_value_t = Scope::Project)]
    scope: Scope,

    /// Number of days for --graph / --since filters.
    #[arg(long, default_value_t = 30)]
    since: u32,

    /// Rows for --history.
    #[arg(long, default_value_t = 20)]
    limit: u32,

    /// Machine-readable JSON output.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, clap::Args)]
struct InitArgs {
    /// Install the hook globally (all supported agents on this machine).
    #[arg(short = 'g', long)]
    global: bool,

    /// Show currently-installed hooks instead of installing.
    #[arg(long)]
    show: bool,

    /// Uninstall previously installed hooks.
    #[arg(long)]
    uninstall: bool,

    /// Enable context-pruning hook (`shard distill` for conversation history).
    #[arg(long)]
    context_prune: bool,

    /// Enable file-read compression hook (`shard cat` for AI file reads).
    #[arg(long)]
    cat_compress: bool,
}

/// Public entry point invoked from `main.rs`.
pub fn dispatch() -> Result<ExitCode> {
    let cli = Cli::parse();

    match cli.command {
        Command::Stats(args) => {
            analytics::run(analytics::Options {
                history: args.history,
                graph: args.graph,
                since_days: args.since,
                history_limit: args.limit,
                json: matches!(args.format, OutputFormat::Json),
                scope: args.scope,
            })
            .context("stats command failed")?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Init(args) => hooks::run(hooks::InitOptions {
            global: args.global,
            show: args.show,
            uninstall: args.uninstall,
            context_prune: args.context_prune,
            cat_compress: args.cat_compress,
        })
        .map(|_| ExitCode::SUCCESS),
        Command::Check => {
            let ok = doctor::run().context("check command failed")?;
            Ok(if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
        Command::Clean => {
            clean::run().context("clean command failed")?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Log { prefix, raw, intent } => {
            log_cmd::run(log_cmd::LogOptions { prefix, raw, intent })
                .context("log command failed")?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Distill { file, intent, quiet, window } => {
            let parsed_intent = intent.as_deref().and_then(|i| {
                use crate::intent::Intent;
                Some(match i {
                    "debug:test-failure" => Intent::DebugTestFailure,
                    "commit:generate" => Intent::CommitGenerate,
                    other => Intent::Custom(other.to_owned()),
                })
            });
            distill::run(distill::DistillOptions {
                file: file.map(std::path::PathBuf::from),
                intent: parsed_intent,
                quiet,
                window_lines: window,
            }).context("distill command failed")?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Cat { path, intent, quiet, head } => {
            let parsed_intent = intent.as_deref().and_then(|i| {
                use crate::intent::Intent;
                Some(match i {
                    "debug:test-failure" => Intent::DebugTestFailure,
                    "commit:generate" => Intent::CommitGenerate,
                    other => Intent::Custom(other.to_owned()),
                })
            });
            read_cmd::run(read_cmd::CatOptions {
                path: std::path::PathBuf::from(path),
                intent: parsed_intent,
                quiet,
                head_lines: head,
            }).context("cat command failed")?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Bench => {
            bench::run().context("bench command failed")?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Exec { argv } => run_proxied(argv),
        Command::External(argv) => run_proxied(argv),
    }
}

fn run_proxied(argv: Vec<String>) -> Result<ExitCode> {
    // Build a single-threaded tokio runtime for the run: PTY I/O is fed by
    // native threads (reader, writer), and the async side of Path B is
    // I/O-bound (SQLite writes, buffered file appends). A single-threaded
    // runtime keeps startup cheap so Shard's overhead stays within the <5ms
    // target for tiny commands.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;

    let bridge = ShardPTYBridge::new(argv).context("build ShardPTYBridge")?;
    let outcome = rt.block_on(bridge.run()).context("proxied run failed")?;

    // Ensure all buffered output has drained before the process exits, so the
    // last chunk isn't lost when stdout is piped.
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();

    // Map child exit code into ExitCode. Non-zero from the child propagates.
    let code = outcome.exit_code.clamp(0, 255) as u8;
    Ok(ExitCode::from(code))
}
