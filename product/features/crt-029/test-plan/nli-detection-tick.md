# Test Plan: NLI Detection Tick (crt-029)

Source file: `crates/unimatrix-server/src/services/nli_detection_tick.rs`
Pseudocode: `pseudocode/nli-detection-tick.md`

Functions covered:
- `pub async fn run_graph_inference_tick(store, nli_handle, vector_index, rayon_pool, config)`
- `fn select_source_candidates(all_active, existing_edge_set, isolated_ids, max_sources) -> Vec<u64>`
- `async fn write_inferred_edges_with_cap(store, pairs, nli_scores, supports_threshold, max_edges) -> usize`

Risks addressed: R-01(residual), R-02, R-05, R-08, R-09(partial), R-12, R-13

---

## Testing Approach

`run_graph_inference_tick` has async dependencies on `Store`, `NliServiceHandle`,
`VectorIndex`, and `RayonPool`. Full end-to-end unit tests require mock implementations of
all four. Use the following strategy:

- **`select_source_candidates`**: pure function (no async, no external calls) — test directly
  with constructed slices and `HashSet` inputs. No mocks needed.
- **`write_inferred_edges_with_cap`**: async but only depends on `Store` and scalar inputs.
  Use an in-memory `Store` and mock `NliScores` structs. No live NLI model needed (SR-08 goal).
- **`run_graph_inference_tick` guard path**: test only the Phase 1 early-return by stubbing
  `NliServiceHandle` to return `Err`. Verify no DB calls are made.
- **R-09 / C-14 rayon boundary**: cannot be caught by unit tests. Covered by grep gate and
  independent code review (see below).

All tests in `#[cfg(test)]` module inside `nli_detection_tick.rs`.
All async tests use `#[tokio::test]`.

---

## Unit Test Expectations — `select_source_candidates`

### AC-06c / R-02 — Cap before embedding

#### `test_select_source_candidates_cap_enforced`
- Input: 200 active entries (all same category), empty `existing_edge_set`, empty `isolated_ids`, `max_sources = 10`
- Call `select_source_candidates(&all_active, &edge_set, &isolated, 10)`
- Assert returned `Vec` length is exactly 10 (not 200)
- Assert no entry appears twice in the result

#### `test_select_source_candidates_cap_larger_than_entries`
- Input: 5 active entries, `max_sources = 20`
- Assert returned `Vec` length is 5 (bounded by available entries, not cap)

#### `test_select_source_candidates_empty_input`
- Input: 0 active entries, `max_sources = 10`
- Assert returns empty `Vec`

### AC-07 / R-12 — Priority ordering

#### `test_select_source_candidates_cross_category_first`
- Input: construct entries where:
  - Entries A, B, C have `category = "decision"` and entry D has `category = "pattern"` (A–D all active)
  - `existing_edge_set`: empty
  - `isolated_ids`: empty
  - `max_sources = 2`
- Note: `select_source_candidates` produces source IDs, not pairs. The cross-category
  prioritization here means entries that participate in cross-category pairs get priority.
  The test must set up the active slice so cross-category entries are identifiable.
- Assert cross-category-eligible entry IDs appear first in the output
- If cap = 2 and 2 cross-category entries exist, assert both are in the output; no same-category entries

#### `test_select_source_candidates_isolated_second`
- Input:
  - 5 active entries, same category
  - `isolated_ids` contains IDs for entries X, Y (2 entries)
  - `existing_edge_set`: empty
  - `max_sources = 2`
- Assert returned `Vec` contains X and Y (isolated entries get second-tier priority)
- Assert non-isolated entries are excluded when cap = 2

#### `test_select_source_candidates_remainder_by_created_at`
- Input:
  - 5 active entries, same category, none isolated
  - `existing_edge_set`: empty
  - `max_sources = 3`
  - Entries have distinct `created_at` values in known order
- Assert returned `Vec` contains the 3 most recently created entries (descending `created_at`)

#### `test_select_source_candidates_priority_ordering_combined`
- Input: 3 cross-category entries, 3 isolated entries, 4 same-category non-isolated entries; `max_sources = 5`
- Assert first 3 entries in result are cross-category entries
- Assert entries 4–5 are from the isolated set
- Assert same-category non-isolated entries are absent

---

## Unit Test Expectations — `write_inferred_edges_with_cap`

### AC-08 / AC-09 / AC-11 / R-08 — Cap, threshold, Supports-only

#### `test_write_inferred_edges_with_cap_cap_enforced`
- Setup: in-memory `Store`, 10 pairs of entry IDs (all pairs have real entries in DB),
  10 mock `NliScores` each with `entailment = 0.9` (above threshold), `contradiction = 0.8`
- Call `write_inferred_edges_with_cap(store, &pairs_10, &scores_10, 0.7, 3)`
- Assert return value is `3` (exactly cap edges written)
- Assert GRAPH_EDGES has exactly 3 rows (not 10)

#### `test_write_inferred_edges_threshold_strict_greater`
- Setup: 3 pairs, scores: `[0.71, 0.70, 0.69]` entailment vs threshold `0.70`
- Call `write_inferred_edges_with_cap(store, &pairs, &scores, 0.70, 10)`
- Assert return value is `1` (only the 0.71 pair exceeds strict `>`)
- Assert the pair with entailment exactly 0.70 is NOT written (AC-09)
- Assert the pair with 0.69 is NOT written

#### `test_write_inferred_edges_supports_only_no_contradicts`
- Setup: 5 pairs, all scores have high `contradiction = 0.95` AND high `entailment = 0.9`
- Call `write_inferred_edges_with_cap(store, &pairs, &scores, 0.7, 10)`
- Assert all 5 edges written as `Supports` (entailment exceeds threshold)
- Assert GRAPH_EDGES has NO rows with `relation_type = 'Contradicts'`
- This verifies the tick discards contradiction scores entirely (AC-10a, R-01 residual)

#### `test_write_inferred_edges_zero_eligible`
- Setup: 5 pairs, all scores have `entailment = 0.5` (below threshold 0.7)
- Call `write_inferred_edges_with_cap(store, &pairs, &scores, 0.7, 10)`
- Assert return value is `0`
- Assert GRAPH_EDGES is empty

#### `test_write_inferred_edges_cap_at_exact_count`
- Setup: 3 pairs, all eligible (entailment > threshold), `max_edges = 3`
- Assert return value is `3` (cap == available eligible pairs — all written)

#### `test_write_inferred_edges_insert_or_ignore_idempotency`
- Setup: pre-seed GRAPH_EDGES with a `Supports` edge `(A, B)`
- Call `write_inferred_edges_with_cap` with pair `(A, B)` and eligible score
- Assert no duplicate row is created (GRAPH_EDGES still has exactly 1 row for that pair)
- Assert return value is `0` for the duplicate attempt (INSERT OR IGNORE silently drops it;
  the cap counter does not increment for duplicate ignores — confirm this matches implementation)

#### `test_write_inferred_edges_edge_source_nli`
- Setup: write one eligible pair
- Query GRAPH_EDGES for the written row
- Assert `source = 'nli'` (EDGE_SOURCE_NLI constant — AC-13)
- Assert `bootstrap_only = false`

---

## Unit Test Expectations — `run_graph_inference_tick` (guard path)

### AC-05 — NLI not ready early return

#### `test_run_graph_inference_tick_nli_not_ready_no_op`
- Setup: `NliServiceHandle` stub that returns `Err` from `get_provider()`
- Call `run_graph_inference_tick(store, &stub_handle, &vector_index, &rayon_pool, &config)`
- Assert: no DB reads (mock `Store` records calls — assert 0 calls to `query_by_status`,
  `query_entries_without_edges`, `query_existing_supports_pairs`)
- Assert: function returns without panic

---

## Edge Case Unit Tests

### Edge cases from RISK-TEST-STRATEGY.md

#### `test_tick_empty_entry_set`
- Setup: empty `entries` table, NLI ready
- Run full tick (or test via `select_source_candidates` with empty input)
- Assert: 0 edges written, no panic, function completes

#### `test_tick_single_active_entry`
- Setup: 1 active entry
- `select_source_candidates` produces `[entry_id]`; HNSW search returns no neighbours
- Assert: 0 pairs collected, 0 NLI calls, 0 edges written

#### `test_tick_all_pairs_pre_filtered`
- Setup: `existing_supports_pairs` pre-populated with all possible pairs from the active set
- All pairs are in the pre-filter HashSet
- Assert: NLI scoring not called (0 pairs pass pre-filter)

#### `test_tick_pair_deduplication`
- Setup: entry A and entry B both appear as HNSW neighbours of each other; this produces
  candidates `(A, B)` and `(B, A)` from separate HNSW searches
- Assert: after normalization, only one pair `(min(A,B), max(A,B))` is scored
- Assert: exactly one NLI call for that pair, not two

#### `test_tick_source_embedding_none_skipped`
- Setup: 3 source candidates; mock `get_embedding` returns `None` for candidate 2
- Assert: candidate 2 is skipped; candidates 1 and 3 are processed
- Assert: no panic

#### `test_tick_score_batch_length_mismatch`
- Setup: mock scorer returns a result `Vec<NliScores>` shorter than the input pairs slice
- Assert: tick handles the mismatch defensively (no panic, no index out-of-bounds);
  only pairs with corresponding scores are processed

#### `test_tick_idempotency`
- Setup: 2 eligible pairs, run `write_inferred_edges_with_cap` twice with same data
- Assert: GRAPH_EDGES row count after second run equals row count after first run
  (INSERT OR IGNORE prevents doubling; AC-16)

---

## R-09 / C-14 — Rayon/Tokio Boundary (Compile-Invisible)

**These cannot be caught by unit tests running on the Tokio runtime.**

### Grep Gates (mandatory, pre-merge)

```bash
# Gate 1: No Handle::current() anywhere in the file
grep -n 'Handle::current' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty output

# Gate 2: Any .await inside a rayon closure is gate-blocking
grep -n '\.await' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Note: .await is valid OUTSIDE the closure (e.g., on the rayon_pool.spawn() future itself
# and on store calls). All matches must be reviewed; any match INSIDE the closure body is a defect.
```

### Independent Code Review Requirement

The agent or reviewer checking R-09 MUST NOT be the same agent that wrote the rayon closure.
This requirement is non-negotiable (C-14). The reviewer must:
1. Locate the `rayon_pool.spawn()` call in `nli_detection_tick.rs`
2. Read the entire closure body
3. Confirm: no `Handle::current()`, no `.await`, no function calls that internally await
4. Sign off explicitly in the Gate 3c report

The test plan agent cannot satisfy this requirement for the implementation agent's own code.
The Stage 3c tester serves as the independent reviewer.

---

## Integration Harness Tests (Stage 3c)

Three new tests to add to `suites/test_lifecycle.py` (see OVERVIEW.md):

1. `test_graph_inference_tick_writes_supports_edges` — verifies graph edges written, source = 'nli'
2. `test_graph_inference_tick_no_contradicts_edges` — verifies no Contradicts from tick path
3. `test_graph_inference_tick_nli_disabled` — verifies nli_enabled=false gate

All three use `server` fixture (fresh DB per test). They require NLI to be available in the
integration environment (ONNX model loaded). If the environment lacks NLI capability, these
tests should be marked `@pytest.mark.skipif(not nli_available(), ...)`.

---

## Assertions Summary

| AC-ID | Test Name | Expected Result |
|-------|-----------|-----------------|
| AC-05 | `test_run_graph_inference_tick_nli_not_ready_no_op` | 0 DB calls, no panic |
| AC-06c/R-02 | `test_select_source_candidates_cap_enforced` | len <= max_sources |
| AC-07/R-12 | `test_select_source_candidates_cross_category_first` | Cross-category IDs at head |
| AC-07/R-12 | `test_select_source_candidates_isolated_second` | Isolated IDs before non-isolated |
| AC-07/R-12 | `test_select_source_candidates_priority_ordering_combined` | Full 3-tier order |
| AC-08 | (grep gate) | 0 spawn_blocking calls in file |
| AC-09 | `test_write_inferred_edges_threshold_strict_greater` | Exactly 0.70 is NOT written |
| AC-10a/R-01 | `test_write_inferred_edges_supports_only_no_contradicts` | 0 Contradicts rows |
| AC-10a | (grep gate) | `grep -n 'Contradicts' nli_detection_tick.rs` empty |
| AC-11/R-08 | `test_write_inferred_edges_with_cap_cap_enforced` | Exactly cap edges written |
| AC-13 | `test_write_inferred_edges_edge_source_nli` | source = 'nli', bootstrap_only = false |
| AC-16/R-13 | `test_tick_idempotency` | Row count does not double on re-run |
| AC-R09/R-09 | Grep gate + independent code review | No tokio handles inside rayon closure |
| (edge) | `test_tick_empty_entry_set` | 0 edges, no panic |
| (edge) | `test_tick_pair_deduplication` | 1 NLI call for (A,B)/(B,A) pair |
| (edge) | `test_tick_source_embedding_none_skipped` | Remaining candidates processed |
