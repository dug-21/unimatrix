# Test Plan: codex-install (`lib/codex-install.js`)

Risks: R-02, R-08, R-19, R-20, R-36, AC-04/13. Files: update `codex-install.test.js`, `codex-toml-surgical.test.js`, `c14-codex.test.js`, `c14-negative.test.js` (counts non-decreasing).
- Hooks only in `~/.codex/hooks.json`; owned entries removed from project `.codex/hooks.json` and `~/.codex/config.toml [hooks]`; exactly one owned entry per event; foreign hooks untouched.
- `[mcp_servers.unimatrix]` upserted in `~/.codex/config.toml` surgically: comments, ordering, unrelated tables byte-preserved; owned MCP removed from project `.codex/config.toml` (D-062); foreign `unimatrix` table left + D-065 warning; malformed TOML -> abort untouched.
- Hook string byte-fixed (D-028); init prints `/hooks` approval step; writes no trust hashes.
- Literal Codex command through `$SHELL -lc` (bash/zsh/fish where present) and `cmd.exe /C` (win, D-073 raw string): records `codex-cli` in `last-hook.json` and stores event (R-19 scenario 4).
- Codex-only project (no Claude config, user-scope MCP) stores events (D-044).
- c14 verifier spine still asserts hook-fires from the written command's manifest (ADR-006 nan-023 #5768).
