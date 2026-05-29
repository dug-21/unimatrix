//! Shared SHA-256 hash verification for ONNX model files.
//!
//! Extracted from `nli_handle.rs` so both the NLI and embedding model loading
//! paths can verify file integrity before constructing ONNX sessions (bugfix-651).

use std::path::Path;

use sha2::{Digest, Sha256};

/// Verify the SHA-256 hash of an ONNX model file.
///
/// `model_dir` is the directory containing the model file.
/// `onnx_filename` is the filename within that directory (e.g., `"model.onnx"`).
/// `expected_hex` is the expected SHA-256 hash as a 64-char lowercase hex string.
///
/// Returns `Ok(())` on match, `Err(String)` with a mismatch description on failure.
pub(crate) fn verify_sha256(
    model_dir: &Path,
    onnx_filename: &str,
    expected_hex: &str,
) -> Result<(), String> {
    let onnx_file = model_dir.join(onnx_filename);
    let bytes = std::fs::read(&onnx_file)
        .map_err(|e| format!("failed to read model file for hash check: {e}"))?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual_hex = format!("{:x}", hasher.finalize());

    if actual_hex.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(format!("expected {expected_hex}, got {actual_hex}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_sha256_correct_hash() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let model_file = tmp_dir.path().join("model.onnx");
        let content = b"test model content";
        std::fs::write(&model_file, content).unwrap();

        let mut hasher = Sha256::new();
        hasher.update(content);
        let expected = format!("{:x}", hasher.finalize());

        let result = verify_sha256(tmp_dir.path(), "model.onnx", &expected);
        assert!(
            result.is_ok(),
            "Correct hash must pass verification: {result:?}"
        );
    }

    #[test]
    fn test_verify_sha256_wrong_hash_returns_err() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let model_file = tmp_dir.path().join("model.onnx");
        std::fs::write(&model_file, b"some bytes").unwrap();

        let wrong_hash = "b".repeat(64);
        let result = verify_sha256(tmp_dir.path(), "model.onnx", &wrong_hash);
        assert!(result.is_err(), "Wrong hash must fail verification");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("expected") && msg.contains(&wrong_hash),
            "Error message must contain expected hash: {msg}"
        );
    }

    #[test]
    fn test_verify_sha256_missing_file_returns_err() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let result = verify_sha256(tmp_dir.path(), "model.onnx", &"a".repeat(64));
        assert!(result.is_err(), "Missing file must fail hash verification");
        assert!(
            result.unwrap_err().contains("failed to read"),
            "Error must describe read failure"
        );
    }

    #[test]
    fn test_verify_sha256_case_insensitive() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let model_file = tmp_dir.path().join("model.onnx");
        let content = b"case test";
        std::fs::write(&model_file, content).unwrap();

        let mut hasher = Sha256::new();
        hasher.update(content);
        let expected_lower = format!("{:x}", hasher.finalize());
        let expected_upper = expected_lower.to_uppercase();

        assert!(verify_sha256(tmp_dir.path(), "model.onnx", &expected_upper).is_ok());
    }
}
