//! Compaction engine top-level dispatcher.
//!
//! Splits input into lines, classifies, dispatches to the archetype
//! compactor, applies intent bias, and returns the result along with the
//! selected archetype for metrics.

use crate::intent::Intent;

use super::archetype::Archetype;
use super::{classify, intent_bias, linear, passthrough, tabular, tree};

/// Result of a compaction pass.
pub struct CompactionOutput {
    pub archetype: Archetype,
    pub text: String,
}

/// Compact `input`, optionally biased by `intent`. Text is expected to be
/// already free of ANSI escape sequences (produced by the VTE tokenizer's
/// text-only fold).
pub fn compact(input: &str, intent: Option<&Intent>) -> CompactionOutput {
    // Cheap short-circuit: tiny inputs don't benefit from compaction and the
    // classifier confidence floor would fall through to passthrough anyway.
    if input.len() < 256 {
        return CompactionOutput {
            archetype: Archetype::Passthrough,
            text: input.to_owned(),
        };
    }

    let lines: Vec<&str> = input.lines().collect();
    let archetype = classify::classify(&lines);
    let text = match archetype {
        Archetype::Tabular => tabular::compact(&lines),
        Archetype::LinearLog => linear::compact(&lines),
        Archetype::Tree => tree::compact(&lines),
        Archetype::Passthrough => passthrough::compact(&lines),
    };
    let text = intent_bias::apply(intent, &text);
    CompactionOutput { archetype, text }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_input_bypasses() {
        let out = compact("hi", None);
        assert!(matches!(out.archetype, Archetype::Passthrough));
        assert_eq!(out.text, "hi");
    }

    #[test]
    fn large_repeating_input_compresses() {
        let big: String = (0..200)
            .map(|i| format!("Connection error at 10.0.0.{i}\n"))
            .collect();
        let out = compact(&big, None);
        assert!(matches!(out.archetype, Archetype::LinearLog));
        assert!(out.text.len() < big.len() / 3);
    }
}
