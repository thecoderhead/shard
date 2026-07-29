//! ANSI-preserving byte tokenizer (Phase 1 skeleton).
//!
//! The tokenizer wraps [`vte::Parser`] and categorises the incoming byte stream
//! into three token classes:
//!
//! * [`Token::Text`]    — plain printable characters + whitespace. These are
//!   the only tokens Phase 2 compaction operates on.
//! * [`Token::Sgr`]     — Select-Graphic-Rendition sequences (colors, bold,
//!   underline). Preserved verbatim through compaction so summarised output
//!   remains visually readable.
//! * [`Token::Control`] — cursor movement, screen clear, mode-set, etc. These
//!   trigger the interactive-TTY fallback: seeing enough of them within a
//!   sliding window forces pure passthrough.
//!
//! Phase 1 exposes the tokenizer via [`Tokenizer::feed`] which streams tokens
//! into a callback. Phase 2 compaction plugs into the same callback so no
//! rewrite is needed.

pub mod tokenizer;

pub use tokenizer::{Token, Tokenizer};
