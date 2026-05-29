# cache-path-resolution -- Pseudocode

## Purpose

Add `UNIMATRIX_MODEL_CACHE` environment variable support to `EmbedConfig::resolve_cache_dir()` as the second priority in the four-level cache path resolution chain (ADR-001, ADR-002). This is the sole Rust code change for nan-015. All call sites already use `EmbedConfig::default()` with `cache_dir: None`, so the env var insertion captures all of them without modifying any call site.

## File

`crates/unimatrix-embed/src/config.rs`

## Modified Function: `resolve_cache_dir`

Current implementation (lines 40-51):

```
fn resolve_cache_dir(&self) -> PathBuf:
    if self.cache_dir is Some(dir):
        return dir.clone()

    if dirs::cache_dir() is Some(cache):
        return cache / "unimatrix" / "models"

    return ".unimatrix" / "models"
```

New implementation -- insert env var check between steps 1 and 3:

```
fn resolve_cache_dir(&self) -> PathBuf:
    // Step 1: Explicit config field (highest priority -- test overrides, operator config)
    if self.cache_dir is Some(dir):
        return dir.clone()

    // Step 2: Container redirect via environment variable (ADR-001)
    // Empty string is treated as unset (ADR-002 invariant, R-07 guard)
    let env_result = std::env::var("UNIMATRIX_MODEL_CACHE")
    if env_result is Ok(env_dir) AND env_dir is not empty:
        return PathBuf::from(env_dir)

    // Step 3: Platform-specific default (unchanged)
    if dirs::cache_dir() is Some(cache):
        return cache / "unimatrix" / "models"

    // Step 4: Last resort fallback (unchanged)
    return ".unimatrix" / "models"
```

### Implementation Notes

- The env var name `UNIMATRIX_MODEL_CACHE` must be a literal string, not a constant -- it is only referenced once in Rust code. The Dockerfile references the same string; keeping it as a literal in both places makes grep-based verification straightforward.
- `std::env::var()` returns `Err` when the variable is unset and `Ok("")` when set to empty string. Both cases must fall through to step 3.
- The `!env_dir.is_empty()` guard prevents `PathBuf::from("")` which would produce a relative path at the filesystem root (R-07).
- No `tracing` log added for env var resolution -- this function is called during startup (not hot path) and the resolved path is logged downstream by callers.

## Doc Comment Update

Update the `resolve_cache_dir` doc comment (lines 34-39) to document the new env var step:

```
/// Resolve the cache directory.
///
/// Resolution precedence (ADR-002):
/// 1. `cache_dir` field (explicit config or test override)
/// 2. `UNIMATRIX_MODEL_CACHE` env var (container redirect, empty = unset)
/// 3. `dirs::cache_dir()` platform default + `unimatrix/models`
/// 4. `.unimatrix/models` fallback
```

## Error Handling

`resolve_cache_dir()` returns `PathBuf` (infallible). No new error paths introduced. All four resolution steps produce a valid `PathBuf`. The caller (`ensure_model`, `ensure_nli_model`) handles filesystem errors when accessing the resolved path.

## Key Test Scenarios

All tests use `temp_env` or `std::env::set_var`/`remove_var` with serial test execution (env vars are process-global).

### T-01: Env var set and non-empty returns env var path (R-01 scenario 1)

```
set UNIMATRIX_MODEL_CACHE = "/tmp/test-cache"
config = EmbedConfig { cache_dir: None, ..default }
result = config.resolve_cache_dir()
assert result == PathBuf("/tmp/test-cache")
```

### T-02: Env var unset falls through to dirs (R-01 scenario 2, R-12)

```
unset UNIMATRIX_MODEL_CACHE
config = EmbedConfig { cache_dir: None, ..default }
result = config.resolve_cache_dir()
assert result contains "unimatrix" and "models"  // dirs::cache_dir() path
```

### T-03: Config field wins over env var (R-01 scenario 3)

```
set UNIMATRIX_MODEL_CACHE = "/tmp/env-path"
config = EmbedConfig { cache_dir: Some("/explicit"), ..default }
result = config.resolve_cache_dir()
assert result == PathBuf("/explicit")
```

### T-04: Empty env var treated as unset (R-07)

```
set UNIMATRIX_MODEL_CACHE = ""
config = EmbedConfig { cache_dir: None, ..default }
result = config.resolve_cache_dir()
assert result != PathBuf("")
assert result contains "unimatrix" and "models"  // fell through to dirs
```

### T-05: Existing tests continue to pass

The existing `test_resolve_cache_dir_custom` and `test_resolve_cache_dir_default` tests (lines 98-117) must continue passing unchanged. They test steps 1 and 3 respectively; the env var insertion at step 2 does not affect them as long as `UNIMATRIX_MODEL_CACHE` is not set in the test environment.

**Note**: If the CI environment happens to set `UNIMATRIX_MODEL_CACHE`, `test_resolve_cache_dir_default` would fail. The implementation agent should ensure T-02 unsets the variable to confirm fallthrough behavior, and T-05 should explicitly unset it before running.
