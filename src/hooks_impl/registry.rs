//! Shared file editing — sentinel-block for shell scripts, JSON merge for
//! agent configs.
//!
//! Shell scripts use `>>> shard managed >>>` / `<<< shard managed <<<` marker
//! lines. JSON configs use a `_shard_managed` field as sentinel and merge
//! their hook payload into the existing config.

use std::fs;
use std::io::Write as _;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;

use super::claude::TargetType;

pub const BEGIN: &str = "# >>> shard managed >>>";
pub const END: &str = "# <<< shard managed <<<";

/// Result of a single-file edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditOutcome {
    Installed,
    Replaced,
    AlreadyPresent,
    Removed,
    NotFound,
}

/// Reported by `shard init --show`.
#[derive(Debug, Clone, Serialize)]
pub struct HookStatus {
    pub target: String,
    pub path: String,
    pub installed: bool,
}

/// Aggregate result of `shard init` / `--uninstall`.
#[derive(Debug, Clone, Serialize)]
pub struct InstallOutcome {
    pub target: String,
    pub path: String,
    pub outcome: EditOutcome,
}

// ── Shell-script sentinel-block editing ──────────────────────────────────────

/// Insert or replace Shard's managed block in a shell-script `path`.
pub fn upsert_block(path: &Path, block_body: &str) -> Result<EditOutcome> {
    let original = fs::read_to_string(path).unwrap_or_default();
    let normalized = original.replace("\r\n", "\n");
    let managed = format!("{BEGIN}\n{block_body}\n{END}");

    let (new_contents, outcome) = if let Some((start, end)) = find_block(&normalized) {
        let existing = &normalized[start..end];
        if existing.trim() == managed.trim() {
            return Ok(EditOutcome::AlreadyPresent);
        }
        let mut s = String::with_capacity(normalized.len());
        s.push_str(&normalized[..start]);
        s.push_str(&managed);
        s.push_str(&normalized[end..]);
        (s, EditOutcome::Replaced)
    } else {
        let mut s = normalized;
        if !s.is_empty() && !s.ends_with('\n') {
            s.push('\n');
        }
        s.push('\n');
        s.push_str(&managed);
        s.push('\n');
        (s, EditOutcome::Installed)
    };

    write_with_backup(path, new_contents.as_bytes())?;
    Ok(outcome)
}

/// Remove Shard's managed block from a shell-script `path`.
pub fn remove_block(path: &Path) -> Result<EditOutcome> {
    if !path.exists() {
        return Ok(EditOutcome::NotFound);
    }
    let original = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let normalized = original.replace("\r\n", "\n");
    let Some((start, end)) = find_block(&normalized) else {
        return Ok(EditOutcome::NotFound);
    };
    let mut s = String::with_capacity(normalized.len());
    s.push_str(&normalized[..start]);
    if s.ends_with("\n\n") {
        s.pop();
    }
    s.push_str(&normalized[end..]);
    fs::write(path, s).with_context(|| format!("write to {} failed", path.display()))?;
    Ok(EditOutcome::Removed)
}

fn find_block(s: &str) -> Option<(usize, usize)> {
    let start = s.find(BEGIN)?;
    let after_start = start + BEGIN.len();
    let end_marker = s[after_start..].find(END)?;
    let end = after_start + end_marker + END.len();
    Some((start, end))
}

pub fn is_installed(path: &Path) -> bool {
    let Ok(s) = fs::read_to_string(path) else {
        return false;
    };
    find_block(&s.replace("\r\n", "\n")).is_some()
}

// ── JSON config merge editing ───────────────────────────────────────────────

/// Inject Shard's `PreToolUse` hook into a JSON config file.
///
/// Uses a `_shard_managed` sentinel field instead of comment markers.
/// Parses the existing JSON, merges in the hook block, and writes back.
pub fn upsert_json_block(path: &Path, hook_body: &str) -> Result<EditOutcome> {
    let hook_value: serde_json::Value =
        serde_json::from_str(hook_body).context("invalid JSON hook payload")?;

    // Read existing config or start with an empty object.
    let mut existing: serde_json::Value = if path.exists() {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&raw).unwrap_or(serde_json::Value::Object(Default::default()))
    } else {
        serde_json::Value::Object(Default::default())
    };

    // Check if already present with the same content.
    if existing.get("_shard_managed").and_then(|v| v.as_bool()) == Some(true) {
        if existing.get("hooks") == hook_value.get("hooks") {
            return Ok(EditOutcome::AlreadyPresent);
        }
    }

    // Deep-merge the hook object into existing.
    if let (serde_json::Value::Object(ref mut obj), serde_json::Value::Object(hook_obj)) =
        (&mut existing, hook_value)
    {
        for (k, v) in hook_obj {
            obj.insert(k, v);
        }
    }

    let json_bytes = serde_json::to_string_pretty(&existing)
        .context("serialize merged JSON config")?;

    write_with_backup(path, json_bytes.as_bytes())?;
    Ok(EditOutcome::Installed)
}

/// Remove Shard's hook from a JSON config file (by deleting `_shard_managed`
/// and the `hooks` key that was injected).
pub fn remove_json_block(path: &Path) -> Result<EditOutcome> {
    if !path.exists() {
        return Ok(EditOutcome::NotFound);
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut existing: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or(serde_json::Value::Object(Default::default()));

    if existing.get("_shard_managed").and_then(|v| v.as_bool()) != Some(true) {
        return Ok(EditOutcome::NotFound);
    }

    if let serde_json::Value::Object(ref mut obj) = existing {
        obj.remove("_shard_managed");
        obj.remove("hooks");
    }

    let json_bytes = serde_json::to_string_pretty(&existing)
        .context("serialize cleaned JSON config")?;
    fs::write(path, json_bytes).with_context(|| format!("write to {} failed", path.display()))?;
    Ok(EditOutcome::Removed)
}

/// Check if a JSON file has the `_shard_managed` sentinel.
pub fn is_json_installed(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let Ok(raw) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    val.get("_shard_managed").and_then(|v| v.as_bool()) == Some(true)
}

// ── Shared helpers ──────────────────────────────────────────────────────────

fn write_with_backup(path: &Path, contents: &[u8]) -> Result<()> {
    if path.exists() {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("txt");
        if let Some(parent) = path.parent() {
            if let Ok(entries) = std::fs::read_dir(parent) {
                let mut backups: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.starts_with(&format!("{}.bak-", ext)))
                            .unwrap_or(false)
                    })
                    .collect();
                backups.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
                for old in backups.iter().rev().skip(1) {
                    let _ = std::fs::remove_file(old.path());
                }
            }
        }
        let ts = Utc::now().format("%Y%m%dT%H%M%S");
        let backup = path.with_extension(format!("{}.bak-{ts}", ext));
        fs::copy(path, &backup)
            .with_context(|| format!("failed to write backup at {}", backup.display()))?;
    } else if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent dir for {}", path.display()))?;
    }
    let mut f = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .with_context(|| format!("failed to open {} for write", path.display()))?;
    f.write_all(contents)
        .with_context(|| format!("write to {} failed", path.display()))?;
    Ok(())
}

// ── Public entry points ──────────────────────────────────────────────────────

pub fn run_install(global: bool) -> Result<Vec<InstallOutcome>> {
    let mut results = Vec::new();
    for target in super::claude::targets(global)? {
        let outcome = match target.target_type {
            TargetType::ShellScript => upsert_block(&target.path, &target.body),
            TargetType::Json => upsert_json_block(&target.path, &target.body),
        }?;
        results.push(InstallOutcome {
            target: target.name,
            path: target.path.display().to_string(),
            outcome,
        });
    }
    for target in super::shells::targets(global)? {
        let outcome = upsert_block(&target.path, &target.body)?;
        results.push(InstallOutcome {
            target: target.name,
            path: target.path.display().to_string(),
            outcome,
        });
    }
    Ok(results)
}

pub fn run_uninstall(global: bool) -> Result<Vec<InstallOutcome>> {
    let mut results = Vec::new();
    for target in super::claude::targets(global)? {
        let outcome = match target.target_type {
            TargetType::ShellScript => remove_block(&target.path),
            TargetType::Json => remove_json_block(&target.path),
        }?;
        results.push(InstallOutcome {
            target: target.name,
            path: target.path.display().to_string(),
            outcome,
        });
    }
    for target in super::shells::targets(global)? {
        let outcome = remove_block(&target.path)?;
        results.push(InstallOutcome {
            target: target.name,
            path: target.path.display().to_string(),
            outcome,
        });
    }
    Ok(results)
}

pub fn run_show(global: bool) -> Result<Vec<HookStatus>> {
    let mut out = Vec::new();
    for target in super::claude::targets(global)? {
        let installed = match target.target_type {
            TargetType::ShellScript => is_installed(&target.path),
            TargetType::Json => is_json_installed(&target.path),
        };
        out.push(HookStatus {
            target: target.name,
            path: target.path.display().to_string(),
            installed,
        });
    }
    for target in super::shells::targets(global)? {
        out.push(HookStatus {
            target: target.name,
            path: target.path.display().to_string(),
            installed: is_installed(&target.path),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn round_trip() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("cfg.txt");
        fs::write(&file, "user config\n").unwrap();

        assert_eq!(upsert_block(&file, "shard alias git").unwrap(), EditOutcome::Installed);
        assert!(is_installed(&file));
        assert_eq!(
            upsert_block(&file, "shard alias git").unwrap(),
            EditOutcome::AlreadyPresent
        );
        assert_eq!(upsert_block(&file, "shard alias git v2").unwrap(), EditOutcome::Replaced);
        assert_eq!(remove_block(&file).unwrap(), EditOutcome::Removed);
        assert!(!is_installed(&file));
    }

    #[test]
    fn json_round_trip() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("settings.json");
        let payload = r#"{"hooks":{"PreToolUse":{"Bash":"shard {{command}}"}},"_shard_managed":true}"#;

        assert_eq!(upsert_json_block(&file, payload).unwrap(), EditOutcome::Installed);
        assert!(is_json_installed(&file));
        assert_eq!(upsert_json_block(&file, payload).unwrap(), EditOutcome::AlreadyPresent);
        assert_eq!(remove_json_block(&file).unwrap(), EditOutcome::Removed);
        assert!(!is_json_installed(&file));
    }
}
