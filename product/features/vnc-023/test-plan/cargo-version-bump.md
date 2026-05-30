# Test Plan: cargo-version-bump (C1)

## Component

`crates/unimatrix-server/Cargo.toml` -- change rmcp version from `=0.16.0` to `=1.7.0`.

## Risks Covered

- **R-05 (High)**: CVE-2026-42559 not fully resolved
- **R-07 (Medium)**: UDS IntoTransport blanket impl fails
- **R-10 (High)**: http crate version mismatch
- **R-11 (Medium)**: ErrorData::invalid_params signature changed

## Unit Test Expectations

No Rust unit tests for this component -- verification is compile gates and inspection.

## Verification Tests

### V-01: Version string correct (R-05, AC-01)
- **Assert**: `Cargo.toml` contains `version = "=1.7.0"` in the rmcp dependency line
- **Method**: `grep 'version = "=1.7.0"' crates/unimatrix-server/Cargo.toml`

### V-02: All 6 feature flags present (AC-01)
- **Assert**: rmcp dependency line includes `server`, `client`, `transport-io`, `macros`, `transport-streamable-http-server`, `transport-streamable-http-server-session`
- **Method**: Inspect Cargo.toml rmcp features array

### V-03: Cargo.lock resolves rmcp 1.7.0 (R-05)
- **Assert**: `Cargo.lock` contains `name = "rmcp"` with `version = "1.7.0"`
- **Method**: `grep -A1 'name = "rmcp"' Cargo.lock | grep '1.7.0'`

### V-04: Single http crate version (R-10, AC-07 support)
- **Assert**: `cargo tree -i http` shows only one `http` version (1.x.x)
- **Method**: `cargo tree -i http 2>&1 | grep '^http ' | wc -l` equals 1

### V-05: Workspace compiles (R-07, R-11, AC-01)
- **Assert**: `cargo build --workspace` exits 0
- **Method**: Compile gate

### V-06: UDS transport compiles (R-07, AC-06)
- **Assert**: No explicit `transport-async-rw` feature added to Cargo.toml
- **Method**: `grep -c 'transport-async-rw' crates/unimatrix-server/Cargo.toml` equals 0

### V-07: ErrorData::invalid_params unchanged (R-11, AC-04)
- **Assert**: `git diff crates/unimatrix-server/src/mcp/tools.rs` shows zero changes to `ErrorData::invalid_params` call sites
- **Method**: Diff review -- 8 call sites unmodified

## Edge Cases

- If rmcp 1.7.0 pulls in `http` 2.x transitively, extension propagation (R-01) silently breaks with no compile error. V-04 catches this.
- If `transport-async-rw` is no longer transitively enabled, UDS fails at runtime even if it compiles. V-05 + existing UDS tests cover this.
