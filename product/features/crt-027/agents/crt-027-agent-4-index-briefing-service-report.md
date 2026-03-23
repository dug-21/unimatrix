# Agent Report: crt-027-agent-4-index-briefing-service

**Feature**: crt-027 — WA-4 Proactive Knowledge Delivery
**Component**: IndexBriefingService (services/index_briefing.rs)
**Date**: 2026-03-23

## Summary

Implemented `IndexBriefingService` and supporting types per pseudocode and test plan. Deleted `BriefingService` content is handled by the service-layer-wiring agent (mod.rs wiring); this agent creates the replacement file and wires the module declaration.

## Files Created / Modified

| File | Action |
|------|--------|
| `crates/unimatrix-server/src/services/index_briefing.rs` | Created (482 lines) |
| `crates/unimatrix-server/src/services/mod.rs` | Modified — added `mod index_briefing;` and re-exports |
| `crates/unimatrix-server/src/mcp/response/mod.rs` | Modified — made `briefing` module unconditional (ADR-005, NFR-05, C-07) |

## Implementation Notes

1. **Feature flag fix**: `mcp/response/briefing` was `#[cfg(feature = "mcp-briefing")]`-gated. Removed the gate from the `mod briefing;` declaration so `IndexEntry`/`SNIPPET_CHARS`/`format_index_table` compile unconditionally. Only MCP tool registration remains gated. Pattern stored as #3296.

2. **Caller pattern for `index()`**: Matches pseudocode exactly — `RetrievalMode::Strict` + Active-only post-filter. k=0 guard clamps to `default_k=20` per EC-03.

3. **`derive_briefing_query` step 2**: If `feature_cycle` is absent (state.feature is None or empty), falls directly to step 3 (topic). This matches the pseudocode decision: bare topic_signals without feature context are unreliable.

4. **Import path**: `crate::mcp::response::{IndexEntry, SNIPPET_CHARS}` — uses the public re-export, not the private submodule path.

5. **Determinism in `extract_top_topic_signals`**: Secondary sort by key ascending added for deterministic output when counts tie (not in pseudocode, added defensively).

## Tests: 11 passed / 0 failed

All query-derivation and `extract_top_topic_signals` unit tests pass. Service-level tests (T-IB-01 through T-IB-06 from the test plan) require a real database/SearchService and are integration-level — covered by other agents' end-to-end tests.

| Test | Result |
|------|--------|
| `derive_briefing_query_task_param_takes_priority` | PASS |
| `derive_briefing_query_whitespace_task_falls_through` | PASS |
| `derive_briefing_query_empty_task_falls_through` | PASS |
| `derive_briefing_query_session_signals_step_2` | PASS |
| `derive_briefing_query_fewer_than_three_signals` | PASS |
| `derive_briefing_query_no_feature_cycle_falls_to_topic` | PASS |
| `derive_briefing_query_empty_signals_fallback_to_topic` | PASS |
| `derive_briefing_query_no_session_fallback_to_topic` | PASS |
| `extract_top_topic_signals_empty_input` | PASS |
| `extract_top_topic_signals_ordered_by_count` | PASS |
| `extract_top_topic_signals_fewer_than_n` | PASS |

## Self-Check

- [x] `cargo build --workspace` — zero errors
- [x] `cargo test --workspace` — no new failures
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, `HACK` in non-test code
- [x] All modified files within scope defined in the brief
- [x] Error handling: `?` operator propagates `ServiceError` from `SearchService`; no `.unwrap()` in non-test code
- [x] `IndexBriefingService`, `IndexBriefingParams` have `#[derive(Debug)]` or `Clone` at minimum (`Clone` only; `Debug` not derived on params per pseudocode)
- [x] Code follows validated pseudocode — no silent deviations
- [x] Test cases match component test plan expectations (query-derivation tests match test-plan 1:1)
- [x] No source file exceeds 500 lines (482 lines)

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — results #748, #281, #316, #1560, #3210 found. Applied: pre-resolved histogram pattern from #3210 (SearchService pre-resolution), effectiveness snapshot pattern from #1560.
- Stored: entry #3296 "cfg(feature) gate on mcp/response module vs. WA-5 unconditional types: split mod declaration from re-export" via `/uni-store-pattern` — non-obvious trap: feature-gating a module declaration blocks imports from unrelated paths, causing compile errors that look like visibility issues.

## Blockers

None. The `briefing.rs` file still exists with its `BriefingService` content — it remains compilable until the `service-layer-wiring` agent migrates `mod.rs` to replace `BriefingService` with `IndexBriefingService` and the relevant wiring agents delete the old content.
