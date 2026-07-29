//! Compaction engine — Phase 2.
//!
//! Takes the buffered child output (text-only, after ANSI tokenization has
//! stripped control sequences) and produces a compact summary optimised for
//! LLM context windows.
//!
//! Structural archetypes (no tool-specific hacks):
//!
//! * [`Archetype::Tabular`] — aligned column output (`docker ps`,
//!   `kubectl get pods`, `ls -l`): keep header + top3 + bottom3 + folded
//!   summary of the middle.
//! * [`Archetype::LinearLog`] — noisy repeating logs (`cargo test`,
//!   `npm install`, application logs): variable substitution + LSH-based
//!   dedup + retention of the last 5 lines.
//! * [`Archetype::Tree`] — hierarchical output (`tree`, JSON, YAML):
//!   depth-2 pruning with child-count summaries.
//!
//! Selection is a sliding-window classifier ([`classify`]) that looks at the
//! first N lines and picks the archetype whose signature dominates. If none
//! is strong enough, we fall through to [`Archetype::Passthrough`] which
//! only performs universal cleanups (trailing whitespace, ANSI-artifact
//! stripping) and returns the input.
//!
//! Intent bias ([`crate::intent`]) can lock the archetype and adjust
//! retention rules; e.g. `SHARD_INTENT=debug:test-failure` keeps only
//! lines matching failure signatures plus ±10 lines of context.

pub mod archetype;
pub mod classify;
pub mod engine;
pub mod intent_bias;
pub mod linear;
pub mod passthrough;
pub mod streaming;
pub mod tabular;
pub mod tree;

pub use archetype::Archetype;
#[allow(unused_imports)]
pub use engine::{CompactionOutput, compact};
