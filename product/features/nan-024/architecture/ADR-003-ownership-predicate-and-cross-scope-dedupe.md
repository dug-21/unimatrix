## ADR-003: One ownership predicate module and cross-scope dedupe

### Context
`merge-settings.js:20-27` patterns miss npx and shim forms (SR-08); broadening risks deleting foreign entries (SR-09). Claude combines hooks across scopes; Codex merges hook layers additively (OD-4 evidence). D-026 fixes destinations: Claude hooks -> `.claude/settings.local.json`; Codex hooks -> `~/.codex/hooks.json` only.

### Decision
- `lib/hook-ownership.js` exports `isUnimatrixCommand(cmd: string): boolean` and `OWNED_FORMS` (named regexes): bare/abs `unimatrix hook`, bare/abs `unimatrix-server hook`, optional `LD_LIBRARY_PATH=... ` prefix, `node <path>/hook-client/index.js` (quoted/bare), `npx [-y] [@dug-21/]unimatrix hook`, `$HOME`/`~` shim form, and the absolute shim form (single- or double-quoted path ending `/.unimatrix/bin/unimatrix` or `.unimatrix/bin/unimatrix.cmd`, either slash). Every arm requires `hook\s+<KnownEvent>` or `hook-client/index.js` adjacency; substring "unimatrix" never matches alone.
- `merge-settings.js` and `codex-install.js` import it; local pattern arrays and the `normalizeCommandSource` string arm are deleted (D-008).
- Scope (D-026):
  - Claude: write `.claude/settings.local.json`; remove owned entries from `.claude/settings.json` and `~/.claude/settings.json`.
  - Codex: write `~/.codex/hooks.json`; remove owned entries from project `.codex/hooks.json` and `[hooks]` in `~/.codex/config.toml`.
  - MCP (D-060): write Claude local scope `~/.claude.json` `projects[<root>].mcpServers.unimatrix` and Codex `~/.codex/config.toml` `[mcp_servers.unimatrix]`. Remove owned Unimatrix entries from project `.mcp.json` `mcpServers` project `.codex/config.toml` `[mcp_servers]`, and project `opencode.json` `mcp.unimatrix` (D-064; the OpenCode write target is in ADR-006). Foreign servers are preserved byte-for-byte in content. If removal leaves `.mcp.json` with an empty `mcpServers`, the file is kept (never deleted). It is not rewritten when nothing owned is present.
- MCP ownership: `isUnimatrixMcpEntry(name, entry): boolean` in `lib/hook-ownership.js`. True iff `name === "unimatrix"` and the command/args match an owned MCP form: `node <path>/mcp-bridge.js <hash>`, `npx [-y] [@dug-21/]unimatrix mcp-bridge`, bare/abs `unimatrix-server`/`unimatrix` with a `serve`/stdio verb, the `sh -c` and `cmd` shim forms, or the D-047 fallback. A foreign entry named `unimatrix` in a project file is left in place, and init warns.
- Foreign entry at a per-machine `unimatrix` key (D-065, R-40): applies to `~/.claude.json` `projects[<root>].mcpServers.unimatrix`, `~/.codex/config.toml` `[mcp_servers.unimatrix]` and `~/.config/opencode/opencode.json` `mcp.unimatrix`. The entry is left in place and never overwritten. No Unimatrix entry is written for that harness. init (and `--dry-run`) warns `unimatrix: <file> key <key> holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written`. init's exit code is unaffected.
- `~/.claude.json` is owned by Claude Code and written by it concurrently. init reads it, changes only `projects[<root>].mcpServers.unimatrix`, and writes via tmp + rename. A file that fails to parse aborts the MCP leg: init exits 1 with an error naming the file, and never overwrites it (FR-07j, AC-16, R-37).
  - Foreign entries untouched; files with nothing owned are not rewritten. Every user-level write and removal is listed in `--dry-run`.
- Tests: MCP migration: seeded project `.mcp.json` with Unimatrix + foreign servers -> Unimatrix removed, foreign servers preserved (regression guard "removal preserving foreign servers"); `~/.claude.json` other keys/projects untouched; Codex user/project config same; project `opencode.json` `mcp.unimatrix` removed with other `mcp` entries preserved (D-064).
- Tests (D-065): seed a foreign entry at each per-machine `unimatrix` key. Assert it is byte-unchanged, no Unimatrix entry is written, and the warning names the file and key.
- Tests: positive per legacy form (AC-05), negative foreign cases (`my-unimatrix-hook.sh`, `echo unimatrix hook`), multi-scope seed asserting exactly one owned entry per event in the destination (AC-13, D-010).

### Consequences
- New forms require one table row plus tests.
- Codex user-level hooks fire in every project; the shared project predicate (ADR-004) keeps them silent outside Unimatrix projects.
