# Agent Report: nan-015-agent-3-cache-path-resolution

## Files Modified

- `crates/unimatrix-embed/src/config.rs` -- added `UNIMATRIX_MODEL_CACHE` env var check to `resolve_cache_dir()` as priority 2 (ADR-001, ADR-002)

## Implementation Details

### Approach

Extracted `resolve_cache_dir_with_env(env_value: Option<String>)` as a private helper that takes the env var value as a parameter. The public `resolve_cache_dir()` calls `std::env::var("UNIMATRIX_MODEL_CACHE").ok()` and delegates.

This pattern was necessary because:
1. The crate uses `#![forbid(unsafe_code)]`
2. Rust 2024 edition makes `std::env::set_var` unsafe
3. Tests cannot mutate env vars without unsafe blocks

The pattern matches the existing approach in `unimatrix-server/src/infra/config.rs:2214` (`resolve_env_config_path`).

### Four-level precedence chain (ADR-002)

1. `self.cache_dir` field (Some -> return) -- unchanged
2. `UNIMATRIX_MODEL_CACHE` env var (non-empty -> return PathBuf) -- NEW
3. `dirs::cache_dir()` + "unimatrix/models" -- unchanged
4. `.unimatrix/models` fallback -- unchanged

Empty string guard (`!env_dir.is_empty()`) prevents R-07 (PathBuf::from("") producing relative path).

## Tests

- 106 passed, 0 failed, 27 ignored (unimatrix-embed)
- 5 new tests added:
  - `test_resolve_cache_dir_env_var_used_when_field_none` (R-01 scenario 1)
  - `test_resolve_cache_dir_unset_env_falls_to_dirs` (R-01 scenario 2)
  - `test_resolve_cache_dir_config_field_wins_over_env_var` (R-01 scenario 3)
  - `test_resolve_cache_dir_empty_env_var_falls_through` (R-07)
  - `test_resolve_cache_dir_fallback_path_construction` (R-01 scenario 4)
- All existing tests continue to pass unchanged

## Static Verification

### R-02: Call Site Divergence

Grep confirmed all 8 call sites from the Architecture table use `EmbedConfig::default().resolve_cache_dir()` or receive the result as a parameter. No call site constructs a path independently.

### Env Var Name Cross-Check

`UNIMATRIX_MODEL_CACHE` string does NOT yet appear in Dockerfile -- expected, as the Dockerfile agent handles that separately. The string is correct in `config.rs`.

## Issues

None. No blockers.

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] `cargo test --workspace` passes (no new failures)
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in non-test code
- [x] All modified files within scope (config.rs only)
- [x] No `.unwrap()` in non-test code
- [x] Code follows validated pseudocode -- no silent deviations
- [x] Test cases match component test plan expectations
- [x] File is 205 lines (under 500-line limit)
- [x] `cargo clippy` clean (collapsible_if fixed via let-chain syntax)
- [x] `cargo fmt` applied

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-002 (#4651) and ADR-001 (#4650) for cache path precedence. Also found #70 (original cache dir ADR) and #4633 (UNIMATRIX_CONFIG env removal pattern from nxs-013). context_search for patterns returned no directly applicable results.
- Stored: nothing novel to store -- the parameterized-env-var-for-testability pattern already exists in the codebase (config.rs:2214 in unimatrix-server) and is a straightforward application of the existing convention.
