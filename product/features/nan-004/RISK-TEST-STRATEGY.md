# Risk-Based Test Strategy: nan-004 (Versioning & Packaging)

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | settings.json merge corrupts existing user configuration (drops hooks, permissions, or reorders content) | High | High | Critical |
| R-02 | Absolute binary path in hook commands becomes invalid after `node_modules` reinstall or project directory move | High | High | Critical |
| R-03 | Platform binary fails at runtime on clean Ubuntu 22.04 due to missing ONNX Runtime shared library or glibc version mismatch | High | Med | High |
| R-04 | Init command duplicates hook entries on repeated runs (idempotency failure) | High | Med | High |
| R-05 | JS shim `init` interception misroutes non-init subcommands or fails to forward exit codes from the Rust binary | Med | Med | Med |
| R-06 | Version drift between Cargo.toml workspace version and npm package.json versions causes publish failure or version mismatch in the field | Med | Med | Med |
| R-07 | CI pipeline fails due to Rust 1.89 not being available on GitHub Actions runner or `patches/anndists` missing from checkout | Med | High | High |
| R-08 | Postinstall ONNX model download blocks or fails `npm install` in firewalled/CI environments | Med | Med | Med |
| R-09 | `.mcp.json` merge drops existing MCP server entries when adding the unimatrix entry | High | Med | High |
| R-10 | Skill file copy overwrites a user-modified skill file without warning during version upgrade | Low | Med | Low |
| R-11 | Project root detection diverges between JS (init.js) and Rust (detect_project_root) implementations, producing different project hashes | High | Low | Med |
| R-12 | Binary rename breaks existing hook configurations in this repository if changes are not atomic | Med | Low | Low |
| R-13 | `require.resolve` for platform binary fails due to npm hoisting, pnpm, or yarn PnP node_modules layouts | Med | Med | Med |
| R-14 | Malformed `.claude/settings.json` causes init to crash instead of producing a diagnostic error | Med | Med | Med |
| R-15 | npm publish order dependency — root package published before platform package causes install failures for users | High | Med | High |

## Risk-to-Scenario Mapping

### R-01: settings.json Merge Corrupts Existing Configuration
**Severity**: High
**Likelihood**: High
**Impact**: User loses custom hooks, permissions, or tool settings. Silent data loss — user may not notice until a workflow breaks.

**Test Scenarios**:
1. Merge into an empty file (creates full hooks structure).
2. Merge into a file with only a `permissions` block and no `hooks` key — permissions must survive unchanged.
3. Merge into a file with existing non-unimatrix hooks (e.g., a custom `PreToolUse` hook) — custom hooks preserved, unimatrix hooks appended.
4. Merge into a file with pre-rename `unimatrix-server hook` commands — old commands updated in place, not duplicated.
5. Merge into a file with hook entries using absolute paths from a previous init — paths updated in place.
6. Merge into a file with extra top-level keys (arbitrary JSON) — all keys preserved after merge.
7. Round-trip test: merge, read back, merge again — output identical.

**Coverage Requirement**: `merge-settings.js` must have unit tests covering all 7 scenarios above. The merge function must be pure (input JSON + config -> output JSON) for testability.

### R-02: Absolute Binary Path Invalidation
**Severity**: High
**Likelihood**: High
**Impact**: All 7 hooks and the MCP server fail to start. Unimatrix is completely non-functional until user re-runs `npx unimatrix init`.

**Test Scenarios**:
1. Init writes correct absolute path (not symlink, not relative) to `.mcp.json` and all 7 hook commands.
2. Simulate `node_modules` rebuild (delete and reinstall) — hooks fail with a meaningful "binary not found" error from the OS.
3. Init after reinstall repairs all paths.
4. Path resolves through symlinks correctly (npm may symlink `.bin/` entries).

**Coverage Requirement**: Integration test: init -> verify paths -> simulate path break -> re-init -> verify paths restored.

### R-03: Binary Runtime Failure on Clean System
**Severity**: High
**Likelihood**: Med
**Impact**: Binary installs but crashes on first invocation. User sees cryptic linker error. Total adoption blocker.

**Test Scenarios**:
1. Run `ldd` on the release binary — verify no missing shared libraries.
2. Execute the binary in a clean Ubuntu 22.04 Docker container (no dev tools installed) — `unimatrix version` succeeds.
3. Verify ONNX Runtime is statically linked or its `.so` is bundled alongside the binary.
4. Verify Oniguruma (tokenizers `onig` feature) does not depend on a system library not present on Ubuntu 22.04.

**Coverage Requirement**: CI pipeline must include a `ldd` check and a clean-container smoke test as gating steps before npm publish.

### R-04: Init Idempotency Failure (Duplicate Hooks)
**Severity**: High
**Likelihood**: Med
**Impact**: Hook events fire multiple unimatrix hooks per event, causing duplicate context injections, performance degradation, or conflicting responses.

**Test Scenarios**:
1. Run init twice — `.claude/settings.json` has exactly 7 unimatrix hook entries (one per event), not 14.
2. Run init, manually edit a hook command, run init again — the manual edit is overwritten (unimatrix-owned hook is updated), but no duplicate appears.
3. Run init, add a non-unimatrix hook to an event, run init again — non-unimatrix hook preserved, unimatrix hook count unchanged.
4. `.mcp.json` has exactly one `unimatrix` key after two init runs.

**Coverage Requirement**: The ADR-004 prefix-match dedup logic must be tested for each of the 4 regex patterns.

### R-05: JS Shim Routing and Exit Code Passthrough
**Severity**: Med
**Likelihood**: Med
**Impact**: `npx unimatrix hook SessionStart` fails or returns wrong exit code, breaking Claude Code hook contract. Or `npx unimatrix export` accidentally routes to init logic.

**Test Scenarios**:
1. `npx unimatrix init` routes to JS init logic (not the Rust binary).
2. `npx unimatrix hook SessionStart` routes to the Rust binary (not JS init).
3. `npx unimatrix export` routes to the Rust binary.
4. `npx unimatrix` (no args) routes to the Rust binary (MCP server mode).
5. Rust binary exits with code 1 — JS shim propagates exit code 1.
6. Rust binary exits with code 0 — JS shim propagates exit code 0.
7. `npx unimatrix --version` routes to Rust binary, not init.

**Coverage Requirement**: Unit tests for the shim's argv routing logic. Integration test for exit code passthrough.

### R-06: Version Drift Between Cargo.toml and npm package.json
**Severity**: Med
**Likelihood**: Med
**Impact**: Published npm package claims version X but binary reports version Y. Confusing diagnostics. Schema migration may behave unexpectedly if versions are used for gating.

**Test Scenarios**:
1. After `/release` skill runs, all 9 `Cargo.toml` crates report the same version (via `version.workspace = true`).
2. All npm `package.json` files match the Cargo workspace version.
3. CI pipeline rejects publish if versions mismatch.
4. `unimatrix version` output matches the npm package version.

**Coverage Requirement**: CI pre-publish validation step that compares Cargo.toml version to all package.json versions.

### R-07: CI Pipeline Failure (Toolchain or Patch Missing)
**Severity**: Med
**Likelihood**: High
**Impact**: Release pipeline fails. No packages published. Blocks release until manually diagnosed.

**Test Scenarios**:
1. Workflow installs Rust 1.89 explicitly (not `stable` which may be older).
2. Workflow asserts `patches/anndists/` directory exists before running `cargo build`.
3. Workflow fails fast with clear error if patch directory is missing.
4. Workflow fails fast if Rust version is below 1.89.

**Coverage Requirement**: The workflow file must include explicit assertion steps. Test by running the workflow on a PR (dry-run mode, no publish).

### R-08: Postinstall ONNX Download Failure
**Severity**: Med
**Likelihood**: Med
**Impact**: `npm install` hangs or fails in CI/corporate environments. If postinstall fails hard (exit code 1), the entire `npm install` fails.

**Test Scenarios**:
1. Postinstall with network available — model downloaded, exit 0.
2. Postinstall with network blocked — warning on stderr, exit 0.
3. Postinstall with model already cached — skip download, exit 0.
4. Postinstall when platform binary is missing (e.g., unsupported arch) — warn, exit 0.
5. Postinstall with disk full — warn, exit 0 (never exit non-zero).

**Coverage Requirement**: Postinstall must unconditionally exit 0. Every error path wraps in try/catch with stderr warning.

### R-09: .mcp.json Merge Drops Existing Servers
**Severity**: High
**Likelihood**: Med
**Impact**: User's other MCP servers (e.g., filesystem, GitHub) stop working. User must manually restore `.mcp.json`.

**Test Scenarios**:
1. Init on project with no `.mcp.json` — creates file with only unimatrix entry.
2. Init on project with existing `.mcp.json` containing other servers — all servers preserved.
3. Init on project with existing unimatrix entry (path changed) — entry updated, others untouched.
4. Init on `.mcp.json` with nested `env` and `args` fields in other servers — all fields preserved exactly.

**Coverage Requirement**: Unit tests for `.mcp.json` merge logic with multi-server fixtures.

### R-10: Skill File Overwrite Without Warning
**Severity**: Low
**Likelihood**: Med
**Impact**: User customizations to skill files are lost on upgrade. Low severity because skill files are meant to be package-managed, but user may not expect it.

**Test Scenarios**:
1. Init copies all 13 skill directories.
2. Init overwrites existing unimatrix skill files on re-run.
3. Non-unimatrix skill files in `.claude/skills/` are untouched.

**Coverage Requirement**: Integration test verifying file counts and non-unimatrix skill preservation.

### R-11: Project Root Detection Divergence (JS vs Rust)
**Severity**: High
**Likelihood**: Low
**Impact**: JS init computes a different project hash than the Rust binary, creating the database in the wrong directory. MCP server cannot find the database.

**Test Scenarios**:
1. Init from project root — JS and Rust resolve the same path.
2. Init from a subdirectory — both walk up to the same `.git`.
3. Init in a git worktree — both resolve the worktree root, not the `.git` file target.
4. Project root with symlinks in path — both canonicalize to the same real path.

**Coverage Requirement**: Integration test: init creates DB, then start the MCP server — server finds and opens the same DB.

### R-13: require.resolve Fails on Non-Standard Package Managers
**Severity**: Med
**Likelihood**: Med
**Impact**: `npx unimatrix init` or `npx unimatrix hook` fails with "Cannot find module" error. Total failure for pnpm/yarn users.

**Test Scenarios**:
1. `UNIMATRIX_BINARY` env var override works as fallback.
2. `resolve-binary.js` produces a clear error message when resolution fails.
3. Error message includes the attempted package name and supported platforms.

**Coverage Requirement**: Unit test for the `UNIMATRIX_BINARY` fallback path. Error message test.

### R-14: Malformed settings.json Handling
**Severity**: Med
**Likelihood**: Med
**Impact**: Init crashes with an unhandled JSON parse error. User sees a stack trace instead of a diagnostic.

**Test Scenarios**:
1. `settings.json` contains invalid JSON — init prints diagnostic error, does not modify the file, does not crash.
2. `settings.json` is empty (0 bytes) — treated as `{}`.
3. `settings.json` contains JSON but `hooks` key is not an object — diagnostic error.

**Coverage Requirement**: Unit tests for `merge-settings.js` error paths.

### R-15: npm Publish Order Dependency
**Severity**: High
**Likelihood**: Med
**Impact**: If root package `@dug-21/unimatrix` is published before `@dug-21/unimatrix-linux-x64`, users who install immediately get a broken package (optional dep not yet available).

**Test Scenarios**:
1. CI workflow publishes platform package before root package.
2. If platform publish fails, root publish is skipped.
3. Version tag in each package matches before publish.

**Coverage Requirement**: Workflow step ordering must be verified by review. Platform package publish must be a prerequisite for root package publish.

## Integration Risks

- **JS shim <-> Rust binary boundary**: The shim exec's the binary with forwarded args. Argument quoting, special characters in paths (spaces, Unicode), and signal handling (SIGTERM propagation) are all integration risks. The `execFileSync` call must use array args, not shell string interpolation.
- **init.js <-> merge-settings.js <-> filesystem**: Init reads, merges, and writes back. File locking is absent — concurrent init runs could corrupt settings.json. Low likelihood but worth noting.
- **init.js <-> Rust binary for DB creation**: Init calls `unimatrix version --project-dir <root>` to trigger DB creation. If the Rust binary's `--project-dir` flag parsing differs from what init.js passes, the DB is created in the wrong location.
- **CI build artifact <-> npm package**: The binary built in one CI job is downloaded as an artifact in another. Artifact corruption, permission loss (`chmod +x`), or wrong architecture would produce a package with a non-functional binary.
- **optionalDependencies resolution**: npm must resolve the correct platform package based on `os`/`cpu` fields. If the fields are misconfigured, users get no binary or the wrong binary.

## Edge Cases

- **Project with no `.git` directory**: Init must fail with a clear error, not walk to filesystem root.
- **Project root at `/`**: Unlikely but the walk-up algorithm must have a termination condition.
- **`.claude/settings.json` is a directory**: Init must detect and error, not crash.
- **`node_modules` is on a different filesystem/mount**: Absolute paths still work but may break in Docker volume mounts where host and container paths differ.
- **Very long absolute path to binary**: Hook commands in settings.json could exceed shell argument limits if the path is extremely long (unlikely but possible with deeply nested node_modules).
- **Concurrent `npx unimatrix init` runs**: No file locking — could produce corrupted JSON. Low likelihood.
- **Init run with `--dry-run` followed by init run without**: Must produce identical results to running init directly (dry-run must not leave side effects).
- **Binary path contains spaces**: JSON escaping handles this, but shell command strings in hook `command` fields need proper quoting.

## Security Risks

- **Postinstall script execution**: The postinstall runs arbitrary code after `npm install`. While this is standard npm behavior, the postinstall must only download the ONNX model from the expected Hugging Face URL. It must not execute arbitrary network requests or modify files outside `~/.cache/unimatrix-embed/`.
- **Absolute paths in settings.json expose filesystem layout**: Hook commands contain full paths like `/home/user/project/node_modules/...`. This is visible to any tool reading `.claude/settings.json`. Low risk since this file is local-only.
- **npm token in CI**: The `NPM_TOKEN` secret must be scoped to publish only. If leaked, an attacker could publish malicious package versions. Use GitHub environment protection rules.
- **Binary authenticity**: No code signing or checksum verification for the downloaded binary. A compromised npm registry could serve a malicious binary. Mitigated by private scope (smaller attack surface) but worth noting for public publishing.
- **Path traversal in skill file copy**: If skill file names contain `../`, the copy operation could write outside `.claude/skills/`. The init command must normalize paths and reject traversal sequences.
- **Blast radius**: A compromised `unimatrix` binary runs on every Claude Code hook event (7 events, some on every tool call). It has access to the user's filesystem and environment variables. The blast radius is equivalent to arbitrary code execution in the user's shell.

## Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| Platform binary missing (unsupported OS) | JS shim prints supported platforms list, exits 1 |
| ONNX model download fails | Postinstall warns on stderr, exits 0. Server downloads lazily on first start |
| `.mcp.json` is read-only | Init reports permission error with the file path, exits 1 |
| `.claude/settings.json` is malformed JSON | Init warns, refuses to modify, suggests manual fix, exits 1 |
| Project has no `.git` | Init reports "could not find project root", exits 1 |
| Binary crashes on `unimatrix version` | Init reports validation failure with the binary's stderr output |
| `npm publish` fails for platform package | CI halts, root package is not published |
| Version mismatch detected in CI | CI halts with diagnostic listing mismatched files |
| Database migration fails during init | Init reports the migration error from the binary's stderr |
| Disk full during skill copy | Init reports partial failure, lists which files succeeded |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (ONNX runtime shared library) | R-03 | CI includes `ldd` check and clean-container smoke test (C10 build job). Architecture open question #1 acknowledges this needs validation. |
| SR-02 (Oniguruma/glibc portability) | R-03 | NFR-04 specifies Ubuntu 22.04 LTS baseline (glibc 2.35). CI builds natively on ubuntu-latest. |
| SR-03 (Patched anndists in CI) | R-07 | FR-30 requires workflow to verify `patches/anndists` exists before building. |
| SR-04 (Rust 1.89 toolchain in CI) | R-07 | FR-29 specifies `dtolnay/rust-toolchain` with explicit 1.89 pin. |
| SR-05 (nan-004/nan-003 boundary confusion) | — | Addressed by FR-17 (init prints summary suggesting `/unimatrix-init` as next step). Not a testable risk — it is a UX concern. |
| SR-06 (Binary rename breaking change) | R-12 | ADR-002 specifies atomic rename in a single commit. No backward compat shim needed (no external consumers). |
| SR-07 (Two deliverables with different risk profiles) | — | Architecture delivery phasing (4 waves) addresses this. Not a testable risk. |
| SR-08 (settings.json merge complexity) | R-01, R-04, R-14 | ADR-004 specifies prefix-match identification with isolated `merge-settings.js` module and explicit edge case handling. |
| SR-09 (Hook PATH resolution) | R-02 | ADR-001 resolves: absolute paths in all hook commands. Tradeoff documented (re-run init after move/reinstall). |
| SR-10 (Postinstall ONNX in firewalled environments) | R-08 | FR-22 requires postinstall to succeed with warning on failure. NFR-03 requires postinstall never fails npm install. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 11 scenarios |
| High | 4 (R-03, R-04, R-07, R-09, R-15) | 17 scenarios |
| Medium | 5 (R-05, R-06, R-08, R-11, R-13, R-14) | 19 scenarios |
| Low | 2 (R-10, R-12) | 4 scenarios |
