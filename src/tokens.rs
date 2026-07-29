//! Token counting — multi-backend with tiktoken-rs + byte-heuristic fallback.
//!
//! Phase 1 shipped a fast byte-count heuristic (`ceil(bytes/4)`) aligned with
//! OpenAI's `cl100k_base` average English density. Now defaults to
//! `tiktoken-rs` for exact BPE tokenization with automatic fallback.
//!
//! The [`new_counter`] factory and [`counter`] lazy static return a
//! `Box<dyn TokenCounter>` backed by tiktoken's `cl100k_base` if available,
//! otherwise falling back to the byte heuristic.

use std::sync::LazyLock;

/// Trait for pluggable token counters.
pub trait TokenCounter: Send + Sync {
    fn count(&self, text: &[u8]) -> u64;
}

/// Byte-count heuristic counter: `ceil(bytes / 4)`.
pub struct ApproxCounter;

impl TokenCounter for ApproxCounter {
    #[inline]
    fn count(&self, text: &[u8]) -> u64 {
        approx_from_bytes(text.len() as u64)
    }
}

/// tiktoken-backed counter using `cl100k_base`.
pub struct TiktokenCounter {
    bpe: tiktoken_rs::CoreBPE,
}

impl TokenCounter for TiktokenCounter {
    fn count(&self, text: &[u8]) -> u64 {
        let s = std::str::from_utf8(text).unwrap_or_default();
        self.bpe.encode_ordinary(s).len() as u64
    }
}

/// Lazily-initialised global token counter: tries tiktoken once, falls back
/// to approx on failure.
static GLOBAL_COUNTER: LazyLock<Box<dyn TokenCounter>> = LazyLock::new(|| {
    match tiktoken_rs::cl100k_base() {
        Ok(bpe) => {
            tracing::debug!(target: "shard::tokens", "tiktoken-rs cl100k_base loaded");
            Box::new(TiktokenCounter { bpe })
        }
        Err(e) => {
            tracing::warn!(target: "shard::tokens", %e, "tiktoken unavailable, using approx fallback");
            Box::new(ApproxCounter)
        }
    }
});

/// Create a token counter, preferring tiktoken-rs with heuristic fallback.
/// Caches the result so subsequent calls share the same tiktoken instance.
pub fn new_counter() -> &'static dyn TokenCounter {
    GLOBAL_COUNTER.as_ref()
}

/// Approximate token count via byte heuristic: `ceil(bytes / 4)`.
/// Matches the average English token density of `cl100k_base`.
#[inline]
pub fn approx_from_bytes(n: u64) -> u64 {
    n.saturating_add(3) / 4
}

/// Convenience wrapper for slice inputs — uses the global counter.
#[inline]
pub fn approx(text: &[u8]) -> u64 {
    new_counter().count(text)
}

/// Compute savings percentage. Returns `0.0` when `input == 0` to avoid NaN.
#[inline]
pub fn savings_pct(tokens_in: u64, tokens_out: u64) -> f64 {
    if tokens_in == 0 {
        return 0.0;
    }
    let saved = tokens_in.saturating_sub(tokens_out) as f64;
    (saved / tokens_in as f64) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_bytes_is_zero_tokens() {
        assert_eq!(approx_from_bytes(0), 0);
    }

    #[test]
    fn ceiling_rounds_up() {
        assert_eq!(approx_from_bytes(1), 1);
        assert_eq!(approx_from_bytes(3), 1);
        assert_eq!(approx_from_bytes(4), 1);
        assert_eq!(approx_from_bytes(5), 2);
    }

    #[test]
    fn savings_saturates() {
        assert!((savings_pct(100, 20) - 80.0).abs() < 1e-9);
        assert_eq!(savings_pct(0, 0), 0.0);
        assert_eq!(savings_pct(10, 20), 0.0);
    }

    #[test]
    fn counter_trait_approx() {
        let c = ApproxCounter;
        assert_eq!(c.count(b"hello"), 2); // 5 bytes / 4 = ceil(1.25) = 2
    }

    #[test]
    fn counter_factory_returns_something() {
        let c = new_counter();
        // Should always return a counter (may be approx or tiktoken).
        assert!(c.count(b"test") > 0);
    }
}
