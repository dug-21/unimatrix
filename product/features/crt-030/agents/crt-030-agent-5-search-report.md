# crt-030 Agent 5 Report — search.rs Step 6d Implementation

## Agent ID
crt-030-agent-5-search

## Task
Implement Step 6d (PPR expansion block) in `crates/unimatrix-server/src/services/search.rs`.
Wire five PPR config fields into `SearchService`. Insert the block between Step 6b and Step 6c.

## Files Modified

- `crates/unimatrix-server/src/services/search.rs`
- `crates/unimatrix-server/src/services/mod.rs`

## Implementation Summary

### Import
Added `personalized_pagerank` to the `use unimatrix_engine::graph::{...}` import block.

### SearchService struct and new()
Added five PPR fields: `ppr_alpha`, `ppr_iterations`, `ppr_inclusion_threshold`,
`ppr_blend_weight`, `ppr_max_expand` — following the existing `nli_top_k`/`fusion_weights`
field pattern (doc-commented, stored from `SearchService::new()` parameters).

### mod.rs wiring
Passed the five `inference_config.ppr_*` values into `SearchService::new()`.

### Step 6d block
Inserted between Step 6b (supersession injection) and Step 6c (co-access prefetch):
- Phase 1: Build seed scores from `phase_snapshot` (affinity × HNSW sim, cold-start → ×1.0)
- Phase 2: Normalize to sum 1.0; zero-sum guard skips PPR when all scores are zero
- Phase 2b: Call `personalized_pagerank(&typed_graph, &seed_scores, alpha, iterations)`
- Phase 3: Blend existing HNSW candidates: `new_sim = (1 - w) * sim + w * ppr_score`
- Phase 4: Collect PPR-only candidates with score strictly > threshold, sort desc, cap at max_expand
- Phase 5: Fetch sequentially, apply quarantine check (R-08 Critical), push with `initial_sim = w * ppr_score`

Entire block guarded with `if !use_fallback` (zero allocation when fallback active, AC-12).

### phase_snapshot relocation
The `phase_snapshot` extraction block (col-031 pre-loop) was originally positioned after Step 6c.
Step 6d needs it before Step 6c. Moved the entire block to before Step 6d. This is the key
non-obvious integration requirement documented in pattern #3746.

## Tests Added

16 new unit tests in `services::search::tests::step_6d` module:

| Test | Covers |
|------|--------|
| `test_step_6d_skipped_when_use_fallback_true` | AC-12 / R-02 |
| `test_step_6d_entry_at_exact_threshold_not_included` | R-06 / AC-13 |
| `test_step_6d_entry_just_above_threshold_included` | R-06 |
| `test_step_6d_pool_expansion_capped_at_ppr_max_expand` | E-04 |
| `test_step_6d_blend_formula_known_values` | AC-15 |
| `test_step_6d_blend_weight_zero_leaves_hnsw_unchanged` | R-03 |
| `test_step_6d_blend_weight_one_overwrites_hnsw` | R-11 |
| `test_step_6d_ppr_only_entry_blend_weight_zero_initial_sim_is_zero` | R-03 |
| `test_step_6d_ppr_only_entry_initial_sim_formula` | AC-14 |
| `test_step_6d_all_zero_hnsw_scores_skips_ppr` | FM-05 |
| `test_step_6d_none_phase_snapshot_uses_hnsw_score_only` | R-10 |
| `test_step_6d_non_uniform_phase_snapshot_amplifies_seeds` | AC-16 |
| `test_step_6d_ppr_surfaces_support_entry` | AC-17 |
| `test_step_6d_quarantine_check_applies_to_fetched_entries` | R-08 Critical |
| `test_step_6d_expansion_sorted_by_ppr_score_desc` | sort correctness |
| `test_fusion_weights_default_sum_unchanged_by_crt030` | I-03 regression guard |

## Test Results

- `cargo build -p unimatrix-server`: PASS (0 errors, 14 pre-existing warnings — none from my files)
- `cargo test -p unimatrix-server search`: 144 passed, 0 failed (128 pre-existing + 16 new)
- `cargo test --workspace`: all pass (pre-existing flaky test `col018_prompt_at_limit_not_truncated`
  fails intermittently due to model-loading timing when run with full workspace; passes in isolation;
  confirmed pre-existing by stash verification)

## Constraints Verified

- [x] `use_fallback = true` guard at top of block — zero allocation path (AC-12)
- [x] Inclusion threshold comparison is strictly `>` not `>=` (AC-13 / R-06)
- [x] Quarantine check applied to every PPR-fetched entry (R-08 Critical)
- [x] No lock re-acquisition — reads from already-cloned `phase_snapshot` (ADR-006)
- [x] Sequential store fetches, no batch (ADR-008 / C-10)
- [x] No `phase_affinity_score()` call in Step 6d (R-10 code review gate)
- [x] Step ordering: `6b → 6d → 6c → 7` confirmed by line grep
- [x] `personalized_pagerank` called inline synchronously (C-04, ADR-008)
- [x] No FusionWeights modification (I-03)

## Issues Encountered

**phase_snapshot scope issue**: The pseudocode spec stated phase_snapshot was "already extracted
by col-031 pre-loop block before Step 7". In the actual code it was positioned after Step 6c —
AFTER the intended Step 6d insertion point. Required moving the entire phase_snapshot block to
before Step 6d. This is a non-obvious structural dependency not visible from the pseudocode alone.

**PPR score for isolated nodes**: My initial test expected isolated-node PPR scores of 1.0
(pure teleportation). Actual value is `(1 - alpha) * seed = 0.15 * 1.0 = 0.15`. The power
iteration accumulates both teleportation AND neighbor contributions; with no outgoing edges,
only teleportation contributes. Tests corrected to use analytically derived expected values.

**FusionWeights::default()**: The derived `Default` impl initializes all fields to 0.0.
Must use `FusionWeights::from_config(&InferenceConfig::default())` to get actual values.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 17 entries; entries #3736, #3687, #3730
  confirmed PPR direction semantics, phase_affinity_score cold-start contract, and pipeline step
  numbering conventions. Pattern #3637 confirmed `tracing::` qualified logging requirement.
- Stored: entry #3746 "Moving a pre-loop extraction block when inserting a new pipeline step
  in search.rs" via `/uni-store-pattern` — captures the phase_snapshot scope relocation gotcha
  for future agents inserting steps between 6b and 6c.
