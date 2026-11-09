//! Trace and observation ID generation for Langfuse.
//!
//! Langfuse uses W3C Trace Context IDs: a trace ID is 32 lowercase hex
//! characters (16 bytes) and an observation (span) ID is 16 lowercase hex
//! characters (8 bytes). With a seed, the IDs are derived from the SHA-256 of
//! the seed, so the same external ID always maps to the same Langfuse ID.
//!
//! ```
//! use langfuse_rust_sdk::ids::{create_observation_id, create_trace_id};
//!
//! // Correlate an external request ID with a Langfuse trace.
//! let trace_id = create_trace_id(Some("request-42"));
//! assert_eq!(trace_id, create_trace_id(Some("request-42")));
//! assert_eq!(trace_id.len(), 32);
//!
//! // Random IDs when there is nothing to correlate.
//! assert_eq!(create_observation_id(None).len(), 16);
//! ```

use sha2::{Digest, Sha256};

/// Create a trace ID: 32 lowercase hex characters.
///
/// With `Some(seed)` the ID is the first 16 bytes of `SHA-256(seed)` as hex, so
/// the same seed always yields the same ID. With `None` (or an empty seed) the
/// ID is random and never all zeros.
pub fn create_trace_id(seed: Option<&str>) -> String {
    match seed.filter(|s| !s.is_empty()) {
        Some(seed) => seeded_hex(seed, 16),
        None => format!("{:032x}", random_non_zero_u128()),
    }
}

/// Create an observation (span) ID: 16 lowercase hex characters.
///
/// With `Some(seed)` the ID is the first 8 bytes of `SHA-256(seed)` as hex, so
/// the same seed always yields the same ID. With `None` (or an empty seed) the
/// ID is random and never all zeros.
pub fn create_observation_id(seed: Option<&str>) -> String {
    match seed.filter(|s| !s.is_empty()) {
        Some(seed) => seeded_hex(seed, 8),
        None => format!("{:016x}", random_non_zero_u64()),
    }
}

/// Lowercase hex of the first `bytes` bytes of `SHA-256(seed)`.
pub(crate) fn seeded_hex(seed: &str, bytes: usize) -> String {
    Sha256::digest(seed.as_bytes())[..bytes]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// All-zero IDs are invalid in W3C Trace Context.
fn random_non_zero_u128() -> u128 {
    loop {
        let id = rand::random::<u128>();
        if id != 0 {
            return id;
        }
    }
}

fn random_non_zero_u64() -> u64 {
    loop {
        let id = rand::random::<u64>();
        if id != 0 {
            return id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_lower_hex(s: &str) -> bool {
        s.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
    }

    /// Reference values: the first 16 bytes (8 for observations) of
    /// `SHA-256(seed)`, as lowercase hex.
    #[test]
    fn seeded_ids_are_truncated_sha256() {
        assert_eq!(
            create_trace_id(Some("session-456")),
            "da8e78131d7a9924039c74c39a01e0a1"
        );
        assert_eq!(
            create_observation_id(Some("session-456")),
            "da8e78131d7a9924"
        );
        assert_eq!(
            create_trace_id(Some("añ/ü")),
            "ba648a53ff58e9f28abe04d1f178adf6"
        );
    }

    #[test]
    fn random_ids_have_w3c_shape() {
        for _ in 0..100 {
            let trace = create_trace_id(None);
            let obs = create_observation_id(None);
            assert_eq!(trace.len(), 32);
            assert_eq!(obs.len(), 16);
            assert!(is_lower_hex(&trace) && is_lower_hex(&obs));
            assert_ne!(trace, "0".repeat(32));
            assert_ne!(obs, "0".repeat(16));
        }
        assert_ne!(create_trace_id(None), create_trace_id(None));
    }

    #[test]
    fn empty_seed_is_random() {
        assert_ne!(create_trace_id(Some("")), create_trace_id(Some("")));
        assert_eq!(create_observation_id(Some("")).len(), 16);
    }
}
