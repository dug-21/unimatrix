# Agent Report: crt-040-agent-6-path-c-loop

**Agent ID:** crt-040-agent-6-path-c-loop
**Task:** Implement Path C (cosine Supports detection) in `nli_detection_tick.rs`
**Feature:** crt-040

---

## Files Modified

- `crates/unimatrix-server/src/services/nli_detection_tick.rs`

---

## Changes Made

### 1. Imports extended
- Added `EDGE_SOURCE_COSINE_SUPPORTS` to `unimatrix_store` import
- Added `write_graph_edge` to `nli_detection` import

### 2. `MAX_COSINE_SUPPORTS_PER_TICK: usize = 50` constant
Added immediately after `MAX_INFORMS_PER_TICK`, with full doc comment including TODO note for config-promotion (ADR-004, SR-03).

### 3. Removed joint early-return (AC-19)
Deleted `if candidate_pairs.is_empty() && informs_metadata.is_empty()` block. Replaced with a comment explaining the AC-19 rationale. The Path B entry gate (`if candidate_pairs.is_empty()`) is retained — it guards only the NLI batch.

### 4. `run_cosine_supports_path` private async helper
Extracted Path C into a private helper in the `// Private helpers` section. Signature:

```rust
async fn run_cosine_supports_path(
    store: &Store,
    config: &InferenceConfig,
    candidate_pairs: &[(u64, u64, f32)],
    existing_supports_pairs: &HashSet<(u64, u64)>,
    category_map: &HashMap<u64, &str>,
    timestamp: u64,
)
```

Guard order: `!cosine.is_finite()` → threshold → budget cap (break) → category pair filter via HashMap → existing_supports_pairs pre-filter → `write_graph_edge`.

Budget counter incremented ONLY on `true` return from `write_graph_edge`. `false` return (UNIQUE conflict or SQL error): no warn, no counter increment.

Unconditional `tracing::debug!` observability log fires after the loop with fields `cosine_supports_candidates` and `cosine_supports_edges_written`.

### 5. Call site in `run_graph_inference_tick`
Path C call inserted after Path A observability log and before Path B entry gate comment (ADR-003). Passes the Phase 5 `category_map` by reference (see Knowledge Stewardship below).

### 6. Module doc comment updated
Added `# Path C: Cosine Supports (crt-040)` section to the file-level `//!` doc.

### 7. Unit tests (10 tests)
Tests added to `#[cfg(test)] mod tests`:

| Test | TC | Covers |
|------|----|--------|
| `test_path_c_qualifying_pair_writes_supports_edge` | TC-01 | AC-01, R-01 |
| `test_path_c_below_threshold_no_edge` | TC-02 | AC-02 |
| `test_path_c_disallowed_category_no_edge` | TC-03 | AC-03, R-01 |
| `test_path_c_existing_pair_skipped` | TC-04 | AC-04 |
| `test_path_c_runs_unconditionally_nli_disabled` | TC-05 | AC-05, FR-13 |
| `test_path_c_budget_cap_50_from_60_qualifying` | TC-07 | AC-12 (budget cap) |
| `test_path_c_budget_counter_not_incremented_on_unique_conflict` | TC-08 | R-07 |
| `test_path_c_nan_cosine_no_edge` | TC-09 | R-09 |
| `test_path_c_observability_log_fires_with_empty_candidates` | TC-12 | AC-19, R-06 |
| `test_max_cosine_supports_per_tick_value_and_independence` | TC-18 | ADR-004 |

New helper `insert_test_entry_with_category` added to test module for configurable category insertion.

---

## Test Results

- **Path C unit tests:** 10 passed / 0 failed
- **Full workspace:** 4278 passed / 0 failed / 0 new failures

---

## Constraints Verified

- [x] No new HNSW scan — reuses `candidate_pairs` from Phase 4
- [x] No per-pair DB lookup — `category_map` from Phase 5 (O(1) HashMap)
- [x] No `spawn_blocking`, `score_batch`, or rayon in Path C
- [x] `run_graph_inference_tick` remains infallible — no `?` propagation
- [x] `write_nli_edge` not modified
- [x] Joint early-return removed (AC-19)
- [x] Path B entry gate retained
- [x] `EDGE_SOURCE_COSINE_SUPPORTS` constant used (not literal string)
- [x] `timestamp` reused from Path A (not recomputed)
- [x] `false` return from `write_graph_edge`: no warn, no budget increment
- [x] Observability log unconditional, after loop
- [x] No `.unwrap()` in production code
- [x] 500-line rule addressed via helper extraction

---

## Issues Encountered

None. All Wave 1 (store constant, inference-config) and Wave 2 (write_graph_edge) dependencies were already implemented by prior agents. Build passed immediately after adding imports.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-003 (#4029), ADR-004 (#4030), write_graph_edge pattern (#4025), and the nli_detection_tick cross-category informs pattern (#3937). All applied.
- Stored: entry #4038 "Reuse Phase 5 category_map (&str) in Path C helper rather than building a second HashMap" via `/uni-store-pattern`

  Key finding: the pseudocode spec prescribed `HashMap<u64, String>` for category_map in Path C. The Phase 5 sort already builds `HashMap<u64, &str>` from `all_active`. Passing this by reference to the extracted helper avoids a second allocation and clone loop. The `&str` lifetime holds for the entire tick stack frame. This deviation from the pseudocode is explicitly noted as an implementation detail the test plan is agnostic on.
