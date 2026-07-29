//! Tree archetype: hierarchical indented output.
//!
//! Strategy: parse indentation depth per line, prune subtrees deeper than
//! [`MAX_DEPTH`], and replace pruned children with a `[N more]` summary.

use std::fmt::Write as FmtWrite;

const MAX_DEPTH: usize = 2;
const MAX_CHILDREN_PER_LEVEL: usize = 6;

pub fn compact(lines: &[&str]) -> String {
    if lines.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let mut skipped_at_depth: std::collections::HashMap<usize, usize> = Default::default();
    let mut current_visible_depth = 0usize;

    for line in lines {
        let depth = indent_depth(line);
        if depth <= MAX_DEPTH {
            // Flush any pending skip counter for lower/equal depths.
            if let Some(n) = skipped_at_depth.remove(&(current_visible_depth + 1)) {
                if n > 0 {
                    let pad = "  ".repeat(current_visible_depth + 1);
                    let _ = write!(out, "{pad}... [{n} more]\n");
                }
            }
            out.push_str(line);
            out.push('\n');
            current_visible_depth = depth;
        } else {
            *skipped_at_depth.entry(depth).or_insert(0) += 1;
        }
    }
    // Trailing summary.
    let mut totals: Vec<(usize, usize)> = skipped_at_depth.into_iter().collect();
    totals.sort();
    for (depth, n) in totals {
        if n > 0 {
            let pad = "  ".repeat(depth.saturating_sub(1));
            let _ = write!(out, "{pad}... [{n} more at depth {depth}]\n");
        }
    }
    out
}

fn indent_depth(line: &str) -> usize {
    let mut cols = 0usize;
    for ch in line.chars() {
        match ch {
            ' ' => cols += 1,
            '\t' => cols += 4,
            '│' | '├' | '└' => cols += 4,
            '─' => cols += 1,
            _ => break,
        }
    }
    cols / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prunes_deep_branches() {
        let lines = vec![
            "src",
            "├── main.rs",
            "├── cli",
            "│   ├── router.rs",
            "│   └── deep",
            "│       ├── inner.rs",
            "│       └── inner2.rs",
            "└── pty",
            "    ├── bridge.rs",
            "    └── mod.rs",
        ];
        let out = compact(&lines);
        assert!(out.contains("main.rs"));
        assert!(out.contains("[") && out.contains("more"));
    }
}
