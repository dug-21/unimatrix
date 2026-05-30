# Component: cargo-version-bump

## Purpose

Change the rmcp version pin from `=0.16.0` to `=1.7.0` in `crates/unimatrix-server/Cargo.toml`. This is the root change that triggers all downstream compilation fixes.

## Change

**File**: `crates/unimatrix-server/Cargo.toml`, line 33

```
// Before:
rmcp = { version = "=0.16.0", features = ["server", "client", "transport-io", "macros", "transport-streamable-http-server", "transport-streamable-http-server-session"] }

// After:
rmcp = { version = "=1.7.0", features = ["server", "client", "transport-io", "macros", "transport-streamable-http-server", "transport-streamable-http-server-session"] }
```

The features list is unchanged. All 6 features are verified present in rmcp 1.7.0.

## Verification

1. `cargo update -p rmcp` resolves to 1.7.0
2. `cargo build -p unimatrix-server` -- expected to fail at struct literal sites (this is correct; subsequent components fix those)
3. No other dependency lines change -- `http = "1"`, `schemars = "1"` remain compatible

## Error Handling

Not applicable -- this is a declarative config change.

## Key Test Scenarios

1. After all components are applied: `cargo build --workspace` succeeds
2. `cargo tree -i http` shows a single `http` major version (no duplication)
3. Cargo.lock shows rmcp 1.7.0, not 0.16.x
