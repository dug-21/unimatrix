# Test Plan: C9 — Version Synchronization

## Static Validation Tests

These tests run as part of CI or as a pre-release check.

### Workspace Version Inheritance

- `test_root_cargo_toml_has_workspace_version`: Assert `[workspace.package]` section in root `Cargo.toml` contains `version = "0.5.0"`.
- `test_all_9_crates_use_workspace_version`: For each of the 9 crates in `crates/*/Cargo.toml`, assert `version.workspace = true` is present and no hardcoded `version = "x.y.z"` exists in `[package]`.
- `test_server_crate_inherits_edition`: Assert `crates/unimatrix-server/Cargo.toml` has `edition.workspace = true`.

### npm Version Match

- `test_root_npm_version_matches_cargo`: Parse version from root `Cargo.toml` and from `packages/unimatrix/package.json`. Assert they are equal.
- `test_platform_npm_version_matches_cargo`: Parse version from root `Cargo.toml` and from `packages/unimatrix-linux-x64/package.json`. Assert they are equal.
- `test_optional_dependency_version_matches`: Assert the version specifier for `@dug-21/unimatrix-linux-x64` in root package.json `optionalDependencies` matches the platform package version.

### Binary Version Output

- `test_binary_version_matches_cargo`: Run `./target/release/unimatrix version`. Assert output contains the version from root `Cargo.toml`.

## Implementation

These tests can be implemented as a shell script (`scripts/check-versions.sh`) or as part of the CI pre-publish step. The script:
1. Extracts version from `Cargo.toml` `[workspace.package]`.
2. Compares against all `crates/*/Cargo.toml` (checks for `version.workspace = true`).
3. Compares against all `packages/*/package.json` version fields.
4. Exits 1 with diagnostic if any mismatch.

## Risk Coverage

| Risk ID | Scenario | Test |
|---------|----------|------|
| R-06 | Version drift between Cargo and npm | `test_root_npm_version_matches_cargo`, `test_platform_npm_version_matches_cargo` |
| R-06 | Binary reports wrong version | `test_binary_version_matches_cargo` |
