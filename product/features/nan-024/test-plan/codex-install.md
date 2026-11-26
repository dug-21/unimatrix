# Test Plan: codex-install (`lib/codex-install.js`)

Risks: R-02, R-08, R-19, R-20, R-36, AC-04/13. Files: update `codex-install.test.js`, `codex-toml-surgical.test.js`, `c14-codex.test.js`, `c14-negative.test.js` (counts non-decreasing).
- Hooks only in `~/.codex/hooks.json`; owned entries removed from project `.codex/hooks.json` and `~/.codex/config.toml [hooks]`; exactly one owned entry per event; foreign hooks untouched.
- `[mcp_servers.unimatrix]` upserted in `~/.codex/config.toml` surgically: comments, ordering, unrelated tables byte-preserved; owned MCP removed from project `.codex/config.toml` (D-062); foreign `unimatrix` table left + D-065 warning; malformed TOML -> abort untouched.
- Hook string byte-fixed (D-028); init prints `/hooks` approval step; writes no trust hashes.
- Literal Codex command through `$SHELL -lc` (bash/zsh/fish where present) and `cmd.exe /C` (win, D-073 raw string): records `codex-cli` in `last-hook.json` and stores event (R-19 scenario 4).
- Codex-only project (no Claude config, user-scope MCP) stores events (D-044).
- c14 verifier spine still asserts hook-fires from the written command's manifest (ADR-006 nan-023 #5768).

## Rework (D-081): detection and destinations
Hooks go to `$CODEX_HOME/hooks.json` (default `~/.codex`), MCP to `$CODEX_HOME/config.toml`. Temp HOME; PATH controlled per case (fake `codex` stub in a temp bin dir, PATH otherwise stripped of real codex). Detection matrix, each row asserts hooks written (or not) AND destination path:

| # | Trigger | Setup | Expected |
|---|---|---|---|
| 1 | project `.codex/` | fixture repo has `.codex/`; no `$CODEX_HOME`, no PATH codex, no flag | `~/.codex/hooks.json` written |
| 2 | existing `$CODEX_HOME` dir | `CODEX_HOME=<tmp>/ch` exists (empty); nothing else | `<tmp>/ch/hooks.json` + `config.toml` written; `~/.codex` NOT created |
| 3 | codex on PATH | stub `codex` executable on PATH; nothing else | `~/.codex/hooks.json` written |
| 4 | `--harness codex-cli` | flag only | written; `~/.codex` created |
| 5 | none | no trigger | no Codex file created or touched (tree snapshot equal); init exit 0; no Codex approval step printed |
| 6 | `CODEX_HOME` set but dir absent, no other trigger | | not written (row 5 behaviour) |
| 7 | multiple triggers | rows 1+2 | single write to `$CODEX_HOME`, one owned entry per event |
- No XDG: `XDG_CONFIG_HOME` set to a temp dir -> no write under it.
- Owned entries removed from project `.codex/hooks.json` (when row 1) and from `$CODEX_HOME/config.toml [hooks]`; foreign preserved; byte-fixed string across upgrade at custom `$CODEX_HOME` (D-028).
- Invalid `$CODEX_HOME` (relative path like `./x`, empty string, a path that is a regular file): DEFAULT = ignore and warn (flagged default, pending confirmation): stderr one warning naming the value, falls back to `~/.codex` only if another trigger fires, else no write; never writes relative to cwd (assert cwd tree unchanged). If the decision flips to hard error, only this bullet changes.
- Re-detection idempotent: second init writes nothing (mtime unchanged).
- `status` reads the same `$CODEX_HOME/hooks.json` for the approval/never-fired report.
