//! Full `shard init` implementation — Phase 3.
//!
//! Installs Shard rewrite hooks into supported AI coding agents and shells:
//!
//! * **Claude Code** — appends a UserPromptSubmit / PreToolUse hook to
//!   `~/.claude/settings.json` that transparently rewrites shell commands
//!   into `shard <cmd>`.
//! * **Cursor / Copilot / Windsurf / Cline** — same technique via each
//!   agent's config file (many share the Claude schema).
//! * **Shell aliases** — bash/zsh (`.bashrc`/`.zshrc`), PowerShell profile
//!   (`$PROFILE`), fish (`config.fish`). Aliases wrap the most common
//!   noise-heavy commands so the developer benefits when they run them
//!   interactively too.
//!
//! Design decisions:
//!
//! * **Idempotent** — every write is bracketed with sentinel comments
//!   (`# >>> shard managed >>>` / `# <<< shard managed <<<`) and existing
//!   blocks are replaced rather than duplicated.
//! * **Non-destructive** — a `.bak-<timestamp>` backup is written next to
//!   any file modified.
//! * **Show / Uninstall** — `shard init --show` scans the same files and
//!   reports what's currently active; `--uninstall` removes the sentinel
//!   blocks, leaving the rest of the file untouched.
//! * **Feature flags** — `--context-prune` and `--cat-compress` are opt-in
//!   and disabled by default. They install feature-specific aliases into
//!   a companion script sourced by the main hook block.

pub mod claude;
pub mod shells;
pub mod registry;

use anyhow::{Context, Result};

use self::registry::{InstallOutcome, upsert_block};

/// Like [`registry::run_install`] but for the opt-in context-prune /
/// cat-compress features. Writes a feature hook script to `.shard/features.sh`
/// that defines convenience aliases for AI agents.
pub fn run_install_features(context_prune: bool, cat_compress: bool) -> Result<Vec<InstallOutcome>> {
    let home = dirs::home_dir().context("no home directory found")?;
    let shard_dir = home.join(".shard");
    std::fs::create_dir_all(&shard_dir).context("create .shard dir")?;

    let mut body = String::new();
    body.push_str("# Shard feature hooks — regenerate with `shard init --context-prune --cat-compress`\n");
    body.push_str("# shellcheck disable=all\n");
    body.push_str("[ -z \"$SHARD_FEATURES_ACTIVE\" ] || return\n");

    if context_prune {
        body.push_str(
            r#"
# shard distill — compact conversation history to save tokens.
# Usage from AI context:
#   cat conversation.log | shard distill
#   shard distill --window 10 < long_output.txt
alias shard_distill="shard distill"
"#,
        );
    }

    if cat_compress {
        body.push_str(
            r#"
# shard cat / read — compact file content for AI consumption.
# Usage from AI agent:
#   shard cat src/main.rs
#   shard cat --head 100 Cargo.toml
alias shard_cat="shard cat"
alias shard_read="shard cat"
"#,
        );
    }

    body.push_str("\nexport SHARD_FEATURES_ACTIVE=1\n");

    let features_path = shard_dir.join("features.sh");
    let outcome = upsert_block(&features_path, body.trim_end())?;

    Ok(vec![InstallOutcome {
        target: "shard-features".into(),
        path: features_path.display().to_string(),
        outcome,
    }])
}
