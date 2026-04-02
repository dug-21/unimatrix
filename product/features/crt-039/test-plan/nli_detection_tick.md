# Test Plan: nli_detection_tick.rs — Tick Implementation

Component: `crates/unimatrix-server/src/services/nli_detection_tick.rs`
Pseudocode: `product/features/crt-039/pseudocode/nli_detection_tick.md`

---

## What Changes

1. `run_graph_inference_tick` restructured as two-path function (Option Z):
   - Path A: Phase 4b → Phase 5 cap → Informs write loop (unconditional)
   - Path B: Phase 6/7/8 Supports path (conditional on `get_provider()` success)
2. `apply_informs_composite_guard` simplified from 5 guards to 2 (temporal + cross-feature);
   `nli_scores: &NliScores` and `config: &InferenceConfig` parameters removed.
3. `NliCandidatePair::Informs` and `PairOrigin::Informs` enum variants removed.
4. `format_nli_metadata_informs` replaced by `format_informs_metadata(cosine, src_cat, tgt_cat)`.
5. Phase 4b explicit Supports-set subtraction added (AC-13).
6. `tracing::debug!` observability log added at Phase 4b completion (AC-17).
7. Module-level doc comment updated to describe dual-path nature (FR-12).

---

## Existing Test Infrastructure (Fixtures and Helpers)

The test module at line 899 of `nli_detection_tick.rs` provides these helpers that new tests
MUST reuse (extend, do not duplicate):

| Helper | Purpose | Notes |
|--------|---------|-------|
| `insert_test_entry(store, id)` | Insert minimal entry row into real DB | `created_at = id` for deterministic ordering |
| `make_entry(id, category, created_at)` | Build `EntryRecord` (no DB) | Used for source candidate construction |
| `make_rayon_pool()` | Create 2-thread test pool | Reuse in all integration tests |
| `informs_config()` | `InferenceConfig::default()` | Use for all Phase 4b tests |
| `make_informs_candidate(src_cat, tgt_cat, cosine, src_ts, tgt_ts, src_fc, tgt_fc)` | Build `InformsCandidate` | 9-field struct, all required |

After crt-039: `informs_passing_scores()` helper becomes unused (no remaining `apply_informs_composite_guard`
calls pass `NliScores`). It must be removed or the compiler will warn. All tests that previously
called `apply_informs_composite_guard(&scores, &candidate, &config)` must be updated to
`apply_informs_composite_guard(&candidate)`.

---

## TC-01 — `test_phase4b_writes_informs_when_nli_not_ready` (Integration)

**Risk**: R-02 (test coverage gap), R-07 (Phase 8b skipped when no Supports candidates)
**AC**: AC-02, AC-15

**Arrange**: Real Store (tempdir), two entries with embeddings, NliServiceHandle::new()
(Loading state). Config: `nli_informs_cosine_floor = 0.50`, `informs_category_pairs` set
to include the category pair of the two test entries. CRITICAL: the test corpus must be
configured so **no pair exceeds `supports_candidate_threshold`** — this exercises R-07 by
ensuring `candidate_pairs` is empty and the Informs path runs without any Path B work.

**Act**: Call `run_graph_inference_tick(&store, &not_ready_handle, &vector_index,
&make_rayon_pool(), &config)`.

**Assert**:
```rust
let edges = store.query_graph_edges().await.unwrap();
let informs_count = edges.iter().filter(|e| e.relation_type == "Informs").count();
assert!(informs_count >= 1, "TC-01: at least one Informs edge must be written when NLI not ready");
let supports_count = edges.iter().filter(|e| e.relation_type == "Supports").count();
assert_eq!(supports_count, 0, "TC-01: zero Supports edges when NLI not ready");
```

**Setup note**: Two entries need actual embeddings stored in VectorIndex — insert entries
via `insert_test_entry`, then call `vector_index.upsert()` with real (or synthetic deterministic)
vectors. The test must produce HNSW neighbors above the cosine floor — see existing
`test_phase8b_edge_weight_equals_cosine_times_ppr_weight` for the embedding setup pattern.

**TC-01 is separate from TC-02** — do not combine into a single `#[tokio::test]`.

---

## TC-02 — `test_phase8_no_supports_when_nli_not_ready` (Integration)

**Risk**: R-01 (silent Supports corruption), R-02 (coverage gap)
**AC**: AC-02, AC-14, AC-16

**Arrange**: Real Store, entries with embeddings such that at least one pair is above
`supports_candidate_threshold` (cosine > 0.65). NliServiceHandle::new() (Loading state).

**Act**: Call `run_graph_inference_tick`.

**Assert**:
```rust
let edges = store.query_graph_edges().await.unwrap();
let supports: Vec<_> = edges.iter().filter(|e| e.relation_type == "Supports").collect();
assert_eq!(supports.len(), 0, "TC-02: zero Supports edges when NLI not ready — R-01 guard");
// May assert Informs edges present (incidental positive signal) but this is not required
```

**Critical**: TC-02 is a separate test. It must explicitly test the Path B gate, not just
restate TC-01's assertion. The two tests exercise different corpus setups (TC-01: no Supports
candidates; TC-02: Supports candidates present but NLI unavailable).

---

## TC-03 — `test_apply_informs_composite_guard_temporal_guard` (Unit)

**Risk**: R-12 (guard logic regression)
**AC**: FR-05, AC-03

**Arrange/Assert** (both cases in one test or two separate `#[test]` functions — choose
one `#[test]` with two assertions to keep temporal guard tests grouped):

```rust
// Source newer than target — must fail
let candidate_newer = make_informs_candidate("lesson-learned", "decision", 0.55,
    2_000_000, 1_000_000, "crt-020", "crt-030"); // source_ts > target_ts
assert!(
    !apply_informs_composite_guard(&candidate_newer),
    "TC-03a: guard must return false when source_created_at >= target_created_at"
);

// Source older than target — must pass
let candidate_older = make_informs_candidate("lesson-learned", "decision", 0.55,
    1_000_000, 2_000_000, "crt-020", "crt-030"); // source_ts < target_ts
assert!(
    apply_informs_composite_guard(&candidate_older),
    "TC-03b: guard must return true when source_created_at < target_created_at (temporal only)"
);
```

**Note**: After crt-039, `apply_informs_composite_guard` signature is `(candidate: &InformsCandidate) -> bool`.
The test calls it with ONE argument — not `(&scores, &candidate, &config)`.

---

## TC-04 — `test_apply_informs_composite_guard_cross_feature_guard` (Unit)

**Risk**: R-12 (guard logic regression)
**AC**: FR-05, AC-03

Three cases required:

```rust
// Both non-empty and equal — must fail
let same_cycle = make_informs_candidate("ll", "dec", 0.55, 1_000_000, 2_000_000, "crt-020", "crt-020");
assert!(!apply_informs_composite_guard(&same_cycle),
    "TC-04a: both cycles non-empty and equal → false");

// Source empty — must pass
let src_empty = make_informs_candidate("ll", "dec", 0.55, 1_000_000, 2_000_000, "", "crt-020");
assert!(apply_informs_composite_guard(&src_empty),
    "TC-04b: source cycle empty → true");

// Target empty — must pass
let tgt_empty = make_informs_candidate("ll", "dec", 0.55, 1_000_000, 2_000_000, "crt-020", "");
assert!(apply_informs_composite_guard(&tgt_empty),
    "TC-04c: target cycle empty → true");

// Both non-empty and different — must pass
let diff_cycle = make_informs_candidate("ll", "dec", 0.55, 1_000_000, 2_000_000, "crt-020", "crt-030");
assert!(apply_informs_composite_guard(&diff_cycle),
    "TC-04d: both cycles non-empty and different → true");
```

---

## TC-05 — `test_phase4b_cosine_floor_boundary` (Unit)

**Risk**: R-12 (boundary semantics `>=` vs `>`)
**AC**: AC-05, FR-08

Two assertions within one `#[test]` function (or split into two):

```rust
let config = InferenceConfig { nli_informs_cosine_floor: 0.5, ..InferenceConfig::default() };
let (src_cat, tgt_cat) = (config.informs_category_pairs[0][0].as_str(), config.informs_category_pairs[0][1].as_str());

// Exactly 0.500 — must be included (inclusive >=)
assert!(
    phase4b_candidate_passes_guards(0.500_f32, src_cat, tgt_cat, 1_000, 2_000, "crt-020", "crt-030", &config),
    "TC-05a: cosine exactly 0.500 must pass Phase 4b cosine guard (inclusive >=)"
);

// Exactly 0.499 — must be excluded
assert!(
    !phase4b_candidate_passes_guards(0.499_f32, src_cat, tgt_cat, 1_000, 2_000, "crt-020", "crt-030", &config),
    "TC-05b: cosine exactly 0.499 must be excluded by Phase 4b (below floor)"
);
```

Note: the spec names these TC-05 (`test_phase4b_cosine_floor_0500_included`) and TC-06
(`test_phase4b_cosine_floor_0499_excluded`) as separate tests. Either two tests or one test
with two assertions is acceptable — the assertions are what matter.

---

## TC-06 — `test_cosine_floor_default` (Unit, see also config.md)

**AC**: AC-04, FR-07

This test belongs in `config.rs` tests (see config.md). In `nli_detection_tick.rs`,
the equivalent is updating `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold`
to use `cosine_in_band = 0.50` (the new floor boundary):

```rust
// After crt-039: the band that tests "uses floor, not threshold" is [0.50, supports_threshold)
// A cosine of exactly 0.50 passes Phase 4b (>= 0.50) and would NOT pass Phase 4 (> 0.65)
let cosine_in_band = 0.50_f32;
let phase4b_accepts = phase4b_candidate_passes_guards(cosine_in_band, ...);
assert!(phase4b_accepts, "cosine 0.50 >= nli_informs_cosine_floor 0.50 must be accepted by Phase 4b");

let config_supports_threshold = config.supports_candidate_threshold;
assert!(cosine_in_band <= config_supports_threshold, "sanity: cosine is not above supports threshold");
```

---

## TC-07 — `test_phase4b_explicit_supports_set_subtraction` (Unit)

**Risk**: R-03 (mutual-exclusion gap at boundary)
**AC**: AC-13, FR-06

This test requires access to Phase 4b internal logic — either by calling a testable helper
function that performs the subtraction, or by setting up a full in-function scenario.
The preferred approach is to test the data structure invariant directly.

The core assertion: given a `candidate_pairs: HashSet<(u64,u64)>` containing pair `(1, 2)`,
after Phase 4b runs with this pre-populated set, pair `(1, 2)` must NOT appear in
`informs_metadata`.

```rust
// Setup: candidate_pairs contains a pair at cosine 0.68 (above supports threshold 0.65)
// This pair was selected by Phase 4 and should be subtracted from Phase 4b output

// If the subtraction function is testable in isolation:
let candidate_pairs: HashSet<(u64, u64)> = [(1_u64, 2_u64)].into_iter().collect();
let mut informs_metadata = vec![
    make_informs_candidate("lesson-learned", "decision", 0.68, 1_000_000, 2_000_000, "crt-020", "crt-030"),
    make_informs_candidate("lesson-learned", "decision", 0.55, 3_000_000, 4_000_000, "crt-020", "crt-030"),
];
// Apply the subtraction (Phase 4b logic)
informs_metadata.retain(|c| !candidate_pairs.contains(&(c.source_id, c.target_id)));
// Assert pair (1,2) at cosine 0.68 is absent
assert!(
    !informs_metadata.iter().any(|c| c.source_id == 1 && c.target_id == 2),
    "TC-07: pair at cosine 0.68 present in candidate_pairs must be absent from informs_metadata"
);
```

Boundary variant (R-03 specific): a pair at exactly cosine 0.50 that satisfies
`informs_category_pairs` but was NOT in `candidate_pairs` (since Phase 4 uses strict `>`)
SHOULD appear in `informs_metadata`. This validates the asymmetric boundary:

```rust
// cosine exactly 0.50: excluded from Phase 4 (> 0.50 strict fails), included in Phase 4b (>= 0.50)
// and NOT subtracted (not in candidate_pairs)
// Therefore it SHOULD be in informs_metadata
let boundary_pair = make_informs_candidate("lesson-learned", "decision", 0.50, 1_000, 2_000, "c1", "c2");
// boundary_pair.source_id/target_id not in candidate_pairs
assert!(
    !candidate_pairs.contains(&(boundary_pair.source_id, boundary_pair.target_id)),
    "pair at 0.50 should not be in candidate_pairs (Phase 4 uses strict >, not >=)"
);
```

---

## AC-17 Observability Log — Grep Verification

**AC-17**: A `tracing::debug!` call at Phase 4b completion records all four fields.
Verification at gate-3c by grep — this is a required check, not optional:

```bash
grep -n 'informs_candidates_found' crates/unimatrix-server/src/services/nli_detection_tick.rs
```
Must return at least one match in the Phase 4b/5 region (roughly the area between the
Phase 4b HNSW scan loop and the Phase 5 `truncate` call).

The four fields must ALL be present in a single `tracing::debug!` macro invocation:
`informs_candidates_found`, `informs_candidates_after_dedup`, `informs_candidates_after_cap`,
`informs_edges_written`.

Code ordering check: `informs_candidates_found` must be assigned BEFORE the
`existing_informs_pairs` dedup filter runs, so it captures the raw pre-dedup count.

---

## R-04 — Dead Enum Variant Removal (Compile-Time)

**Risk**: R-04
**AC**: ADR-001 consequences

No runtime test. Verification is:

1. `cargo build --workspace` with `#![deny(dead_code)]` active passes. If
   `NliCandidatePair::Informs` or `PairOrigin::Informs` is retained, this fails.

2. Pre-merge grep:
```bash
grep -rn 'NliCandidatePair::Informs\|PairOrigin::Informs' \
  crates/unimatrix-server/src/
```
Must return empty in production code (test code may temporarily reference them during
the transition but all such tests are deleted as part of TR-01/TR-02/TR-03).

3. Every `match pair_origin { ... }` and `match nli_candidate_pair { ... }` site must
   compile without a `_ =>` wildcard covering a removed variant.

---

## R-06 — Stale Call Sites (Compile-Time)

After removing `nli_scores` from `apply_informs_composite_guard`, all call sites must
pass exactly one argument. Verification:

```bash
grep -n 'apply_informs_composite_guard' crates/unimatrix-server/src/services/nli_detection_tick.rs
```
Every match must show `apply_informs_composite_guard(&candidate)` — not `apply_informs_composite_guard(&scores, ...)`.

---

## R-08 — format_nli_metadata_informs Cleanup

After `format_informs_metadata` replaces `format_nli_metadata_informs`:

```bash
grep -n 'format_nli_metadata_informs' crates/unimatrix-server/src/services/nli_detection_tick.rs
```
Must return empty (function deleted). Informs edge metadata must include `cosine`,
`source_category`, `target_category` and must NOT include `nli_neutral`, `nli_entailment`,
`nli_contradiction`.

Metadata content assertion (part of TC-01 or a standalone unit test):
```rust
let metadata = format_informs_metadata(0.55_f32, "lesson-learned", "decision");
// Must contain cosine field
assert!(metadata.contains("cosine"), "R-08: metadata must have cosine field");
// Must NOT contain NLI score fields
assert!(!metadata.contains("nli_neutral"), "R-08: metadata must not have nli_neutral");
assert!(!metadata.contains("nli_entailment"), "R-08: metadata must not have nli_entailment");
```

---

## Edge Case Tests Required

| Scenario | Test | Assertion |
|----------|------|-----------|
| Empty active entry set | Existing `test_tick_empty_entry_set_select_candidates` — verify it still passes | No panic, no writes |
| Single active entry | Existing `test_tick_single_active_entry` — verify it still passes | HNSW returns no neighbors above floor, no writes |
| All candidates deduped (all in `existing_informs_pairs`) | New: setup with all pairs pre-existing, run tick | `informs_edges_written = 0`, no panic, log still emits |
| Exactly 25 candidates (cap boundary) | `test_phase5_informs_always_gets_dedicated_budget` — update if `make_informs_candidate` signature changes | 25th written, 26th not written |

The cap boundary tests `test_phase5_informs_always_gets_dedicated_budget` and
`test_phase5_informs_small_pool_all_kept` are existing tests that must continue to pass
unchanged (their logic is independent of the guard changes).

---

## Tests to Update (NliScores References)

All tests that call `apply_informs_composite_guard` with the old 3-argument signature:
- `test_phase8b_writes_informs_edge_when_all_guards_pass` (line ~1460)
- `test_phase8b_no_informs_when_timestamps_equal` (line ~1543)
- `test_phase8b_no_informs_when_source_newer_than_target` (line ~1561)
- `test_phase8b_no_informs_when_same_feature_cycle` (line ~1583)
- `test_apply_informs_composite_guard_both_empty_passes` (line ~1660)
- `test_phase8b_no_informs_when_entailment_exceeds_supports_threshold` (line ~1980)

Each must be updated to pass `&candidate` only (remove `&scores, &config`).

Additionally, any test calling `format_nli_metadata_informs` must be updated to call
`format_informs_metadata` with the new signature. The test
`test_format_nli_metadata_informs_includes_neutral` (line ~2143) must be deleted or
replaced with a test asserting the new metadata structure.
