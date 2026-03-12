# Risk Coverage Report: nan-004

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | settings.json merge corrupts existing user configuration | merge-settings.test.js: 7 R-01 scenarios (empty, permissions, non-unimatrix hooks, pre-rename, abs path update, extra keys, round-trip) + identification patterns (5 tests) + dedup (2 tests) + edge cases (5 tests) | PASS (22 tests) | Full |
| R-02 | Absolute binary path invalidation after reinstall | init.test.js: writeMcpJson tests (4 tests verify abs path), resolve-binary.test.js: env override + symlink resolution | PASS (7 tests) | Full |
| R-03 | Binary runtime failure on clean Ubuntu 22.04 | release.yml includes ldd check step; not locally testable (CI-only) | N/A (CI) | Partial — CI pipeline validates; local ldd not executed |
| R-04 | Init idempotency failure (duplicate hooks) | merge-settings.test.js: test_merge_idempotent_round_trip, test_three_consecutive_merges_no_growth, test_each_event_has_exactly_one_unimatrix_entry, test_dedup_removes_extra_unimatrix_hooks; init.test.js: test_init_idempotent | PASS (5 tests) | Full |
| R-05 | JS shim routing and exit code passthrough | shim.test.js: 7 routing tests (init, init --dry-run, hook, export, no-args, version, --version) + 3 exit code tests + 3 error handling tests | PASS (13 tests) | Full |
| R-06 | Version drift between Cargo.toml and npm package.json | Static verification: Cargo.toml workspace version = 0.5.0, all 9 crates use version.workspace = true, both package.json files = 0.5.0, binary outputs "unimatrix 0.5.0" | PASS (verified) | Full |
| R-07 | CI pipeline failure (toolchain or patch missing) | release.yml validated as valid YAML; step ordering reviewed (Rust 1.89 pin, patches/anndists assertion present) | PASS (structural) | Partial — full validation requires CI dry-run |
| R-08 | Postinstall ONNX download failure | postinstall.test.js: binary calls model-download, network failure exits 0, binary missing exits 0, disk full exits 0, model cached succeeds, try/catch wraps all code | PASS (6 tests) | Full |
| R-09 | .mcp.json merge drops existing servers | init.test.js: writeMcpJson tests — creates on clean project, preserves existing servers, updates unimatrix entry, preserves nested env/args, malformed throws, dry-run no-op | PASS (6 tests) | Full |
| R-10 | Skill file overwrite without warning | init.test.js: copySkills tests — copies skill dirs, overwrites existing unimatrix skills, preserves non-unimatrix skills, dry-run no-op | PASS (4 tests) | Full |
| R-11 | Project root detection divergence (JS vs Rust) | init.test.js: detectProjectRoot tests — finds .git in current dir, walks up to .git, no .git errors, stops at filesystem root | PASS (4 tests) | Partial — JS-only; full JS-Rust agreement requires end-to-end test |
| R-12 | Binary rename breaks existing hook configurations | Rust CLI parsing tests pass; .mcp.json references "unimatrix" (verified); .claude/settings.json references "unimatrix hook" (verified); binary name = "unimatrix" (verified); all infra-001 suites pass after harness update | PASS | Full |
| R-13 | require.resolve fails on non-standard package managers | resolve-binary.test.js: UNIMATRIX_BINARY env fallback, nonexistent path throws, unsupported platform throws with platform list, missing package throws | PASS (6 tests) | Full |
| R-14 | Malformed settings.json handling | merge-settings.test.js: malformed JSON errors with diagnostic, empty file treated as {}, hooks key not object errors, hooks key array errors | PASS (4 tests) | Full |
| R-15 | npm publish order dependency | release.yml structural review: platform package publish step precedes root package publish | PASS (structural) | Partial — full validation requires CI execution |

## Test Results

### Rust Unit Tests (cargo test --workspace)
- Total: 2253 (2235 + 18 ignored)
- Passed: 2235
- Failed: 0
- Ignored: 18

### JavaScript Unit Tests (node --test)
- Total: 81
- Passed: 81
- Failed: 0

### Integration Tests (infra-001)

#### Smoke Suite (-m smoke)
- Total: 19
- Passed: 18
- Failed: 0
- Xfail: 1 (GH#111 — pre-existing volume rate limit)

#### Protocol Suite
- Total: 13
- Passed: 13
- Failed: 0

#### Tools Suite
- Total: 71
- Passed: 70
- Failed: 0
- Xfail: 1 (pre-existing: test_status_includes_observation_fields)

#### Lifecycle Suite
- Total: 16
- Passed: 16
- Failed: 0

#### Integration Total (deduplicated across suites)
- Suites run: smoke, protocol, tools, lifecycle
- Unique tests: ~100 (some overlap in smoke subset)
- All passing or pre-existing xfail

### Harness Update
- Updated `product/test/infra-001/harness/conftest.py`: `_resolve_binary()` now searches for `unimatrix` instead of `unimatrix-server` in `target/release/` and `target/debug/`.

## Static Verifications

| Check | Result | Detail |
|-------|--------|--------|
| All 9 Cargo.toml crates use version.workspace = true | PASS | 9/9 crates confirmed |
| Workspace version = 0.5.0 | PASS | Root Cargo.toml [workspace.package] |
| npm package.json versions = 0.5.0 | PASS | Both @dug-21/unimatrix and @dug-21/unimatrix-linux-x64 |
| Binary outputs "unimatrix 0.5.0" | PASS | Both debug and release builds |
| .mcp.json references "unimatrix" (not unimatrix-server) | PASS | Command: target/release/unimatrix |
| .claude/settings.json references "unimatrix hook" (not unimatrix-server) | PASS | All 7 hook events use "unimatrix hook {event}" |
| release.yml is valid YAML | PASS | python yaml.safe_load succeeds |
| optionalDependencies in root package.json | PASS | @dug-21/unimatrix-linux-x64 present |
| Platform package has os/cpu fields | PASS | os: ["linux"], cpu: ["x64"] |
| /release skill exists | PASS | .claude/skills/release/SKILL.md present |
| CHANGELOG.md exists | N/A | Not yet generated — created on first /release run |

## Gaps

| Risk ID | Gap | Explanation |
|---------|-----|-------------|
| R-03 | No local ldd or clean-container test | This risk is CI-only by design. The release pipeline includes ldd validation and clean-container smoke test. Cannot be validated locally without Docker. |
| R-07 | No CI dry-run execution | Workflow structure validated (valid YAML, correct step ordering), but actual execution requires pushing a v* tag to GitHub Actions. |
| R-11 | No cross-runtime (JS + Rust) path agreement test | JS project root detection tested in isolation. Full agreement between JS and Rust path resolution requires end-to-end test with both runtimes in sequence. Low likelihood risk. |
| R-15 | No CI execution of publish ordering | Publish step ordering verified structurally in the YAML. Actual execution ordering requires CI run. |

No high-priority risks have coverage gaps. R-03 and R-07 are CI-only by design. R-11 is low-likelihood. R-15 is structurally verified.

## Xfail References

| Test | Issue | Suite | Reason |
|------|-------|-------|--------|
| test_store_1000_entries | GH#111 | volume (via smoke) | Pre-existing: rate limit blocks volume test |
| test_status_includes_observation_fields | Pre-existing | tools | Pre-existing: observation field assertion |

No new xfail markers were added by nan-004. Both are pre-existing.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS (structural) | packages/unimatrix/package.json has optionalDependencies with os/cpu fields. Platform package structure verified. Full npm install requires publish. |
| AC-02 | PASS | postinstall.test.js: 6 tests covering download success, network failure (exit 0), binary missing (exit 0), disk full (exit 0), model cached. |
| AC-03 | PASS | init.test.js writeMcpJson: creates .mcp.json with abs path, preserves existing servers, updates existing unimatrix entry. |
| AC-04 | PASS | merge-settings.test.js: 22+ tests covering all 7 merge scenarios, identification patterns, dedup, error handling, edge cases. All 7 hook events verified. |
| AC-05 | PASS | init.test.js copySkills: copies skill dirs, overwrites existing, preserves non-unimatrix skills, dry-run no-op. |
| AC-06 | PASS (structural) | init.test.js: test_full_init_creates_mcp_and_settings validates the init flow. DB creation uses unimatrix binary which is verified working. |
| AC-07 | PASS | init.test.js: test_reports_diagnostic_on_validation_failure verifies diagnostic error on binary failure. Binary validation via "unimatrix version" confirmed working. |
| AC-08 | PASS | init.test.js: test_init_idempotent + merge-settings.test.js: idempotent round trip + three consecutive merges no growth + dedup removes extra hooks. |
| AC-09 | PASS | resolve-binary.test.js: platform map, env override, error messages, symlink resolution. shim.test.js: routing and error handling. |
| AC-10 | PASS (structural) | .github/workflows/release.yml exists and is valid YAML. Rust 1.89 pin, patches assertion, build/package/publish steps present. Full execution requires tag push. |
| AC-11 | PASS | Cargo.toml workspace version = 0.5.0, both package.json = 0.5.0, binary = "unimatrix 0.5.0". |
| AC-12 | PASS | packages/unimatrix/package.json has optionalDependencies. packages/unimatrix-linux-x64/package.json has os: ["linux"], cpu: ["x64"]. |
| AC-13 | PASS | init.test.js printSummary: test_prints_unimatrix_init_suggestion verifies "/unimatrix-init" suggestion in output. |
| AC-14 | PASS | init.test.js: test_dry_run_does_not_write_files. merge-settings.test.js: test_dry_run_does_not_write_file, test_dry_run_returns_actions_and_content. copySkills: test_dry_run_does_not_copy_skills. writeMcpJson: test_dry_run_does_not_write_mcp_json. |
| AC-15 | PASS | All 9 crates confirmed version.workspace = true. Root Cargo.toml version = "0.5.0". |
| AC-16 | PASS | .claude/skills/release/SKILL.md exists. |
| AC-17 | N/A | CHANGELOG.md not yet generated. Created on first /release invocation. Not a test failure — it is a runtime artifact. |
