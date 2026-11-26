# nan-024-agent-14-sec-fix report

## Changes
- F1: new `packages/unimatrix/lib/atomic-write.js` (`atomicWrite`, `existingMode`): tmp+rename that preserves an existing file's mode (& 0o777, chmod'd umask-proof); new files get `newMode` (default 0o644).
  - `lib/claude-json.js`: `atomicWriteText` now delegates to it (existing-mode preserved; `.mcp.json` removal and settings*.json inherit).
  - `lib/opencode-install.js`: `atomicWriteJson` delegates; user-level opencode.json new file = 0600.
  - `lib/codex-install.js`: user config.toml / hooks.json new files = 0600 (existing preserved).
  - Project-level files (settings.local.json, .mcp.json, project opencode.json/.codex) keep default 0644 for new files; existing modes never loosened.
- F3: `permissions: contents: read` added to nan-024-e2e.yml, q9-windows-mcp-proof.yml, ci.yml (jobs use checkout/setup-node/cache/artifact actions only; no other scopes needed).
- Tests added (POSIX-only, win32 skip-guard, tmp fixture homes): codex-install (config.toml, hooks.json preserve + new 0600), opencode-install, merge-settings (mergeSettings, writeClaudeMcp, removeOwnedMcpJson).
- lib/hook-client untouched.

## Gates
Touched suites 198/198; `npm run test:package` 733 pass, 0 fail, 1 skipped; size 115012/115012 stripped (at cap, unchanged), zero-deps OK, test-count PASS (1336).

## Known residuals (no code change)
- F2: lost-update window between mtime+size recheck and rename in writeClaudeMcp against a running Claude Code; rename replaces a symlinked ~/.claude.json with a regular file.
- F4: (a) trust.js lstat-then-require race needs same-user write on 0700 dir; win32 trust.js loads unchecked (icacls at install). (b) Windows per-event check compares file identity, not ACLs. (c) hook-route prepare() does not pass clientRoot to trustSet. Same-user boundary only.

## Knowledge Stewardship
- Queried: context_briefing -- no relevant entries (nothing on mode preservation for JS rewrites).
- Stored: nothing novel -- pattern recorded in lib/atomic-write.js; reviewer already noted recurrence threshold.

Unimatrix entries consulted: #1195, #4682
