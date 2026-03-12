# Test Plan: C1 — npm Package Structure

## Unit Tests

### package.json Validation (root: @dug-21/unimatrix)

- `test_root_package_has_bin_field`: Assert `package.json` contains `"bin": { "unimatrix": "bin/unimatrix.js" }`.
- `test_root_package_has_optional_dependencies`: Assert `optionalDependencies` contains `"@dug-21/unimatrix-linux-x64"` with matching version.
- `test_root_package_has_postinstall`: Assert `scripts.postinstall` is `"node postinstall.js"`.
- `test_root_package_version_is_0_5_0`: Assert `version` field equals `"0.5.0"`.
- `test_root_package_scope_is_dug_21`: Assert `name` equals `"@dug-21/unimatrix"`.

### package.json Validation (platform: @dug-21/unimatrix-linux-x64)

- `test_platform_package_os_field`: Assert `os` is `["linux"]`.
- `test_platform_package_cpu_field`: Assert `cpu` is `["x64"]`.
- `test_platform_package_version_matches_root`: Assert platform version equals root version.
- `test_platform_package_has_no_dependencies`: Assert `dependencies` and `devDependencies` are absent or empty.

### Skills Directory

- `test_skills_directory_has_13_entries`: Assert `packages/unimatrix/skills/` contains exactly 13 subdirectories.
- `test_each_skill_has_skill_md`: Assert every skill subdirectory contains a `SKILL.md` file.

## Integration Tests

None. Package structure is validated by static assertions on file contents.

## Edge Cases

- AC-12: `optionalDependencies` pattern must use exact version (not range) so platform and root packages are always in lockstep.

## Risk Coverage

| Risk ID | Scenario | Test |
|---------|----------|------|
| R-15 | Package structure enables correct publish order | `test_platform_package_version_matches_root` |
