# Test Plan: search.rs Step 6d

## Component

`crates/unimatrix-server/src/services/search.rs` — Step 6d block

Step 6d inserts between Step 6b (supersession injection) and Step 6c (co-access prefetch):
```
Step 6b → Step 6d (PPR expansion) → Step 6c (co-access prefetch) → Step 7 (NLI)
```

Tests live in the `#[cfg(test)]` module of `search.rs` or `search_tests.rs`.
Tests involving `entry_store.get()` use `#[tokio::test]` (async).
Tests involving only synchronous score math use `#[test]`.

---

## Test Helpers

### `make_results_with_scores(entries: &[(u64, EntryStatus, f64)]) -> Vec<(EntryRecord, f64)>`
Constructs a `Vec<(EntryRecord, f64)>` with synthetic entries. The `f64` is the HNSW similarity.
Sets `EntryRecord.status` per the input. All other EntryRecord fields take valid defaults.

### `make_graph_with_supports_edge(source: u64, target: u64) -> TypedRelationGraph`
Returns a `TypedRelationGraph` with a Supports edge `source → target`.

### `default_ppr_cfg() -> PprConfig` (or inline struct fields)
Returns the default PPR config values: `alpha=0.85, iterations=20, threshold=0.05, blend=0.15, max_expand=50`.

### Mock store / stub store
For tests requiring `entry_store.get()`, use the existing test gateway pattern (`new_permissive()`
with a throwaway SQLite store, entry #315/#264). Alternatively, insert entries into the store
in the arrange phase and pass the real store to Step 6d.

---

## AC-11 / I-01: Step Ordering

### `test_step_6d_comment_order_in_search_rs` (code review gate)
Verification: in `search.rs`, confirm the source text contains step comments in this order:
`// Step 6b` ... `// Step 6d` ... `// Step 6c` ... `// Step 7`.
Method: code review or `grep -n "Step 6" crates/unimatrix-server/src/services/search.rs` returns
lines in ascending order: 6b, 6d, 6c, 7.

### `test_step_6d_ppr_entry_participates_in_coaccess_prefetch`
Arrange: TypedRelationGraph with Supports A→B (B is HNSW seed, A is PPR-only neighbor).
HNSW pool: `[(B, 0.9)]`. Store has entries A and B. Config: threshold=0.01 (low), blend=0.15.
Act: Run Step 6d only (not full pipeline). Confirm A is in `results_with_scores` after Step 6d.
Then verify that co-access prefetch (Step 6c) would use A as an anchor candidate by checking
the expanded pool contains A before 6c begins.
This validates that Step 6d precedes Step 6c — if the order were reversed, A would not be in
the pool when 6c runs.

---

## AC-12 / R-02: use_fallback = true — Bit-for-Bit Identity

### `test_step_6d_skipped_when_use_fallback_true`
Arrange: `use_fallback = true`. HNSW pool: `[(id=1, sim=0.8), (id=2, sim=0.6)]`.
TypedRelationGraph: non-empty (to prove PPR would have run otherwise).
Act: run Step 6d.
Assert:
- `results_with_scores` contains exactly `[(1, 0.8), (2, 0.6)]` (IDs and scores unchanged).
- The `Vec` before and after Step 6d are equal by value (`assert_eq!`).
- No additional entries were appended.

### `test_step_6d_use_fallback_true_no_allocation`
Verify (by code review or structural test): when `use_fallback = true`, the Step 6d block
performs no `HashMap::new()` call and no `entry_store.get()` call.
Method: code review of the guard placement (`if use_fallback { /* skip */ }` must be the first
statement in the Step 6d block, before any allocation).

---

## AC-13 / R-06: Inclusion Threshold Boundary (Strictly Greater-Than)

### `test_step_6d_entry_at_exact_threshold_not_included` (R-06 critical)
Arrange: PPR score map contains entry id=99 with score exactly equal to `ppr_inclusion_threshold`
(e.g., both = 0.05). Entry 99 is NOT in the HNSW pool.
Act: run Step 6d expansion phase.
Assert: entry id=99 is NOT in `results_with_scores`.
Rationale: AC-13 specifies `> threshold` (strictly greater). Entry at exactly the threshold is
excluded.

### `test_step_6d_entry_just_above_threshold_included` (R-06)
Arrange: Same as above but entry id=99 has PPR score = `ppr_inclusion_threshold + f64::EPSILON`.
Act: run Step 6d expansion.
Assert: entry id=99 IS in `results_with_scores`.

### `test_step_6d_pool_expansion_capped_at_ppr_max_expand` (E-04)
Arrange: PPR score map contains 100 entries all above threshold. `ppr_max_expand = 10`.
Act: run Step 6d expansion.
Assert: `results_with_scores.len() == original_pool_len + 10` (exactly 10 new entries).
Assert: the 10 added entries are the 10 highest-PPR-scoring entries (sort by score desc verified).

### `test_step_6d_expansion_sorted_by_ppr_score_desc`
Arrange: PPR score map with entries {A: 0.3, B: 0.8, C: 0.5}, all above threshold.
`ppr_max_expand = 2`. None in HNSW pool.
Act: run Step 6d.
Assert: B and C are added (top-2 by PPR score), not A. B appears before or at same position as C.

---

## AC-13 / R-08: Quarantine Enforcement (Critical)

### `test_step_6d_quarantined_entry_not_appended` (R-08 scenario 1)
Arrange: PPR score map contains entry id=77 with score 0.3 (above default threshold of 0.05).
Entry 77 NOT in HNSW pool. `entry_store.get(77)` returns an `EntryRecord` with
`status = EntryStatus::Quarantined`.
Act: run Step 6d.
Assert: entry id=77 is NOT in `results_with_scores`.
Assert: no panic, clean completion.

### `test_step_6d_active_entry_appended` (R-08 scenario 2)
Arrange: Same setup but `entry_store.get(77)` returns `status = EntryStatus::Active`.
Act: run Step 6d.
Assert: entry id=77 IS in `results_with_scores`.

### `test_step_6d_fetch_error_silently_skipped` (R-05 scenario 1)
Arrange: PPR score map has entries {1: 0.3, 2: 0.2} both above threshold, neither in HNSW pool.
`entry_store.get(1)` returns `Err(...)`. `entry_store.get(2)` returns a valid active entry.
Act: run Step 6d.
Assert: entry id=1 is NOT in `results_with_scores`.
Assert: entry id=2 IS in `results_with_scores`.
Assert: no panic, clean completion.

### `test_step_6d_all_fetches_fail_pool_unchanged` (R-05 / FM-02)
Arrange: PPR score map with 5 entries above threshold. All `entry_store.get()` calls return `Err`.
HNSW pool has 3 entries.
Act: run Step 6d.
Assert: `results_with_scores.len() == 3` (unchanged — no new entries added).

---

## AC-14: PPR-Only Entry Initial Similarity

### `test_step_6d_ppr_only_entry_initial_sim` (AC-14)
Arrange: PPR score map entry id=55 with PPR score 0.4. Not in HNSW pool.
`ppr_blend_weight = 0.15`. `entry_store.get(55)` returns active entry.
Act: run Step 6d.
Assert: the entry appended for id=55 has similarity = `0.15 * 0.4 = 0.06`.
Tolerance: `(actual - 0.06).abs() < 1e-9`.

### `test_step_6d_ppr_only_entry_blend_weight_zero` (R-03)
Arrange: PPR score map entry id=55 with score 0.4. Not in pool. `ppr_blend_weight = 0.0`.
Act: run Step 6d.
Assert: appended entry has similarity = `0.0 * 0.4 = 0.0`.

---

## AC-15: Blend Formula for Existing HNSW Candidates

### `test_step_6d_blend_formula_known_values` (AC-15)
Arrange: HNSW pool has entry id=10 with similarity 0.8. PPR score map has id=10 with score 0.4.
`ppr_blend_weight = 0.15`.
Act: run Step 6d blend phase.
Assert: id=10's similarity in `results_with_scores` = `(1.0 - 0.15) * 0.8 + 0.15 * 0.4 = 0.85*0.8 + 0.15*0.4 = 0.68 + 0.06 = 0.74`.
Tolerance: `(actual - 0.74).abs() < 1e-9`.

### `test_step_6d_blend_weight_zero_leaves_hnsw_unchanged` (R-03 scenario 3)
Arrange: HNSW pool entry id=10 with similarity 0.8. PPR score map has id=10 with score 0.4.
`ppr_blend_weight = 0.0`.
Act: run blend phase.
Assert: id=10's similarity = `1.0 * 0.8 + 0.0 * 0.4 = 0.8` (unchanged).
Tolerance: exact equality.

### `test_step_6d_blend_weight_one_overwrites_hnsw` (R-11 scenario 1)
Arrange: HNSW pool entry id=10 with similarity 0.9. PPR score map id=10 with score 0.2.
`ppr_blend_weight = 1.0`.
Act: run blend phase.
Assert: id=10's similarity = `0.0 * 0.9 + 1.0 * 0.2 = 0.2` (HNSW score fully replaced by PPR).

### `test_step_6d_blend_weight_one_ppr_only_entry_gets_ppr_score` (R-11 scenario 2)
Arrange: PPR-only entry id=20 with PPR score 0.8. Not in HNSW pool. `ppr_blend_weight = 1.0`.
Act: append phase (initial_sim = ppr_blend_weight * ppr_score = 1.0 * 0.8 = 0.8).
Assert: id=20's similarity = 0.8.
Assert: id=20 ranks above HNSW candidates with similarity < 0.8 in the pool.

### `test_step_6d_hnsw_entries_without_ppr_score_unchanged`
Arrange: HNSW pool has entries {A: 0.7, B: 0.5}. PPR score map only has entry A (not B).
Act: run blend phase.
Assert: A's similarity is updated by blend formula.
Assert: B's similarity is unchanged at 0.5.

---

## AC-16 / R-10: Personalization Vector Construction

### `test_step_6d_non_uniform_phase_snapshot_changes_seeds` (AC-16)
Arrange: HNSW pool with two entries: id=1 (sim=0.8) and id=2 (sim=0.6).
Phase snapshot: `{id=1: phase_affinity=2.0, id=2: phase_affinity=1.0}` (non-uniform).
Expected seeds before normalization: `{1: 1.6, 2: 0.6}`. After normalization: `{1: 1.6/2.2, 2: 0.6/2.2}`.
Without phase affinity (uniform 1.0): seeds before norm `{1: 0.8, 2: 0.6}` → `{1: 0.8/1.4, 2: 0.6/1.4}`.
Assert: the seeds differ from the uniform baseline for both entries.
Assert: no NaN, no Inf in the seed values.

### `test_step_6d_none_phase_snapshot_uses_hnsw_score_only` (R-10 / AC-06 cold-start)
Arrange: HNSW pool with id=1 (sim=0.8), id=2 (sim=0.6). Phase snapshot: `None`.
Expected: seeds = `{1: 0.8, 2: 0.6}` normalized → `{1: 0.8/1.4, 2: 0.6/1.4}`.
Assert: seed values match the HNSW-score-normalized baseline exactly.

### `test_step_6d_no_phase_affinity_score_direct_call_in_step_6d` (R-10 code review gate)
Verification: `grep "phase_affinity_score(" crates/unimatrix-server/src/services/search.rs | grep "Step 6d"`
must return no results. Step 6d reads from the already-cloned snapshot; it does NOT call
`phase_affinity_score()` directly.
(Alternatively: code review of search.rs confirms the snapshot read pattern is used.)

---

## AC-06: Zero-Sum Personalization Vector Guard

### `test_step_6d_all_zero_hnsw_scores_skips_ppr` (FM-05)
Arrange: HNSW pool: `[(id=1, sim=0.0), (id=2, sim=0.0)]`.
Phase snapshot: `None` (cold-start, affinity=1.0). Seeds before normalization: `{1: 0.0, 2: 0.0}`. Sum=0.0.
Act: run Step 6d.
Assert: `results_with_scores` is unchanged (pool identical before and after).
Assert: `personalized_pagerank` was NOT called (zero-sum guard fires).

---

## AC-17: Integration Scenario — Entry Surfaces via PPR

### `test_step_6d_ppr_surfaces_support_entry` (AC-17 inline unit test)
Arrange:
- TypedRelationGraph: Supports edge A→B (A=100, B=200).
- HNSW pool: `[(B=200, sim=0.8)]`. Entry A is NOT in the HNSW pool.
- Store: entries A=100 (Active) and B=200 (Active) exist.
- Config: `ppr_threshold=0.001` (very low), `ppr_blend_weight=0.15`, `ppr_max_expand=10`.
Act: Run Step 6d with these inputs.
Assert: `results_with_scores.iter().any(|(e, _)| e.id == 100)` — A surfaces.
Assert: A's similarity = `ppr_blend_weight * ppr_score_of_A` (within tolerance 1e-9).
This is the canonical acceptance test for AC-17.

---

## I-02: Co-Access Anchor Interaction

### `test_step_6d_ppr_top_entry_becomes_coaccess_anchor`
Arrange: After Step 6d, a PPR-only entry with high blended score ranks at the top of
`results_with_scores`. Verify that when Step 6c (co-access prefetch) runs over the full
expanded pool, it uses the PPR-surfaced entry as one of its top-K anchors.
Method: This test may need to exercise the co-access prefetch call with the post-Step-6d pool
as input and confirm the anchor selection includes the PPR-injected entry.
Note: If co-access prefetch anchor selection is not directly unit-testable, verify via the
T-PPR-IT-01 integration test (OVERVIEW.md) instead.

---

## I-03: FusionWeights Sum Invariant

### `test_fusion_weights_default_sum_unchanged_by_crt030`
Arrange: `FusionWeights::default()`.
Assert: the sum of all weight fields equals the expected pre-crt-030 value.
Rationale: crt-030 must not have modified `FusionWeights`. This is a regression guard.

---

## R-10 / AC-04: Doc-Comment SR-01 Disclaimer

Code review gate (not a runtime test):
- `graph_ppr.rs` doc-comment on `personalized_pagerank` must contain the text:
  "SR-01 constrains `graph_penalty` and `find_terminal_active` to Supersedes-only traversal;
  it does not restrict new retrieval functions from using other edge types."
- `ppr_blend_weight` field in `InferenceConfig` doc-comment must describe both roles:
  blend coefficient for existing HNSW candidates, AND floor similarity for PPR-only entries.
