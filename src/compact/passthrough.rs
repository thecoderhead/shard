//! Passthrough archetype — universal cleanup only.
//!
//! Applied when the classifier can't confidently pick a structural archetype,
//! or when `SHARD_INTENT` explicitly disables compaction. Strips trailing
//! whitespace and empties consecutive blank lines.

pub fn compact(lines: &[&str]) -> String {
    let mut out = String::with_capacity(lines.iter().map(|l| l.len() + 1).sum());
    let mut prev_blank = false;
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            if prev_blank {
                continue;
            }
            prev_blank = true;
        } else {
            prev_blank = false;
        }
        out.push_str(trimmed);
        out.push('\n');
    }
    out
}
