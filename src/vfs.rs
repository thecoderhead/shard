pub mod cache;

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub const ROOT_DIR: &str = ".shard";
pub const LOGS_SUBDIR: &str = "logs";
pub const METRICS_DB: &str = "metrics.db";

pub const LOG_RETENTION_COUNT: usize = 100;

pub fn root_for_cwd() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to read current working directory")?;
    Ok(cwd.join(ROOT_DIR))
}

/// Ensure `.shard/` and `.shard/logs/` exist. Idempotent.
/// Also ensures `.shard/` is in `.gitignore` so secrets in cached logs are
/// never accidentally committed.
pub fn ensure_root(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join(LOGS_SUBDIR))
        .with_context(|| format!("failed to create {}", root.display()))?;
    // Atomically add .shard/ to .gitignore if one exists.
    add_to_gitignore_if_missing();
    Ok(())
}

/// Append `.shard/` to the project's `.gitignore` if one exists and the entry
/// isn't already present. Best-effort — failures are silently ignored.
fn add_to_gitignore_if_missing() {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        _ => return,
    };
    let gitignore_path = cwd.join(".gitignore");
    if !gitignore_path.exists() {
        return;
    }
    let content = match fs::read_to_string(&gitignore_path) {
        Ok(c) => c,
        _ => return,
    };
    // Check for both / and \ leading variants
    if content.contains(".shard/") || content.contains("/.shard/") || content.contains(".shard") {
        return;
    }
    let entry = format!("\n# shard — log & metrics cache\n.shard/\n");
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore_path)
        .and_then(|mut f| std::io::Write::write_all(&mut f, entry.as_bytes()));
}

pub fn metrics_db_path(root: &Path) -> PathBuf {
    root.join(METRICS_DB)
}

#[allow(dead_code)]
pub fn logs_dir(root: &Path) -> PathBuf {
    root.join(LOGS_SUBDIR)
}
