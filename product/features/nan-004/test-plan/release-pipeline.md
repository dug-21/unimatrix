# Test Plan: C10 — Release Pipeline

## YAML Structure Validation

The release pipeline is a GitHub Actions workflow file. Most validation is via code review and dry-run execution.

### Trigger Configuration

- `test_triggers_on_v_tags_only`: Assert `on.push.tags` contains `['v*']`. Assert no `on.pull_request` or `on.push.branches` trigger.

### Build Job

- `test_rust_toolchain_pin`: Assert the workflow uses `dtolnay/rust-toolchain` with version `1.89` (not `stable`).
- `test_patches_assertion_step`: Assert a step exists that runs `test -d patches/anndists` or equivalent before `cargo build`.
- `test_binary_stripped`: Assert a step runs `strip target/release/unimatrix` after build.
- `test_ldd_check_step`: Assert a step runs `ldd target/release/unimatrix` and validates no "not found" entries (R-03).
- `test_cargo_test_in_release`: Assert `cargo test --release` runs as a gating step.

### Package Job

- `test_platform_publish_before_root`: Assert the `npm publish` step for `@dug-21/unimatrix-linux-x64` appears BEFORE the step for `@dug-21/unimatrix` (R-15).
- `test_platform_failure_blocks_root`: Assert the root publish step depends on (or is sequenced after) the platform publish step, so a platform publish failure prevents root publish.
- `test_version_validation_before_publish`: Assert a step compares Cargo.toml version to package.json versions before any `npm publish`.
- `test_executable_permission_set`: Assert `chmod +x` is applied to the binary before packaging.
- `test_npm_token_from_secret`: Assert `NODE_AUTH_TOKEN` or `NPM_TOKEN` is sourced from `${{ secrets.NPM_TOKEN }}`.

### Release Job

- `test_github_release_created`: Assert a step creates a GitHub Release (e.g., `actions/create-release` or `gh release create`).

## Dry-Run Validation

Push a release candidate tag (e.g., `v0.5.0-rc.1`) to trigger the workflow. Verify:
1. Rust 1.89 installed successfully.
2. `patches/anndists` assertion passes.
3. Binary builds and tests pass.
4. `ldd` check passes (no missing libraries).
5. npm publish steps execute (use `--dry-run` flag for publish if available).

## Risk Coverage

| Risk ID | Scenario | Test |
|---------|----------|------|
| R-03 | Binary runtime failure | `test_ldd_check_step` |
| R-07 | CI toolchain/patch missing | `test_rust_toolchain_pin`, `test_patches_assertion_step` |
| R-15 | Publish order wrong | `test_platform_publish_before_root`, `test_platform_failure_blocks_root` |
