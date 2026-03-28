# Agent Report: col-031-agent-8-service-layer

**Feature**: col-031 — Phase-Conditioned Frequency Table
**Component**: `services/mod.rs` ServiceLayer wiring
**Agent ID**: col-031-agent-8-service-layer

---

## Task Summary

Wire `PhaseFreqTableHandle` through `ServiceLayer` and fix all `SearchService::new` and `spawn_background_tick` construction call sites to pass the handle as a required non-optional parameter (ADR-005).

---

## Files Modified

- `crates/unimatrix-server/src/services/mod.rs` — primary target
- `crates/unimatrix-server/src/eval/runner/replay.rs` — AC-16 fix
- `crates/unimatrix-server/src/main.rs` — both spawn_background_tick call sites
- `crates/unimatrix-server/src/mcp/tools.rs` — handler + test
- `crates/unimatrix-server/src/services/index_briefing.rs` — ServiceSearchParams
- `crates/unimatrix-server/src/test_support.rs` — TestHarness search + search_with_filter
- `crates/unimatrix-server/src/uds/listener.rs` — UDS search path
- `crates/unimatrix-server/src/services/search.rs` — test literals

---

## 7-Site Grep Audit

| Site | File | PhaseFreqTableHandle passed |
|------|------|-----------------------------|
| `SearchService::new` call | `services/mod.rs` | YES — `Arc::clone(&phase_freq_table)` |
| `SearchService::new` definition | `services/search.rs` | YES — parameter added by agent-6 |
| `spawn_background_tick` call (daemon path) | `src/main.rs` line ~706 | YES — `phase_freq_table_handle` |
| `spawn_background_tick` call (stdio path) | `src/main.rs` line ~1099 | YES — `phase_freq_table_handle` |
| `spawn_background_tick` definition | `background.rs` | YES — added by agent-7 |
| `background_tick_loop` definition | `background.rs` | YES — added by agent-7 |
| `run_single_tick` definition | `background.rs` | YES — added by agent-7 |

Test helper sites (ServiceSearchParams `current_phase: None`):
- `test_support.rs` — both `search` and `search_with_filter` helpers: YES
- `eval/profile/layer.rs` — uses `ServiceLayer::with_rate_config` (no `SearchService::new` direct call): wired via ServiceLayer

`Option<PhaseFreqTableHandle>` audit: grep returns zero results in actual code (only in doc comments).

---

## Changes Made in `services/mod.rs`

1. **Struct field**: `phase_freq_table: PhaseFreqTableHandle` added after `ml_inference_pool`
2. **Handle creation**: `let phase_freq_table = PhaseFreqTable::new_handle();` in `with_rate_config` after `typed_graph_state`
3. **SearchService::new call**: `Arc::clone(&phase_freq_table)` as last argument
4. **ServiceLayer literal**: `phase_freq_table` field added
5. **Accessor**: `pub fn phase_freq_table_handle(&self) -> PhaseFreqTableHandle` added after `contradiction_cache_handle()`
6. **Tests**: 3 unit tests added (AC-05, R-14)

## AC-16 Fix (replay.rs)

Added `current_phase: record.context.phase.clone()` to `ServiceSearchParams` in `run_single_profile`. This is the non-separable prerequisite for AC-12 — without it, all eval replays had `current_phase = None` making the phase-explicit scoring gate vacuous.

---

## Tests

- **2265 pass, 1 fail** (pre-existing failure in `phase_freq_table.rs:test_rebuild_normalization_last_entry_in_five_bucket` — rank normalization formula owned by agent-5, not in scope)
- New tests (all pass):
  - `services::tests::test_service_layer_phase_freq_table_handle_returns_arc_clone` — AC-05
  - `services::tests::test_service_layer_phase_freq_table_handle_shared_state` — AC-05
  - `services::tests::test_service_layer_phase_freq_table_handle_is_non_optional` — R-14

---

## Build

`cargo build --workspace` — **PASS** (zero errors, 13 pre-existing warnings)

---

## Blockers

None. The pre-existing test failure in `phase_freq_table.rs` is a normalization formula issue owned by agent-5 (rank-5 of 5 returns 0.2 instead of 0.8 — the formula uses `(rank-1)/N` but the test expects `1.0 - (rank-1)/N`).

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #3689 (ADR-005, required non-optional handle), #3213 (background tick hidden construction site pattern), #3216 (lesson: silent wiring bypass), and #1560 (Arc<RwLock> background-tick cache pattern). All applied.
- Stored: nothing novel to store — the pattern of wiring a new `PhaseFreqTableHandle` through `ServiceLayer` is identical to the `typed_graph_state` pattern (crt-021) documented in entry #3248. The lesson about `run_single_tick` hidden construction site (#3216) was the key risk; it was mitigated by ADR-005 making the parameter required (compile error enforcement). No new trap discovered that isn't already captured in the knowledge base.
