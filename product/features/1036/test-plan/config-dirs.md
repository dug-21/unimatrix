# Test Plan — C1 config-dirs (`lib/config-dirs.js`) — new `test/config-dirs.test.js`

Risks: R-04, R-05, R-17, R-24. Support only; ACs discharged in wire/status/init.

## Unit
- `resolveConfigDirs({env:{}, home:H, platform:"linux"})` → `claude.dir=H/.claude`, `claudeJson=H/.claude.json`, `userSettings=H/.claude/settings.json`, `opencode.configJson=H/.config/opencode/opencode.json`, `codex.dir=H/.codex`, every `source:"default"`, `warnings:[]`.
- `CLAUDE_CONFIG_DIR=D` → `claudeJson=D/.claude.json`, `userSettings=D/settings.json`, source `CLAUDE_CONFIG_DIR`; `legacy.userSettings=H/.claude/settings.json`; `alternates` contains `{harness:"claude-code", file:H/.claude.json, var:"CLAUDE_CONFIG_DIR"}` (D-028).
- OpenCode precedence (measured): `OPENCODE_CONFIG_DIR` > `XDG_CONFIG_HOME/opencode` > default; both set → source `OPENCODE_CONFIG_DIR`, alternates include `$XDG/opencode/opencode.json` (AP-1); `legacy.opencodeJson=H/.config/opencode/opencode.json`.
- `CODEX_HOME`: `codexHome(env,home)` output equals resolver `codex.dir`; D-085 warning text unchanged.
- Edge values per var (R-17): `""` → unset, no warning; `"  "`, `~/x`, `cfg`, `"a\0b"` → exactly one warning naming the var, result equals unset case; `<abs>/` → normalized, no warning.
- Purity: `env` object frozen (`Object.freeze`) — no throw; `process.env` sentinel var differing from injected env is ignored (FR-02).
- Never throws: home nonexistent, env values non-string.
- `sameRealFile`: identical path; trailing slash; symlinked dir to default; only-one-exists (realpath of existing ancestor); different files → false; win32 injected with `C:` vs `c:` → true.

## Grep (FR-01)
- No lib file except `config-dirs.js` joins `".claude.json"`, `".config","opencode"` or `".codex"` onto home; no `process.env.(CLAUDE_CONFIG_DIR|OPENCODE_CONFIG_DIR|XDG_CONFIG_HOME|CODEX_HOME)` outside resolver/entry points.
