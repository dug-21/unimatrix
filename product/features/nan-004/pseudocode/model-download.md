# C8: Model Download Subcommand — Pseudocode

## Purpose

Expose ONNX model download as a CLI subcommand for postinstall use (C6). Thin wrapper around `unimatrix_embed::ensure_model()`.

## Prerequisite: Make ensure_model Public

The `download` module in `crates/unimatrix-embed/src/lib.rs` is currently private:
```rust
mod download;  // private
```

Change to:
```rust
pub mod download;
```

Or add a re-export:
```rust
pub use download::ensure_model;
```

Preferred approach: re-export, since callers should not depend on the download module's internal structure.

Add to `crates/unimatrix-embed/src/lib.rs`:
```rust
pub use download::ensure_model;
```

## Modified File: crates/unimatrix-server/src/main.rs

### handle_model_download()

```
FUNCTION handle_model_download() -> Result<(), Box<dyn Error>>:
    // Use default embed config to get model and cache directory
    LET config = EmbedConfig::default()

    // Resolve cache directory (same logic as the embed pipeline)
    // EmbedConfig.cache_dir is Option<PathBuf>, None means use platform default
    LET cache_dir = config.cache_dir
        .unwrap_or_else(|| {
            // Platform-specific default: ~/.cache/unimatrix-embed/ on Linux
            LET base = dirs::cache_dir()
                .expect("could not determine cache directory")
            base.join("unimatrix-embed")
        })

    eprintln!("Downloading ONNX model to {}...", cache_dir.display())

    // Call ensure_model (blocking, downloads if not cached)
    MATCH unimatrix_embed::ensure_model(config.model, &cache_dir):
        Ok(model_dir) => {
            eprintln!("Model ready at {}", model_dir.display())
            Ok(())
        }
        Err(e) => {
            eprintln!("Model download failed: {e}")
            Err(Box::new(e))
        }
    END MATCH
```

## Notes on Cache Directory Resolution

The `EmbedConfig::default()` sets `cache_dir: None`. The embedding pipeline resolves this at runtime to the platform cache directory. The `handle_model_download` function must replicate this resolution since it calls `ensure_model` directly rather than going through the full embedding pipeline.

Check how the existing code resolves `cache_dir` when it is `None`:

Looking at the embed crate's `ensure_model` signature: `pub fn ensure_model(model: EmbeddingModel, cache_dir: &Path) -> Result<PathBuf>` -- it takes a concrete `&Path`, not an `Option`. So the caller must resolve the default before calling.

The existing server code uses `EmbedServiceHandle::start_loading(EmbedConfig::default())` which handles resolution internally. For the model-download subcommand, we must resolve the cache dir ourselves using `dirs::cache_dir()`.

## Error Handling

- Cache directory cannot be determined: panic with "could not determine cache directory" (this would mean no HOME or XDG_CACHE_HOME, which is a broken environment).
- Model download fails (network, disk): Print error to stderr, return Err. The postinstall (C6) catches this and exits 0 anyway.
- Model already cached: `ensure_model` returns Ok immediately (no download needed).

## Key Test Scenarios

1. `unimatrix model-download` with no cached model: downloads model, prints progress to stderr, exits 0.
2. `unimatrix model-download` with model already cached: exits 0 quickly, prints "Model ready".
3. `unimatrix model-download` with network unavailable: prints error to stderr, exits 1.
4. Sync path: no tokio runtime initialized.
