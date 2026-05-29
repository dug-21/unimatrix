# Test Plan: cache-path-resolution

**File**: `crates/unimatrix-embed/src/config.rs`
**Function**: `EmbedConfig::resolve_cache_dir(&self) -> PathBuf`

## Unit Tests

All tests go in `crates/unimatrix-embed/src/config.rs` `mod tests`. Tests that set environment variables must use `std::sync::Mutex` or run with `cargo test -- --test-threads=1` to avoid race conditions, since env vars are process-global.

### R-01: Cache Path Precedence (Critical -- 4 scenarios)

Each scenario tests one level of the precedence chain from ADR-002.

**Test 1: Config field wins over env var**
```
Name: test_resolve_cache_dir_config_field_wins_over_env_var
Arrange: Set UNIMATRIX_MODEL_CACHE=/tmp/env-path. Create EmbedConfig with cache_dir: Some("/explicit".into()).
Act: Call resolve_cache_dir().
Assert: Returns PathBuf::from("/explicit"). Env var is ignored.
Risk: R-01 scenario 3
```

**Test 2: Env var used when config field is None**
```
Name: test_resolve_cache_dir_env_var_used_when_field_none
Arrange: Set UNIMATRIX_MODEL_CACHE=/tmp/test-cache. Create EmbedConfig::default() (cache_dir: None).
Act: Call resolve_cache_dir().
Assert: Returns PathBuf::from("/tmp/test-cache").
Risk: R-01 scenario 1
```

**Test 3: Env var unset falls through to dirs::cache_dir()**
```
Name: test_resolve_cache_dir_unset_env_falls_to_dirs
Arrange: Remove UNIMATRIX_MODEL_CACHE from environment. Create EmbedConfig::default().
Act: Call resolve_cache_dir().
Assert: Return value contains "unimatrix" and "models" in the path (dirs::cache_dir() + suffix). On Linux, this is typically ~/.cache/unimatrix/models.
Risk: R-01 scenario 2, R-12
```

**Test 4: Last-resort fallback when dirs returns None**
```
Name: test_resolve_cache_dir_fallback_when_dirs_unavailable
Note: This scenario is hard to test directly because dirs::cache_dir() depends on HOME.
Arrange: Unset UNIMATRIX_MODEL_CACHE. Unset HOME (and XDG_CACHE_HOME on Linux). Create EmbedConfig::default().
Act: Call resolve_cache_dir().
Assert: Returns PathBuf::from(".unimatrix").join("models").
Risk: R-01 scenario 4
Caveat: May need cfg(test) mock or conditional assertion depending on platform behavior of dirs::cache_dir() when HOME is unset.
```

### R-07: Empty String Env Var Guard (Med -- 1 scenario)

```
Name: test_resolve_cache_dir_empty_env_var_falls_through
Arrange: Set UNIMATRIX_MODEL_CACHE="". Create EmbedConfig::default().
Act: Call resolve_cache_dir().
Assert: Does NOT return PathBuf::from(""). Returns the dirs::cache_dir() path (contains "unimatrix/models").
Risk: R-07
```

## Static Verification

### R-02: Call Site Divergence (High)

```
Method: Grep codebase for all uses of resolve_cache_dir, ensure_model, ensure_nli_model, and cache_dir.
Command: grep -rn "resolve_cache_dir\|cache_dir\|ensure_model\|ensure_nli_model" crates/unimatrix-server/src/ crates/unimatrix-embed/src/
Assert:
  1. All call sites from the Architecture table (8 sites) use EmbedConfig::default().resolve_cache_dir() or receive the result as a parameter.
  2. No call site constructs a path like PathBuf::from("/shared/models") or PathBuf::from("/data/.cache/unimatrix/models") directly.
  3. No call site reads UNIMATRIX_MODEL_CACHE independently of resolve_cache_dir().
Risk: R-02
```

### R-04: NLI Hash Verification Through New Path (High)

```
Method: Code inspection of NLI startup path.
Files: crates/unimatrix-server/src/infra/nli_handle.rs
Assert:
  1. spawn_load_task() calls SHA-256 verification BEFORE Session::builder().commit_from_file().
  2. The cache_dir used for NLI comes from resolve_cache_dir() (via NliConfig.cache_dir).
  3. Verify-then-load ordering (lesson #4642) is preserved -- hash check on the file at the resolved path, not a hardcoded path.
Risk: R-04
```

### R-08: Partial File Corruption Handling (Med)

```
Method: Code inspection of ensure_model() and ensure_nli_model() in download.rs.
Assert:
  1. File existence check uses non-zero size (not just path exists).
  2. ONNX session load failure triggers retry state machine (Loading -> Failed -> Retrying).
  3. NLI path has SHA-256 check that catches corrupt files before ONNX load.
Risk: R-08
Note: No runtime test needed -- existing retry behavior is unchanged. Inspection confirms the path still works with the new cache directory.
```

## Env Var Name Consistency (Cross-Component)

```
Method: String comparison.
Assert: The env var name in config.rs (std::env::var("UNIMATRIX_MODEL_CACHE")) exactly matches the Dockerfile ENV directive (ENV UNIMATRIX_MODEL_CACHE=/shared/models).
Risk: Integration risk from RISK-TEST-STRATEGY.md -- misspelling causes silent fallback.
```

## Test Execution Notes

- Env var tests MUST serialize (--test-threads=1 or internal mutex) because std::env::set_var / std::env::remove_var are process-global.
- The `serial_test` crate or a simple Mutex guard can enforce this without --test-threads=1.
- Existing tests in config.rs (test_resolve_cache_dir_custom, test_resolve_cache_dir_default) should remain -- they cover the pre-nan-015 behavior and continue to pass.
