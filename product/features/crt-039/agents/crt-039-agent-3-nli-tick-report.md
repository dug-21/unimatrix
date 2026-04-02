# Agent Report: crt-039-agent-3-nli-tick

Component: `crates/unimatrix-server/src/services/nli_detection_tick.rs`
Feature: crt-039 — Tick Decomposition: Decouple Structural Graph Inference from NLI Gate

---

## Work Completed

Implemented Option Z structural split per IMPLEMENTATION-BRIEF.md and pseudocode/nli_detection_tick.md.

### Production Changes

**1. Module-level doc comment updated (FR-12)**
Describes dual-path nature: Path A (unconditional structural Informs), Path B (NLI Supports, gated). Notes deferred module rename to Group 3.

**2. Phase 1 early-return removed**
`get_provider()` call at function entry deleted. Provider is now bound only inside Path B entry gate.

**3. `NliCandidatePair::Informs` variant removed**
Enum now contains only `SupportsContradict`. All match sites updated. No wildcard arms were masking the removed variant.

**4. `PairOrigin::Informs` variant removed**
Enum now contains only `SupportsContradict`. Phase 6 Informs text-fetch block deleted entirely.

**5. Phase 4b: `informs_candidates_found` counter added**
Declared before the HNSW loop. Incremented after `phase4b_candidate_passes_guards` passes, before the `existing_informs_pairs` and `seen_informs_pairs` dedup checks.

**6. Phase 4b: Supports-set subtraction added (R-03, FR-06, AC-13)**
After the Phase 4b loop, `supports_candidate_set` is built from `candidate_pairs` (both directions). `informs_metadata.retain(...)` subtracts it.

**7. Phase 5: `informs_candidates_after_dedup` and `informs_candidates_after_cap` counters added**
Old merged early-return (`if candidate_pairs.is_empty() && informs_metadata.is_empty()`) moved to after caps. Old SR-03 log replaced by Path A observability log.

**8. Path A write loop (Phase 8b) added — unconditional**
Iterates `informs_metadata`, calls `apply_informs_composite_guard(candidate)` (1-arg), computes weight, calls `format_informs_metadata`, writes via `write_nli_edge`. `informs_edges_written` tracked.

**9. AC-17 observability log emitted after Path A**
`tracing::debug!(informs_candidates_found, informs_candidates_after_dedup, informs_candidates_after_cap, informs_edges_written, "graph inference tick Phase 4b: Informs candidate pipeline")`

**10. Path B entry gate added (R-01)**
`if candidate_pairs.is_empty() { return; }` followed by `get_provider()` match. Err path returns with debug log. This is the sole entry to Phase 6/7/8.

**11. Phase 6 Informs text-fetch block removed**
Phase 6 now fetches Supports pairs only. `pair_origins` contains only `SupportsContradict` entries.

**12. Phase 7 `merged_pairs` map updated**
`PairOrigin::Informs(...)` match arm removed. Single `SupportsContradict` arm, exhaustive.

**13. Phase 8b (old NliCandidatePair::Informs loop) removed**
Replaced by Path A write loop above.

**14. `apply_informs_composite_guard` simplified (ADR-002)**
Signature: `fn apply_informs_composite_guard(candidate: &InformsCandidate) -> bool`
Retains guard 2 (temporal) and guard 3 (cross-feature). Guards 1, 4, 5 removed.

**15. `format_nli_metadata_informs` replaced by `format_informs_metadata`**
New signature: `fn format_informs_metadata(cosine: f32, source_category: &str, target_category: &str) -> String`
Emits `{"cosine": ..., "source_category": ..., "target_category": ...}`. No NLI score fields.

### Test Changes

**Removed (TR):**
- `test_run_graph_inference_tick_nli_not_ready_no_op` (TR-01)
- `test_phase8b_no_informs_when_neutral_exactly_0_5` (TR-02)
- `test_phase8b_writes_informs_when_neutral_just_above_0_5` (TR-03)
- `test_phase8b_no_informs_when_entailment_exceeds_supports_threshold` (TR-05 equivalent — FR-11 guard removed)
- `informs_passing_scores()` helper (no longer used after NliScores removed from guard)
- `test_format_nli_metadata_informs_includes_neutral` (replaced by structural fields test)

**Added (TC):**
- `test_phase4b_writes_informs_when_nli_not_ready` (TC-01, integration)
- `test_phase8_no_supports_when_nli_not_ready` (TC-02, integration)
- `test_apply_informs_composite_guard_temporal_guard` (TC-03, unit)
- `test_apply_informs_composite_guard_cross_feature_guard` (TC-04, unit)
- `test_phase4b_cosine_floor_boundary` (TC-05+06 combined, unit)
- `test_phase4b_explicit_supports_set_subtraction` (TC-07, unit)
- `test_format_informs_metadata_contains_structural_fields` (R-08 replacement)

**Updated:**
- `test_phase8b_writes_informs_edge_when_all_guards_pass` — removed NliScores arg, updated metadata assertion to check `cosine` not `nli_neutral`
- `test_phase8b_no_informs_when_timestamps_equal` — 1-arg call
- `test_phase8b_no_informs_when_source_newer_than_target` — 1-arg call
- `test_phase8b_no_informs_when_same_feature_cycle` — 1-arg call
- `test_apply_informs_composite_guard_both_empty_passes` — 1-arg call
- `test_phase8b_edge_weight_equals_cosine_times_ppr_weight` — uses `format_informs_metadata`
- `test_second_tick_does_not_write_duplicate_informs_edge` — uses `format_informs_metadata`
- `test_second_tick_query_existing_informs_pairs_loads_prior_edge` — uses `format_informs_metadata`
- `test_phase8b_no_informs_when_cosine_below_floor` — uses 0.499 (floor now 0.50)
- `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold` — band updated to `[0.50, supports_threshold)`, uses cosine 0.50

---

## Test Results

`cargo test --workspace`: **2572 passed, 0 failed** across all crates.

`cargo build --workspace`: clean (zero errors; 17 pre-existing warnings in unimatrix-server, none in nli_detection_tick.rs).

---

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] `cargo test --workspace` passes (no new failures)
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in non-test code
- [x] All modified files are within scope defined in the brief
- [x] Error handling uses project error type with context, no `.unwrap()` in non-test code
- [x] New structs: `InformsCandidate` unchanged; no new structs added
- [x] Code follows validated pseudocode — no silent deviations
- [x] Test cases match component test plan expectations
- [x] No source file exceeds 500 lines in non-test production code (net-negative change per OVERVIEW.md)
- [x] AC-17 grep: `grep -n 'informs_candidates_found' nli_detection_tick.rs` returns matches at lines 290, 367, 502
- [x] R-04 grep: `grep -rn 'NliCandidatePair::Informs\|PairOrigin::Informs'` returns empty in production code
- [x] R-06 grep: all `apply_informs_composite_guard` call sites use single argument
- [x] R-08 grep: `format_nli_metadata_informs` absent from production code
- [x] C-07: no domain string literals in new production code (TC-01/TC-02 use runtime config values)

---

## Issues / Blockers

None. The `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold` test had already been updated by another agent in a prior pass (it existed with the 0.50 floor already). No conflicts found.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned ADRs #4017, #4018, #4019 (all three crt-039 decisions), plus relevant patterns #3937, #3949, #3653. Confirmed no prior pattern covered the counter-placement gotcha.
- Stored: entry #4020 "Observability counters in tick pipeline loops must be incremented before dedup-check continue statements" via `/uni-store-pattern` — captures the non-obvious requirement that `informs_candidates_found` must be incremented before `existing_informs_pairs` and `seen_informs_pairs` checks, not after, so all four AC-17 values are independently observable.
