# C8: Download Module -- Pseudocode

## Purpose

Model download from HuggingFace Hub via `hf-hub` crate and cache management via `dirs`.

## File: `crates/unimatrix-embed/src/download.rs`

```
USE std::path::{Path, PathBuf}
USE std::fs
USE hf_hub::api::sync::Api
USE crate::model::EmbeddingModel
USE crate::error::{EmbedError, Result}

/// Ensure model files (ONNX model + tokenizer) exist in cache.
/// Downloads from HuggingFace Hub if not already cached.
/// Returns path to the model directory containing model.onnx and tokenizer.json.
pub fn ensure_model(model: EmbeddingModel, cache_dir: &Path) -> Result<PathBuf>:
    // Build model subdirectory path
    model_dir = cache_dir.join(model.cache_subdir())

    onnx_path = model_dir.join(model.onnx_filename())
    tokenizer_path = model_dir.join("tokenizer.json")

    // Check if both files already exist and are non-empty
    IF onnx_path.exists() AND tokenizer_path.exists():
        IF file_size(&onnx_path) > 0 AND file_size(&tokenizer_path) > 0:
            return Ok(model_dir)

    // Create cache directory if needed
    fs::create_dir_all(&model_dir)?    // Io error propagates via From

    // Download via hf-hub
    api = Api::new()
        .map_err(|e| EmbedError::Download(format!("failed to create HF Hub API: {e}")))?

    repo = api.model(model.model_id().to_string())

    // Download ONNX model file
    downloaded_onnx = repo.get(model.onnx_filename())
        .map_err(|e| EmbedError::Download(
            format!("failed to download {}: {e}", model.onnx_filename())
        ))?

    // Download tokenizer
    downloaded_tokenizer = repo.get("tokenizer.json")
        .map_err(|e| EmbedError::Download(
            format!("failed to download tokenizer.json: {e}")
        ))?

    // Copy from hf-hub cache to our cache directory
    // hf-hub downloads to its own cache; we copy to ours for layout control
    IF downloaded_onnx != onnx_path:
        fs::copy(&downloaded_onnx, &onnx_path)?

    IF downloaded_tokenizer != tokenizer_path:
        fs::copy(&downloaded_tokenizer, &tokenizer_path)?

    // Validate files exist and are non-empty
    IF NOT onnx_path.exists() OR file_size(&onnx_path) == 0:
        return Err(EmbedError::ModelNotFound { path: onnx_path })

    IF NOT tokenizer_path.exists() OR file_size(&tokenizer_path) == 0:
        return Err(EmbedError::ModelNotFound { path: tokenizer_path })

    Ok(model_dir)

fn file_size(path: &Path) -> u64:
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
```

## Design Notes

- `hf-hub` `Api::new()` creates an anonymous client (no token needed for public models).
- `repo.get(filename)` downloads if not in hf-hub's own cache and returns the path.
- We copy from hf-hub's cache to our `~/.cache/unimatrix/models/` layout for predictable structure.
- Model ID sanitization (slash -> underscore) is done by `EmbeddingModel::cache_subdir()`.
- Files are validated to be non-empty after download to catch partial downloads.
- R-05: Download failure is a high risk. All errors are typed as `EmbedError::Download` or `EmbedError::Io`.
- R-10: Cache path resolution failure is a medium risk.
