# Test Plan: feature-flag-flip

## Test Cases

### T-04: Project path db filename (R-02)
- Under `backend-sqlite` feature: `ensure_data_directory()` returns path ending in `unimatrix.db`
- Without `backend-sqlite` feature: `ensure_data_directory()` returns path ending in `unimatrix.redb`
- This is a unit test in `crates/unimatrix-engine/src/project.rs`
- The cfg-gated assertion adapts to the active feature

### T-06: Compilation matrix (R-03)
- `cargo build -p unimatrix-server` (default features) compiles successfully
- `cargo build -p unimatrix-server --no-default-features --features mcp-briefing` compiles successfully
- `cargo check -p unimatrix-store` (default) succeeds
- `cargo check -p unimatrix-store --no-default-features` succeeds
- `cargo check -p unimatrix-engine` (with and without backend-sqlite) succeeds

### T-07: Backend selection (R-03)
- Default build uses SQLite (verified by T-04 returning .db suffix)
- Non-default build uses redb (verified by T-04 returning .redb suffix)
- The existing store tests pass under both backends

### Existing test regression
- `cargo test -p unimatrix-store` (default = backend-sqlite) passes all existing tests
- `cargo test -p unimatrix-engine` passes all existing tests
- `cargo test -p unimatrix-server` (default = backend-sqlite) passes all existing tests
- `cargo test --workspace` passes

## Notes

The feature-flag-flip is a Cargo.toml change plus one cfg-gate in project.rs. The risk is misconfiguration, not logic errors. The compilation matrix test (T-06) is the primary verification -- if both configurations compile and tests pass, the flip is correct.

The existing `sqlite_parity.rs` and `sqlite_parity_specialized.rs` integration tests in the store crate provide ongoing regression coverage for SQLite backend correctness.
