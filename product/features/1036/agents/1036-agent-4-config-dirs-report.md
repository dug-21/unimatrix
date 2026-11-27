# 1036-agent-4-config-dirs — C1 config-dirs

Files: packages/unimatrix/lib/config-dirs.js (new, 173 lines), packages/unimatrix/test/config-dirs.test.js (new).
Tests: node --test test/config-dirs.test.js — 10 pass / 0 fail. Not committed (per instruction). No deps added. Consumers not rewired.

## Deviations / notes
- `claude.dir` default = `<home>/.claude` (test plan) — pseudocode said `home`. Flag for pseudocode alignment.
- win32 `path.win32.isAbsolute("/cfg")` is true (rooted, no drive); accepted as absolute.
- `sameRealFile` with injected win32 on a non-win32 host compares `path.win32.resolve` forms case-insensitively (no fs realpath).
- `CODEX_HOME` trailing separator now stripped (legacy codex-install.js didn't); D-085 warning text byte-identical.
- OpenCode fallback label when XDG valid: `$XDG_CONFIG_HOME/opencode`.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- ADR-002 #5960 (resolver contract), #251 (tmp-dir leak in tests), #1207 (node:test imports).
- Stored: nothing novel to store -- straightforward pure resolver; win32 rooted-path quirk is Node-documented.

Unimatrix entries consulted: #5960, #251, #1207
