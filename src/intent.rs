//! `SHARD_INTENT` environment variable parsing.
//!
//! AI agent wrappers set `SHARD_INTENT=<domain>:<hint>` to bias compaction. The
//! stream interceptor in Phase 1 records the intent verbatim in the metrics
//! database; Phase 2 compaction archetypes consume it to tune retention rules.

use std::env;

pub const ENV_VAR: &str = "SHARD_INTENT";

/// Parsed intent hint. Unknown values are preserved verbatim as
/// [`Intent::Custom`] so future consumers can pattern-match without dropping
/// data that Phase 1 didn't yet understand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// `SHARD_INTENT="debug:test-failure"` — retain failed assertions +
    /// surrounding lines.
    DebugTestFailure,
    /// `SHARD_INTENT="commit:generate"` — strip diff hunks, keep filenames +
    /// stats.
    CommitGenerate,
    /// Any other user-supplied intent string, forwarded verbatim.
    Custom(String),
}

impl Intent {
    pub fn as_str(&self) -> &str {
        match self {
            Intent::DebugTestFailure => "debug:test-failure",
            Intent::CommitGenerate => "commit:generate",
            Intent::Custom(s) => s.as_str(),
        }
    }
}

/// Read [`ENV_VAR`] from the process environment. Returns [`None`] if unset or
/// empty (empty string is treated as unset to match shell hook idioms).
pub fn from_env() -> Option<Intent> {
    let raw = env::var(ENV_VAR).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(match trimmed {
        "debug:test-failure" => Intent::DebugTestFailure,
        "commit:generate" => Intent::CommitGenerate,
        other => Intent::Custom(other.to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn known_intents_parse() {
        env::set_var(ENV_VAR, "debug:test-failure");
        assert_eq!(from_env(), Some(Intent::DebugTestFailure));
        env::set_var(ENV_VAR, "commit:generate");
        assert_eq!(from_env(), Some(Intent::CommitGenerate));
        env::remove_var(ENV_VAR);
    }

    #[test]
    #[serial]
    fn unknown_intents_preserved() {
        env::set_var(ENV_VAR, "refactor:extract-fn");
        assert_eq!(
            from_env(),
            Some(Intent::Custom("refactor:extract-fn".to_owned()))
        );
        env::remove_var(ENV_VAR);
    }

    #[test]
    #[serial]
    fn empty_is_none() {
        env::set_var(ENV_VAR, "");
        assert_eq!(from_env(), None);
        env::remove_var(ENV_VAR);
    }
}
