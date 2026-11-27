# 1036-agent-14-wire-plan — C4 wiring plan

Files: packages/unimatrix/lib/wire-plan.js (new, ~125 lines), packages/unimatrix/test/wire-plan.test.js (new). Not committed.
Tests: 20/20 pass (`node --test test/wire-plan.test.js`).

## Deviations / notes for wire.js rewiring agent
- Codex is one step `CX` (pseudocode), not W3/W4/R7-R9 as the test plan lists; maybeWireCodex keeps internal order and its SafeWriteError-only rethrow.
- Store is not a plan step; wire.js runs installStore first.
- `source` is stamped on legs only when the step source is not "default" (keeps default-case manifests byte-identical to existing wire tests).
- Extra ctx seams: `gitRun` (passed to ensureSettingsLocalIgnored `run`), `pathLookup` (to maybeWireCodex). wire.js should pass neither in production.
- R3 dropped when it is the same real file as project settings.json; R4 dropped when same as R3 or R2.
- W5 no longer wrapped in try/catch (old dispatchOpencode turned errors into skipped-malformed); per pseudocode errors propagate. writeOpencodeMcp already returns skipped-malformed for bad JSON, so only writer/IO errors now abort.
- Surfaces per ARCH §6a / real module legs: W5 and R10 "retrieval"; R11/R12 "mcp".
- W2 uses claude-json `claudeMcpLegs` (R1 leg from the same mutate); R6 uses `removeOwnedMcpJsonLeg`. No API mismatch with pseudocode found.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- ADR-004 #5959, nan-023 ADR-001 #5763, wire-orchestrator traps #5776.
- Stored: nothing novel to store -- implementation follows ADR-004 directly.
