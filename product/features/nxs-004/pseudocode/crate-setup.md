# Pseudocode: crate-setup

## Purpose
Create the unimatrix-core crate skeleton and add it to the workspace.

## Files to Create

### crates/unimatrix-core/Cargo.toml

```
[package]
name = "unimatrix-core"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[features]
async = ["dep:tokio"]
test-support = [
  "unimatrix-store/test-support",
  "unimatrix-vector/test-support",
  "unimatrix-embed/test-support"
]

[dependencies]
unimatrix-store = { path = "../unimatrix-store" }
unimatrix-vector = { path = "../unimatrix-vector" }
unimatrix-embed = { path = "../unimatrix-embed" }
thiserror = "2"
tokio = { version = "1", features = ["rt"], optional = true }

[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["rt", "macros"] }
```

### crates/unimatrix-core/src/lib.rs

```
#![forbid(unsafe_code)]

mod error;
mod traits;
mod adapters;

#[cfg(feature = "async")]
pub mod async_wrappers;

// Re-exports (component: re-exports)
// Error
pub use error::CoreError;
// Traits
pub use traits::{EntryStore, VectorStore, EmbedService};
// Adapters
pub use adapters::{StoreAdapter, VectorAdapter, EmbedAdapter};
// Domain types from unimatrix-store
pub use unimatrix_store::{
    EntryRecord, NewEntry, QueryFilter, Status, TimeRange,
    DatabaseConfig, Store, StoreError,
};
// Domain types from unimatrix-vector
pub use unimatrix_vector::{SearchResult, VectorConfig, VectorIndex, VectorError};
// Domain types from unimatrix-embed
pub use unimatrix_embed::{EmbeddingProvider, EmbedConfig, OnnxProvider, EmbedError};
```

### Files to Modify

### Cargo.toml (workspace root)
No change needed -- `members = ["crates/*"]` glob already includes new crate.

### crates/unimatrix-store/Cargo.toml
Add sha2 dependency:
```
[dependencies]
sha2 = "0.10"
```

## Key Test Scenarios
- `cargo build -p unimatrix-core` compiles
- `cargo build -p unimatrix-core --features async` compiles
- No circular dependency errors
