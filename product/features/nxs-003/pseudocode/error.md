# C1: Error Module -- Pseudocode

## Purpose

Define `EmbedError` enum and `Result` type alias. First crate in workspace to use `thiserror` 2.0.

## File: `crates/unimatrix-embed/src/error.rs`

```
USE std::path::PathBuf
USE thiserror::Error

#[derive(Debug, Error)]
ENUM EmbedError:
    #[error("onnx runtime error: {0}")]
    OnnxRuntime(#[from] ort::Error)

    #[error("tokenizer error: {0}")]
    Tokenizer(String)

    #[error("model download failed: {0}")]
    Download(String)

    #[error("model not found: {path}")]
    ModelNotFound { path: PathBuf }

    #[error("io error: {0}")]
    Io(#[from] std::io::Error)

    #[error("dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize }

TYPE Result<T> = std::result::Result<T, EmbedError>
```

## Design Notes

- Uses Architecture's `ModelNotFound { path }` variant (more specific than Specification's `ModelLoad(String)`) per Alignment Report W2.
- `EmptyInput` variant from Architecture is dropped -- AC-12 says empty string is valid input.
- `Tokenizer(String)` converts via `map_err` rather than `From` impl to avoid coupling to tokenizer crate error types.
- `#[from]` auto-generates `From<ort::Error>` and `From<std::io::Error>`.
- thiserror 2.0 generates both `Display` and `Error` implementations.
