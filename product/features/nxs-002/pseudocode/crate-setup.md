# C1: Crate Setup -- Pseudocode

## Purpose

Create the `unimatrix-vector` crate scaffolding within the Cargo workspace.

## Files to Create

### `crates/unimatrix-vector/Cargo.toml`

```toml
[package]
name = "unimatrix-vector"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[features]
test-support = ["unimatrix-store/test-support"]

[dependencies]
unimatrix-store = { path = "../unimatrix-store" }
hnsw_rs = { version = "0.3", features = ["simdeez_f"] }
anndists = "0.1"

[dev-dependencies]
tempfile = "3"
rand = "0.9"
```

### `crates/unimatrix-vector/src/lib.rs`

```rust
#![forbid(unsafe_code)]

// Module declarations will be added as components are implemented.
// Initial skeleton:
mod error;
mod config;
mod filter;
mod index;
mod persistence;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;

pub use config::VectorConfig;
pub use error::{VectorError, Result};
pub use index::{VectorIndex, SearchResult};
```

## Workspace Integration

The workspace `Cargo.toml` at the repo root already has `members = ["crates/*"]`, so adding `crates/unimatrix-vector/` automatically includes it.

## Verification

- `cargo build --workspace` succeeds.
- `#![forbid(unsafe_code)]` is the first attribute in `lib.rs`.
- Crate uses `edition.workspace = true` (edition 2024).
