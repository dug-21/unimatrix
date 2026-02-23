# C11: Lib Module -- Pseudocode

## Purpose

Crate root with `#![forbid(unsafe_code)]`, module declarations, and public re-exports.

## File: `crates/unimatrix-embed/src/lib.rs`

```
#![forbid(unsafe_code)]

mod config;
mod download;
mod error;
mod model;
mod normalize;
mod onnx;
mod pooling;
mod provider;
mod text;

#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers;

// Public re-exports
pub use config::EmbedConfig;
pub use error::{EmbedError, Result};
pub use model::EmbeddingModel;
pub use normalize::{l2_normalize, l2_normalized};
pub use onnx::OnnxProvider;
pub use provider::EmbeddingProvider;
pub use text::{embed_entries, embed_entry, prepare_text};
```

## Cargo.toml: `crates/unimatrix-embed/Cargo.toml`

```toml
[package]
name = "unimatrix-embed"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[features]
test-support = []

[dependencies]
ort = { version = "2.0.0-rc.11", features = ["download-binaries"] }
tokenizers = { version = "0.21", default-features = false, features = ["onig"] }
hf-hub = "0.4"
dirs = "6"
thiserror = "2"

[dev-dependencies]
approx = "0.5"
```

## Design Notes

- `#![forbid(unsafe_code)]` at crate root (AC-15).
- Module ordering follows dependency order in declarations.
- `test_helpers` is `pub mod` (not `pub use`) so downstream crates can access it via `unimatrix_embed::test_helpers::*`.
- `test_helpers` is gated behind `#[cfg(any(test, feature = "test-support"))]`.
- Public API surface: EmbedConfig, EmbedError, Result, EmbeddingModel, EmbeddingProvider, OnnxProvider, l2_normalize, l2_normalized, prepare_text, embed_entry, embed_entries.
- The workspace Cargo.toml already uses `crates/*` glob so no modification needed.
