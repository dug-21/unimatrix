# C2: Error Module -- Pseudocode

## Purpose

Define `VectorError` enum and `Result<T>` type alias for all vector index operations.

## File: `crates/unimatrix-vector/src/error.rs`

```
use std::fmt;

ENUM VectorError:
    DimensionMismatch { expected: usize, got: usize }
        // Embedding or query dimension does not match config.dimension

    Store(unimatrix_store::StoreError)
        // Propagated from VECTOR_MAP operations

    Persistence(String)
        // File I/O errors during dump/load

    EmptyIndex
        // NOT used for search (search returns empty vec).
        // Reserved for operations that require a non-empty index.

    EntryNotInIndex(u64)
        // Entry ID has no vector mapping in IdMap.

    Index(String)
        // hnsw_rs internal errors (wrapped as String)

    InvalidEmbedding(String)
        // NaN or infinity values detected in embedding/query

IMPL Display for VectorError:
    DimensionMismatch => "dimension mismatch: expected {expected}, got {got}"
    Store(e)         => "store error: {e}"
    Persistence(msg) => "persistence error: {msg}"
    EmptyIndex       => "index is empty"
    EntryNotInIndex(id) => "entry {id} not found in index"
    Index(msg)       => "index error: {msg}"
    InvalidEmbedding(msg) => "invalid embedding: {msg}"

IMPL std::error::Error for VectorError:
    source():
        Store(e) => Some(e)
        _ => None

IMPL From<unimatrix_store::StoreError> for VectorError:
    fn from(e) -> Self = VectorError::Store(e)

TYPE Result<T> = std::result::Result<T, VectorError>
```

## Design Notes

- `From<StoreError>` enables `?` propagation from store calls.
- All variants carry enough context for meaningful error messages.
- No `From<std::io::Error>` -- persistence wraps IO errors manually to add path context.
- `InvalidEmbedding` covers NaN, infinity, and subnormal values (W2 alignment).
