# Agent Report: col-031-agent-6-search-scoring

**Agent ID**: col-031-agent-6-search-scoring
**Feature**: col-031 — Phase-Conditioned Frequency Table
**Task**: Wire PhaseFreqTableHandle into the fused scoring hot-path in `search.rs`
**GH Issue**: #414

---

## Files Modified

- `crates/unimatrix-server/src/services/search.rs`

---

## Changes Delivered

### 1. Import
Added `use crate::services::phase_freq_table::PhaseFreqTableHandle;`

### 2. `ServiceSearchParams` — new field
Added `pub current_phase: Option<String>` after `category_histogram`, with full doc comment covering the three behavioral states (None, Some+fallback, Some+active).

### 3. `SearchService` struct — new field
Added `phase_freq_table: PhaseFreqTableHandle` after `fusion_weights`, documented as non-optional (ADR-005).

### 4. `SearchService::new()` — new parameter
Added `phase_freq_table: PhaseFreqTableHandle` as the final parameter and wired it into the constructor body.

### 5. Pre-loop phase snapshot extraction
Inserted after `boost_map` prefetch (Step 6c) and before NLI scoring (Step 7). Implements:
- Lock acquisition order comment naming all three handles in sequence
- Three cases: None / use_fallback=true guard / active snapshot clone
- Snapshot type: `Option<HashMap<String, Vec<(u64, f32)>>>` keyed by category

### 6. Scoring loop — `phase_explicit_norm`
Replaced the `phase_explicit_norm: 0.0` placeholder with the snapshot lookup:
- `None` snapshot → `0.0` (cold-start identity, NFR-04)
- `Some(snapshot)`, absent category → `1.0` (neutral)
- `Some(snapshot)`, absent entry → `1.0` (neutral)
- `Some(snapshot)`, present entry → `rank_score as f64`

### 7. `FusedScoreInputs` construction
Updated comment from "ADR-003 placeholder — always 0.0" to `// col-031: from pre-built phase snapshot`.

---

## Tests

**10 new unit tests** added in `#[cfg(test)] mod tests`:

| Test | Covers |
|------|--------|
| `test_scoring_current_phase_none_sets_phase_explicit_norm_zero` | AC-11 Test 1 |
| `test_scoring_use_fallback_true_sets_phase_explicit_norm_zero` | AC-11 Test 2, R-03 primary |
| `test_scoring_score_identity_cold_start` | AC-11 Test 3, NFR-04 |
| `test_scoring_populated_snapshot_produces_nonzero_norm` | Active scoring path |
| `test_scoring_absent_entry_in_snapshot_norm_is_neutral` | ADR-003 absent-entry contract |
| `test_scoring_absent_category_in_snapshot_norm_is_neutral` | ADR-003 absent-category contract |
| `test_service_search_params_current_phase_accepts_none` | Backward compatibility |
| `test_scoring_snapshot_bucket_rank_lookup` | Multi-entry rank ordering |
| `test_scoring_lock_released_before_scoring_loop` | R-06 lock release |
| Helper functions: `compute_phase_explicit_norm`, `make_phase_snapshot` | Test infrastructure |

**Test results**: 98 passed, 0 failed (search module only)

---

## Build Status

`cargo test -p unimatrix-server --lib -- services::search`: **98 passed, 0 failed**

`cargo build --workspace`: **2 errors** — both from `main.rs` call sites for `spawn_background_tick` needing 22 arguments. These are from the background tick agent's scope (ADR-005 wiring sites in `main.rs`), not from `search.rs`. No errors originate from the search module changes.

---

## Implementation Notes

### Snapshot type choice
The pseudocode spec in `search_scoring.md` shows two variants:
1. `Vec<(u64, f32)>` (flat list for a single phase) with `extract_phase_snapshot` helper
2. `HashMap<String, Vec<(u64, f32)>>` (category-keyed, all categories for a phase)

I implemented variant 2 (the HashMap form) as specified in `search_scoring.md` Change 5, because the scoring loop iterates over candidates from diverse categories. The flat Vec form would require category filtering per-entry in the loop, negating the O(1) category lookup benefit.

### `extract_phase_snapshot` not needed
The IMPLEMENTATION-BRIEF referenced `extract_phase_snapshot` as a possible helper on `PhaseFreqTable`. The `phase_freq_table.rs` agent did not implement this helper (per the `PhaseFreqTable` file read). The snapshot extraction is done inline in the pre-loop block by filtering and collecting from `guard.table` — this is consistent with what the pseudocode shows and requires no change to `phase_freq_table.rs`.

### Lock ordering verification
The pre-loop comment names the full chain:
```
EffectivenessStateHandle -> TypedGraphStateHandle -> PhaseFreqTableHandle
```
This matches the lock acquisition order required by ADR-004 col-031 (#3682).

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` -- surfaced ADR entries #3677, #3689, #3688, #3682; pattern #3207 (FusedScoreInputs extension pattern), #3616 (post-scoring filter insertion point). Applied: cold-start guard placement, lock ordering, absent-entry 1.0 contract.
- Stored: entry #3694 "Phase snapshot extraction pattern: acquire read lock once pre-loop, clone to HashMap, release before scoring loop" via /uni-store-pattern — captures the None/Some(fallback)/Some(active) three-case pattern and the critical distinction that None→0.0 but absent-entry-in-Some→1.0.
