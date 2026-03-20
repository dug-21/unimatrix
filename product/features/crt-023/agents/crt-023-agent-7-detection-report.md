# Agent Report: crt-023-agent-7-detection

**Feature**: crt-023 (NLI Contradiction Detection)
**Component**: Post-store NLI detection + bootstrap edge promotion
**Agent ID**: crt-023-agent-7-detection

## Deliverables

### Files Created
- `/workspaces/unimatrix/crates/unimatrix-server/src/services/nli_detection.rs` (NEW, 1233 lines)

### Files Modified
- `/workspaces/unimatrix/crates/unimatrix-server/src/services/store_ops.rs` — added `NliStoreConfig` struct, `nli_handle`/`nli_cfg` fields to `StoreService`, fire-and-forget NLI spawn in `insert()`

Note: Other required files (`services/mod.rs`, `background.rs`, `main.rs`, `test_support.rs`, `read.rs`) were already modified and committed by other wave agents (agents 6 and 8) before this session resumed.

## Implementation Summary

### `nli_detection.rs`
- `run_post_store_nli`: fire-and-forget async handler that queries k nearest neighbors via HNSW, scores pairs via `rayon_pool.spawn()` (W1-2), writes `Supports`/`Contradicts` edges respecting circuit breaker cap (`max_contradicts_per_tick` counts both edge types combined per AC-13/R-09)
- `maybe_run_bootstrap_promotion`: background tick entry point; idempotent via COUNTERS marker (ADR-005); defers when NLI not ready (FR-25)
- `run_bootstrap_promotion`: fetches all `bootstrap_only=1 Contradicts` edges, scores batch via single rayon spawn, promotes above threshold or deletes below threshold, sets marker on completion
- All DB writes use `write_pool_server()` (SR-02)
- Metadata written as `{"nli_entailment": f32, "nli_contradiction": f32}` JSON (AC-11)

### `store_ops.rs`
- `NliStoreConfig`: config snapshot struct avoiding `Arc<InferenceConfig>` thread through service layer
- `StoreService` fields: `nli_handle: Arc<NliServiceHandle>`, `nli_cfg: NliStoreConfig`
- Post-HNSW spawn: guarded by `nli_cfg.enabled && nli_handle.is_ready_or_loading() && !embedding.is_empty()`; uses ADR-004 move semantics (embedding Vec moved into task)

## Test Results

- `cargo test --workspace`: **all tests pass** (zero failures, zero new failures)
- `nli_detection` module: **11/11 tests pass**

Tests covering: format_nli_metadata, empty embedding guard, NLI-not-ready guard, bootstrap zero rows (AC-12a), bootstrap idempotency (ADR-005), bootstrap deferral (FR-25), confirms above threshold (AC-12b), refutes below threshold (AC-12), second run idempotency, W1-2 thread recording, edge count limit (AC-13)

## Root Cause Analysis: Failing Tests

Three tests (`test_bootstrap_promotion_confirms_above_threshold`, `test_bootstrap_promotion_refutes_below_threshold`, `test_bootstrap_promotion_nli_inference_runs_on_rayon_thread`) were failing due to:

**Root cause**: `insert_test_entry_raw` bound `NULL` for `previous_hash`, `feature_cycle`, and `trust_source` columns, which have `NOT NULL DEFAULT ''` constraints in the schema. SQLite enforces NOT NULL even when a DEFAULT is defined — a DEFAULT only applies when the column is omitted from the INSERT, not when NULL is explicitly provided. With `INSERT OR IGNORE`, the violation was silently swallowed, leaving the entries table empty. `run_bootstrap_promotion` then found the bootstrap edge (after fixing `query_bootstrap_contradicts` to use `write_pool_server()`) but skipped all rows because `get_content_via_write_pool` returned `EntryNotFound`.

**Fix**: Changed `NULL` to `''` for the three `NOT NULL DEFAULT ''` columns in `insert_test_entry_raw`.

Secondary fix: Changed `get_content_via_write_pool` calls (replacing `store.get()` which uses `read_pool()`) to avoid WAL isolation issues in tests — though the primary fix above made this moot.

## Architecture Compliance

- W1-2: All NLI inference via `rayon_pool.spawn()` — verified
- ADR-004: Embedding `Vec<f32>` moved into fire-and-forget task after HNSW insert — no clone
- ADR-005: Idempotency via `bootstrap_nli_promotion_done` COUNTERS key
- FR-25: Deferral when NLI not ready (no marker set, retry next tick)
- AC-13/R-09: Circuit breaker counts Supports + Contradicts combined
- SR-02: All edge writes use `write_pool_server()`

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` and `unimatrix-store` -- found conventions for `write_pool_server()` vs `read_pool()` usage in WAL mode
- Stored: entry via `/uni-store-pattern` — "SQLite NOT NULL DEFAULT '' gotcha: INSERT OR IGNORE silently drops row when NULL bound for NOT NULL column" (see below)

## Commit

`5d02e16 impl(nli-detection): post-store NLI detection + bootstrap edge promotion (#327)`
