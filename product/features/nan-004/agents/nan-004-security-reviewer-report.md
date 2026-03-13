# Security Review: nan-004-security-reviewer

## Risk Level: low

## Summary

nan-004 introduces npm packaging, binary rename, init command, and release pipeline. The changes are primarily distribution infrastructure with minimal security surface. No new external inputs are processed by the Rust server itself. The JS init command operates on local project files with appropriate validation. No blocking findings.

## Findings

### Finding 1: Skill directory name not checked for path traversal
- **Severity**: low
- **Location**: packages/unimatrix/lib/init.js:133-134
- **Description**: The `copySkills` function checks file names within skill directories for `..` traversal, but does not check the skill directory names themselves (`skillDir` variable from `readdirSync`). If a malicious skill directory name contained `..`, the `path.join(targetDir, skillDir)` destination could escape `.claude/skills/`. However, the source directory is `packages/unimatrix/skills/` which is package-controlled content, so exploitation requires a compromised package -- at which point the attacker already has arbitrary code execution via postinstall.
- **Recommendation**: For defense-in-depth, add the same `includes("..")` check for `skillDir` names. Low priority since the threat model requires a compromised npm package.
- **Blocking**: no

### Finding 2: is_unimatrix_process broadened match could match unrelated binaries
- **Severity**: low
- **Location**: crates/unimatrix-server/src/infra/pidfile.rs:161
- **Description**: The `is_unimatrix_process` check now matches any process whose binary filename is `"unimatrix"`. This is broader than the previous `"unimatrix-server"` match. A process from a different project named `unimatrix` would be incorrectly identified, potentially receiving SIGTERM during stale PID cleanup. The legacy `"unimatrix-server"` fallback is correctly preserved.
- **Recommendation**: Acceptable risk. The PID file path is project-specific (`~/.unimatrix/{hash}/`), making collisions extremely unlikely. No action needed.
- **Blocking**: no

### Finding 3: Postinstall correctly exits 0 on all error paths
- **Severity**: informational
- **Location**: packages/unimatrix/postinstall.js
- **Description**: Verified that all error paths in postinstall exit with code 0, as required by R-08 in the risk strategy. This prevents npm install failures in firewalled environments. The outer try-catch and final `process.exit(0)` ensure this.
- **Recommendation**: None -- this is correct.
- **Blocking**: no

### Finding 4: No command injection in hook commands
- **Severity**: informational
- **Location**: packages/unimatrix/lib/merge-settings.js:111, packages/unimatrix/bin/unimatrix.js:32
- **Description**: Hook commands are constructed via string concatenation (`binaryPath + " hook " + event`). The `binaryPath` comes from `require.resolve` or `UNIMATRIX_BINARY` env var, and `event` comes from a hardcoded constant array. The JS shim uses `execFileSync` (not shell execution), which prevents shell injection. The settings.json hook commands are interpreted by Claude Code's shell, but the binary path and event names are controlled by the init process, not user input.
- **Recommendation**: None -- command construction is safe.
- **Blocking**: no

### Finding 5: NPM_TOKEN handled via GitHub secrets
- **Severity**: informational
- **Location**: .github/workflows/release.yml:128,134
- **Description**: The NPM_TOKEN is properly referenced via `${{ secrets.NPM_TOKEN }}` and not hardcoded. The workflow uses `--access restricted` for both packages, limiting exposure.
- **Recommendation**: None -- secrets handling is correct.
- **Blocking**: no

### Finding 6: Release workflow permissions are appropriately scoped
- **Severity**: informational
- **Location**: .github/workflows/release.yml:10-11
- **Description**: The workflow has `contents: write` permission at the top level, which is needed for creating GitHub releases. This is the minimum permission needed. The workflow is triggered only on `v*` tags, limiting when it runs.
- **Recommendation**: None -- appropriate scope.
- **Blocking**: no

### Finding 7: No new Rust dependencies introduced
- **Severity**: informational
- **Location**: Cargo.lock diff
- **Description**: The Cargo.lock changes are version bumps only (0.1.0 to 0.5.0 across all workspace crates). No new external dependencies were added. The `unimatrix_embed::ensure_model` was already implemented -- it is only newly re-exported as `pub`.
- **Recommendation**: None.
- **Blocking**: no

## Blast Radius Assessment

**Worst case if the init command has a subtle bug**: Corrupted `.claude/settings.json` or `.mcp.json` causing all Claude Code hooks and the MCP server to stop working for the project. Recovery is manual editing or deleting the corrupted file and re-running init. This is a local-only impact -- no data loss, no remote exposure.

**Worst case if the binary rename has a subtle bug**: The `is_unimatrix_process` check might fail to identify a stale process, leading to a lock contention error on database open. The retry logic (3 attempts with exponential backoff) mitigates this. Worst case is a failed server start requiring manual PID cleanup.

**Worst case if the release pipeline has a bug**: Incorrect package published to npm. Mitigated by version validation step and restricted access scope. Recovery is `npm unpublish` within 72 hours or a corrective publish.

## Regression Risk

- **Binary rename**: The binary name changes from `unimatrix-server` to `unimatrix`. All internal references (.mcp.json, .claude/settings.json, hook commands, error messages, PID check) are updated. The `is_unimatrix_process` check retains backward compatibility by matching both names. Regression risk is low.
- **Version workspace inheritance**: All 9 crates move from hardcoded `version = "0.1.0"` to `version.workspace = true`. This is a Cargo-level change that does not affect runtime behavior. Cargo.lock confirms all crates resolve to 0.5.0. No regression risk.
- **New subcommands (version, model-download)**: Additive changes to the CLI that do not affect existing subcommands (hook, export, import, server mode). The `None` arm (default server mode) is unchanged. No regression risk.
- **ensure_model re-export**: `pub use download::ensure_model` adds a public API surface to `unimatrix-embed`. This is additive and does not change existing behavior.

## PR Comments
- Posted 1 review comment on PR #221
- Blocking findings: no

## Knowledge Stewardship
- Stored: nothing novel to store -- findings are feature-specific, no generalizable anti-pattern detected across multiple features
