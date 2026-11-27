# 1036-agent-15-status — C15 status

## Files
- packages/unimatrix/lib/status.js (modified)
- packages/unimatrix/test/status.test.js (new)

## Changes
- `collect(ctx)` injectable (env/home/platform/cwd); all paths from `resolveConfigDirs`.
- Removed private `codexHome` and `home/.claude.json`; `mcpNode` reads `dirs.claude.claudeJson` / `dirs.codex.configToml`; codex hooks read `dirs.codex.hooksJson`.
- Added `configDirs` (exact §6a field set), `claudeMcp` (via `readClaudeMcpState`), `opencodeConfig`, `staleLegacy`, `alternateProfiles`; internal `_dirs` dropped from `--json` by existing `_` filter.
- renderText adds FR-34/35/38 lines after existing output.

## Tests
- test/status.test.js: 12/12 pass.
- test/launcher-status.test.js: 3 fail, all expected consequences of the spec, file outside my scope:
  1. "field order exact" — exact key list; new FR-34/35 lines appended.
  2. "--json key order and shape" — exact key list; new FR-36 keys appended.
  3. "Windows fallback node missing" — runs with platform emulated as win32 on Linux; resolver (C1) builds `path.win32` paths from a POSIX home, so `~/.claude.json` is unreadable on the host. Needs test update (real win32 CI covers it) or a C1 decision on emulated platforms.
- cli-routing.test.js: pass.
- Size gate: lib/hook-client untouched; n/a.
- No deps added. Not committed (per instructions).

## Issues
- Init `resolved:` manifest not present yet; SR-09 parity test compares against `resolveConfigDirs` (the source init uses).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- ADR-002 #5960 (single resolver), ADR-007 #5961 (resolved-file reporting).
- Stored: nothing novel to store -- the win32-emulation path issue is reported for triage first.
