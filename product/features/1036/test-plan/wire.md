# Test Plan — C6 orchestrator (`lib/wire.js`) — extend `test/wire.test.js`

Risks: R-04, R-11, R-12, R-13, R-19, R-05. Discharges AC-1(a,b), AC-3 (user/global), AC-4, AC-6, AC-7 via `wire()`/`init()` with injected `env`/`home`/`platform`/`fault`, or spawned `init`.

## AC-1 (D-028)
- `env={CLAUDE_CONFIG_DIR:D}`, seeded owned local-scope + top-level entry in `H/.claude.json` → `D/.claude.json` `projects[R].mcpServers.unimatrix` = entry; `H/.claude.json` bytes+mtime unchanged; no leg path equals `H/.claude.json`.
- Same-file fixtures `CLAUDE_CONFIG_DIR=H`, `H/`, symlink→H: one entry, no other-profile.

## AC-3 user/global
- `.claude.json`, `$CODEX_HOME/{hooks.json,config.toml}`, user `opencode.json` as absolute/relative/2-hop links incl. outside home → link kept, target changed, no `.tmp-`.

## AC-4 RT table (RT-01..04, 06..10)
- Table-driven; each row seeds owned + foreign siblings + foreign at `unimatrix` key. Owned removed; foreign-at-key kept with warning naming file and key (R-19); JSON deep-equal incl. key order, indent reused (tab/2/4); TOML byte-identical outside owned table; untouched files bytes+mtime equal.

## AC-6
- Seed duplicates at every RT location; counting scanner over resolved + RT files: ≤1 owned per (harness,event) and (harness,MCP key); AP-1..AP-3 excluded.

## AC-7 / R-11 / R-12 / R-13
- `fault before-rename` once on W2 → `updated`, concurrent change + entry present.
- every attempt → exit 1 `conflict` naming file.
- `after-rename` strip on W1 → exit 1 `verify-failed`; legacy `~/.claude/settings.json` owned hooks still present; R4 not run. Same on W2 under override: default `~/.claude.json` intact.
- W2 `skipped-foreign` → R1 not run; W5 `skipped-malformed` → R10 twin kept.
- Rename failure on W3, W4, W5 → exit 1 naming file, never `skipped*`; no temp.

## Output
- `resolved:` lines per destination with `[VAR]` source; FR-51 note once when any override effective; FR-52 notice printed once only when a `.claude.json` leg is created/updated/removed; `[dry-run] ` prefix in dry-run.
- Grep: `gitIgnored`, `GITIGNORE_WARNING`, dispatch functions removed.
