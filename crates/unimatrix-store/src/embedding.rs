//! SQLite embedding blob serialization helpers (ADR-001, crt-043).
//!
//! These are the canonical encode/decode helpers for Vec<f32> BLOB columns in SQLite.
//! Every new embedding BLOB column introduced after crt-043 must have analogous paired
//! helpers (`encode_X_embedding` / `decode_X_embedding`) defined in the same PR as
//! the write path.
//!
//! Serialization: bincode v2 with serde, config::standard().
//! Rationale: self-describing length prefix, model-upgrade-safe, no new dependency (ADR-001).
//!
//! Visibility: pub. `encode_goal_embedding` must be callable from `unimatrix-server`
//! (cross-crate call from the goal-embedding spawn). Both helpers are promoted to pub
//! together for symmetry (OVERVIEW.md WARN-2 Resolution).
//!
//! Group 6 context: Group 6 will consume decoded embeddings through a future store
//! query method (e.g., `get_cycle_start_embedding`) that decodes internally. However,
//! `decode_goal_embedding` is promoted to `pub` now to avoid a breaking change when
//! Group 6 ships, and because `encode_goal_embedding` requires `pub` regardless.

use bincode::error::{DecodeError, EncodeError};

/// Serialize a Vec<f32> embedding to a SQLite BLOB using bincode standard config.
///
/// Uses `bincode::serde::encode_to_vec(vec, config::standard())`.
///
/// Returns an error if bincode serialization fails (should be unreachable for a
/// valid Vec<f32> with standard config, but the Result is propagated so callers
/// can emit a tracing::warn! and skip the UPDATE rather than panicking).
///
/// This is the canonical write-path helper for all SQLite embedding BLOB columns.
/// Every future embedding BLOB column must use this exact config (standard()) — see ADR-001.
pub fn encode_goal_embedding(vec: Vec<f32>) -> Result<Vec<u8>, EncodeError> {
    bincode::serde::encode_to_vec(vec, bincode::config::standard())
}

/// Deserialize a SQLite BLOB back to Vec<f32> using bincode standard config.
///
/// Uses `bincode::serde::decode_from_slice(bytes, config::standard())`.
/// The bytes-consumed count returned by decode_from_slice is discarded.
///
/// Returns `DecodeError` if the bytes are malformed or use a different config.
/// No panic: all error paths return Err.
///
/// Group 6/7 read sites MUST call this via a future store query method
/// (e.g., `get_cycle_start_embedding`) rather than directly — see OVERVIEW.md WARN-2.
pub fn decode_goal_embedding(bytes: &[u8]) -> Result<Vec<f32>, DecodeError> {
    let (vec, _bytes_consumed): (Vec<f32>, usize) =
        bincode::serde::decode_from_slice(bytes, bincode::config::standard())?;
    Ok(vec)
}

#[cfg(test)]
mod tests {
    use super::*;

    // EMBED-U-01 / R-02 scenario 1: round-trip test (AC-14).
    // Encodes a 384-element Vec<f32> (actual embed pipeline dimension), decodes it back,
    // asserts element-wise equality. Float equality is exact because bincode does no
    // lossy transform — it serializes the raw IEEE 754 bytes.
    #[test]
    fn test_encode_decode_round_trip() {
        let original: Vec<f32> = (0..384).map(|i| i as f32 * 0.001).collect();
        let bytes = encode_goal_embedding(original.clone())
            .expect("encode should not fail for valid Vec<f32>");
        let decoded = decode_goal_embedding(&bytes)
            .expect("decode should not fail for bytes produced by encode");
        assert_eq!(
            original, decoded,
            "round-trip encode→decode must be lossless"
        );
    }

    // EMBED-U-02 / R-02 scenario 2: malformed bytes produce DecodeError, not panic.
    //
    // In bincode v2 standard config, Vec<f32> is encoded as a varint length prefix followed
    // by 4 bytes per element. Byte 0x0A = varint for 10 (claiming 10 f32 = 40 bytes), but
    // only 4 bytes follow — bincode returns UnexpectedEnd (DecodeError), not a panic.
    #[test]
    fn test_decode_malformed_bytes_returns_error() {
        // 0x0A = varint 10 → claims 10 f32 elements (40 bytes), only 4 follow → UnexpectedEnd
        let truncated: &[u8] = &[0x0A, 0x01, 0x02, 0x03, 0x04];
        let result = decode_goal_embedding(truncated);
        assert!(
            result.is_err(),
            "truncated bytes must return DecodeError, not Ok"
        );
    }

    // EMBED-U-03 / R-02 scenario 3: helper is a thin wrapper — encoding via helper
    // produces identical bytes to a direct bincode::serde call with standard().
    #[test]
    fn test_encode_matches_direct_bincode_call() {
        let vec: Vec<f32> = vec![1.0, 2.0, 3.0];
        let via_helper = encode_goal_embedding(vec.clone()).expect("helper encode must succeed");
        let via_direct = bincode::serde::encode_to_vec(&vec, bincode::config::standard())
            .expect("direct bincode encode must succeed");
        assert_eq!(
            via_helper, via_direct,
            "helper must produce same bytes as direct bincode call with standard() config"
        );
    }

    // Additional: zero-length vector round-trips correctly.
    // Validates helper is not fragile on edge-case input (empty goal text produces 0-dim vec).
    #[test]
    fn test_encode_decode_empty_vec() {
        let empty: Vec<f32> = vec![];
        let bytes = encode_goal_embedding(empty.clone()).expect("encode empty vec must succeed");
        let decoded = decode_goal_embedding(&bytes).expect("decode empty vec bytes must succeed");
        assert_eq!(empty, decoded, "empty vec must round-trip correctly");
    }

    // Additional: 768-element vector (future model upgrade dimension) round-trips correctly.
    // Validates self-describing length prefix handles dimension changes without migration.
    #[test]
    fn test_encode_decode_768_dim_vec() {
        let large: Vec<f32> = (0..768).map(|i| i as f32 * 0.0001).collect();
        let bytes = encode_goal_embedding(large.clone()).expect("encode 768-dim vec must succeed");
        let decoded = decode_goal_embedding(&bytes).expect("decode 768-dim vec bytes must succeed");
        assert_eq!(large, decoded, "768-dim vec must round-trip correctly");
    }
}
