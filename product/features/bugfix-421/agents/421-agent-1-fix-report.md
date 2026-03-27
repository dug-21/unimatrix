# Agent Report: 421-agent-1-fix

**Bug:** GH #421 — NLI detection tick makes zero progress after first tick exhausts candidate pool
**Branch:** bugfix/421-nli-tick-deterministic-reselect
**PR:** https://github.com/dug-21/unimatrix/pull/422

## Files Modified

- `crates/unimatrix-server/Cargo.toml` — added `rand = "0.9"`
- `crates/unimatrix-server/src/services/nli_detection_tick.rs` — all fix changes
- `Cargo.lock` — updated by cargo

## Changes Made

### RC-1 Fix: Deterministic re-selection
Removed `tier2.sort_by(|a, b| b.created_at.cmp(&a.created_at))`. Added `tier1.shuffle(&mut rng)` and `tier2.shuffle(&mut rng)` using `rand::rng()` (rand 0.9 API). Both tiers shuffled independently before `chain` + `take(max_sources)`.

### RC-2 Fix: No-embedding entries polluting tier 1
Added `embedded_ids: &HashSet<u64>` parameter to `select_source_candidates`. Both tier loops now guard with `if !embedded_ids.contains(&entry.id) { continue; }`. In `run_graph_inference_tick` Phase 3, `embedded_ids` is built from `all_active.iter().filter(|e| vector_index.contains(e.id))` before the selector call — one O(N) pass, no new public methods on any type.

## New Tests

1. `test_select_source_candidates_excludes_no_embedding_entries` — verifies entries absent from `embedded_ids` are excluded from both tiers
2. `test_select_source_candidates_nondeterministic_rotation` — verifies correctness properties (len, no duplicates, valid IDs) for shuffled selection
3. `test_select_source_candidates_remainder_by_created_at` — updated from ordered equality assertion to set-membership assertion (order is now non-deterministic by design)

All 8 existing `select_source_candidates` call sites in tests updated to pass an `&embedded_ids` HashSet (all-IDs set for non-embedding tests, preserving original intent).

## Test Results

22 passed / 0 failed in `nli_detection_tick` module. Full `unimatrix-server` suite: all pass.

## Issues

None. No blockers.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entries #3668 (lesson on stable ordering causing stall), #3655 (two-bound pattern), #3669 (async placement lesson, incorrectly named thread_rng)
- Stored: entry #3671 "rand::thread_rng() does not exist in rand 0.9 — use rand::rng()" via `/uni-store-pattern`
- Corrected: entry #3669 → #3672 (fixed incorrect `rand::thread_rng()` reference to `rand::rng()`)
