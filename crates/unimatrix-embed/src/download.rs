use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{EmbedError, Result};
use crate::model::EmbeddingModel;

/// Ensure model files (ONNX model + tokenizer) exist in the cache directory.
///
/// Downloads from HuggingFace Hub via `hf-hub` if not already cached.
/// Returns the path to the model directory containing `model.onnx` and `tokenizer.json`.
pub fn ensure_model(model: EmbeddingModel, cache_dir: &Path) -> Result<PathBuf> {
    let model_dir = cache_dir.join(model.cache_subdir());
    let onnx_path = model_dir.join(model.onnx_filename());
    let tokenizer_path = model_dir.join("tokenizer.json");

    // Check if both files already exist and are non-empty
    if onnx_path.exists()
        && tokenizer_path.exists()
        && file_size(&onnx_path) > 0
        && file_size(&tokenizer_path) > 0
    {
        return Ok(model_dir);
    }

    // Create cache directory if needed
    fs::create_dir_all(&model_dir)?;

    // Download via hf-hub
    let api = hf_hub::api::sync::Api::new()
        .map_err(|e| EmbedError::Download(format!("failed to create HF Hub API: {e}")))?;

    let repo = api.model(model.model_id().to_string());

    // Download ONNX model file (repo path may differ from local filename)
    let downloaded_onnx = repo
        .get(model.onnx_repo_path())
        .map_err(|e| EmbedError::Download(format!("failed to download {}: {e}", model.onnx_repo_path())))?;

    // Download tokenizer
    let downloaded_tokenizer = repo
        .get("tokenizer.json")
        .map_err(|e| EmbedError::Download(format!("failed to download tokenizer.json: {e}")))?;

    // Copy from hf-hub cache to our cache directory if paths differ
    if downloaded_onnx != onnx_path {
        fs::copy(&downloaded_onnx, &onnx_path)?;
    }

    if downloaded_tokenizer != tokenizer_path {
        fs::copy(&downloaded_tokenizer, &tokenizer_path)?;
    }

    // Validate files exist and are non-empty
    if !onnx_path.exists() || file_size(&onnx_path) == 0 {
        return Err(EmbedError::ModelNotFound { path: onnx_path });
    }

    if !tokenizer_path.exists() || file_size(&tokenizer_path) == 0 {
        return Err(EmbedError::ModelNotFound {
            path: tokenizer_path,
        });
    }

    Ok(model_dir)
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_size_nonexistent() {
        assert_eq!(file_size(Path::new("/nonexistent/path/file.txt")), 0);
    }

    #[test]
    fn test_ensure_model_creates_directory() {
        let temp = std::env::temp_dir().join("unimatrix-embed-test-download");
        let _ = fs::remove_dir_all(&temp);

        let model = EmbeddingModel::AllMiniLmL6V2;
        let result = ensure_model(model, &temp);

        // This test requires network access; if download fails, verify error type
        match result {
            Ok(model_dir) => {
                assert!(model_dir.join("model.onnx").exists());
                assert!(model_dir.join("tokenizer.json").exists());
            }
            Err(EmbedError::Download(_)) => {
                // Network not available -- acceptable in CI
            }
            Err(e) => {
                let _ = fs::remove_dir_all(&temp);
                panic!("unexpected error type: {e}");
            }
        }

        // Clean up
        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_ensure_model_sanitized_subdir() {
        let temp = std::env::temp_dir().join("unimatrix-embed-test-subdir");
        let _ = fs::remove_dir_all(&temp);

        let model = EmbeddingModel::AllMiniLmL6V2;
        let result = ensure_model(model, &temp);

        if let Ok(model_dir) = &result {
            assert_eq!(
                model_dir.file_name().unwrap().to_str().unwrap(),
                "sentence-transformers_all-MiniLM-L6-v2"
            );
        }

        // Clean up
        let _ = fs::remove_dir_all(&temp);
    }
}
