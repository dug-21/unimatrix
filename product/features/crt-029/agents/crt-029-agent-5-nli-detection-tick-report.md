# Agent Report: crt-029-agent-5-nli-detection-tick

**Feature**: crt-029 — Background Graph Inference (Supports Edges)
**Agent**: crt-029-agent-5-nli-detection-tick
**Task**: Implement `crates/unimatrix-server/src/services/nli_detection_tick.rs`
**Date**: 2026-03-27

## Status

COMPLETE. All gates pass. 20/20 tests passing. Commit: `a1dee89`.

## Files Created / Modified

- `crates/unimatrix-server/src/services/nli_detection_tick.rs` — created (773 lines)

## Implementation Summary

Implemented `run_graph_inference_tick` following the 8-phase algorithm from the pseudocode spec:

1. Load all active entries from store
2. Load existing inferred edge set (dedup guard)
3. Select source candidates — metadata-only cap (AC-06c: cap BEFORE Phase 4 embedding fetch)
4. Fetch embeddings for the already-capped source list only (bounds O(N) calls)
5. Build cosine-filtered candidate pairs using `VectorIndex::search()` (synchronous)
6. Build NLI input pairs (text extraction)
7. Dispatch to `rayon_pool.spawn()` — sync-only closure, no `.await` inside (C-14/R-09)
8. Filter by `nli_entailment_threshold`, write `Supports` edges via `write_inferred_edges_with_cap`

Key design decisions followed:
- **C-13**: `write_inferred_edges_with_cap` writes only `Supports` edges. `contradiction` score from NLI is intentionally discarded. No `contradiction_threshold` parameter.
- **C-14/R-09**: Rayon closure is 100% synchronous. The `.await` is on the Tokio thread (outside the closure), on `rayon_pool.spawn()`'s returned future only.
- **C-01**: Only `rayon_pool.spawn()` used — no `spawn_blocking`.
- **AC-06c**: `select_source_candidates` takes only `&[EntryRecord]` — no embeddings involved. Phase 4 fetches embeddings only for the already-capped Vec<u64>.
- **C-08**: 773 lines (≤ 800 limit).

## Test Results

```
running 20 tests
test services::nli_detection_tick::tests::test_select_source_candidates_empty_returns_empty ... ok
test services::nli_detection_tick::tests::test_select_source_candidates_max_sources_honored ... ok
test services::nli_detection_tick::tests::test_select_source_candidates_isolated_first ... ok
test services::nli_detection_tick::tests::test_select_source_candidates_below_max ... ok
test services::nli_detection_tick::tests::test_select_source_candidates_deduplicates_ids ... ok
test services::nli_detection_tick::tests::test_write_inferred_edges_empty_pairs_returns_zero ... ok
test services::nli_detection_tick::tests::test_write_inferred_edges_no_qualifying_scores ... ok
test services::nli_detection_tick::tests::test_write_inferred_edges_supports_written_when_above_threshold ... ok
test services::nli_detection_tick::tests::test_write_inferred_edges_cap_honored ... ok
test services::nli_detection_tick::tests::test_write_inferred_edges_no_contradicts_written ... ok
test services::nli_detection_tick::tests::test_write_inferred_edges_idempotent ... ok
test services::nli_detection_tick::tests::test_run_tick_no_active_entries ... ok
test services::nli_detection_tick::tests::test_run_tick_single_entry_no_neighbors ... ok
test services::nli_detection_tick::tests::test_run_tick_below_min_active_threshold ... ok
test services::nli_detection_tick::tests::test_run_tick_existing_edge_not_duplicated ... ok
test services::nli_detection_tick::tests::test_run_tick_phase3_cap_applied ... ok
test services::nli_detection_tick::tests::test_inference_config_defaults_used ... ok
test services::nli_detection_tick::tests::test_select_source_candidates_respects_isolated_weight ... ok
test services::nli_detection_tick::tests::test_write_inferred_edges_scores_mismatched_length_safe ... ok
test services::nli_detection_tick::tests::test_run_tick_nli_provider_not_loaded_skips_gracefully ... ok
test result: ok. 20 passed; 0 failed
```

## Pre-Merge Gate Results

| Gate | Check | Result |
|------|-------|--------|
| G-1 | `wc -l nli_detection_tick.rs` = 773 | PASS (≤ 800) |
| G-2 | `grep -n 'spawn_blocking'` | PASS (empty) |
| G-3 | `grep -n 'Handle::current'` in code | PASS (mentions in comments only) |
| G-4 | `grep -n 'write_nli_edge.*Contradicts'` | PASS (empty) |
| G-5 | Workspace build: zero errors | PASS |
| G-6 | 20/20 unit tests pass | PASS |

## Issues Encountered

**1. `CrossEncoderProvider` trait visibility**
`NliServiceHandle::get_provider()` returns `Arc<NliProvider>`. Calling `score_batch` requires the `CrossEncoderProvider` trait in scope. The compiler error was "no method named `score_batch` found for struct `Arc<NliProvider>`". Fixed by adding `use unimatrix_embed::{CrossEncoderProvider, NliScores};`.

**2. `VectorIndex` is synchronous despite architecture diagrams showing `.await`**
The pseudocode/ARCHITECTURE.md described async vector index calls. The actual `VectorIndex::search()` and `get_embedding()` are synchronous methods with internal `RwLock`. The `AsyncVectorStore<T>` wrapper (which adds `spawn_blocking` internally) is a different type used in other contexts. The tick receives `&VectorIndex` directly, so calls are synchronous — consistent with how `nli_detection.rs` uses it.

**3. `SqlxStore` is not `Clone`**
Test `test_write_inferred_edges_idempotent` needed to share `Store` across two calls. Cannot `.clone()` a `Store`. Fixed by wrapping in `Arc<Store>` and passing `&arc_store`.

**4. `EntryRecord` has no `Default` impl**
`..Default::default()` fails to compile. All fields including `pre_quarantine_status: Option<u8>` must be written explicitly. Used struct literal with all fields.

**5. `insert_entry_with_id` does not exist in `test_helpers`**
No convenience function matching the needed signature. Wrote local `insert_test_entry(store: &Store, id: u64)` using raw SQL (`INSERT OR IGNORE INTO entries (...) VALUES (?1, ...)`), mirroring the `insert_test_entry_raw` pattern from `nli_detection.rs`.

**6. File exceeded C-08 800-line limit (was 962 lines after initial implementation)**
Fixed by compacting verbose multi-line phase separator comment blocks down to 1-2 line inline comments. Final size after `cargo fmt`: 773 lines.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries including the rayon/tokio boundary constraint (entry #3653), the VectorIndex sync/async distinction, and the Supports-only design rationale. These were directly applied.
- Stored: entry #3663 "VectorIndex is synchronous — rayon closure must not await (C-14/R-09)" via `/uni-store-pattern` (gotcha: compile-invisible runtime panic when `.await` used inside rayon closure on `VectorIndex` calls that look like they should be async based on architecture docs).
