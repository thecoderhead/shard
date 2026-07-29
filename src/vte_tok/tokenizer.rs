//! `vte`-driven state machine that classifies bytes into
//! [`Sgr`/`Text`/`Control`] tokens. See parent module doc for rationale.

use std::mem;

use vte::{Params, Parser, Perform};

/// A single token emitted by the tokenizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// Printable text (UTF-8 bytes; not required to be a complete codepoint
    /// boundary because we emit chunks eagerly on control-boundary transitions).
    Text(Vec<u8>),
    /// Select-Graphic-Rendition parameters. `bytes` is the fully-formed
    /// `\x1b[<params>m` sequence so downstream consumers can re-emit it as-is.
    Sgr { bytes: Vec<u8> },
    /// Any other control sequence (CSI, OSC, cursor movement, screen clear).
    /// Retained as raw bytes for verbatim pass-through.
    Control { bytes: Vec<u8> },
}

/// Public entry point: fed bytes in arbitrary chunks, emits [`Token`]s via a
/// caller-supplied sink. Not `Send`/`Sync`; keep one per stream.
pub struct Tokenizer<F: FnMut(Token)> {
    parser: Parser,
    performer: Performer<F>,
}

impl<F: FnMut(Token)> Tokenizer<F> {
    pub fn new(sink: F) -> Self {
        Self {
            parser: Parser::new(),
            performer: Performer {
                sink,
                text_buf: Vec::with_capacity(1024),
            },
        }
    }

    /// Feed a byte slice into the tokenizer. Tokens are emitted synchronously
    /// via the sink.
    pub fn feed(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.parser.advance(&mut self.performer, *b);
        }
    }

    /// Flush any buffered text into the sink. Call this when the stream ends
    /// to avoid dropping the tail.
    pub fn finish(mut self) {
        self.performer.flush_text();
    }
}

struct Performer<F: FnMut(Token)> {
    sink: F,
    text_buf: Vec<u8>,
}

impl<F: FnMut(Token)> Performer<F> {
    fn flush_text(&mut self) {
        if !self.text_buf.is_empty() {
            let taken = mem::take(&mut self.text_buf);
            (self.sink)(Token::Text(taken));
        }
    }
}

/// Write an integer as decimal ASCII into `buf`. Avoids allocating a String
/// (which `sub.to_string()` does internally).
fn write_usize(buf: &mut Vec<u8>, val: u64) {
    if val == 0 {
        buf.push(b'0');
        return;
    }
    // Maximum u64 is 20 digits: 18446744073709551615
    let mut tmp = [0u8; 20];
    let mut n = val;
    let mut i = 20;
    while n > 0 {
        i -= 1;
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    buf.extend_from_slice(&tmp[i..]);
}

impl<F: FnMut(Token)> Perform for Performer<F> {
    fn print(&mut self, c: char) {
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        self.text_buf.extend_from_slice(s.as_bytes());
    }

    fn execute(&mut self, byte: u8) {
        // C0 controls: TAB, LF, CR are semantically text; everything else is a
        // control sequence that flushes pending text first.
        match byte {
            b'\t' | b'\n' | b'\r' => self.text_buf.push(byte),
            _ => {
                self.flush_text();
                (self.sink)(Token::Control { bytes: vec![byte] });
            }
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        self.flush_text();
        // Reconstruct the sequence bytes for verbatim replay.
        // Pre-allocate with a capacity that covers most CSI sequences.
        let mut bytes = Vec::with_capacity(32);
        bytes.extend_from_slice(b"\x1b[");
        let mut first = true;
        for group in params.iter() {
            if !first {
                bytes.push(b';');
            }
            first = false;
            for (i, sub) in group.iter().enumerate() {
                if i > 0 {
                    bytes.push(b':');
                }
                // Write integer directly — avoids sub.to_string() allocation.
                write_usize(&mut bytes, sub);
            }
        }
        let mut ch = [0u8; 4];
        let s = action.encode_utf8(&mut ch);
        bytes.extend_from_slice(s.as_bytes());

        if action == 'm' {
            (self.sink)(Token::Sgr { bytes });
        } else {
            (self.sink)(Token::Control { bytes });
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        self.flush_text();
        (self.sink)(Token::Control {
            bytes: vec![0x1b, byte],
        });
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        self.flush_text();
        let mut bytes = Vec::with_capacity(16);
        bytes.extend_from_slice(b"\x1b]");
        for (i, part) in params.iter().enumerate() {
            if i > 0 {
                bytes.push(b';');
            }
            bytes.extend_from_slice(part);
        }
        bytes.extend_from_slice(b"\x1b\\");
        (self.sink)(Token::Control { bytes });
    }

    fn hook(
        &mut self,
        _params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
        self.flush_text();
    }

    fn put(&mut self, byte: u8) {
        (self.sink)(Token::Control { bytes: vec![byte] });
    }

    fn unhook(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(bytes: &[u8]) -> Vec<Token> {
        let out = std::cell::RefCell::new(Vec::new());
        let mut tok = Tokenizer::new(|t| out.borrow_mut().push(t));
        tok.feed(bytes);
        tok.finish();
        out.into_inner()
    }

    #[test]
    fn plain_text_is_one_token() {
        let toks = collect(b"hello world\n");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0], Token::Text(b"hello world\n".to_vec()));
    }

    #[test]
    fn sgr_is_isolated() {
        // ESC [31m red ESC [0m
        let toks = collect(b"\x1b[31mred\x1b[0m");
        assert!(matches!(toks[0], Token::Sgr { .. }));
        assert_eq!(toks[1], Token::Text(b"red".to_vec()));
        assert!(matches!(toks[2], Token::Sgr { .. }));
    }

    #[test]
    fn cursor_move_is_control() {
        // ESC [2J = clear screen
        let toks = collect(b"\x1b[2J");
        assert_eq!(toks.len(), 1);
        assert!(matches!(toks[0], Token::Control { .. }));
    }
}
