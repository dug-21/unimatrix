# Test Plan — C4 wiring plan (`lib/wire-plan.js`) — new `test/wire-plan.test.js`

Risks: R-02, R-05, R-07, R-11. Unit level; AC proof in wire/init.

## buildWirePlan
- No overrides, all harnesses → step ids in order: store, W1, W2(+R1), R2–R4, R6, W3, W4, R7–R9, W5, R10–R12, X1; no R5 (D-028).
- `CLAUDE_CONFIG_DIR=<d>` → no step targets `<home>/.claude.json`; R4 targets `<home>/.claude/settings.json` (D-030).
- `CLAUDE_CONFIG_DIR=<home>` → R4 present (different file `settings.json`), no `.claude.json` twin; `XDG_CONFIG_HOME=<home>/.config` / `OPENCODE_CONFIG_DIR=<home>/.config/opencode` / trailing slash / symlinked dir → no R10.
- Alternates never produce a step.
- `harnessSel`, `hooksOnly`, `writeMcp:false` filter steps as before.

## runWirePlan
- Same steps, `dryRun` true vs false → identical manifest on (harness, surface, action, path, reason, entry/command) (FR-60/61).
- Gated removal after destination `skipped-foreign`/`skipped-malformed` → `skipped-dest-unwired` in both modes.
- Destination throws `SafeWriteError` → rethrown; no later step's `run` called (spy).
- Fixpoint: each W/R/X mutate applied to its own output returns `null` for tab/2/4-space, CRLF, no trailing newline, non-canonical key order (R-02).

## Static
- ≤300 code lines.
