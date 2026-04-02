# crt-039 Implementation Brief
# Tick Decomposition: Decouple Structural Graph Inference from NLI Gate

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-039/SCOPE.md |
| Scope Risk Assessment | product/features/crt-039/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/crt-039/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-039/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-039/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-039/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| background.rs (tick orchestrator) | product/features/crt-039/pseudocode/background.md | product/features/crt-039/test-plan/background.md |
| nli_detection_tick.rs (tick implementation) | product/features/crt-039/pseudocode/nli_detection_tick.md | product/features/crt-039/test-plan/nli_detection_tick.md |
| infra/config.rs (InferenceConfig) | product/features/crt-039/pseudocode/config.md | product/features/crt-039/test-plan/config.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | product/features/crt-039/pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | product/features/crt-039/test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Remove the `nli_enabled` NLI availability guard that was incorrectly blocking Phase 4b
(structural Informs HNSW inference) from ever running in production, so that Informs edges
accumulate from the first tick after deployment. This unblocks all Group 3 graph enrichment
features (cosine Supports replacement, S1 tag co-occurrence, S2 vocabulary, S8 co-retrieval),
which depend on a live Informs graph as their prerequisite.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| D-01: Option A vs B vs X vs Y vs Z for Phase 7 decoupling — how to split Phase 4b from the NLI batch while keeping a single public function | Option Z chosen: internal split within `run_graph_inference_tick`. `get_provider()` moves to Path B entry (after Phase 4b writes). Function signature unchanged. Phase 2 DB reads shared by both paths. `NliCandidatePair::Informs` and `PairOrigin::Informs` variants removed. | SCOPE.md D-01; ADR-001 | product/features/crt-039/architecture/ADR-001-control-flow-split.md (#4017) |
| D-02: Guards 4 and 5 in `apply_informs_composite_guard` after Option Z — mutual exclusion via NLI scores | Remove both. Mutual exclusion is enforced by candidate set separation (Phase 4b explicitly subtracts Phase 4 Supports candidates). `nli_scores` and `config` parameters removed entirely. Two guards retained: temporal (guard 2) and cross-feature (guard 3). | SCOPE.md D-02; ADR-002 | product/features/crt-039/architecture/ADR-002-composite-guard-simplification.md (#4018) |
| D-03: Rayon pool floor when `nli_enabled=false` | No change. Pool floor stays at 4 when `nli_enabled=false`. Phase 4b is pure structural (HNSW + DB reads only) — pool floor of 6 is justified only by NLI CPU-bound inference which Phase 4b does not use. | SCOPE.md D-03 | (no ADR — no change) |
| D-04: Module rename of `nli_detection_tick.rs` | Deferred to Group 3 when NLI is fully removed from Phase 8. Update module-level doc comment to describe dual nature and note deferred rename. | SCOPE.md D-04 | (no ADR — no change) |
| OQ-01: `config` parameter in `apply_informs_composite_guard` | Removed. Neither of the two retained guards (temporal, cross-feature) references any `InferenceConfig` field. Dropping it eliminates future confusion. Architecture confirmed. | ADR-002 | product/features/crt-039/architecture/ADR-002-composite-guard-simplification.md (#4018) |
| OQ-02: Phase 8b control flow placement (Option Z) | Phase 8b (Informs write loop) runs unconditionally after Phase 5. The `get_provider()` conditional return gates only Phase 6/7/8 (Supports path). Phase 8b is outside and after the Path B block. | ADR-001 | product/features/crt-039/architecture/ADR-001-control-flow-split.md (#4017) |
| OQ-03: `format_nli_metadata_informs` disposition | Replaced by `format_informs_metadata(cosine: f32, source_category, target_category)`. NLI score fields (`nli_neutral`, `nli_entailment`, `nli_contradiction`) removed from Informs edge metadata. Structural fields (`cosine`, `source_category`, `target_category`) retained. | ADR-002 | product/features/crt-039/architecture/ADR-002-composite-guard-simplification.md (#4018) |
| OQ-04: File size — submodule split if production code exceeds 500 lines | Architect to assess during pseudocode. If Option Z restructuring + new test code pushes non-test production code past 500 lines, extract Phase 4b and Phase 8b into `nli_detection_tick/structural_informs.rs`. Decision gated on code volume estimate at pseudocode time. | SPEC NFR-06 | product/features/crt-039/architecture/ADR-001-control-flow-split.md (#4017) |
| ADR-003: `nli_informs_cosine_floor` default | Raised from 0.45 to 0.50. Aligns Informs floor with Supports threshold. NLI neutral guard (which previously filtered the 0.45–0.50 band) is removed; raising the floor compensates. Inclusive `>=` semantics unchanged. | ADR-003 | product/features/crt-039/architecture/ADR-003-cosine-floor-raise.md (#4019) |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/background.rs` | Modify | Remove `if inference_config.nli_enabled` gate around `run_graph_inference_tick` call; add ordering invariant comment; add named section comment on contradiction scan block |
| `crates/unimatrix-server/src/nli_detection_tick.rs` | Modify | Restructure `run_graph_inference_tick` as Option Z two-path function; remove Phase 1 early-return from function head; move `get_provider()` call to Path B entry; remove `NliCandidatePair::Informs` and `PairOrigin::Informs` variants; simplify `apply_informs_composite_guard` to 2 guards; replace `format_nli_metadata_informs` with `format_informs_metadata`; add Phase 4b explicit Supports-set subtraction; add Phase 4b observability log; update module doc comment; remove/replace affected tests |
| `crates/unimatrix-server/src/infra/config.rs` | Modify | Change `default_nli_informs_cosine_floor()` return value from `0.45_f32` to `0.5_f32`; update `InferenceConfig::default()` field accordingly; update tests asserting `0.45` |

---

## Data Structures

### `apply_informs_composite_guard` — Signature Change

**Before:**
```rust
fn apply_informs_composite_guard(
    nli_scores: &NliScores,
    candidate: &InformsCandidate,
    config: &InferenceConfig,
) -> bool
```

**After:**
```rust
fn apply_informs_composite_guard(
    candidate: &InformsCandidate,
) -> bool
```

Two guards retained: temporal (`source_created_at < target_created_at`) and cross-feature
(source and target belong to different feature cycles, or at least one cycle field is empty).

### `NliCandidatePair` enum — `Informs` variant removed

**Before:** `SupportsContradict { ... }` | `Informs { ... }`
**After:** `SupportsContradict { ... }` only

`PairOrigin` enum: same — `Informs` variant removed.

### `format_informs_metadata` — new function (replaces `format_nli_metadata_informs`)

```rust
fn format_informs_metadata(
    cosine: f32,
    source_category: &str,
    target_category: &str,
) -> String
```

Emits JSON with `cosine`, `source_category`, `target_category` only. No NLI score fields.

### `InformsCandidate` struct — unchanged

All 9 fields unchanged. Consumed directly by Phase 8b write loop in Path A.

---

## Function Signatures

### `run_graph_inference_tick` — signature unchanged

```rust
pub async fn run_graph_inference_tick(
    store: &Store,
    nli_handle: &NliServiceHandle,
    vector_index: &VectorIndex,
    rayon_pool: &RayonPool,
    config: &InferenceConfig,
) -> Result<(), TickError>
```

Internal restructuring only. Call site in `background.rs` is unchanged except that the
`if inference_config.nli_enabled` guard wrapping the call is removed.

### `phase4b_candidate_passes_guards` — unchanged

```rust
fn phase4b_candidate_passes_guards(
    similarity: f32,
    source_cat: &str,
    target_cat: &str,
    source_ts: i64,
    target_ts: i64,
    source_fc: &str,
    target_fc: &str,
    config: &InferenceConfig,
) -> bool
```

### `default_nli_informs_cosine_floor` — return value changes

```rust
fn default_nli_informs_cosine_floor() -> f32 {
    0.5  // was 0.45
}
```

---

## Control Flow After Split (Option Z)

```
Phase 2: DB reads — active entries, isolated IDs, existing Supports pairs, existing Informs pairs
Phase 3: Source candidate selection (metadata only)
Phase 4: HNSW expansion — Supports candidates (cosine > supports_candidate_threshold)
Phase 4b: HNSW expansion — Informs candidates (cosine >= nli_informs_cosine_floor = 0.5)
          Guards: cosine floor, category pair (from config), temporal, cross-feature, dedup
          Explicit subtraction: exclude Phase 4 Supports candidates from Informs set
          [PATH A — runs unconditionally, no NLI model used]
Phase 5: Independent caps — Supports: max_graph_inference_per_tick; Informs: 25 (hard limit)
         Observability log: candidates_found / after_dedup / after_cap / edges_written

[PATH A — Informs write loop]
  For each InformsCandidate in capped informs_metadata:
    apply_informs_composite_guard(candidate) — temporal + cross-feature only
    write_nli_edge(..., "Informs", cosine * nli_informs_ppr_weight)

[PATH B entry — gated by NLI provider availability]
  if candidate_pairs.is_empty(): return (no Supports work)
  get_provider() → Err: return (no Phase 6/7/8 writes)
  get_provider() → Ok(provider):
    Phase 6: text fetch for Supports candidates only
    Phase 7: W1-2 rayon dispatch, score_batch (Supports only)
    Phase 8: write Supports edges (entailment > threshold)
```

**Ordering invariant in `run_single_tick`:**
```
compaction → co_access_promotion → TypedGraphState::rebuild → PhaseFreqTable::rebuild
→ contradiction_scan (if embed adapter ready && tick_multiple_of_interval)
→ extraction_tick
→ structural_graph_tick (always: run_graph_inference_tick)
```
Note: crt-039 does NOT change the position of contradiction_scan or extraction_tick.
Only the `nli_enabled` gate around `run_graph_inference_tick` is removed — the tick
continues to run last in the sequence, as in the current code.

---

## Constraints

| Constraint | Source |
|-----------|--------|
| C-01: Tick ordering invariant is non-negotiable. compaction → promotion → graph-rebuild → contradiction_scan → extraction_tick → structural_graph_tick. The position of contradiction_scan (before graph inference tick) does NOT change. Only the `nli_enabled` gate wrapping the final step is removed. | SCOPE.md; ADR-001 |
| C-02: W1-2 contract. All `score_batch` calls via `rayon_pool.spawn()`. Phase 4b must not invoke `score_batch`. `spawn_blocking` prohibited. | SCOPE.md; SPEC C-02 |
| C-03: `MAX_INFORMS_PER_TICK = 25` is a hard write limit. `informs_metadata.truncate(25)` executes before any Phase 8b write. | SPEC C-03; FR-09 |
| C-04: Dedup-before-cap ordering. `query_existing_informs_pairs` (Phase 2) applied before Phase 4b candidate selection; Phase 5 cap applies to the already-deduped set. | SPEC C-04; FR-09 |
| C-05: Contradiction scan remains conditional forever. Must remain gated on embed adapter availability and tick interval. Must not become unconditional. Writes to `contradiction_cache` only. | SCOPE.md; SPEC C-05 |
| C-06: `nli_enabled` is not removed. Still gates NLI cross-encoder (Phase 8), rayon pool floor, and contradiction scan scheduling. | SCOPE.md; SPEC C-06 |
| C-07: No domain string literals in production code (C-12). Category pair strings come from `config.informs_category_pairs` only. | SCOPE.md; SPEC C-07 |
| C-08: Production code in `nli_detection_tick.rs` must not exceed 500-line guidance. If Option Z pushes non-test code past this, extract to `nli_detection_tick/structural_informs.rs`. | SPEC C-08; NFR-06 |
| C-09: `nli_informs_cosine_floor` range validation `(0.0, 1.0)` exclusive unchanged. 0.5 is within range. No change to `InferenceConfig::validate()`. | SPEC C-09 |
| R-03 / AC-13: Candidate set separation requires explicit Phase 4b subtraction of Phase 4 Supports candidates — not reliance on threshold arithmetic alone. | SPEC FR-06; RISK R-03 |
| AC-17 (required, not optional): Phase 4b must emit a `tracing::debug!` log at completion recording exactly four fields: `informs_candidates_found`, `informs_candidates_after_dedup`, `informs_candidates_after_cap`, `informs_edges_written`. This is the primary production signal that Phase 4b is running and accumulating edges post-deployment. Gate-3c verification: `grep -n 'informs_candidates_found' crates/unimatrix-server/src/nli_detection_tick.rs` must return at least one match in the Phase 4b/5 region. | SPEC FR-14; AC-17 |

---

## Dependencies

### Crates (no new dependencies)

| Crate | Usage |
|-------|-------|
| `unimatrix-server` | All production code changes in this crate |
| `unimatrix-store` | `query_existing_informs_pairs()`, `query_existing_supports_pairs()`, `write_nli_edge()` — existing methods, no changes |
| `unimatrix-vector` | `VectorIndex::search()`, `VectorIndex::get_embedding()` — existing, no changes |
| `unimatrix-core` | `InferenceConfig`, `EntryRecord`, `Status` — `nli_informs_cosine_floor` default changes in `config.rs` |

### External Services

None. Phase 4b is pure structural — no NLI model, no ONNX calls. The NLI service handle
(`NliServiceHandle`) is still passed to `run_graph_inference_tick` unchanged (Path B uses it),
but Phase 4b never calls `get_provider()`.

### Roadmap Dependency

crt-039 is the Group 2 prerequisite. All Group 3 graph enrichment features — cosine Supports
detection, S1 tag co-occurrence, S2 structural vocabulary, S8 search co-retrieval — depend on
a live Informs graph accumulated by Phase 4b. No Group 3 feature can produce a meaningful graph
until this feature is shipped and ticking.

---

## NOT in Scope

- Replacing the NLI model — blocked on ASS-036 (GGUF failed, no domain-adapted model available)
- Contradiction edge writing to `GRAPH_EDGES` — contradiction scan remains scan-only, writes to `contradiction_cache`
- Removing `nli_enabled` from `InferenceConfig` — still gates NLI cross-encoder, rayon pool floor, contradiction scan
- Changing Phase 8 (Supports) detection path — only Phase 8b (Informs) is modified
- Group 3 graph enrichment (cosine Supports replacement, S1, S2, S8) — blocked on this feature completing
- PPR expander (Group 4)
- Behavioral signal infrastructure (Groups 5/6)
- Module rename of `nli_detection_tick.rs` — deferred to Group 3
- Rayon pool floor change when `nli_enabled=false`
- Schema changes — no new tables, columns, or schema version bump
- Pre-deployment corpus scan at cosine floors 0.45 vs 0.50 (recommended by ADR-003/SR-02 as implementor pre-condition check, not a blocking deliverable — FR-14 observability log provides equivalent production signal from tick 1)

---

## Critical Risks (from RISK-TEST-STRATEGY.md)

| Risk | Severity | Mitigation |
|------|----------|------------|
| R-01: Phase 8b write loop executes before `get_provider()` guard fires — Supports edges written without NLI scores (silent data corruption) | Critical | Option Z structural control flow: `get_provider()` is the sole entry point to Phase 6/7/8. A conditional `return` on `Err` is the only path out that does not reach Phase 8. No code path from `get_provider() Err` to `write_nli_edge` for Supports edges. Validated by TC-02 (integration test, real Store, zero Supports edges asserted). |
| R-02: `test_run_graph_inference_tick_nli_not_ready_no_op` removed without replacement — no regression coverage for Path A / Path B boundary | Critical | TR-01 requires explicit removal; TC-01 (Informs CAN be written when NLI not ready) and TC-02 (Supports NOT written when NLI not ready) are mandatory replacements — two separate integration tests. Pre-merge grep for removed test name must return empty. |
| R-03: Mutual-exclusion gap at cosine 0.50 boundary — pair at exactly 0.50 qualifies for Phase 4b (`>= 0.50`) but not Phase 4 (`> 0.50` strict); explicit Supports-set subtraction absent would allow it to also enter Supports if NLI enabled | Critical | Phase 4b must explicitly subtract Phase 4 `candidate_pairs` set before producing `informs_metadata`. AC-13 mandates this. TC-07 validates the explicit subtraction (pair at 0.68 absent from `informs_metadata`). |
| R-04: `NliCandidatePair::Informs` / `PairOrigin::Informs` dead-code variants retained or partially removed — match arms still reachable, or removal breaks exhaustive matches | Critical | Full removal of both variants and all match arms. `cargo build --workspace` with `#![deny(dead_code)]` active. Pre-merge grep: `grep -rn 'NliCandidatePair::Informs\|PairOrigin::Informs'` in production code must return empty. |

---

## Test Changes Summary

### Tests to Remove (TR)

| Test | Location | Reason |
|------|----------|--------|
| `test_run_graph_inference_tick_nli_not_ready_no_op` | `nli_detection_tick.rs` | Tick is no longer a no-op when NLI not ready; semantics invalidated. Replaced by TC-01 + TC-02. |
| `test_phase8b_no_informs_when_neutral_exactly_0_5` | `nli_detection_tick.rs` | Neutral zone guard removed. Replaced by TC-06. |
| `test_phase8b_writes_informs_when_neutral_just_above_0_5` | `nli_detection_tick.rs` | Neutral zone guard removed. Replaced by TC-05. |

### Tests to Add (TC)

| Test | Type | Assertion |
|------|------|-----------|
| TC-01: `test_phase4b_writes_informs_when_nli_not_ready` | Integration | Phase 4b writes at least one Informs edge when `NliServiceHandle` is in Loading state. Setup must use zero-Supports-candidates corpus (no pair above `supports_candidate_threshold`) to also cover R-07. |
| TC-02: `test_phase8_no_supports_when_nli_not_ready` | Integration | Zero Supports edges when NLI not ready; may contain Informs edges. Separate test from TC-01. |
| TC-03: `test_apply_informs_composite_guard_temporal_guard` | Unit | False when source >= target timestamp; true when source < target. |
| TC-04: `test_apply_informs_composite_guard_cross_feature_guard` | Unit | False when both cycles non-empty and equal; true when either empty or both non-empty and different. |
| TC-05: `test_phase4b_cosine_floor_0500_included` | Unit | Cosine exactly 0.500 passes Phase 4b cosine guard (inclusive `>=`). |
| TC-06: `test_phase4b_cosine_floor_0499_excluded` | Unit | Cosine 0.499 excluded by Phase 4b (below floor). |
| TC-07: `test_phase4b_excludes_supports_candidates` | Unit | Pair at cosine 0.68 (above `supports_candidate_threshold`) present in Phase 4 `candidate_pairs` is absent from `informs_metadata`. |

### Tests to Update (TC-U)

| Test | Change |
|------|--------|
| `test_inference_config_default_nli_informs_cosine_floor` | Assert `0.5_f32` (was `0.45_f32`) |
| `test_validate_nli_informs_cosine_floor_valid_value_is_ok` | Use `0.5` as nominal valid value |
| `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold` | Update band boundary from `[0.45, 0.50)` to `[0.50, supports_threshold)`. Use cosine = 0.50 to verify inclusive floor. |
| All `apply_informs_composite_guard` call sites in tests | Remove `nli_scores` / `NliScores` argument |

---

## Alignment Status

**PASS** — 0 VARIANCE, 0 FAIL. Two WARNs from initial review resolved post-synthesis:
- WARN-1 (OQ-01 signature inconsistency): `config` parameter dropped from `apply_informs_composite_guard`; spec updated to match ARCHITECTURE.md. Closed.
- WARN-2 (AC-13 explicit subtraction): Confirmed as intended by human. Explicit Phase 4 set subtraction (FR-06, AC-13, TC-07) is the implementation obligation. Closed.

Full report: product/features/crt-039/ALIGNMENT-REPORT.md
