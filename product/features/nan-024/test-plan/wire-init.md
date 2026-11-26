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

## Rework (D-077, D-078, D-082, D-080)
- D-082 preflight (exit codes assert, assembled via real CLI process): 
  - `init` where node cannot be recorded -> exit 1, stderr names cause, no files changed except nothing partial (tree snapshot equal to pre-run).
  - `wire` with recorded node missing (deleted after a good init) -> exit 1 preflight, message tells to re-run `init`, no hook/MCP file modified.
  - upgrade that does NOT re-record node (version bump path) with recorded node missing -> exit 1; upgrade that re-records succeeds and rewrites the Windows fallback MCP entry (D-057).
  - `--dry-run` with missing node: reports the failure and exits 1 (or documented non-zero), writes nothing.
  - Preflight passing path unchanged: exit 0, outputs identical to prior run.
- D-078: Linux local init leaves `pkg/<version>/node_modules/<platform pkg>`; cloud attach on Linux without local mode does not require it.
- D-080: init passes `home` explicitly to `buildShimCommand`/`isInstalled`; run init with `process.env.HOME` decoy different from the injected test HOME and assert nothing is written under the decoy (HOME guard, NFR-05).
- D-081 end to end: see codex-install.md; init output prints the Codex path actually written (`$CODEX_HOME/hooks.json`).
- Windows MCP entry in D-047 form for every Windows install (D-072); `command` equals `install.json.node` byte-for-byte.
