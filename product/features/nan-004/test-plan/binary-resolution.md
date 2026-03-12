# Test Plan: C3 — Binary Resolution

## Unit Tests (packages/unimatrix/test/resolve-binary.test.js)

### Platform Detection

- `test_linux_x64_resolves_correct_package`: On linux/x64, `resolveBinary()` attempts `require.resolve('@dug-21/unimatrix-linux-x64/bin/unimatrix')`.
- `test_resolved_path_is_absolute`: Assert the returned path starts with `/`.

### UNIMATRIX_BINARY Env Fallback

- `test_env_override_takes_precedence`: Set `process.env.UNIMATRIX_BINARY = '/custom/path/unimatrix'`. Assert `resolveBinary()` returns `'/custom/path/unimatrix'` without calling `require.resolve`.
- `test_env_override_with_nonexistent_path_throws`: Set `process.env.UNIMATRIX_BINARY = '/nonexistent/path/unimatrix'`. Assert `resolveBinary()` throws with message containing `UNIMATRIX_BINARY points to non-existent file`.

### Error Cases

- `test_unsupported_platform_throws`: Mock `process.platform = 'win32'`, `process.arch = 'x64'`. Assert `resolveBinary()` throws with message containing `Supported platforms`.
- `test_missing_package_throws_with_platform_info`: Mock `require.resolve` to throw `MODULE_NOT_FOUND`. Assert error message includes `@dug-21/unimatrix-linux-x64` and `linux-x64`.
- `test_error_message_lists_all_supported_platforms`: Assert the error message includes every key from the PLATFORMS map.

### Platform Map

- `test_platform_map_contains_linux_x64`: Assert the internal PLATFORMS map has exactly the entry `"linux-x64": "@dug-21/unimatrix-linux-x64"`.

## Risk Coverage

| Risk ID | Scenario | Test |
|---------|----------|------|
| R-02 | Binary path is absolute | `test_resolved_path_is_absolute` |
| R-02 | Env override with invalid path throws | `test_env_override_with_nonexistent_path_throws` |
| R-13 | require.resolve fails on non-standard layouts | `test_env_override_takes_precedence`, `test_missing_package_throws_with_platform_info` |
| R-13 | Clear error on resolution failure | `test_error_message_lists_all_supported_platforms` |
