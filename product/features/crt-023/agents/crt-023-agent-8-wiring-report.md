# Agent Report: crt-023-agent-8-wiring

**Agent ID**: crt-023-agent-8-wiring
**Wave**: 4 (server startup wiring, eval integration, model-download CLI)
**Date**: 2026-03-20

## Sub-tasks Completed

### Sub-task A: NLI auto-quarantine threshold guard (ADR-007, FR-22b, AC-25)

**Files modified:**
- `crates/unimatrix-store/src/read.rs` — added `ContradictEdgeRow` struct and `query_contradicts_edges_for_entry(entry_id: u64)` async method on `SqlxStore`
- `crates/unimatrix-store/src/lib.rs` — added `ContradictEdgeRow` to public `use read::{...}` export
- `crates/unimatrix-server/src/background.rs` — added NLI guard in `process_auto_quarantine`, plus `nli_auto_quarantine_allowed()`, `parse_nli_contradiction_from_metadata()`, `NliQuarantineCheck` enum; threaded `nli_enabled: bool` and `nli_auto_quarantine_threshold: f32` through `spawn_background_tick` → `background_tick_loop` → `run_single_tick` → `maintenance_tick` → `process_auto_quarantine`

**Logic:**
1. If `nli_enabled` is false, the guard is skipped (existing behavior preserved).
2. Query all `Contradicts` edges targeting the candidate entry.
3. If no edges exist → Allowed (topology penalty from another cause).
4. If any edge is non-NLI (source != "nli") or is bootstrap_only → Allowed (human-authored contradiction; NLI score irrelevant).
5. If ALL edges are NLI-origin and any has `nli_contradiction` score <= threshold → BlockedBelowThreshold (skip this cycle).
6. Otherwise → Allowed.

**Tests added** (9 tests in `background.rs` `#[cfg(test)]`):
- 5 unit tests for `parse_nli_contradiction_from_metadata`
- 4 integration tests for `nli_auto_quarantine_allowed` (below-threshold blocked, above-threshold allowed, mixed edges allowed, no edges allowed)

### Sub-task B: EvalServiceLayer NLI handle wiring (ADR-006, AC-18)

`eval/profile/layer.rs` was already complete from the linter's earlier pass. Layer has:
- `nli_handle: Option<Arc<NliServiceHandle>>` field
- Step 2: validates `nli_model_name` via `NliModel::from_config_name()`
- Step 6b: constructs `Arc<NliServiceHandle>` and calls `start_loading()` when `nli_enabled`
- `has_nli_handle()` and `nli_handle()` accessor methods

**Tests added** (3 tests in `eval/profile/layer_tests.rs`):
- `test_from_profile_nli_disabled_no_nli_handle`
- `test_from_profile_nli_enabled_has_nli_handle`
- `test_from_profile_invalid_nli_model_name_returns_config_invariant`

### Sub-task C: model-download CLI flags (AC-16)

`main.rs` `ModelDownload` struct variant already has `nli: bool` and `nli_model: Option<String>` fields from the linter's earlier work.

**Tests added** (in `main_tests.rs`):
- `test_model_download_nli_flag_parsed`
- `test_model_download_nli_model_flag_parsed`
- `test_model_download_nli_model_requires_nli`
- `test_model_download_nli_deberta_flag_parsed`

### Sub-task D: Server startup NLI wiring + pool floor (ADR-001 crt-023)

`main.rs` already completed by linter:
- Pool floor: `effective_pool_size = if nli_enabled { rayon_pool_size.max(6).min(8) } else { rayon_pool_size }`
- `NliServiceHandle::new()` constructed, `start_loading()` called
- Both daemon and stdio `spawn_background_tick` call sites pass `nli_handle` and `inference_config`

`eval/runner/layer.rs` — `wait_for_nli_ready()` with `NliNotReadyForEval` enum (Failed/Timeout variants), 60s max wait, 500ms poll interval.

`eval/runner/mod.rs` — SKIPPED profile tracking; `skipped.json` written when any profiles are skipped; guard against empty layers.

## Build and Test Results

```
cargo build --workspace  →  0 errors, 0 warnings (clean)
cargo test --workspace   →  All agent-8 tests pass
```

**Pre-existing failures** (not caused by this agent's changes):
- 3 tests in `services::nli_detection::tests` (`test_bootstrap_promotion_*`) from untracked `nli_detection.rs` written by wave 3 agent — these fail because `nli_detection.rs` depends on NLI infrastructure that requires model loading in tests. Not part of agent-8 scope.

## Files Created/Modified

- `crates/unimatrix-store/src/read.rs` — `ContradictEdgeRow` + `query_contradicts_edges_for_entry`
- `crates/unimatrix-store/src/lib.rs` — export `ContradictEdgeRow`
- `crates/unimatrix-server/src/background.rs` — NLI guard + NliQuarantineCheck enum + 9 tests
- `crates/unimatrix-server/src/eval/profile/layer_tests.rs` — 3 NLI wiring tests
- `crates/unimatrix-server/src/eval/runner/layer.rs` — `wait_for_nli_ready`
- `crates/unimatrix-server/src/eval/runner/mod.rs` — SKIPPED profile tracking

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-store`, `unimatrix-server/background` — found existing patterns for store query methods, background tick threading; no surprises.
- Stored: entry via `/uni-store-pattern` — "NLI auto-quarantine guard: thread `nli_enabled` + `nli_auto_quarantine_threshold` through the full background tick chain; use `write_pool_server()` not `write_pool()` in server-context tests (the latter is `pub(crate)`)." This gotcha (`write_pool()` vs `write_pool_server()`) caused an E0616 compile error during development and is invisible from the source code.
