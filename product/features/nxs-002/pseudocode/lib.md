# C8: Lib Module -- Pseudocode

## Purpose

Crate root with module declarations and public re-exports.

## File: `crates/unimatrix-vector/src/lib.rs`

```
#![forbid(unsafe_code)]

mod config;
mod error;
mod filter;
mod index;
mod persistence;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;

// Public re-exports
pub use config::VectorConfig;
pub use error::{VectorError, Result};
pub use index::{VectorIndex, SearchResult};
```

## Design Notes

- `filter` is NOT re-exported. `EntryIdFilter` is `pub(crate)` -- internal implementation detail.
- `persistence` is NOT a separate module in the public API. Dump/load are methods on `VectorIndex`.
- The `test-support` feature flag mirrors nxs-001's pattern: downstream crates can access `test_helpers` for testing.
- `#![forbid(unsafe_code)]` is the first attribute, matching nxs-001's pattern.
- Module order matches the conceptual dependency chain: config/error (no internal deps) -> filter -> index -> persistence.
