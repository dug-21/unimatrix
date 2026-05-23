# Test Plan: config-env-override

## Component

Config loading path (likely `crates/unimatrix-server/src/infra/config.rs` or a new helper in `main.rs`). Adds `UNIMATRIX_CONFIG` env var as the highest-priority config file source, checked before `dirs::config_dir()` fallback.

## Risk Coverage

| Risk | Scenario | Test |
|------|----------|------|
| R-13 (Med) | HOME=/data causes config_dir() to miss /etc/unimatrix/config.toml | `test_unimatrix_config_env_overrides_home_based_resolution` |
| R-13 (Med) | UNIMATRIX_CONFIG not set falls back to existing behavior | `test_config_fallback_when_env_var_absent` |

## Unit Tests

### test_unimatrix_config_env_overrides_home_based_resolution

**Arrange**:
- Create a temp directory simulating `/etc/unimatrix/`
- Write a valid `config.toml` to that path
- Set `UNIMATRIX_CONFIG` env var to that path
- Set `HOME` to a different temp directory (simulating `/data`)

**Act**: Call the config resolution function (the new `resolve_config_path()` or equivalent).

**Assert**:
- Returned path equals the `UNIMATRIX_CONFIG` value
- Does NOT return a path under `$HOME/.config/`

Note: Use `std::env::set_var` / `std::env::remove_var` with care -- these are not thread-safe in Rust tests. If needed, isolate in a separate test binary or use `#[serial_test::serial]`.

### test_config_fallback_when_env_var_absent

**Arrange**:
- Ensure `UNIMATRIX_CONFIG` is NOT set (remove if present)
- Set `HOME` to a temp directory

**Act**: Call the config resolution function.

**Assert**:
- The function falls back to the existing `dirs::config_dir()` / home-based path resolution
- Does NOT error when `UNIMATRIX_CONFIG` is absent

### test_unimatrix_config_env_nonexistent_file_falls_through

**Arrange**:
- Set `UNIMATRIX_CONFIG` to a path that does not exist (e.g., `/tmp/nonexistent/config.toml`)

**Act**: Call the config resolution function.

**Assert**:
- The function falls through to the next config source (does not error, does not return the nonexistent path)
- The Implementation Brief specifies: `if p.exists() { return Some(p); }` -- a nonexistent UNIMATRIX_CONFIG path is silently skipped

### test_load_config_with_unimatrix_config_env

**Arrange**:
- Create a temp data directory with the standard `.unimatrix/` structure
- Write a valid config.toml with a non-default setting (e.g., `[server] tick_interval_secs = 120`)
- Set `UNIMATRIX_CONFIG` to that file
- Set `HOME` to a different temp dir (no config there)

**Act**: Call `load_config(home_dir, data_dir)` (or the modified version that checks `UNIMATRIX_CONFIG` first).

**Assert**:
- The loaded config reflects the value from the `UNIMATRIX_CONFIG` file (tick_interval_secs = 120)
- The global config path under `$HOME/.unimatrix/config.toml` was NOT used

## Integration Tests

No new infra-001 tests. Config loading happens at startup before MCP tools register. The env var override has no MCP-visible behavioral change -- it only affects which config file is read.

## Edge Cases

- **UNIMATRIX_CONFIG set to empty string**: Should be treated as absent (fall through to default resolution).
- **UNIMATRIX_CONFIG set to a directory, not a file**: `p.exists()` returns true for directories. The config loader should check `p.is_file()` or handle the TOML parse error gracefully.
- **UNIMATRIX_CONFIG set to a valid file with invalid TOML**: Should produce a `ConfigError`, not silently fall through. The env var path, once validated as existing, should be treated as authoritative.
