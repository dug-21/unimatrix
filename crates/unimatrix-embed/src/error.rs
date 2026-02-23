use std::path::PathBuf;

/// Typed error enum for the embedding pipeline.
///
/// Covers all failure modes: ONNX runtime errors, tokenizer errors,
/// download failures, model loading failures, dimension mismatches,
/// and I/O errors. Uses `thiserror` 2.0 for `Display` and `Error` derives.
#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    /// ONNX Runtime error (session creation, inference).
    #[error("onnx runtime error: {0}")]
    OnnxRuntime(#[from] ort::Error),

    /// Tokenizer error (loading, encoding).
    #[error("tokenizer error: {0}")]
    Tokenizer(String),

    /// Model download failed.
    #[error("model download failed: {0}")]
    Download(String),

    /// Model file not found in cache after download.
    #[error("model not found: {path}")]
    ModelNotFound { path: PathBuf },

    /// I/O error (file operations, cache directory).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Embedding dimension mismatch (unexpected output from model).
    #[error("dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },
}

/// Result type alias for the embedding pipeline.
pub type Result<T> = std::result::Result<T, EmbedError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;

    #[test]
    fn test_tokenizer_error_display() {
        let err = EmbedError::Tokenizer("bad token".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("tokenizer error"));
        assert!(msg.contains("bad token"));
    }

    #[test]
    fn test_download_error_display() {
        let err = EmbedError::Download("network timeout".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("model download failed"));
        assert!(msg.contains("network timeout"));
    }

    #[test]
    fn test_model_not_found_error_display() {
        let err = EmbedError::ModelNotFound {
            path: PathBuf::from("/tmp/missing.onnx"),
        };
        let msg = format!("{err}");
        assert!(msg.contains("model not found"));
        assert!(msg.contains("/tmp/missing.onnx"));
    }

    #[test]
    fn test_io_error_from() {
        let io_err = std::io::Error::new(ErrorKind::NotFound, "file missing");
        let embed_err: EmbedError = io_err.into();
        assert!(matches!(embed_err, EmbedError::Io(_)));
    }

    #[test]
    fn test_dimension_mismatch_display() {
        let err = EmbedError::DimensionMismatch {
            expected: 384,
            got: 768,
        };
        let msg = format!("{err}");
        assert!(msg.contains("expected 384"));
        assert!(msg.contains("got 768"));
    }

    #[test]
    fn test_result_alias() {
        let result: Result<Vec<f32>> = Ok(vec![1.0]);
        assert!(result.is_ok());
    }
}
