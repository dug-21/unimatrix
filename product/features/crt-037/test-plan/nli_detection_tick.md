# crt-037 Test Plan: nli_detection_tick.rs (Phase 4b + Phase 8b)

**Component**: `crates/unimatrix-server/src/services/nli_detection_tick.rs`
**Nature of change**: New Phase 4b HNSW scan; extended Phase 5 cap logic; Phase 7 batch
now typed `Vec<NliCandidatePair>`; new Phase 8b Informs write loop; two new module-private
types (`NliCandidatePair` enum, `InformsCandidate` struct).
**Risks addressed**: R-03, R-04, R-05, R-06, R-07, R-08, R-11, R-12, R-13, R-14, R-15,
R-16, R-17, R-19, R-20.

---

## Critical Mandate: R-20

All tests in this file — AC-13 through AC-23 — must be delivered in the **same wave** as
Phase 4b/8b code. Treating these as post-gate optional additions will produce a gate
REWORKABLE FAIL (entry #3579 pattern). This is a hard delivery requirement.

---

## Test Infrastructure Notes

Phase 4b/8b tests are Rust integration tests that call `run_graph_inference_tick` (or an
extracted helper) with:
- Controlled store (in-memory SQLite with seeded entries)
- Mock NLI scorer that returns deterministic `NliScores` values per text pair
- Assertable `GRAPH_EDGES` state post-tick

Use the existing `nli_detection_tick.rs` test module's infrastructure (fixtures, mock
providers). Extend existing helpers — do not create isolated scaffolding.

---

## Section 1: NliCandidatePair Routing (R-04)

### Tagged Union Pattern Matching

**Test**: `test_phase8_writes_supports_not_informs_for_supports_contradict_variant`
- Arrange: `Vec<NliCandidatePair>` with one `SupportsContradict` element
  (high entailment `0.8`, low neutral `0.1`) and one `Informs` element
  (low entailment `0.1`, high neutral `0.8`)
- Act: run Phase 8 logic only (filter to `SupportsContradict` arms, call write path)
- Assert: exactly one `Supports` edge written; zero `Informs` edges written
  — covers R-04 scenario 1

**Test**: `test_phase8b_writes_informs_not_supports_for_informs_variant`
- Arrange: same vec as above; all guard fields on `InformsCandidate` valid (pass all predicates)
- Act: run Phase 8b logic only (filter to `Informs` arms, call write path)
- Assert: exactly one `Informs` edge written; zero `Supports` edges written
  — covers R-04 scenario 2

**Test**: `test_informs_pair_with_high_entailment_not_written_by_phase8`
- Arrange: `Informs` variant with `entailment = 0.9 > supports_edge_threshold`; all
  `InformsCandidate` guard fields populated; neutral `= 0.7`
- Act: run Phase 8 only
- Assert: zero `Supports` edges written — `Informs` variant is compile-time excluded from
  Phase 8 match arm
  — covers R-04 scenario 3

---

## Section 2: Composite Guard Predicates (R-03)

Each guard predicate must have its own independent negative test. The AC-13 happy path
does not substitute for these. R-20 gate check requires all five.

### AC-13: Happy Path — All Guards Pass

**Test**: `test_phase8b_writes_informs_edge_when_all_guards_pass`
- Arrange: `InformsCandidate` with:
  - `source_category = "lesson-learned"`, `target_category = "decision"` (in default pairs)
  - `cosine = 0.50` (above default floor `0.45`)
  - `source_created_at = 1_000_000`, `target_created_at = 2_000_000` (source < target)
  - `source_feature_cycle = "crt-020"`, `target_feature_cycle = "crt-030"` (different)
  - `nli_scores.neutral = 0.6 > 0.5` (strictly greater)
  - `nli_scores.entailment = 0.2 < supports_edge_threshold` (FR-11 exclusion passes)
- Act: Phase 8b composite guard check + write
- Assert: exactly one `Informs` row in `GRAPH_EDGES`
  — covers AC-13, R-20 gate check 1

### Guard 1: Temporal Ordering — Equal Timestamps (AC-14, R-03 scenario 1)

**Test**: `test_phase8b_no_informs_when_timestamps_equal`
- Arrange: same as AC-13 but `source_created_at = target_created_at = 1_500_000`
- Act: Phase 8b
- Assert: zero `Informs` rows — strict `<` guard, equal timestamps excluded
  — covers AC-14 first case, R-20 gate check 2 (partial)

### Guard 2: Temporal Ordering — Source Newer Than Target (AC-14, R-03 scenario 2)

**Test**: `test_phase8b_no_informs_when_source_newer_than_target`
- Arrange: `source_created_at = 3_000_000`, `target_created_at = 1_000_000` (reversed)
- Act: Phase 8b
- Assert: zero `Informs` rows — temporal direction wrong
  — covers AC-14 second case, R-20 gate check 2

### Guard 3: Same Feature Cycle (AC-15, R-03 scenario 3)

**Test**: `test_phase8b_no_informs_when_same_feature_cycle`
- Arrange: `source_feature_cycle = "crt-037"`, `target_feature_cycle = "crt-037"`
- All other fields: valid
- Act: Phase 8b
- Assert: zero `Informs` rows
  — covers AC-15, R-20 gate check 2

### Guard 4: Category Pair Not in Config (AC-16, R-03 scenario 4)

**Test**: `test_phase8b_no_informs_when_category_pair_not_in_config`
- Arrange: `source_category = "decision"`, `target_category = "decision"`
  (`("decision", "decision")` not in default `informs_category_pairs`)
- All other fields: valid
- Act: Phase 8b
- Assert: zero `Informs` rows
  — covers AC-16, R-20 gate check 2

### Guard 5: Cosine Below Floor (AC-17, R-03 scenario 5)

**Test**: `test_phase8b_no_informs_when_cosine_below_floor`
- Arrange: `cosine = 0.44` against default floor `0.45`
- Act: Phase 4b candidate filtering (or Phase 8b if cosine is re-checked there)
- Assert: no `Informs` pair produced / no `Informs` row written
  — covers AC-17, R-20 gate check 2

### Guard 6: Neutral Exactly 0.5 (R-03 scenario 6, R-07 boundary)

**Test**: `test_phase8b_no_informs_when_neutral_exactly_0_5`
- Arrange: `nli_scores.neutral = 0.5`; all other fields valid
- Act: Phase 8b composite guard
- Assert: zero `Informs` rows — strict `>` required, `0.5` excluded
  — covers R-07 boundary

**Test**: `test_phase8b_writes_informs_when_neutral_just_above_0_5`
- Arrange: `nli_scores.neutral = 0.5000001`; all other fields valid
- Act: Phase 8b
- Assert: one `Informs` row — just above boundary writes

### Dual Failure Test (R-03 scenario 8)

**Test**: `test_phase8b_one_pass_one_fail_writes_exactly_one_row`
- Arrange: two `Informs` candidates in batch:
  - Candidate 1: all guards pass
  - Candidate 2: `source_created_at = target_created_at` (temporal guard fails)
- Act: Phase 8b for both candidates
- Assert: exactly one `Informs` row written

---

## Section 3: FR-11 Mutual Exclusion (R-07, R-19)

**Test**: `test_phase8b_no_informs_when_entailment_exceeds_supports_threshold`
- Arrange: `NliScores { entailment: 0.75, neutral: 0.6, contradiction: 0.1 }`;
  `supports_edge_threshold = 0.6`; all other InformsCandidate fields valid
- Act: Phase 8b composite guard
- Assert: zero `Informs` rows — FR-11 entailment exclusion guard fires
  — covers R-07 scenario 2, R-19 scenario 1

**Test**: `test_fr11_entailment_exclusion_pair_may_get_supports_from_phase8`
- Arrange: same pair but run through Phase 8 (SupportsContradict path) with
  `entailment = 0.75`
- Act: Phase 8
- Assert: one `Supports` row written — same pair that Phase 8b rejected becomes Supports
  — covers R-19 "only Supports edge written" requirement

---

## Section 4: Metadata Survival (R-05)

**Test**: `test_phase8b_edge_weight_equals_cosine_times_ppr_weight`
- Arrange: full tick with one qualifying pair; `cosine = 0.55`,
  `nli_informs_ppr_weight = 0.6` (from config)
- Act: `run_graph_inference_tick`
- Assert: `GRAPH_EDGES.weight` for the `Informs` row equals `0.55 * 0.6 = 0.33`
  (within f32 epsilon `1e-6`) — covers AC-20, R-05 scenario 3

**Test**: `test_phase8b_metadata_from_phase4b_survives_to_write`
- Arrange: full tick; qualifying pair from different feature cycles
- Act: tick
- Assert: `Informs` row written with `source = "nli"` (EDGE_SOURCE_NLI constant)
  — covers AC-19

**Test**: `test_phase8b_feature_cycle_propagation_no_null`
- Arrange: full tick with pair from `source.feature_cycle = "crt-020"`,
  `target.feature_cycle = "crt-030"`
- Act: tick
- Assert: `Informs` row present — guard passed means feature cycle fields were populated
  (if they were None, the cross-feature guard would have suppressed the edge)

---

## Section 5: Cap Priority Sequencing (R-06, ADR-002)

**Test**: `test_phase5_supports_fills_cap_zero_informs_accepted`
- Arrange: `max_graph_inference_per_tick = N`; `N` qualifying SupportsContradict pairs and
  5 qualifying Informs pairs
- Act: Phase 5 sequential reservation
- Assert: `merged_pairs.len() == N`; all `N` elements are `SupportsContradict` variant
  — covers R-06 scenario 1

**Test**: `test_phase5_partial_cap_informs_fills_remainder`
- Arrange: `max_cap = 10`; `7` SupportsContradict pairs; `10` Informs pairs
- Act: Phase 5
- Assert: `merged_pairs.len() == 10`; first 7 are `SupportsContradict`; last 3 are `Informs`
  — covers R-06 scenario 2

**Test**: `test_phase5_no_supports_all_informs_up_to_cap`
- Arrange: `max_cap = 5`; 0 SupportsContradict; 8 Informs pairs
- Act: Phase 5
- Assert: `merged_pairs.len() == 5`; all are `Informs` variant
  — covers R-06 scenario 3

**Test**: `test_phase5_merged_len_never_exceeds_max_cap_property`
- Arrange: multiple cases — `(supports=0, informs=20, cap=10)`,
  `(supports=10, informs=10, cap=10)`, `(supports=5, informs=15, cap=8)`,
  `(supports=0, informs=0, cap=5)`
- Assert: `merged.len() <= max_cap` in every case
  — covers R-11 invariant

**Test**: `test_phase5_cap_zero_produces_empty_merged`
- Arrange: `max_graph_inference_per_tick = 0`; 5 pairs of each type
- Act: Phase 5
- Assert: `merged_pairs.len() == 0`; no panic, no divide-by-zero
  — covers R-11 edge case

---

## Section 6: Cap Accounting Math (R-11)

**Test**: `test_phase5_remaining_computed_after_truncation`
- Arrange: `max_cap = 5`; `supports_pairs` initially has 8 elements
- Act: Phase 5 — truncate supports to cap, compute remaining, truncate informs
- Assert: `remaining = 0` (not `5 - 8 = -3`, which would cause underflow)
  — verifies `remaining = max_cap - supports_pairs.len()` is computed after truncation,
    not before; `remaining` is non-negative (saturating or bounded)

---

## Section 7: Log Assertions (R-12, SR-03)

**Test**: `test_phase5_log_informs_dropped_when_cap_exceeded`
- Arrange: capture logs; `max_cap = N`; `N` Supports pairs; 5 Informs pairs
- Act: Phase 5 + tick
- Assert: debug log line contains `informs_candidates_dropped = 5` (or equivalent)
  — covers R-12 scenario 1, SR-03

**Test**: `test_phase5_log_informs_accepted_when_cap_not_exceeded`
- Arrange: capture logs; 0 Supports pairs; 5 Informs pairs; `max_cap = 10`
- Act: Phase 5 + tick
- Assert: debug log contains `informs_candidates_accepted = 5`,
  `informs_candidates_dropped = 0`
  — covers R-12 scenario 2

**Test**: `test_phase5_log_zero_total_when_no_candidates`
- Arrange: no Informs-eligible entries in graph
- Act: tick
- Assert: debug log emits the cap-accounting line with `informs_candidates_total = 0`
  — covers R-12 scenario 3 (log line present even when total is zero)

---

## Section 8: Category Filter and Domain Agnosticism (R-08)

**Test**: `test_phase4b_excludes_source_with_non_matching_category`
- Arrange: `informs_category_pairs = [["lesson-learned", "decision"]]`; source entry with
  `category = "convention"`; valid target with `category = "decision"`
- Act: Phase 4b candidate selection
- Assert: zero `NliCandidatePair::Informs` elements produced for this source
  — covers R-08 scenario 2

**Test**: `test_phase4b_empty_category_pairs_produces_zero_candidates`
- Arrange: `informs_category_pairs = []`; otherwise valid entries
- Act: Phase 4b
- Assert: zero Informs pairs, no panic
  — covers R-08 scenario 3 (and edge case from RISK-TEST-STRATEGY.md §Edge Cases)

CI grep gate (non-negotiable, AC-22):
```bash
grep -n '"lesson-learned"\|"decision"\|"pattern"\|"convention"' \
  crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty
```

---

## Section 9: Rayon Closure Async Contamination (R-14, AC-21)

CI grep gates (non-negotiable):
```bash
grep -n 'Handle::current' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty

grep -n '\.await' crates/unimatrix-server/src/services/nli_detection_tick.rs
# Filtered to rayon closure body. Expected: empty inside spawn closure.
```

**Test**: `test_tick_completes_without_panic_requiring_nli_scoring`
- Arrange: full tick with qualifying pairs that enter Phase 7 scoring
- Act: `run_graph_inference_tick` to completion
- Assert: no panic, no timeout — indirectly validates sync-only rayon closure
  — covers AC-21 runtime aspect

---

## Section 10: Dedup / Duplicate Prevention (R-17, AC-23)

**Test**: `test_second_tick_does_not_write_duplicate_informs_edge`
- Arrange: full tick with one qualifying pair; tick runs, `Informs` row written
- Act: run tick again with same qualifying pair
- Assert: `SELECT COUNT(*) FROM graph_edges WHERE relation_type = 'Informs' AND
  source_id = ? AND target_id = ?` equals `1` (not `2`)
  — covers AC-23, R-17 scenario 1

**Test**: `test_second_tick_query_existing_informs_pairs_loads_prior_edge`
- Arrange: same as above — after first tick, one `Informs` row in `GRAPH_EDGES`
- Act: second tick Phase 2 calls `query_existing_informs_pairs`
- Assert: returned set contains the previously-written `(source_id, target_id)` pair —
  verifies pre-filter is loaded and the second tick's Phase 4b dedup check fires
  — covers R-17 scenario 2

---

## Section 11: HNSW Cosine Floor (AC-18)

**Test**: `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold`
- Arrange: configure `nli_informs_cosine_floor = 0.45`, `supports_candidate_threshold = 0.50`
- Inject pair with `cosine = 0.47` (in band `[0.45, 0.50)`)
- Act: Phase 4b scan
- Assert: pair enters Phase 4b candidate set (cosine >= 0.45)

**Test**: `test_phase4b_pair_in_cosine_band_processed_by_phase4b_not_phase4`
- Arrange: same setup; verify the `0.47`-cosine pair does NOT appear in the Phase 4
  (SupportsContradict) candidate list — it is below `supports_candidate_threshold`
  — covers AC-18

---

## Section 12: Edge Weight Finite Guard (R-15, C-13)

**Test**: `test_informs_edge_weight_is_finite_before_write`
- Arrange: valid qualifying pair with `cosine = 0.55`, `nli_informs_ppr_weight = 0.6`
- Act: Phase 8b weight computation: `cosine * nli_informs_ppr_weight`
- Assert: `weight.is_finite()` is true — `0.55 * 0.6 = 0.33` is finite
  — covers R-15, C-13

---

## Section 13: Zero-Regression on Supports/Contradicts (R-16)

**Test**: `test_existing_supports_detection_unchanged_after_batch_type_refactor`
- Arrange: existing test fixture that asserts a `Supports` edge is written for a known
  high-entailment pair
- Act: run tick with refactored `Vec<NliCandidatePair>` batch
- Assert: `Supports` edge count equals pre-refactor baseline
  — covers R-16 scenario 2/4

All existing tests in the `nli_detection_tick.rs` test module must pass without modification.
If any existing test requires a structural change, that is a regression and must not proceed
to gate without explicit documentation.

---

## Section 14: Null Feature Cycle Guard (Edge Case)

**Test**: `test_phase8b_no_informs_when_source_feature_cycle_null`
- Arrange: source entry with `feature_cycle = null`; target with valid `feature_cycle`
- Act: Phase 4b or Phase 8b guard
- Assert: no `Informs` edge written — null feature cycle treated as "same" or excluded per FR-09
  — covers edge case from RISK-TEST-STRATEGY.md §Edge Cases

---

## Acceptance Criteria Covered

| AC-ID | Test Name(s) |
|-------|-------------|
| AC-13 | `test_phase8b_writes_informs_edge_when_all_guards_pass` |
| AC-14 | `test_phase8b_no_informs_when_timestamps_equal`, `test_phase8b_no_informs_when_source_newer_than_target` |
| AC-15 | `test_phase8b_no_informs_when_same_feature_cycle` |
| AC-16 | `test_phase8b_no_informs_when_category_pair_not_in_config` |
| AC-17 | `test_phase8b_no_informs_when_cosine_below_floor` |
| AC-18 | `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold`, `test_phase4b_pair_in_cosine_band_processed_by_phase4b_not_phase4` |
| AC-19 | `test_phase8b_metadata_from_phase4b_survives_to_write` |
| AC-20 | `test_phase8b_edge_weight_equals_cosine_times_ppr_weight` |
| AC-21 | CI grep gate (`Handle::current`) + `test_tick_completes_without_panic_requiring_nli_scoring` |
| AC-22 | CI grep gate (domain strings) |
| AC-23 | `test_second_tick_does_not_write_duplicate_informs_edge`, `test_second_tick_query_existing_informs_pairs_loads_prior_edge` |
