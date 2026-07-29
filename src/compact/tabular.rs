//! Tabular archetype: aligned-column output.
//!
//! Strategy: keep the first row (header), the top 3 data rows, the bottom 3
//! data rows, and fold the middle into a single "…N rows folded…" line with
//! a per-column value distribution summary where possible.

use ahash::AHashMap;
use std::fmt::Write as FmtWrite;

const HEAD: usize = 3;
const TAIL: usize = 3;

pub fn compact(lines: &[&str]) -> String {
    if lines.len() <= 1 + HEAD + TAIL {
        return lines.join("\n");
    }

    let header = lines[0];
    let data = &lines[1..];
    if data.len() <= HEAD + TAIL {
        return lines.join("\n");
    }

    let folded_count = data.len().saturating_sub(HEAD + TAIL);

    // Try to summarise one interesting column (heuristic: last column).
    let folded = &data[HEAD..data.len() - TAIL];
    let summary_note = summarise_last_column(folded).unwrap_or_default();

    let mut out = String::new();
    let _ = writeln!(out, "{header}");
    for l in &data[..HEAD] {
        let _ = writeln!(out, "{l}");
    }
    if summary_note.is_empty() {
        let _ = writeln!(out, "... [{folded_count} rows folded] ...");
    } else {
        let _ = writeln!(out, "... [{folded_count} rows folded: {summary_note}] ...");
    }
    for l in &data[data.len() - TAIL..] {
        let _ = writeln!(out, "{l}");
    }
    out
}

/// Bucket the last whitespace-separated token of each folded row and emit
/// counts. Returns `None` when the distribution is uninformative (single
/// value or too many uniques).
fn summarise_last_column(rows: &[&str]) -> Option<String> {
    let mut counts: AHashMap<&str, usize> = AHashMap::new();
    for row in rows {
        let token = row.split_whitespace().next_back().unwrap_or("");
        if token.is_empty() {
            continue;
        }
        *counts.entry(token).or_insert(0) += 1;
    }
    if counts.is_empty() || counts.len() > 8 {
        return None;
    }
    let mut pairs: Vec<(&&str, &usize)> = counts.iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(a.1));
    Some(
        pairs
            .into_iter()
            .map(|(k, v)| format!("{v} {k}"))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_middle_rows() {
        let mut lines = vec!["ID   NAME     STATUS"];
        for i in 0..20 {
            lines.push(match i % 3 {
                0 => "abc  svc-a    Running",
                1 => "def  svc-b    Running",
                _ => "ghi  svc-c    Restarting",
            });
        }
        let out = compact(&lines);
        assert!(out.contains("ID   NAME"));
        assert!(out.contains("rows folded"));
        assert!(out.len() < lines.join("\n").len() / 2);
    }
}
