//! Optional secret redaction pipeline for raw-log output.
//!
//! Disabled by default. Enable with `SHARD_REDACT=1` or `SHARD_REDACT=auto`
//! (auto enables redaction when `.env`, `.env.*`, or `secrets` files are
//! detected in the project root).
//!
//! Scans plain-text output for patterns resembling:
//! - `-----BEGIN.*KEY-----` blocks
//! - `ghp_*`, `gho_*` (GitHub tokens)
//! - `sk-*` (OpenAI keys)
//! - `AKIA*` (AWS access keys)
//! - Generic `(pass|secret|token|key|password|apikey|api_key)=<value>` pairs
//! - JWT-like base64-encoded tokens (`eyJ*`)
//!
//! Redacted lines are replaced with `[REDACTED: <pattern-name>]`.

use std::env;
use std::sync::LazyLock;
use regex::Regex;

const ENV_VAR: &str = "SHARD_REDACT";

/// Whether redaction is enabled for this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactMode {
    Off,
    On,
    Auto,
}

impl RedactMode {
    pub fn from_env() -> Self {
        match env::var(ENV_VAR).as_deref() {
            Ok("1" | "on" | "true" | "yes") => Self::On,
            Ok("auto") => Self::Auto,
            _ => Self::Off,
        }
    }

    fn should_redact(&self) -> bool {
        match self {
            Self::On => true,
            Self::Auto => auto_heuristic(),
            Self::Off => false,
        }
    }
}

/// Quick heuristic: check if `.env`, `.env.*`, or files containing `secret`
/// or `key` exist in the cwd.
fn auto_heuristic() -> bool {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        _ => return false,
    };
    let entries = match std::fs::read_dir(&cwd) {
        Ok(e) => e,
        _ => return false,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.starts_with(".env") || name.contains("secret") || name.contains("key") || name == "secrets" {
            return true;
        }
    }
    false
}

// Redaction patterns, built once.
static PATTERNS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    vec![
        ("ssh-key", Regex::new(r"-----BEGIN\s*(?:RSA|DSA|EC|OPENSSH|PRIVATE)\s*KEY-----").unwrap()),
        ("github-token", Regex::new(r"\bgh[poasu]_[A-Za-z0-9_]{36,40}\b").unwrap()),
        ("openai-key", Regex::new(r"\bsk-[A-Za-z0-9]{32,48}\b").unwrap()),
        ("aws-key", Regex::new(r"\bAKIA[0-9A-Z]{16}\b").unwrap()),
        ("jwt-token", Regex::new(r"\beyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\b").unwrap()),
        ("generic-secret", Regex::new(r#"(?i)(pass|secret|token|key|password|apikey|api_key|auth_token)\s*[:=]\s*['"]?\S{8,}"#).unwrap()),
        ("bearer-token", Regex::new(r"(?i)bearer\s+[A-Za-z0-9._-]{16,}").unwrap()),
    ]
});

/// Redact sensitive patterns from `text`. Returns the redacted version and
/// a count of redactions applied.
pub fn redact(text: &str) -> (String, u64) {
    let mode = RedactMode::from_env();
    if !mode.should_redact() {
        return (text.to_owned(), 0);
    }

    let mut result = text.to_owned();
    let mut total = 0u64;

    for (name, re) in PATTERNS.iter() {
        let replaced = re.replace_all(&result, |_caps: &regex::Captures| {
            total += 1;
            format!("[REDACTED: {name}]")
        });
        result = replaced.into_owned();
    }

    (result, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_ssh_key() {
        let line = "-----BEGIN RSA PRIVATE KEY-----\nabc123\n-----END RSA PRIVATE KEY-----";
        let (out, count) = redact(line);
        assert!(count > 0);
        assert!(out.contains("[REDACTED: ssh-key]"));
    }

    #[test]
    fn redacts_github_token() {
        let line = "token=ghp_abcdefghijklmnopqrstuvwxyz1234567890";
        let (out, _) = redact(line);
        assert!(out.contains("[REDACTED: github-token]") || out.contains("[REDACTED: generic-secret]"));
    }

    #[test]
    fn unchanged_when_off() {
        // SHARD_REDACT not set — should be Off by default
        let (out, count) = redact("hello world");
        assert_eq!(count, 0);
        assert_eq!(out, "hello world");
    }

    #[test]
    fn redacts_aws_key() {
        let line = "AKIAIOSFODNN7EXAMPLE";
        let (out, count) = redact(line);
        assert!(count > 0);
        assert!(out.contains("[REDACTED: aws-key]"));
    }
}
