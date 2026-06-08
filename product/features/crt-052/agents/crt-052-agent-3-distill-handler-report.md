# crt-052 Agent 3 — C6 Distill Helper / Handler Glue

## Scope delivered
C6: `distill_before_purge` helper in `crates/unimatrix-server/src/mcp/distill_handler.rs`
(NEW) + thin wiring at the four `context_cycle_review` `result.is_ok()` returns in
`mcp/tools.rs`. Wave A.

## Files modified/created
- `crates/unimatrix-server/src/mcp/distill_handler.rs` (NEW — 730 lines incl. tests; production ends at line 300)
- `crates/unimatrix-server/src/mcp/mod.rs` (registered `distill_handler` module)
- `crates/unimatrix-server/src/mcp/tools.rs` (four thin call sites; `let result` → `let mut result`)
- `crates/unimatrix-observe/src/lib.rs` (re-exported the C4 candidate types at crate root)

## Tests
20/20 pass in `mcp::distill_handler::tests`. 37/37 existing `cycle_review` tests pass.
514/514 `unimatrix-observe` lib tests pass. Workspace lib build clean; no clippy
warnings from new code.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_get(3793, 4750, 4851, 4850) +
  context_search -- surfaced ADR-004/005 (#4850/#4851), the #3793 memoization persist
  trap, and the #4750 four-return pattern. Applied all.
- Stored: entry #4866 "Assembly-level attach = append Content item on CallToolResult,
  not a serde field" via /uni-store-pattern.

## Confirmations
- Four return sites wired (purged-signals, cached-MetricVector, memoization-hit,
  full-pipeline): distill → attach → purge at each, via the ONE shared helper.
- Candidates never reach the memoized struct, proven two ways: (1) structurally —
  `RetrospectiveReport` has no candidate field, so `store_cycle_review`/
  `cycle_review_index` physically cannot carry them (test
  `test_candidates_structurally_absent_from_memoized_report`); (2) temporally — the
  attach mutates only the in-flight `CallToolResult` AFTER `store_cycle_review` ran.
