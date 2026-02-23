# Test Plan: crate-setup

## Scope

Verify unimatrix-core crate compiles, workspace integration works, and feature flags function.

## Unit Tests

None -- this component is structural (Cargo.toml, lib.rs skeleton).

## Compilation Tests

### test_crate_compiles
- `cargo build -p unimatrix-core` succeeds with no errors.

### test_async_feature_compiles
- `cargo build -p unimatrix-core --features async` succeeds.
- Verifies tokio dependency resolves under `async` feature.

### test_no_circular_deps
- `cargo build --workspace` succeeds.
- No circular dependency errors between unimatrix-core and lower crates.

### test_forbid_unsafe_code
- `crates/unimatrix-core/src/lib.rs` contains `#![forbid(unsafe_code)]`.

## Risk Coverage

| Risk | Covered By |
|------|-----------|
| R-11 (partial) | test_crate_compiles -- verifies basic dependency resolution |

## AC Coverage

| AC | Covered By |
|----|-----------|
| AC-01 (partial) | test_crate_compiles -- crate exists |
| AC-22 (partial) | test_forbid_unsafe_code |
