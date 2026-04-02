# SPECIFICATION: crt-039 — Tick Decomposition: Decouple Structural Graph Inference from NLI Gate

**Feature ID**: crt-039  
**Spec Version**: 1.0  
**Status**: Ready for Architect

---

## Objective

The background tick pipeline gates `run_graph_inference_tick` on `inference_config.nli_enabled`,
which is `false` by default and has no deployable model (GGUF failed ASS-036). This causes Phase
4b (structural Informs inference via HNSW cosine) — which uses no NLI model at all — to never
run in production. This feature decouples Phase 4b from NLI availability so structural Informs
edges accumulate from tick 1. It also removes a task-mismatched NLI neutral-zone guard from
`apply_informs_composite_guard`, separates contradiction scan into a clearly-named conditional
tick step, and raises the `nli_informs_cosine_floor` default from 0.45 to 0.5 as a structural
compensating filter.

---

## Functional Requirements

**FR-01**: `run_graph_inference_tick` must be called unconditionally on every background tick.
The `if inference_config.nli_enabled { ... }` guard wrapping the call in `background.rs:760`
must be removed with no replacement condition. Verification: inspect `run_single_tick` call site;
assert the call is present whether `nli_enabled` is `true` or `false`.

**FR-02**: Phase 4b (structural Informs HNSW scan) inside `run_graph_inference_tick` must
execute and be capable of writing Informs edges when the NLI model is not loaded. The Phase 1
`get_provider()` early-return must be restructured so it gates only the Phase 8 (Supports)
code path, not Phase 4b. Verification: integration test with NLI not ready shows Informs edges
written (SR-05 assertion a).

**FR-03**: Phase 8 (NLI Supports edge writing) must not execute when `get_provider()` returns
`Err`. If the NLI provider is not ready, Phase 8 must not write any Supports edges and must
not invoke `score_batch`. Verification: integration test with NLI not ready shows zero Supports
edges written (SR-05 assertion b, SR-04 invariant).

**FR-04**: `apply_informs_composite_guard` must not reference `nli_scores.neutral` in any form.
The function signature must not include `nli_scores: &NliScores` as a parameter. The remaining
guards are temporal (source created before target) and cross-feature (source and target belong
to different feature cycles, unless either cycle field is empty). Verification: inspect function
signature and body; assert no `nli_scores` reference.

**FR-05**: `apply_informs_composite_guard` after the change retains exactly two guards:
- Guard 2 (temporal): `candidate.source_created_at < candidate.target_created_at`
- Guard 3 (cross-feature): `candidate.source_feature_cycle.is_empty() || candidate.target_feature_cycle.is_empty() || candidate.source_feature_cycle != candidate.target_feature_cycle`

No other guards. Verification: unit tests for each guard independently; function body inspection.

**FR-06**: The mutual-exclusion invariant between Supports edges (Phase 8) and Informs edges
(Phase 8b) — ensuring no pair is written as both edge types — must be enforced by candidate set
separation between Phase 4 and Phase 4b. Phase 4 produces Supports candidates using cosine
strictly greater than `supports_candidate_threshold`. Phase 4b produces Informs candidates using
cosine greater than or equal to `nli_informs_cosine_floor`. Because `nli_informs_cosine_floor`
(default 0.5) is less than `supports_candidate_threshold` (default 0.65, validated ≥ 0.65 per
ASS-035), the ranges overlap in `[nli_informs_cosine_floor, supports_candidate_threshold]`. The
exclusion mechanism for overlapping pairs is explicit: Phase 4b must subtract the Phase 4
Supports candidate set before producing its output — pairs that are already in the Phase 4
Supports candidate set must not appear in the Phase 4b Informs candidate set. Verification: unit
test asserting that a pair with cosine 0.68 (above `supports_candidate_threshold` 0.65) is
absent from `informs_metadata` when it appears in `candidate_pairs`.

**FR-07**: The default value of `nli_informs_cosine_floor` in `InferenceConfig::default()` and
`default_nli_informs_cosine_floor()` must be 0.5. Verification: unit test
`test_inference_config_default_nli_informs_cosine_floor` updated to assert 0.5.

**FR-08**: A candidate pair with HNSW cosine similarity of exactly 0.499 must be excluded by
Phase 4b. A candidate pair with cosine similarity of exactly 0.500 must be included (inclusive
floor, `>=` semantics). Verification: unit tests at boundary values.

**FR-09**: `MAX_INFORMS_PER_TICK = 25` is a hard write limit on Phase 5 Informs candidate
truncation. It is not a soft warning. The `informs_metadata.truncate(MAX_INFORMS_PER_TICK)` call
must execute before any Informs edge write in Phase 8b. Dedup pre-filter (`query_existing_informs_pairs`)
must be applied in Phase 2, before Phase 4b candidate selection, not after. Verification: the
ordering in code (Phase 2 dedup → Phase 4b candidate selection → Phase 5 truncation → Phase 8b
write) is the spec. No write path bypasses the cap.

**FR-10**: The contradiction scan block in `background.rs` must be structured as a clearly-named
independent tick step. It must retain its existing condition unchanged:
`current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS) && embed_service.get_adapter().await.is_ok()`.
A code comment on the block must state: "Contradiction scan: gated on embed adapter availability
and interval; runs independently of structural_graph_tick." No behavioral change. Verification:
diff shows only comment and structural annotation changes in the contradiction scan block.

**FR-11**: The tick ordering invariant in `run_single_tick` must be preserved and documented with
an explicit ordering comment:
```
// Tick ordering invariant (non-negotiable):
// compaction → promotion → graph-rebuild → structural_graph_tick (always)
//   → contradiction_scan (if embed adapter ready, every CONTRADICTION_SCAN_INTERVAL_TICKS)
```
Verification: inspect `run_single_tick` body for correct ordering and presence of comment.

**FR-12**: The module-level doc comment in `nli_detection_tick.rs` must be updated to describe
the dual nature of the module: structural Informs path (Phase 4b, pure HNSW cosine, no NLI) and
NLI Supports path (Phase 8, NLI cross-encoder required). The comment must note that the module
rename is deferred to Group 3 when NLI is fully removed from Phase 8. Verification: doc comment
present and accurate at top of file.

**FR-13**: Phase 4b must produce Informs candidates using `informs_metadata: Vec<InformsCandidate>`
populated from HNSW search results. Each candidate must pass `phase4b_candidate_passes_guards`
before entering the set. Category pair membership (from `config.informs_category_pairs`) is
enforced as a source-category pre-filter. Domain string literals must not appear in production
code (C-12 constraint). Verification: no string literals of category names in Phase 4b code;
all category checks use config-derived values.

**FR-14**: A structured log line at Phase 4b completion (before Phase 5 cap) must record:
`informs_candidates_found` (raw count before dedup and cap), `informs_candidates_after_dedup`
(after `existing_informs_pairs` filter), `informs_candidates_after_cap` (after truncation),
`informs_edges_written` (actual writes). This observability requirement addresses SR-06: without
this signal, there is no way to distinguish "floor too high" from "all candidates deduped" from
"cap applying correctly." Verification: log fields present in tracing::debug! call; confirmed
by log-level test or inspection.

---

## Non-Functional Requirements

**NFR-01 (Performance)**: Phase 4b is pure structural: HNSW cosine queries and DB reads. It
must not invoke the rayon pool (`ml_inference_pool.spawn`). When `nli_enabled = false`, no
rayon pool usage occurs during the structural graph tick. Pool floor remains 4 (not 6) when
`nli_enabled = false`. No change to the pool floor logic.

**NFR-02 (Throughput bound)**: Phase 4b Informs write rate is bounded by `MAX_INFORMS_PER_TICK = 25`
edges per tick. At a 15-minute tick interval, the maximum accumulation rate is 100 Informs edges
per hour. This is a hard ceiling, not a throughput target.

**NFR-03 (Correctness)**: When `nli_enabled = false` (production default), `score_batch` must
never be called. The W1-2 contract (all `CrossEncoderProvider::score_batch` calls go through
`rayon_pool.spawn`; `spawn_blocking` and inline async NLI are prohibited) applies only to Phase 8
(Supports). Phase 4b must not route through Phase 7 (NLI batch dispatch).

**NFR-04 (Compatibility)**: Existing `informs_category_pairs` configuration values and all
other `InferenceConfig` fields except `nli_informs_cosine_floor` default are unchanged.
Deployments that have overridden `nli_informs_cosine_floor` in TOML retain their configured
value; only the default changes.

**NFR-05 (Eval gate)**: After all changes, the eval harness
(`product/research/ass-039/harness/scenarios.jsonl`) must show MRR >= 0.2913, the crt-038
baseline. No regression from the behavioral ground truth baseline.

**NFR-06 (File size)**: `nli_detection_tick.rs` is currently ~2,200 lines including tests.
Production code changes in this file must not push the non-test production code section above
the 500-line guidance. If Option B structural split requires new helper functions that breach
this limit, the Informs-only path must be extracted to a submodule (e.g.,
`nli_detection_tick/informs.rs`). The architect must assess and decide.

**NFR-07 (Zero behavioral change to contradiction scan)**: The contradiction scan block in
`background.rs` may receive comment additions and structural labeling only. No behavioral change:
same condition (`current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS) && get_adapter().is_ok()`),
same rayon dispatch pattern, same write target (`contradiction_cache`).

---

## Acceptance Criteria

Each AC maps to one or more FRs and is directly traceable to SCOPE.md.

**AC-01** (from SCOPE AC-01): `run_graph_inference_tick` is called unconditionally on every tick
regardless of `inference_config.nli_enabled`. The `if inference_config.nli_enabled` guard in
`background.rs` is removed. Verification method: inspect `run_single_tick`; assert no
`nli_enabled` condition wraps the call.

**AC-02** (from SCOPE AC-02): When `nli_enabled = false` and the NLI model is not loaded,
Phase 4b (structural Informs HNSW scan) still executes and can write Informs edges. Phase 8
(Supports) does not write any edges. Verification method: integration test — see TC-01 and TC-02.

**AC-03** (from SCOPE AC-03): `apply_informs_composite_guard` does not reference `nli_scores`
in any form. The function signature does not include an `nli_scores` parameter. Remaining guards
are temporal (guard 2) and cross-feature (guard 3) only. Verification method: function signature
and body inspection; pre-merge grep: `grep -n 'nli_scores' apply_informs_composite_guard` must
return empty.

**AC-04** (from SCOPE AC-04): Default `nli_informs_cosine_floor` is 0.5 in both
`InferenceConfig::default()` and `default_nli_informs_cosine_floor()`. Verification method:
`test_inference_config_default_nli_informs_cosine_floor` asserts 0.5.

**AC-05** (from SCOPE AC-05): A candidate pair with cosine similarity 0.499 is excluded by
Phase 4b. A pair at 0.500 is included (inclusive floor, `>=` semantics). Verification method:
unit tests `test_phase4b_cosine_floor_0499_excluded` and `test_phase4b_cosine_floor_0500_included`.

**AC-06** (from SCOPE AC-06): Contradiction scan continues to be gated on embed adapter
availability and `current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS)`. Behavior
is unchanged. Verification method: diff shows no behavioral change to the contradiction scan
block; existing contradiction scan tests pass.

**AC-07** (from SCOPE AC-07): The ordering invariant is preserved:
compaction → promotion → graph-rebuild → structural_graph_tick (always) →
contradiction_scan (if embed adapter ready, every N ticks).
Verification method: inspect `run_single_tick` call sequence; ordering invariant comment present.

**AC-08** (from SCOPE AC-08): All tests that previously asserted `nli_informs_cosine_floor == 0.45`
are updated to assert 0.5. This includes `test_inference_config_default_nli_informs_cosine_floor`
and the nominal-value variant in `test_validate_nli_informs_cosine_floor_valid_value_is_ok`.
Verification method: `grep -n '0.45' config.rs` in the test section returns no assertions
against the default.

**AC-09** (from SCOPE AC-09): Tests `test_phase8b_no_informs_when_neutral_exactly_0_5` and
`test_phase8b_writes_informs_when_neutral_just_above_0_5` are removed. They must not be
repurposed as vacuous assertions. Replacement tests cover the new guard boundary (cosine floor
0.5) — see TC-05 and TC-06. Verification method: grep for removed test names returns empty.

**AC-10** (from SCOPE AC-10): `cargo test --workspace` passes with no regressions.
Verification method: CI green.

**AC-11** (from SCOPE AC-11): Eval harness MRR >= 0.2913 (behavioral ground truth,
1,585 scenarios, `product/research/ass-039/harness/scenarios.jsonl`). No regression from
crt-038 baseline. Verification method: eval run on harness.

**AC-12** (SR-01): `MAX_INFORMS_PER_TICK = 25` is enforced as a hard write limit via
`informs_metadata.truncate(MAX_INFORMS_PER_TICK)` in Phase 5. This truncation executes before
any Phase 8b write. The dedup pre-filter (`query_existing_informs_pairs`) is applied in Phase 2
before Phase 4b candidate selection. Verification method: code ordering inspection;
`informs_written` counter never exceeds 25 in any test run.

**AC-13** (SR-02): Candidate set separation produces mutual exclusion between Phase 4 Supports
candidates and Phase 4b Informs candidates. Phase 4b must explicitly exclude pairs already
present in the Phase 4 Supports candidate set (`candidate_pairs`). This is not disjoint by
construction — it must be an explicit subtraction. Verification method: unit test asserting that
a pair with cosine above `supports_candidate_threshold` (e.g., 0.68) does not appear in
`informs_metadata` — see TC-07.

**AC-14** (SR-04): After the Phase 1 guard split, Phase 8 entry requires a successful
`get_provider()` call. Phase 4b entry requires no provider. If `get_provider()` returns `Err`,
Phase 8 executes no write and the function returns (or skips) before `score_batch` is called.
Verification method: TC-02 — integration test with NLI not ready, assert zero Supports edges.

**AC-15** (SR-05a): A new test `test_phase4b_writes_informs_when_nli_not_ready` asserts that
Phase 4b CAN write Informs edges when the NLI provider is not ready (NliServiceHandle in
Loading state). This is a positive assertion, not merely absence of failure. Verification
method: test present and passing; at least one Informs edge written.

**AC-16** (SR-05b): A new test `test_phase8_no_supports_when_nli_not_ready` asserts that Phase
8 does NOT write Supports edges when the NLI provider is not ready. This is a separate test
from AC-15 — two independent assertions, not one combined test. The old
`test_run_graph_inference_tick_nli_not_ready_no_op` (which conflated both) is removed.
Verification method: both tests present and passing; zero Supports edges in TC-02.

**AC-17** (SR-06): A `tracing::debug!` call at Phase 4b completion records:
`informs_candidates_found`, `informs_candidates_after_dedup`, `informs_candidates_after_cap`,
`informs_edges_written`. Verification method: log fields present in source; confirmed by
tracing subscriber test or inspection.

**AC-18**: `format_nli_metadata_informs` helper must be updated or removed. If Phase 8b no
longer receives `nli_scores`, the helper's output must not include NLI score fields that are
no longer meaningful. If the function is unused after the change, it must be removed. Verification
method: no dead-code warning for `format_nli_metadata_informs`; no clippy warnings.

---

## Domain Models

### Key Terms

**Phase 4b** — The HNSW cosine scan inside `run_graph_inference_tick` that produces Informs
edge candidates. Operates on metadata and vector embeddings only; uses no NLI cross-encoder.
Applies `phase4b_candidate_passes_guards` to filter by cosine floor, category pair membership,
temporal ordering, and cross-feature constraint.

**Phase 8** — The NLI batch write phase inside `run_graph_inference_tick` that writes Supports
edges. Requires a live NLI provider from `get_provider()`. Gated by entailment score >
`supports_edge_threshold`.

**Phase 8b** — The Informs edge write phase inside `run_graph_inference_tick`. After crt-039,
it receives candidates from Phase 4b directly without NLI scores, and applies only
`apply_informs_composite_guard` (temporal + cross-feature guards).

**structural_graph_tick** — The name used in ordering invariant comments and documentation
for `run_graph_inference_tick`. "Structural" emphasizes that this tick runs regardless of
NLI availability.

**contradiction_scan** — A separate conditional tick step that calls `scan_contradictions`
via rayon pool. Gated on embed adapter availability and tick interval. Not part of
structural_graph_tick. Writes to `contradiction_cache`, not `GRAPH_EDGES`.

**NLI gate (category error)** — The pre-crt-039 condition `if inference_config.nli_enabled`
that wrapped `run_graph_inference_tick`. A category error because it treated NLI model
availability as a precondition for a structurally independent phase.

**Informs edge** — A directional `GRAPH_EDGES` record with `relation_type = 'Informs'`. Source
entry temporally precedes target entry; source and target belong to different feature cycles (or
at least one has an unknown cycle). Written by Phase 8b.

**Supports edge** — A directional `GRAPH_EDGES` record with `relation_type = 'Supports'`.
Written by Phase 8 when NLI entailment score exceeds `supports_edge_threshold`. Requires NLI
provider.

**MAX_INFORMS_PER_TICK** — Constant `= 25`. Hard write limit for Phase 5 Informs truncation.
Budget is independent of Supports budget (`max_graph_inference_per_tick`).

**nli_informs_cosine_floor** — `InferenceConfig` field. Inclusive lower bound on cosine
similarity for Phase 4b Informs candidates. Default after crt-039: 0.5. Semantics: `>=`.

**W1-2 contract** — Architectural rule: all `CrossEncoderProvider::score_batch` invocations
must go through `rayon_pool.spawn()`. `spawn_blocking` and inline async NLI are prohibited.
Applies to Phase 7/Phase 8 only.

### `apply_informs_composite_guard` — Before and After

**Before (5 guards)**:
```
fn apply_informs_composite_guard(
    nli_scores: &NliScores,       // REMOVED
    candidate: &InformsCandidate,
    config: &InferenceConfig,
) -> bool {
    nli_scores.neutral > 0.5                                           // guard 1 REMOVED
        && candidate.source_created_at < candidate.target_created_at  // guard 2 retained
        && (cross-feature check)                                       // guard 3 retained
        && nli_scores.entailment <= config.supports_edge_threshold     // guard 4 REMOVED
        && nli_scores.contradiction <= config.nli_contradiction_threshold  // guard 5 REMOVED
}
```

**After (2 guards)**:
```
fn apply_informs_composite_guard(
    candidate: &InformsCandidate,
) -> bool {
    candidate.source_created_at < candidate.target_created_at         // guard 2 (temporal)
        && (candidate.source_feature_cycle.is_empty()
            || candidate.target_feature_cycle.is_empty()
            || candidate.source_feature_cycle != candidate.target_feature_cycle)  // guard 3 (cross-feature)
}
```

Mutual exclusion between Informs and Supports (previously guards 4 and 5) is enforced by
candidate set separation in Phase 4 vs Phase 4b. Guards 4 and 5 are removed because after
Option B decoupling, Phase 4b candidates do not enter the NLI batch and therefore never have
valid NLI scores to test.

Note: `config` parameter may be retained for future extensibility or dropped if unused. The
architect decides based on whether any config field is consulted by the two remaining guards.
Currently guards 2 and 3 do not reference any config field — this is an open question for the
architect (see Open Questions).

### Phase 4b Data Flow (After crt-039)

```
Phase 2: DB reads
  - query_by_status(Active) → all_active
  - query_entries_without_edges() → isolated_ids
  - query_existing_supports_pairs() → existing_supports_pairs
  - query_existing_informs_pairs() → existing_informs_pairs  [dedup pre-filter, applied HERE]

Phase 3: Source candidate selection (metadata only, no embeddings)
  - select_source_candidates(all_active, ...) → source_candidates

Phase 4: Supports HNSW expansion
  - For each source in source_candidates:
    - HNSW search → neighbors with cosine > supports_candidate_threshold
    - Dedup against existing_supports_pairs
    - → candidate_pairs: Vec<(u64, u64, f32)>  [Supports candidates]

Phase 4b: Informs HNSW expansion (structural, no NLI)
  - Precondition: source category in informs_category_pairs LHS set
  - For each source in source_candidates:
    - HNSW search → neighbors with cosine >= nli_informs_cosine_floor (0.5 default)
    - EXCLUDE neighbors already in candidate_pairs (Phase 4 Supports set)  [MUTUAL EXCLUSION]
    - phase4b_candidate_passes_guards: cosine floor, category pair, temporal, cross-feature
    - Dedup against existing_informs_pairs (DB-level)
    - In-tick dedup (seen_informs_pairs)
    - → informs_metadata: Vec<InformsCandidate>

Phase 5: Independent caps
  - candidate_pairs.truncate(max_graph_inference_per_tick)   [Supports cap]
  - informs_metadata.shuffle(); informs_metadata.truncate(MAX_INFORMS_PER_TICK)  [Informs cap, hard limit]
  - Log: candidates_found / after_dedup / after_cap

[NLI availability check — gate for Phase 7/8 only]
  - get_provider() → Ok(provider) → proceed to Phase 6/7/8
  - get_provider() → Err       → write Phase 4b Informs directly, skip Phase 6/7/8

Phase 6 (NLI path only): Text fetch for Supports candidates
Phase 7 (NLI path only): W1-2 rayon dispatch, score_batch for Supports candidates
Phase 8 (NLI path only): Write Supports edges (nli_scores.entailment > threshold)

Phase 8b: Write Informs edges from informs_metadata
  - For each InformsCandidate:
    - apply_informs_composite_guard(candidate) → temporal + cross-feature only
    - weight = candidate.cosine * config.nli_informs_ppr_weight
    - write_nli_edge(..., "Informs", weight, ...)
  - Log: informs_edges_written, informs_pairs_evaluated
```

---

## User Workflows

### Tick execution (production, nli_enabled=false — the normal case)

1. `run_single_tick` executes the ordered tick sequence.
2. `run_graph_inference_tick` is called unconditionally (FR-01).
3. Phase 2 DB reads complete; `existing_informs_pairs` pre-filter loaded.
4. Phase 4 produces Supports candidates (cosine > threshold) — no writes yet.
5. Phase 4b produces Informs candidates, excluding Phase 4 candidates, applying structural guards.
6. Phase 5 caps both candidate sets.
7. Observability log emitted (FR-14).
8. `get_provider()` returns `Err` (NLI not loaded in production).
9. Phase 8 is skipped entirely — zero Supports edges written.
10. Phase 8b executes with `informs_metadata` directly — applies `apply_informs_composite_guard`.
11. Informs edges are written to `GRAPH_EDGES`.
12. Contradiction scan executes independently if tick interval condition is met (FR-10).

### Tick execution (nli_enabled=true — future/test)

1-7. Same as above.
8. `get_provider()` returns `Ok(provider)`.
9. Phase 6: text fetch for Supports candidates only.
10. Phase 7: W1-2 rayon dispatch, `score_batch` for Supports pairs.
11. Phase 8: Supports edges written based on NLI entailment.
12. Phase 8b: Informs edges written from Phase 4b candidates (as above).
13. Contradiction scan executes independently (same condition).

---

## Constraints

**C-01 (Ordering invariant — non-negotiable)**: The tick sequence in `run_single_tick` must
follow: compaction → promotion → graph-rebuild → structural_graph_tick (always) →
contradiction_scan (if embed adapter ready, every `CONTRADICTION_SCAN_INTERVAL_TICKS`).
No reordering of these steps is permitted.

**C-02 (W1-2 contract)**: All `CrossEncoderProvider::score_batch` calls must go through
`rayon_pool.spawn()`. `spawn_blocking` is prohibited. Inline async NLI is prohibited.
Phase 4b must not invoke `score_batch`. Phase 7 retains W1-2 compliance for Phase 8.

**C-03 (MAX_INFORMS_PER_TICK = 25 — hard limit)**: `informs_metadata.truncate(MAX_INFORMS_PER_TICK)`
is a hard cap. It is not a soft warning and must not be made conditional. It executes in Phase 5
before any write in Phase 8b.

**C-04 (Dedup-before-cap ordering)**: `query_existing_informs_pairs()` dedup pre-filter is applied
in Phase 2 (data fetch), before Phase 4b candidate selection. The cap in Phase 5 applies to the
already-deduped set. The sequence is: Phase 2 dedup → Phase 4b selection → Phase 5 cap → Phase 8b
write. Inversion of dedup and cap is not permitted.

**C-05 (Contradiction scan — conditional forever)**: `scan_contradictions` must remain gated on
embed adapter availability and tick interval. It must never be made unconditional. It writes to
`contradiction_cache`, not `GRAPH_EDGES`.

**C-06 (nli_enabled not removed)**: `InferenceConfig.nli_enabled` remains. It gates: (a) NLI
cross-encoder for Phase 8 Supports, (b) rayon pool floor of 6 at startup, (c) contradiction
scan scheduling. Removing it is out of scope for crt-039.

**C-07 (C-12 — no domain literals)**: Category pair strings in Phase 4b must come exclusively
from `config.informs_category_pairs`. No string literals of category names in production code.

**C-08 (File size)**: Production code changes must not push the non-test code in
`nli_detection_tick.rs` past the 500-line file guidance. Architect must assess split necessity.

**C-09 (nli_informs_cosine_floor range)**: The default change from 0.45 → 0.5 is within the
validated range `(0.0, 1.0)` exclusive. No change to `InferenceConfig::validate()` range check.

---

## Dependencies

**Crates (no new dependencies)**:
- `unimatrix-server` — all changes in this crate
- `unimatrix-store` — `query_existing_informs_pairs()`, `query_existing_supports_pairs()`, `write_nli_edge()` (existing)
- `unimatrix-vector` — `VectorIndex::search()`, `VectorIndex::get_embedding()` (existing)
- `unimatrix-core` — `InferenceConfig`, `EntryRecord`, `Status` (existing)

**Internal components touched**:
- `background.rs` — remove outer NLI gate, add ordering invariant comment, label contradiction scan
- `nli_detection_tick.rs` — restructure Phase 1 guard, modify Phase 4b mutual exclusion, simplify Phase 8b, remove `apply_informs_composite_guard` nli_scores parameter, update module doc comment
- `infra/config.rs` — change `default_nli_informs_cosine_floor()` from 0.45 → 0.5, update `InferenceConfig::default()` doc and assertion

**Test infrastructure**:
- Existing: `test_helpers::open_test_store`, `insert_test_entry`, `make_rayon_pool`, `NliServiceHandle::new()` (Loading state)
- No new fixtures required; extend existing test helpers only

**Roadmap dependency**:
- crt-039 is a prerequisite for all Group 3 graph enrichment features (cosine Supports detection,
  S1 tag co-occurrence, S2 structural vocabulary, S8 search co-retrieval).

---

## Test Specification

### Required New Tests

**TC-01** — `test_phase4b_writes_informs_when_nli_not_ready` (integration, `nli_detection_tick.rs`)
- Setup: two active entries with embeddings in VectorIndex, `NliServiceHandle::new()` (Loading state, `get_provider()` returns Err)
- Execute: `run_graph_inference_tick(...)` with `nli_enabled=false` and `nli_informs_cosine_floor` set to produce at least one candidate pair
- Assert: `store.query_graph_edges()` contains at least one Informs edge
- Addresses: AC-15, SR-05a, AC-02

**TC-02** — `test_phase8_no_supports_when_nli_not_ready` (integration, `nli_detection_tick.rs`)
- Setup: same as TC-01 plus Supports-eligible pair (cosine > `supports_candidate_threshold`)
- Execute: `run_graph_inference_tick(...)` with NLI not ready
- Assert: zero Supports edges in `store.query_graph_edges()`; may contain Informs edges
- Addresses: AC-16, SR-04, SR-05b, AC-02, AC-14
- Note: this is a separate test from TC-01 — two independent assertions

**TC-03** — `test_apply_informs_composite_guard_temporal_guard` (unit, `nli_detection_tick.rs`)
- Assert: guard returns `false` when `source_created_at >= target_created_at`
- Assert: guard returns `true` when `source_created_at < target_created_at` (other guards pass)

**TC-04** — `test_apply_informs_composite_guard_cross_feature_guard` (unit, `nli_detection_tick.rs`)
- Assert: guard returns `false` when both feature cycles non-empty and equal (intra-feature)
- Assert: guard returns `true` when source feature cycle empty
- Assert: guard returns `true` when target feature cycle empty
- Assert: guard returns `true` when both non-empty and different

**TC-05** — `test_phase4b_cosine_floor_0500_included` (unit, replacing `test_phase8b_writes_informs_when_neutral_just_above_0_5`)
- Assert: candidate with cosine exactly 0.500 passes Phase 4b cosine guard (inclusive `>=`)
- Addresses: AC-05, AC-09

**TC-06** — `test_phase4b_cosine_floor_0499_excluded` (unit, replacing `test_phase8b_no_informs_when_neutral_exactly_0_5`)
- Assert: candidate with cosine 0.499 is excluded by Phase 4b (below `nli_informs_cosine_floor = 0.5`)
- Addresses: AC-05, AC-09

**TC-07** — `test_phase4b_excludes_supports_candidates` (unit, `nli_detection_tick.rs`)
- Assert: a pair with cosine above `supports_candidate_threshold` (e.g., 0.68) present in Phase 4 `candidate_pairs` does not appear in Phase 4b `informs_metadata`
- Addresses: AC-13, FR-06, SR-02, SR-03

### Required Updated Tests

**TC-U01** — `test_inference_config_default_nli_informs_cosine_floor` (config.rs)
- Change: assert value is `0.5_f32`, not `0.45_f32`
- Addresses: AC-04, AC-08

**TC-U02** — `test_validate_nli_informs_cosine_floor_valid_value_is_ok` (config.rs)
- Change: use `0.5` as the nominal valid value, not `0.45`
- Addresses: AC-08

**TC-U03** — `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold` (nli_detection_tick.rs)
- Change: update threshold values from `0.47 >= 0.45` to `0.51 >= 0.5` (or equivalent that remains in the band)
- Also verify the lower-band behavior is updated to match the new floor
- Addresses: AC-05, AC-08

### Required Removed Tests

**TR-01** — `test_run_graph_inference_tick_nli_not_ready_no_op` (nli_detection_tick.rs)
- Remove entirely. Semantics are no longer valid: the tick is not a no-op when NLI is not ready.
- Replaced by TC-01 and TC-02 (separate assertions).
- Addresses: AC-16, SR-05

**TR-02** — `test_phase8b_no_informs_when_neutral_exactly_0_5` (nli_detection_tick.rs)
- Remove entirely. Guard 1 (neutral zone) does not exist after crt-039.
- Replaced by TC-06.
- Addresses: AC-09

**TR-03** — `test_phase8b_writes_informs_when_neutral_just_above_0_5` (nli_detection_tick.rs)
- Remove entirely. Guard 1 (neutral zone) does not exist after crt-039.
- Replaced by TC-05.
- Addresses: AC-09

---

## NOT In Scope

The following are explicitly excluded to prevent scope creep:

- **Replacing the NLI model**: blocked on ASS-036 findings; no domain-adapted model available.
- **Implementing contradiction edge writing to GRAPH_EDGES**: contradiction scan remains scan-only (writes to `contradiction_cache`).
- **Removing `nli_enabled` from `InferenceConfig`**: still gates NLI cross-encoder (Phase 8), rayon pool floor, and contradiction scan.
- **Changing Phase 8 (Supports) detection path**: only Phase 8b (Informs) is modified.
- **Group 3 graph enrichment** (cosine Supports replacement, S1 tag co-occurrence, S2 vocabulary, S8 co-retrieval): depend on crt-039 as prerequisite.
- **PPR expander** (Group 4).
- **Behavioral signal infrastructure** (Groups 5/6).
- **Module rename** of `nli_detection_tick.rs`: deferred to Group 3 when NLI is fully removed from Phase 8.
- **Rayon pool floor change**: floor stays at 4 when `nli_enabled=false`; Phase 4b is pure structural (no pool usage).
- **Schema changes**: no new tables, columns, or schema version bump.
- **Pre-deployment corpus scan** at cosine floors 0.45 vs 0.5: recommended by SR-02 but not a blocking requirement. The observability log (FR-14) provides equivalent signal from tick 1 in production.

---

## Open Questions for Architect

**OQ-01 (config parameter in apply_informs_composite_guard)**: **RESOLVED** — `config` parameter dropped. After removing guards 4 and 5, no `InferenceConfig` fields are consulted by the remaining guards. Architecture (ARCHITECTURE.md integration surface table) confirms removal. Spec updated to match.

**OQ-02 (Phase 8b write path for Option B)**: In Option B, Phase 4b candidates no longer enter
Phase 6 (text fetch) or Phase 7 (NLI batch). Phase 8b currently iterates `merged_pairs` which
is built from `pair_origins` zipped with NLI scores. After the split, Phase 8b must iterate
`informs_metadata` directly (not `merged_pairs`). This restructures the Phase 6/7/8/8b loop.
The architect must define the exact control flow boundary: does Phase 8b move outside the NLI
path entirely, or does the function return early from the NLI path and then fall through to
a separate Phase 8b block?

**OQ-03 (format_nli_metadata_informs)**: This helper currently serializes NLI scores (entailment,
contradiction, neutral) for Informs edge metadata. After crt-039, Phase 8b has no NLI scores.
Should the Informs edge metadata be: (a) replaced with a simpler cosine-only JSON, (b) removed
entirely (empty metadata), or (c) the function deleted? The decision affects observability for
Informs edges in the graph. Architect decides before pseudocode.

**OQ-04 (file size — submodule split)**: If the Option B restructuring (separate structural
Informs path from NLI batch path) increases production code line count past 500 lines, the
architect must decide whether to extract Phase 4b and Phase 8b into a submodule
(e.g., `nli_detection_tick/structural_informs.rs`). This decision is gated on the architect's
code volume estimate.

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned 18 entries. Most relevant: #3713 (supports_edge_threshold lesson — threshold tuning blind without log coverage, confirms FR-14 observability requirement), #3937 (nli_detection_tick.rs pattern — tap NLI neutral score for Informs, directly describes the guard being removed), #3656 (crt-029 decision — file split rationale), #3826 (crt-034 decision — promotion tick cap and ordering invariant), #3949 (testing pattern — each composite guard predicate requires independent negative test, confirms TC-03/TC-04 approach), #3971 (bugfix-473 — independent Supports/Informs caps, confirms MAX_INFORMS_PER_TICK constant and independence of the two budgets), #3957 (lesson-learned — cross-feature guard conflating concerns, directly describes the guard at guard 3).
