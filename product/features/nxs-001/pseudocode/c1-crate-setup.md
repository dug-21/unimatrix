# C1: Crate Setup Pseudocode

## Purpose

Establish the Cargo workspace and unimatrix-store crate scaffolding. No Rust code -- configuration only.

## Files to Create

### /Cargo.toml (workspace root)

```toml
[workspace]
members = ["crates/*"]
resolver = "3"

[workspace.package]
edition = "2024"
rust-version = "1.89"
license = "MIT OR Apache-2.0"

[workspace.dependencies]
redb = "3.1"
serde = { version = "1", features = ["derive"] }
bincode = "2"
```

### /crates/unimatrix-store/Cargo.toml

```toml
[package]
name = "unimatrix-store"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[features]
test-support = []

[dependencies]
redb = { workspace = true }
serde = { workspace = true }
bincode = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

### /crates/unimatrix-store/src/lib.rs

Initially empty with `#![forbid(unsafe_code)]` and module declarations.

## Key Constraints

- edition = "2024" (ADR-001)
- rust-version = "1.89" (ADR-001)
- resolver = "3" (Rust 2024 edition requirement)
- `#![forbid(unsafe_code)]` at crate level (NFR-04)
- `test-support` feature flag for downstream test helpers (AC-19)
- No async runtime dependency (ADR-004)

## Test Scenarios

- `cargo build --workspace` compiles with zero errors and zero warnings (AC-01)
