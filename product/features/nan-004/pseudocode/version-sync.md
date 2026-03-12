# C9: Version Synchronization — Pseudocode

## Purpose

Establish lockstep versioning at 0.5.0 across all 9 Rust crates using workspace version inheritance. Per ADR-005, root `Cargo.toml` is the single source of truth.

## Modified File: Cargo.toml (root workspace)

Add `version` to `[workspace.package]`:

```toml
[workspace.package]
version = "0.5.0"
edition = "2024"
rust-version = "1.89"
license = "MIT OR Apache-2.0"
```

## Modified Files: All 9 crate Cargo.toml files

For each of the 9 crates, replace the hardcoded version with workspace inheritance:

### crates/unimatrix-store/Cargo.toml
Replace: `version = "0.1.0"` with `version.workspace = true`

### crates/unimatrix-vector/Cargo.toml
Replace: `version = "0.1.0"` with `version.workspace = true`

### crates/unimatrix-embed/Cargo.toml
Replace: `version = "0.1.0"` with `version.workspace = true`

### crates/unimatrix-core/Cargo.toml
Replace: `version = "0.1.0"` with `version.workspace = true`

### crates/unimatrix-engine/Cargo.toml
Replace: `version = "0.1.0"` with `version.workspace = true`

### crates/unimatrix-adapt/Cargo.toml
Replace: `version = "0.1.0"` with `version.workspace = true`

### crates/unimatrix-observe/Cargo.toml
Replace: `version = "0.1.0"` with `version.workspace = true`

### crates/unimatrix-learn/Cargo.toml
Replace: `version = "0.1.0"` with `version.workspace = true`

### crates/unimatrix-server/Cargo.toml
Replace: `version = "0.1.0"` with `version.workspace = true`

Additionally for unimatrix-server, move edition and rust-version to workspace inheritance:
Replace:
```toml
edition = "2024"
rust-version = "1.89"
```
With:
```toml
edition.workspace = true
rust-version.workspace = true
```

These fields are already in `[workspace.package]` in the root Cargo.toml. Other crates already use workspace inheritance for edition — check each and update if they have hardcoded values.

## Verification

After changes, the following must hold:

1. `cargo metadata --format-version 1 | jq '.packages[] | select(.name | startswith("unimatrix")) | .version'` returns `"0.5.0"` for all 9 crates.
2. `env!("CARGO_PKG_VERSION")` in main.rs compiles to `"0.5.0"`.
3. `cargo build` succeeds with no warnings about version fields.

## Error Handling

No runtime error handling. This is a compile-time/build configuration change. If any crate's `version.workspace = true` is misconfigured, `cargo build` will fail with a clear error about workspace version not being set.

## Key Test Scenarios

1. All 9 crate Cargo.toml files contain `version.workspace = true` (no hardcoded version).
2. Root Cargo.toml has `version = "0.5.0"` in `[workspace.package]`.
3. `cargo build` succeeds.
4. `cargo test` passes (no version-dependent test breakage).
5. `unimatrix version` prints `unimatrix 0.5.0`.
6. unimatrix-server Cargo.toml has `edition.workspace = true` and `rust-version.workspace = true`.
