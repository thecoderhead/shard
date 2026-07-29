//! Apply `SHARD_INTENT` bias to a compacted output.
//!
//! Called after the archetype-specific compactor produces its output. The
//! intent bias further filters the result:
//!
//! * `debug:test-failure` — retain only lines matching failure signatures
//!   (`FAIL`, `error:`, `panicked`, `AssertionError`, etc.) plus ±10 lines
//!   of surrounding context, deduped.
//! * `commit:generate` — strip diff hunk bodies (lines starting with `+`,
//!   `-`, `@@`) but keep file headers and stats lines.
//! * anything else / unset — pass through.

use crate::intent::Intent;

const CONTEXT_LINES: usize = 10;

pub fn apply(intent: Option<&Intent>, compacted: &str) -> String {
    let Some(intent) = intent else {
        return compacted.to_owned();
    };
    match intent {
        Intent::DebugTestFailure => failure_focus(compacted),
        Intent::CommitGenerate => strip_diff_hunks(compacted),
        Intent::Custom(_) => compacted.to_owned(),
    }
}

fn failure_focus(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return String::new();
    }
    let signatures = [
        "FAIL", "FAILED", "error:", "Error:", "ERROR ", "panic", "panicked",
        "AssertionError", "assertion failed", "Traceback", "stack backtrace",
        "test result: FAILED", "FAIL[", "✗ ",
    ];
    let mut keep = vec![false; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        if signatures.iter().any(|s| line.contains(s)) {
            let start = i.saturating_sub(CONTEXT_LINES);
            let end = (i + CONTEXT_LINES + 1).min(lines.len());
            for k in start..end {
                keep[k] = true;
            }
        }
    }
    let mut out = String::new();
    let mut previous_kept = false;
    let mut any_kept = false;
    for (i, line) in lines.iter().enumerate() {
        if keep[i] {
            out.push_str(line);
            out.push('\n');
            previous_kept = true;
            any_kept = true;
        } else if previous_kept {
            out.push_str("...\n");
            previous_kept = false;
        }
    }
    if !any_kept {
        // No signature matched — return the original so we don't blank the AI.
        return text.to_owned();
    }
    out
}

fn strip_diff_hunks(text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        if line.starts_with("+++") || line.starts_with("---") || line.starts_with("diff ") {
            out.push_str(line);
            out.push('\n');
        } else if line.starts_with('+') || line.starts_with('-') || line.starts_with("@@") {
            continue;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_focus_keeps_context() {
        let text = "start\na\nb\nc\nd\ne\nassertion failed at line 42\nf\ng\nh\ni\nj\nk\nl\nm\nend\n";
        let out = failure_focus(text);
        assert!(out.contains("assertion failed"));
        assert!(out.contains("c"));
        assert!(!out.contains("start"));
    }

    #[test]
    fn diff_stripping() {
        let diff = "diff --git a b\n--- a\n+++ b\n@@ -1,3 +1,4 @@\n-old\n+new\n context\n";
        let out = strip_diff_hunks(diff);
        assert!(out.contains("diff --git"));
        assert!(!out.contains("-old"));
        assert!(!out.contains("+new"));
        assert!(!out.contains("@@"));
    }
}
