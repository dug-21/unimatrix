# Pseudocode: core-error

## Purpose
Define CoreError as the unified error type for all core trait methods.

## New File: crates/unimatrix-core/src/error.rs

```
use std::fmt;

#[derive(Debug)]
pub enum CoreError {
    /// Storage engine error (from unimatrix-store)
    Store(unimatrix_store::StoreError),

    /// Vector index error (from unimatrix-vector)
    Vector(unimatrix_vector::VectorError),

    /// Embedding error (from unimatrix-embed)
    Embed(unimatrix_embed::EmbedError),

    /// Async task join failure (from tokio::task::spawn_blocking)
    JoinError(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Store(e) => write!(f, "store error: {e}"),
            CoreError::Vector(e) => write!(f, "vector error: {e}"),
            CoreError::Embed(e) => write!(f, "embed error: {e}"),
            CoreError::JoinError(msg) => write!(f, "async task error: {msg}"),
        }
    }
}

impl std::error::Error for CoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CoreError::Store(e) => Some(e),
            CoreError::Vector(e) => Some(e),
            CoreError::Embed(e) => Some(e),
            CoreError::JoinError(_) => None,
        }
    }
}

impl From<unimatrix_store::StoreError> for CoreError {
    fn from(e: unimatrix_store::StoreError) -> Self {
        CoreError::Store(e)
    }
}

impl From<unimatrix_vector::VectorError> for CoreError {
    fn from(e: unimatrix_vector::VectorError) -> Self {
        CoreError::Vector(e)
    }
}

impl From<unimatrix_embed::EmbedError> for CoreError {
    fn from(e: unimatrix_embed::EmbedError) -> Self {
        CoreError::Embed(e)
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
```

## Error Handling
CoreError is the error type. It wraps all three crate errors + JoinError for async.

## Key Test Scenarios
- From<StoreError> conversion works
- From<VectorError> conversion works
- From<EmbedError> conversion works
- Display format includes source error message
- CoreError implements std::error::Error
- source() returns inner error for Store/Vector/Embed variants
