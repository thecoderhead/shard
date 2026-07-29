//! Sliding-window archetype classifier.

use super::Archetype;

const WINDOW: usize = 20;
const CONFIDENCE_FLOOR: f64 = 0.55;

pub fn classify(lines: &[&str]) -> Archetype {
    if lines.is_empty() {
        return Archetype::Passthrough;
    }
    let sample: Vec<&str> = lines.iter().take(WINDOW).copied().collect();

    let tabular = tabular_score(&sample);
    let tree = tree_score(&sample);
    let linear = linear_score(&sample);

    let (best_arch, best_score) = [
        (Archetype::Tabular, tabular),
        (Archetype::Tree, tree),
        (Archetype::LinearLog, linear),
    ]
    .into_iter()
    .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    .unwrap_or((Archetype::Passthrough, 0.0));

    if best_score >= CONFIDENCE_FLOOR {
        best_arch
    } else {
        Archetype::Passthrough
    }
}

/// Aligned-column signature: fraction of rows whose whitespace-runs land at
/// consistent column positions.
pub fn tabular_score(lines: &[&str]) -> f64 {
    let usable: Vec<&&str> = lines.iter().filter(|l| l.len() >= 8).collect();
    if usable.len() < 3 {
        return 0.0;
    }
    let fps: Vec<Vec<usize>> = usable
        .iter()
        .map(|l| whitespace_column_positions(l))
        .collect();
    use std::collections::HashMap;
    let mut fp_counts: HashMap<Vec<usize>, usize> = HashMap::new();
    for fp in &fps {
        if fp.is_empty() {
            continue;
        }
        *fp_counts.entry(fp.clone()).or_insert(0) += 1;
    }
    let max_shared = fp_counts.values().copied().max().unwrap_or(0);
    max_shared as f64 / usable.len() as f64
}

fn whitespace_column_positions(line: &str) -> Vec<usize> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b' ' {
            let start = i;
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            if i - start >= 2 && start > 0 {
                out.push(start / 4);
            }
        } else {
            i += 1;
        }
    }
    out
}

pub fn tree_score(lines: &[&str]) -> f64 {
    if lines.is_empty() {
        return 0.0;
    }
    let indented = lines
        .iter()
        .filter(|l| {
            let s: &str = l;
            s.starts_with("  ")
                || s.starts_with('\t')
                || s.starts_with("├")
                || s.starts_with("└")
                || s.starts_with("│")
        })
        .count();
    indented as f64 / lines.len() as f64
}

pub fn linear_score(lines: &[&str]) -> f64 {
    if lines.len() < 4 {
        return 0.0;
    }
    use std::collections::HashSet;
    // Fingerprint each line (IP/UUID/NUM/… → placeholders) before dedup.
    // Raw lines often differ only in variable tokens; the structural
    // fingerprint reveals the true repetition ratio that L-Drain exploits.
    let fps: Vec<String> = lines
        .iter()
        .map(|l| super::linear::structural_fingerprint(l))
        .collect();
    let unique: HashSet<&String> = fps.iter().collect();
    let dup_ratio = 1.0 - (unique.len() as f64 / fps.len() as f64);
    let density = (lines.len().min(WINDOW) as f64) / WINDOW as f64;
    (0.35 + 0.65 * dup_ratio) * density
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabular_detected() {
        let lines = vec![
            "CONTAINER ID   IMAGE     STATUS      NAMES",
            "abc123         nginx     Running     web-1",
            "def456         redis     Running     cache-1",
            "ghi789         postgres  Running     db-1",
            "jkl012         mongo     Restarting  mongo-1",
        ];
        assert!(matches!(classify(&lines), Archetype::Tabular));
    }

    #[test]
    fn tree_detected() {
        let lines = vec![
            "src",
            "├── main.rs",
            "├── cli.rs",
            "└── pty",
            "    ├── bridge.rs",
            "    └── mod.rs",
        ];
        assert!(matches!(classify(&lines), Archetype::Tree));
    }

    #[test]
    fn linear_dedup() {
        let lines: Vec<&str> = std::iter::repeat("Connection refused to 10.0.0.5:443")
            .take(15)
            .collect();
        assert!(matches!(classify(&lines), Archetype::LinearLog));
    }

    #[test]
    fn empty_is_passthrough() {
        assert!(matches!(classify(&[]), Archetype::Passthrough));
    }
}
