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
- Invalid `$CODEX_HOME` (D-085, decided): relative path like `./x` or invalid value -> ignored with one stderr warning naming the value; `~/.codex` used (when a trigger fires, else no write); a non-directory (regular file) counts as absent (row 6 behaviour); empty string treated as unset/invalid per implementation, asserted. Never writes relative to cwd (assert cwd tree unchanged).
- Re-detection idempotent: second init writes nothing (mtime unchanged).
- `status` reads the same `$CODEX_HOME/hooks.json` for the approval/never-fired report.

## Rework 2 (D-084): Codex MCP write gating
Same detection as D-081 hooks, rows 1-7 reused for `$CODEX_HOME/config.toml [mcp_servers.unimatrix]`:
| # | Case | Expected |
|---|---|---|
| M1 | any of rows 1-4 triggers | `[mcp_servers.unimatrix]` written to `$CODEX_HOME/config.toml` (owned entry only), listed in `--dry-run` output |
| M2 | no Codex detected (row 5) and row 6 | no `config.toml` created or touched (tree snapshot equal); dry-run lists no Codex MCP write |
| M3 | detected, foreign `unimatrix` table | left untouched + D-065 warning; dry-run notes skip |
- Writes limited to owned entries (foreign tables/comments byte-preserved); dry-run list == actual writes (assert set equality of paths).
- Hooks and MCP always agree: never hooks without MCP or vice versa for the same detection result.

## Rework 3 (D-090)
D-028 byte-fixed test (custom `$CODEX_HOME` upgrade) additionally asserts the hook command is byte-equal across a `bin/trust.js` refresh; shim path unchanged.
