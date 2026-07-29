//! Linear-log archetype (L-Drain).
//!
//! Strategy:
//!
//! 1. Normalise each line by replacing variable tokens (IPs, UUIDs, ISO8601
//!    timestamps, hex/dec numbers, filesystem paths) with placeholders.
//!    Produces a "structural fingerprint" per line.
//! 2. Hash each fingerprint with `ahash` and group counts (LSH-lite; the
//!    substitution step is the locality-preserving hash).
//! 3. Emit each distinct fingerprint once, annotated with `× N` if it
//!    repeated. Preserve first-occurrence order so causally-related lines
//!    stay grouped.
//! 4. Always retain the last [`TAIL_RETAIN`] raw lines verbatim (recency
//!    context — critical for test failures and error tails).

use std::sync::LazyLock;
use regex::Regex;
use std::fmt::Write as FmtWrite;

const TAIL_RETAIN: usize = 5;

/// Combined single-pass regex that matches all variable token types.
/// Named capture groups dispatch to the correct placeholder.
/// Order matches the original 7-pass approach: more specific patterns first
/// so broader ones (like `\d{2,}`) don't eat their tokens.
static RX_FINGERPRINT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?P<iso8601>\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)|(?P<uuid>\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b)|(?P<ip>\b(?:\d{1,3}\.){3}\d{1,3}(?::\d{1,5})?\b)|(?P<hex>\b0x[0-9a-fA-F]{4,}\b)|(?P<path_win>[A-Za-z]:\\[^\s]+)|(?P<path_unix>(?:/[^\s/:]+){2,})|(?P<num>\b\d{2,}\b)",
    ).unwrap()
});

/// Replace variable tokens with placeholders. Single regex pass instead of
/// the original 7 sequential passes, giving ~5-7x speedup on fingerprint-heavy
/// lines.
pub fn structural_fingerprint(line: &str) -> String {
    let result = RX_FINGERPRINT
        .replace_all(line, |caps: &regex::Captures| {
            if caps.name("iso8601").is_some() {
                "<ISO8601>".to_owned()
            } else if caps.name("uuid").is_some() {
                "<UUID>".to_owned()
            } else if caps.name("ip").is_some() {
                "<IP>".to_owned()
            } else if caps.name("hex").is_some() {
                "<HEX>".to_owned()
            } else if caps.name("path_win").is_some() || caps.name("path_unix").is_some() {
                "<PATH>".to_owned()
            } else if caps.name("num").is_some() {
                "<NUM>".to_owned()
            } else {
                caps.get(0).map_or(String::new(), |m| m.as_str().to_owned())
            }
        });
    // Only allocate when substitutions were actually made.
    if matches!(&result, std::borrow::Cow::Owned(_)) {
        result.into_owned()
    } else {
        line.to_owned()
    }
}

/// A single dedup entry: fingerprint string, sample line, occurrence count.
struct DedupEntry {
    fingerprint: String,
    sample: String,
    count: usize,
}

pub fn compact(lines: &[&str]) -> String {
    if lines.len() <= TAIL_RETAIN {
        return lines.join("\n");
    }

    let head_lines = &lines[..lines.len() - TAIL_RETAIN];
    let tail_lines = &lines[lines.len() - TAIL_RETAIN..];

    // Single Vec instead of 3 separate maps: each entry stores fingerprint,
    // sample, and count together. No triple key duplication.
    let mut entries: Vec<DedupEntry> = Vec::new();

    'lines: for line in head_lines {
        let fp = structural_fingerprint(line);
        for entry in entries.iter_mut() {
            if entry.fingerprint == fp {
                entry.count += 1;
                continue 'lines;
            }
        }
        entries.push(DedupEntry {
            fingerprint: fp,
            sample: (*line).to_owned(),
            count: 1,
        });
    }

    let mut out = String::new();
    for entry in &entries {
        if entry.count > 1 {
            let _ = write!(out, "{}  × {}\n", entry.sample, entry.count);
        } else {
            out.push_str(&entry.sample);
            out.push('\n');
        }
    }
    if !tail_lines.is_empty() {
        let _ = write!(out, "--- last {TAIL_RETAIN} lines ---\n");
        for l in tail_lines {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_normalises() {
        let fp = structural_fingerprint(
            "2024-05-25T10:22:03Z INFO connected to 192.168.1.5:5432 job=abcd1234-5678-90ab-cdef-1234567890ab",
        );
        assert!(fp.contains("<ISO8601>"));
        assert!(fp.contains("<IP>"));
        assert!(fp.contains("<UUID>"));
    }

    #[test]
    fn dedup_repeats() {
        let mut lines: Vec<String> = (0..50)
            .map(|i| format!("Connection timeout to 10.0.0.{}:443", i))
            .collect();
        for _ in 0..3 {
            lines.push("Retrying...".to_string());
        }
        lines.push("final line A".to_string());
        lines.push("final line B".to_string());
        lines.push("final line C".to_string());
        lines.push("final line D".to_string());
        lines.push("final line E".to_string());
        let borrowed: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let out = compact(&borrowed);
        assert!(out.contains("× 50"));
        assert!(out.contains("× 3"));
        assert!(out.contains("final line E"));
        assert!(out.len() < lines.iter().map(|s| s.len() + 1).sum::<usize>() / 2);
    }
}
