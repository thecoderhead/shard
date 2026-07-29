//! Agent hook targets — shell aliases + JSON PreToolUse hooks.
//!
//! Claude Code, Cursor, Windsurf, Copilot, and Cline all support a
//! `PreToolUse` hook that rewrites `Bash` invocations before execution.
//! Most share Claude's MCP schema, so a single JSON payload covers them.
//!
//! For shell-based agents (no JSON support), we emit a POSIX alias script
//! at `~/.shard/hooks.sh` that can be sourced in any bootstrap config.

use std::path::PathBuf;

use anyhow::Result;

pub enum TargetType {
    /// Standard sentinel-bracketed shell script (registry::upsert_block).
    ShellScript,
    /// JSON config file merged with serde_json (registry::upsert_json_block).
    Json,
}

pub struct Target {
    pub name: String,
    pub path: PathBuf,
    pub body: String,
    pub target_type: TargetType,
}

/// Shell alias payload — sourced by agent bootstrap configs.
pub const DEFAULT_ALIASES_SH: &str = r#"# Shard hook — rewrites noisy commands through the Shard proxy.
# Regenerate with:  shard init -g
shard_run() { shard "$@"; }
alias git="shard_run git"
alias cargo="shard_run cargo"
alias npm="shard_run npm"
alias pnpm="shard_run pnpm"
alias yarn="shard_run yarn"
alias pytest="shard_run pytest"
alias go="shard_run go"
alias docker="shard_run docker"
alias kubectl="shard_run kubectl"
alias tree="shard_run tree"
alias grep="shard_run grep"
alias rg="shard_run rg"
alias tsc="shard_run tsc"
alias eslint="shard_run eslint"
alias ruff="shard_run ruff"
alias mvn="shard_run mvn"
alias gradle="shard_run gradle"
# Source feature hooks (context-prune, cat-compress) if installed.
_features="$HOME/.shard/features.sh"
[ -f "$_features" ] && . "$_features"
unset _features
export SHARD_HOOK_ACTIVE=1
"#;

/// PreToolUse JSON hook payload — Claude Code, Cursor, Windsurf schema.
///
/// Rewrites every Bash invocation through `shard` to automatically
/// intercept and compact the output.
const PRETOOL_USE_JSON: &str = r#"{
  "hooks": {
    "PreToolUse": {
      "Bash": "shard {{command}}"
    }
  },
  "_shard_managed": true
}
"#;

pub fn targets(_global: bool) -> Result<Vec<Target>> {
    let mut out = Vec::new();
    let home = dirs::home_dir();
    if let Some(home) = home {
        // Shell hook script (sourced by agent bootstrap configs).
        out.push(Target {
            name: "claude-hooks-sh".into(),
            path: home.join(".shard").join("hooks.sh"),
            body: DEFAULT_ALIASES_SH.trim_end().to_owned(),
            target_type: TargetType::ShellScript,
        });

        // Claude Code PreToolUse JSON hook.
        out.push(Target {
            name: "claude-code".into(),
            path: home.join(".claude").join("settings.local.json"),
            body: PRETOOL_USE_JSON.trim_end().to_owned(),
            target_type: TargetType::Json,
        });

        // Cursor (adopts Claude's config schema).
        out.push(Target {
            name: "cursor".into(),
            path: home.join(".cursor").join("settings.json"),
            body: PRETOOL_USE_JSON.trim_end().to_owned(),
            target_type: TargetType::Json,
        });

        // Windsurf (Codeium — uses Claude-compatible schema).
        out.push(Target {
            name: "windsurf".into(),
            path: home.join(".windsurf").join("settings.local.json"),
            body: PRETOOL_USE_JSON.trim_end().to_owned(),
            target_type: TargetType::Json,
        });
    }
    Ok(out)
}
