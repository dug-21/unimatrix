# Test Plan: wire-init (`lib/wire.js`, `lib/init.js`, `~/.claude.json` writer)

Risks: R-08, R-36, R-37, R-38, R-40, R-14, R-16, R-19, R-27, R-35, AC-01/02/09/16. Files: update `wire.test.js`, `init.test.js`, `init-integration.test.js`; new `claude-json.test.js`, `init-migration.test.js`, `init-e2e.test.js` (wiring-e2e).

- Assembled E2E (AC-02/04/07): clean fixture repo + temp HOME, run real `init` (local on Linux, cloud on all), read commands from `settings.local.json` and `~/.codex/hooks.json`, execute through harness shell per event: observation + provider stored; process trace: zero Rust exec; scan all written files with `isUnimatrixCommand` L1/L2 arms: none.
- `--dry-run` file list equals set of files changed by the real run (diff of fixture tree snapshot) incl. user-level edits (R-08).
- Init output contains the Codex approval step and no trust hashes.
- `~/.claude.json` (R-37/R-38): only `projects[<root>].mcpServers.unimatrix` changes (other keys equal after reparse); tmp+rename; concurrent writer process rewriting every few ms while init runs 50x -> file always parses, contains writer's latest keys, exactly one entry; kill init mid-write -> original intact; malformed -> exit 1, byte-unchanged. Key table {plain, symlinked root (realpath), trailing slash, subdir cwd, nested repo, `C:\` vs `c:/` (win cell)}: key equals documented/captured Claude normalisation.
- Migration (R-36/R-40, AC-16): fixtures `.mcp.json`, `.codex/config.toml`, `opencode.json` x {Unimatrix only, + 2 foreign, foreign only, comments/custom indent, legacy forms}: foreign semantically identical, order and TOML comments kept, foreign-only byte/mtime unchanged, Unimatrix-only file kept and valid; malformed -> abort untouched; foreign command named `unimatrix` kept + stderr warning; foreign entry at each per-machine `unimatrix` key unchanged + exact D-065 message, init exit code unaffected.
- Windows MCP entry == D-047 form (D-072); Unix == `sh -c exec "$HOME/.unimatrix/bin/unimatrix" mcp-stdio`. Node re-recorded -> fallback entry rewritten, otherwise byte-identical (D-057); stale node: init/upgrade preflight exit 1 + re-init hint.
- init with `.opencode/` + `opencode.json`: before/after diff shows no new files, `plugin[]` byte-equal (R-14/AC-09).
- No file in `lib/` > 1000 code lines (cheap size assertion).
