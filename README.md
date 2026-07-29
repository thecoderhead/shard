# shard

> **PTY-aware CLI proxy that compresses noisy terminal output before it reaches your AI agent.** 60–90% fewer tokens. Zero fidelity loss. Runs entirely on-device.

Shard sits transparently between an AI coding agent (Claude Code, Cursor, GitHub Copilot, Gemini, and others) and the developer shell. It spawns commands inside a real pseudo-terminal, forwards raw bytes to your console with sub-millisecond latency, and tees the same stream into an analysis pipeline that compresses noisy output into a compact summary.

```
Without Shard                                   With Shard

AI --git status--> shell --> git                AI --git status--> shard --> git
  ^                          |                    ^                 |          |
  |  ~2,000 tokens (raw)     |                    | ~200 tokens     | tee /   |
  +--------------------------+                    +---(summary)-----+ compact  +
                                                                  raw log cached
```

## Install

### npm (recommended)

```bash
npm install -g shard-cli
```

The npm package auto-detects your platform and downloads the correct pre-built binary, or falls back to `cargo build --release` if needed.

### Build from source

```bash
# Install Rust (once)
winget install --id Rustlang.Rustup -e   # Windows
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh    # macOS / Linux

# Build release binary
cargo install --path .

# Verify
shard --version
shard doctor
shard echo hello
shard gain
```

## Quick start

```bash
# Explicit proxy form — run any command through Shard
shard exec git status
shard exec cargo test

# Implicit form — any command works
shard git status
shard cargo test
shard docker ps
shard kubectl get pods

# Install shell hooks for transparent interception
shard init -g

# View analytics
shard gain
shard gain --history
shard gain --graph
shard gain --format json

# Environment check
shard doctor
```

## How it works

Shard runs every command inside a **real pseudo-terminal** (ConPTY on Windows, Unix98 PTY on Linux/macOS) using `portable-pty`. It tees the raw byte stream into two paths:

- **Path A** — Raw bytes forwarded to your console synchronously. Always hot, never blocked.
- **Path B** — Async analysis pipeline: VTE state machine tokenizes bytes into `Sgr`/`Text`/`Control` tokens, a structural classifier picks the right compaction archetype, and the compressed summary is emitted to the AI agent.

The full raw output is cached to `.shard/logs/<uuid>.log` (rotating cap of 100 runs). AI agents can retrieve the complete output via `cat` without re-executing state-changing commands.

## Compaction engine

Shard classifies command output into structural archetypes rather than using tool-specific rules. This means it works on any command without configuration.

| Archetype | Triggered by | Strategy |
|---|---|---|
| **Tabular** | ≥90% aligned whitespace columns | Keep header + separator + top 3 + bottom 3; fold middle into statistical summary |
| **Linear-log** | High structural dedup ratio | L-Drain fingerprinting with LSH grouping; always keep last 5 lines |
| **Tree** | Indented tree or JSON/YAML structure | Prune branches beyond depth 2 when node count exceeds threshold |
| **Passthrough** | Fallback / interactive TTY | No modification; raw bytes forwarded |

### Intent biasing

Set `SHARD_INTENT` before running a command to bias compaction:

```bash
# Keep test failures + stack traces + surrounding context
SHARD_INTENT="debug:test-failure" shard cargo test

# Strip diff hunks; keep file names and line-change stats
SHARD_INTENT="commit:generate" shard git diff HEAD~1
```

## Architecture

```
src/
  main.rs              Entry + tracing bootstrap
  cli.rs               Clap router; external-subcommand catch-all
  pty/
    bridge.rs          ShardPTYBridge: PTY spawn + dual-stream tee
  vte_tok/
    tokenizer.rs       VTE state machine → Sgr/Text/Control tokens
  compact/
    classify.rs        3-archetype structural classifier
    tabular.rs         Header/top3/bottom3/summary folding
    linear.rs          L-Drain LSH dedup + tail retention
    tree.rs            Depth>2 pruning
    intent_bias.rs     SHARD_INTENT biasing
    engine.rs          Top-level dispatcher
  hooks_impl/
    registry.rs        Sentinel-bracketed file editor
    shells.rs          Bash/zsh/fish/PowerShell hook targets
  vfs.rs               .shard/ directory helpers
  vfs/cache.rs         Raw-log VFS writer + rotation
  metrics/db.rs        SQLite-backed runs journal
  analytics.rs         shard gain reporting
  doctor.rs            shard doctor sanity checks
  hooks.rs             shard init implementation
  intent.rs            SHARD_INTENT parsing
  error.rs             Structured error types

extension/             VS Code extension (TypeScript)
  src/extension.ts     Activation, status bar, commands
  src/metrics.ts       SQLite reader, fs.watch live reload
  src/treeView.ts      Explorer sidebar recent-runs tree
  src/dashboard.ts     Full webview dashboard with charts
```

## Features

- **True PTY** — ConPTY on Windows, Unix98 PTY on Linux/macOS. TUI tools (vim, htop, wizards) work unchanged.
- **Dual-stream tee** — Path A streams raw bytes to your console; Path B fans out to the analysis pipeline. Path A stays hot even when Path B stalls.
- **ANSI-preserving tokenizer** — Bytes classified into Sgr/Text/Control tokens via a VTE state machine. Compaction touches only Text; colors and cursor moves survive.
- **VFS raw-log cache** — Every run's raw bytes written to `.shard/logs/<uuid>.log` (rotating cap of 100). AI agents can cat the log without re-executing state-changing commands.
- **SQLite metrics** — One row per run in `.shard/metrics.db`. Bundled SQLite, WAL journalling, no external dependency.
- **Shell hooks** — `shard init -g` installs transparent shell aliases for bash, zsh, fish, and PowerShell.
- **VS Code extension** — Live dashboard showing token savings, daily trends, and per-command breakdowns.
- **Cross-platform** — Windows, macOS, and Linux.
- **Privacy-first** — 100% on-device, no telemetry, no cloud calls. `.shard/` auto-added to `.gitignore` on first run.
- **Secret redaction** — Optional `SHARD_REDACT=1` redacts API keys, tokens, and secrets from cached logs.

## Analytics

```bash
$ shard gain

  Total runs        :  247
  Total tokens in   :  1,842,000
  Total tokens out  :    156,000
  Tokens saved      :  1,686,000  (-91.5%)
  Wall-clock saved  :  0 ms (proxy overhead only)
```

## License

Apache-2.0
