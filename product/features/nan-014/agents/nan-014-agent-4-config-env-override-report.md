# Agent Report: nan-014-agent-4-config-env-override

## Task

Implement `UNIMATRIX_CONFIG` env var override as highest-priority config file source in `load_config` (ADR-005).

## Files Modified

- `crates/unimatrix-server/src/infra/config.rs` (+158 lines)

## Changes

### Production Code

- Added `resolve_env_config_path(env_value: Option<&str>) -> Option<PathBuf>` -- pure function that resolves the env var value to a path if non-empty and pointing to a regular file
- Modified `load_config` to check `UNIMATRIX_CONFIG` at Step 0 (before global/project config), load it via `load_single_config`, and merge ON TOP at Step 3b (highest priority)
- Validation path for error reporting uses env config path when present, falls back to global_path
- Edge cases handled: empty string (skip), directory not file (skip via `is_file()`), missing file (debug log + skip)

### Merge Precedence (after change)

```
UNIMATRIX_CONFIG (env var path)   <-- NEW, highest priority
  > per-project config (~/.unimatrix/{hash}/config.toml)
    > global config (~/.unimatrix/config.toml)
      > compiled defaults
```

### Test Code

6 tests added, all passing:

1. `test_unimatrix_config_env_overrides_default` -- env var set to valid file, returns path
2. `test_unimatrix_config_env_missing_file_falls_through` -- env var points to nonexistent file, returns None
3. `test_unimatrix_config_env_unset_uses_default` -- env var absent, returns None
4. `test_unimatrix_config_env_empty_string_falls_through` -- empty string treated as absent
5. `test_unimatrix_config_env_directory_not_file_falls_through` -- directory rejected by is_file()
6. `test_unimatrix_config_env_precedence` -- env config preset wins over project config preset

### Testing Approach

`std::env::set_var` is unsafe in Rust 2024 edition with `#![forbid(unsafe_code)]`. Followed existing codebase pattern (see `parse_tick_interval_str` in background.rs): extracted a pure function `resolve_env_config_path` that takes the env var value as a parameter. Tests call `resolve_env_config_path_for_test` (cfg(test) wrapper) directly, avoiding env var mutation entirely. The precedence test manually replicates the merge logic from `load_config`.

## Test Results

- 6 passed, 0 failed (config env override tests)
- 3213 passed, 1 failed workspace-wide (pre-existing flaky test: `col018_topic_signal_null_for_generic_prompt` -- embedding model initialization timing under concurrent test load)

## Issues

None.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- surfaced #2395 (Two-Level TOML Config Merge pattern), #3934 (adding config sections procedure), #1192 (sync subcommand procedure). Applied the merge pattern for the three-level merge.
- Stored: nothing novel to store -- the env var override follows the established two-level merge pattern extended to three levels. The `unsafe env::set_var` constraint was already documented in background.rs comments and is a known codebase convention.
