# Security Review: nan-024-security-reviewer

## Risk Level: medium

## Summary
Trust model, shim templating, hook command construction and the effectiveEnv guard hold up. One blocking finding: user-level config rewrites downgrade file modes (0600 -> 0644) on secret-bearing files.

## Findings

### F1 File mode downgrade on rewritten user config (BLOCKING)
- Severity: medium
- Location: lib/claude-json.js atomicWriteText (default mode 0o644); lib/opencode-install.js atomicWriteJson (no mode); callers in lib/codex-install.js (config.toml, hooks.json), lib/merge-settings.js (settings*.json)
- Description: tmp+rename creates a new inode with the default mode; the original mode is not preserved. Reproduced: a 0600 ~/.config/opencode/opencode.json and ~/.codex/config.toml became 0644 after writeOpencodeMcp / writeCodexMcpToml (umask 022). These files commonly hold provider API keys / tokens. Only ~/.claude.json and .mcp.json removal preserve mode.
- Recommendation: stat the existing file and pass its mode (& 0o777) to atomicWriteText / atomicWriteJson; default new files to 0o600 for user-level config.
- Blocking: yes

### F2 ~/.claude.json lost-update window (non-blocking)
- Severity: low-medium
- Location: lib/claude-json.js writeClaudeMcp
- Description: mtime+size recheck then rename is not atomic against a running Claude Code; a write landing between recheck and rename is lost (file holds auth/session state). Also rename replaces a symlinked ~/.claude.json (dotfile managers) with a regular file.
- Recommendation: accept and document, or lstat/realpath target and write to the resolved path.

### F3 CI workflows lack explicit permissions (non-blocking)
- Severity: low
- Location: .github/workflows/nan-024-e2e.yml, q9-windows-mcp-proof.yml, ci.yml
- Description: no `permissions:` block; token defaults depend on repo settings. Triggers are pull_request/push (not pull_request_target); no untrusted input is interpolated into run steps (only matrix/runner values and steps.*.outcome via env). Actions pinned by tag, not SHA (consistent with repo).
- Recommendation: add `permissions: contents: read`.

### F4 Trust bootstrap / Windows cache residuals (informational)
- Severity: low
- Location: lib/shim-entry.js loadTrust; lib/trust.js checkWindowsPerEvent
- Description: (a) trust.js is lstat-checked (owner, mode) then require()d; ~/.unimatrix/bin dir itself is verified only afterwards, leaving a small lstat/require race that needs write on a 0700 same-user dir. On win32 trust.js loads unchecked (relies on icacls lockdown at install). (b) Windows per-event check compares file identity (size/mtime/ino), not ACLs; an actor already granted write who restores mtime evades until the next full check. Requires prior ACL loosening by owner/admin. (c) hook-route prepare() does not pass clientRoot to trustSet, so redirect.json's client target is not owner/mode checked (redirect.json itself is). Same-user trust boundary; no cross-user vector found.
- Blocking: no

### Checked, no issue
- Shim templating: render() uses split/join (no $-pattern replacement); POSIX rejects ' in shim/node/home paths, Windows rejects % and "; hook commands single/double-quoted accordingly. Newlines in node path not rejected but node path must be an existing absolute file.
- Windows PowerShell owner lookup: paths via env var, no interpolation; icacls/whoami via argv arrays, no shell.
- effectiveEnv guard applied in hook-client config.js, remote-resolve.js, hook-route prepare(), mcp-bridge. Cross-repo credential leak prevented. bin mcp-bridge legacy route uses raw env for the headless usage check only.
- JSON/TOML parsing: parse failures abort untouched; no eval of file contents; shim reads root marker without evaluating.
- ReDoS: ownership regexes use bounded classes; no nested quantifiers found.
- No new dependencies, no secrets in diff. rmrf targets derive from package.json version under ~/.unimatrix/pkg.
- last-hook.json: hash is sha256 hex, mode 0600.

## Blast Radius Assessment
Worst case: world-readable API keys/tokens (F1); lost Claude auth/session edits (F2); a trust-check false-pass would execute attacker code from ~/.unimatrix (requires same-user write already). Failure modes of hook route are exit 1 (non-blocking), not data corruption.

## Regression Risk
Hook wiring moves to the user-level shim; .mcp.json / opencode.json / project .codex owned entries are removed by init/wire. Non-atomic copyPackage (rmrf(dest) then rename) can briefly make hooks fail trust ("missing") during concurrent upgrades. Hook ownership regexes are broadened (abs-shim, L6) - foreign commands matching them would be removed/overwritten.

## PR Comments
- Posted 1 review on PR #1033; blocking findings: yes

## Knowledge Stewardship
- Stored: nothing novel to store -- single-PR finding; mode-preservation on tmp+rename rewrite is recurring only if seen again.

## Re-review (r2) @ 101d8953

**Risk level: low. Blocking: no.** F1 and F3 are fixed.

### F1 (0600 config files widened to 0644) -- fixed
New `lib/atomic-write.js` stats the existing file and reuses its mode. Otherwise it uses the caller's `newMode` (default 0644), then `chmodSync` on the tmp file, then rename.

Reproduced in a fixture dir under the scratchpad, never the real home, with `umask 000`:
- Existing 0600 file stays 0600 after rewrite.
- New file with `newMode=0600` is 0600.
- Existing 0640 is preserved.

Callers:
- `claude-json.js` `writeClaudeMcp`: preserves st1.mode, falls back to 0600.
- `claude-json.js` `removeOwnedMcpJson`: preserves the existing mode.
- `opencode-install.js` user config: preserves the existing mode, new file 0600.
- `codex-install.js` config.toml and hooks.json: preserves the existing mode, new file 0600.
- `codex-install.js` remove paths and `merge-settings.js` (existing files only on rewrite): preserve the existing mode.

New tests cover each case. Test suites merge-settings, opencode-install and codex-install: 198/198 pass.

### F3 (workflow permissions) -- fixed
`permissions: contents: read` added to ci.yml, nan-024-e2e.yml and q9-windows-mcp-proof.yml.

### atomic-write.js review (no blocking issues)
- Umask: `chmodSync` on the tmp file makes the result umask-proof.
- Symlinks: rename replaces a symlinked target with a regular file, and the symlink target is untouched. This is the F2 residual, not a regression, since the old code behaved the same.
- Tmp name: `file.<pid>.tmp` is predictable. `writeFileSync` would follow a pre-planted symlink at that name, but that needs same-user write in the target dir. `~/.claude.json` and `~/.codex` are owned by the user, so there is no cross-user vector. `O_EXCL` or a random suffix would be cheap hardening (info).
- Existing 0000 or read-only files: the mode is preserved and the write goes to a new tmp file, so there is no failure.
- New project-level `.claude/settings.local.json` is created 0644 (default). It holds hook commands, not secrets. Acceptable.
- Diff is limited to the fix, tests, the workflow permissions blocks and the fix report. No new dependencies or secrets.

Blast radius: a bug in the mode logic would produce a wrong file mode (loosened or tightened), not data loss. The write is still tmp + rename.
