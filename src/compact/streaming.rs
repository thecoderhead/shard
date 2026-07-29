//! Streaming compactor — incremental windowed compaction.
//!
//! Processes terminal output as it arrives, window-by-window. Classifies
//! the first window to lock in the archetype, then uses that archetype for
//! all subsequent windows. On flush the remaining buffered lines are
//! compacted and the archetype is reported for the footer.
//!
//! This avoids buffering the entire output in memory before emitting the
//! compacted result — the AI sees compacted output incrementally during
//! long-running commands.

use super::archetype::Archetype;
use super::{classify, engine as batch_compact};

/// Default window size in lines.
pub const DEFAULT_WINDOW: usize = 100;

pub struct StreamingCompactor {
    window_size: usize,
    line_buffer: Vec<String>,
    archetype: Option<Archetype>,
    classified: bool,
    flushed: bool,
}

impl StreamingCompactor {
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size: window_size.max(20), // minimum 20 lines for classifier
            line_buffer: Vec::with_capacity(window_size),
            archetype: None,
            classified: false,
            flushed: false,
        }
    }

    /// Feed incoming text. Returns compacted windows ready for emission.
    /// The text may be multi-line — we split on newlines and buffer.
    pub fn feed_text(&mut self, text: &str) -> Vec<String> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut outputs = Vec::new();

        for line in text.lines() {
            self.line_buffer.push(line.to_owned());

            if self.line_buffer.len() >= self.window_size {
                let window = self.flush_window();
                outputs.push(window);
            }
        }

        // If the text ended with a newline, treat the buffer as flush-ready too.
        if text.ends_with('\n') && !self.line_buffer.is_empty() {
            let window = self.flush_window();
            outputs.push(window);
        }

        outputs
    }

    /// Flush remaining buffered lines. Returns the final compacted window, or
    /// None if nothing to flush.
    pub fn flush(&mut self) -> Option<String> {
        if self.flushed {
            return None;
        }
        self.flushed = true;
        if self.line_buffer.is_empty() {
            return None;
        }
        Some(self.flush_window())
    }

    /// The classified archetype, if classification has happened.
    pub fn archetype(&self) -> Option<Archetype> {
        self.archetype
    }

    /// Flush the current line buffer through compaction. Classifies on the
    /// first flush, reuses archetype thereafter.
    fn flush_window(&mut self) -> String {
        let lines: Vec<String> = std::mem::take(&mut self.line_buffer);

        if lines.is_empty() {
            return String::new();
        }

        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();

        // Join lines with newlines for the batch compactor.
        let input = lines.join("\n");

        if !self.classified && line_refs.len() >= 20 {
            self.archetype = Some(classify::classify(&line_refs));
            self.classified = true;
        }

        if self.classified {
            // Use batch compaction with known archetype.
            let result = batch_compact::compact(&input, None);
            self.archetype = Some(result.archetype);
            result.text
        } else {
            // Not enough lines yet for reliable classification — passthrough.
            input
        }
    }
}
