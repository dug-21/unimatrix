# crt-039: Tick Decomposition — Architecture

## System Overview

`run_graph_inference_tick` in `nli_detection_tick.rs` currently runs as a single monolithic
function gated behind two NLI-availability guards:

1. An outer guard in `background.rs`: `if inference_config.nli_enabled` wrapping the entire
   call site.
2. An inner Phase 1 guard in `run_graph_inference_tick`: `nli_handle.get_provider()` early-
   return at the top of the function.

Phase 4b (structural Informs inference via HNSW cosine filter) does not use the NLI
cross-encoder at all. Gating it on NLI availability is a category error. The result is that
no Informs edges are ever written in production (default: `nli_enabled = false`).

crt-039 restructures the function so Phase 4b runs unconditionally while Phase 8 (NLI
Supports) remains gated. This is the Group 2 prerequisite: structural Informs inference
must accumulate edges before Group 3 graph enrichment can deliver.

The contradiction scan is a separate concern — expensive O(N) ONNX — that already has its
own interval gate. It is given a clearly labeled block in `background.rs` to make its
independence from the graph inference tick explicit.

---

## Component Breakdown

### Component 1: `background.rs` — Tick Orchestrator

Owns `run_single_tick`. Responsible for calling tick steps in invariant order and enforcing
the outer gates.

**Before crt-039:**
- `run_graph_inference_tick` is called only when `inference_config.nli_enabled == true`.
- The contradiction scan block has no named section label.

**After crt-039:**
- `run_graph_inference_tick` is called unconditionally on every tick.
- The contradiction scan block gains a named comment section that makes its own condition
  explicit and separates it from the structural graph tick.

The ordering invariant remains inviolate:
```
compaction → co_access_promotion → TypedGraphState::rebuild → PhaseFreqTable::rebuild
→ contradiction_scan (if nli_enabled && tick_multiple)
→ extraction_tick
→ run_graph_inference_tick   ← always
```

### Component 2: `nli_detection_tick.rs` — Tick Implementation

Owns `run_graph_inference_tick` and all its phases. After crt-039 this file hosts two
logically distinct execution paths in a single public function:

**Path A — Structural Informs (Phase 4b):** Runs unconditionally. Uses only HNSW cosine
queries and DB reads. No NLI model required.

**Path B — NLI Supports (Phase 8):** Conditionally executes. Requires a successful
`get_provider()` call. If `get_provider()` returns `Err`, Path B is entirely skipped —
no NLI batch, no Supports edge writes.

The key restructuring: Phase 1's `get_provider()` early-return is moved. Instead of
returning from the entire function on `Err`, it gates only the Phase 8 path by obtaining
the provider before entering Phase 6/7/8, after Phase 4b has already completed.

### Component 3: `infra/config.rs` — InferenceConfig

The `nli_informs_cosine_floor` default is raised from 0.45 to 0.5. The backing function
`default_nli_informs_cosine_floor()` and `InferenceConfig::default()` are both updated.
No validation change needed — 0.5 is within the existing `(0.0, 1.0)` exclusive range.

`nli_enabled` is not changed or removed. It continues to gate: (a) NLI cross-encoder
for Phase 8, (b) rayon pool floor of 6, (c) contradiction scan scheduling.

---

## Control Flow After Split (ADR-001)

The restructured `run_graph_inference_tick` executes in this sequence:

```
Phase 2: DB reads (active entries, isolated IDs, existing Supports pairs, existing Informs pairs)
           ↓
Phase 3: Source candidate selection (metadata only, capped at max_graph_inference_per_tick)
           ↓
Phase 4: HNSW expansion for Supports candidates (cosine > supports_candidate_threshold)
           ↓
Phase 4b: HNSW expansion for Informs candidates (cosine >= nli_informs_cosine_floor)
          All guards: cosine floor, category pair, temporal, cross-feature, dedup
          [STRUCTURAL ONLY — no NLI model used]
           ↓
Phase 5: Independent caps — Supports cap: max_graph_inference_per_tick
                             Informs cap: MAX_INFORMS_PER_TICK (25)
           ↓
   ┌────────────────────────────────────────────────────────────┐
   │ Path A: Write Informs edges directly from Phase 4b output  │
   │ (no NLI batch, no provider required)                        │
   │                                                              │
   │ For each informs_candidate in capped informs_metadata:      │
   │   apply_informs_composite_guard(candidate, config)          │
   │     → temporal guard (source_created_at < target_created_at)│
   │     → cross-feature guard (not same feature cycle)          │
   │   if passes: write_nli_edge(..., "Informs", weight, ...)    │
   └────────────────────────────────────────────────────────────┘
           ↓
   ┌────────────────────────────────────────────────────────────┐
   │ if candidate_pairs.is_empty(): return early (no Phase 8)   │
   │                                                              │
   │ Path B: NLI Supports path                                    │
   │   get_provider() — if Err: return (no writes)               │
   │   Phase 6: fetch text content for Supports pairs only       │
   │   Phase 7: rayon NLI score_batch (W1-2, Supports only)      │
   │   Phase 8: write Supports edges (entailment > threshold)    │
   └────────────────────────────────────────────────────────────┘
```

**Critical invariant for SR-04**: The `get_provider()` call is placed after Phase 4b
and after the Informs write loop. If `get_provider()` returns `Err`, Phase 8 is bypassed
by a conditional `return` that occurs only on the Path B entry point. Phase 4b has
already written its edges and returned normally through Path A before this guard fires.

---

## Phase 4b — Candidate Set Separation (SR-03)

SR-03 asks whether Phase 4 and Phase 4b produce disjoint candidate sets.

The thresholds produce disjoint sets **by construction**, not by explicit set subtraction:

- Phase 4 uses `similarity > supports_candidate_threshold` (default 0.50, **strict greater**).
  A pair with cosine = 0.50 is **excluded** from Phase 4.
- Phase 4b uses `similarity >= nli_informs_cosine_floor` (currently 0.45, raised to 0.50
  by this feature, **inclusive**). A pair with cosine = 0.50 is **included** in Phase 4b.

After crt-039:
- Phase 4 selects pairs with cosine **strictly above** 0.50.
- Phase 4b selects pairs with cosine **at or above** 0.50 where category pairs match the
  `informs_category_pairs` config.

A pair cannot appear in both Phase 4 (Supports) and Phase 4b (Informs) because:
1. Phase 4 is category-agnostic; Phase 4b requires category pair membership via
   `informs_category_pairs`. A pair must satisfy the category filter to enter Phase 4b.
2. Phase 4 uses symmetrized (min, max) dedup; Phase 4b uses directional (source, target)
   dedup with temporal ordering. The semantics are structurally incompatible.
3. The mutual-exclusion concern (guards 4 and 5 in the old `apply_informs_composite_guard`)
   addressed the case where a pair passed both NLI Supports and NLI Informs scoring in the
   merged batch. After the split, Informs candidates never enter the NLI batch. The overlap
   scenario can only occur at the cosine threshold boundary (exactly 0.50) and only for
   pairs satisfying the Informs category filter — which is a different candidate set from
   the Supports HNSW expansion.

**Consequence**: Guards 4 and 5 (`nli_scores.entailment <= supports_edge_threshold`,
`nli_scores.contradiction <= nli_contradiction_threshold`) are removed from
`apply_informs_composite_guard`. The `nli_scores` parameter is removed from the function
signature entirely. The function is renamed conceptually in its doc comment to reflect the
two guards it retains: temporal and cross-feature.

---

## `apply_informs_composite_guard` After Refactor

Current signature:
```rust
fn apply_informs_composite_guard(
    nli_scores: &NliScores,
    candidate: &InformsCandidate,
    config: &InferenceConfig,
) -> bool
```

After crt-039:
```rust
fn apply_informs_composite_guard(
    candidate: &InformsCandidate,
    // nli_scores removed — no NLI in structural Informs path
    // config removed — remaining guards need no config fields
) -> bool
```

Remaining guards (2 of the original 5):
1. `candidate.source_created_at < candidate.target_created_at` — temporal ordering
2. Cross-feature: block only when both feature_cycles are non-empty AND equal

Guards removed:
- Guard 1 (`nli_scores.neutral > 0.5`) — task-mismatched NLI score, removed per D-01/D-02
- Guard 4 (`nli_scores.entailment <= supports_edge_threshold`) — mutual exclusion via NLI,
  handled by candidate set separation
- Guard 5 (`nli_scores.contradiction <= nli_contradiction_threshold`) — same as guard 4

Note: Guards 2 and 3 in the original docstring correspond to the temporal and cross-feature
checks which are already fully evaluated by `phase4b_candidate_passes_guards` during Phase 4b.
`apply_informs_composite_guard` duplicates these for the write path. After the split, both
guards can remain in `apply_informs_composite_guard` — they are cheap and serve as a
defense-in-depth check at write time.

---

## Phase 6 Scope After Split

Phase 6 (text fetch via write_pool) is only needed for the NLI batch in Phase 7. After the
split, Phase 6 fetches text only for Supports candidates — the `informs_metadata` loop is
removed from Phase 6. Phase 4b candidates are written directly without text content (the
`write_nli_edge` call for Informs uses `cosine * ppr_weight` as the edge weight, which is
already computed from the Phase 4b HNSW scan, no NLI score needed).

The `NliCandidatePair` tagged union (crt-037 ADR-001) is no longer needed for the Informs
path. It is retained for the Supports path, but the `Informs` variant of the enum and the
associated `PairOrigin::Informs` variant are removed. The `InformsCandidate` struct is
retained and used directly in the Path A write loop.

---

## Phase 7 Scope After Split

Phase 7 (`rayon_pool.spawn` → `score_batch`) scores **Supports candidates only**. The
Informs candidates are no longer in the batch. The W1-2 contract is preserved: a single
rayon spawn per tick for all NLI scoring, with the Informs path not touching the rayon pool.

---

## Dedup Pre-Filter Placement (SR-01)

The `query_existing_informs_pairs` dedup pre-filter (Phase 2) is applied in two places:

1. **Phase 2**: DB read at tick start. Loads all existing Informs pairs as
   `HashSet<(u64,u64)>` (directional, per crt-037 ADR-003). This happens unconditionally
   before Phase 4b runs.

2. **Phase 4b candidate construction**: The per-pair check
   `existing_informs_pairs.contains(&(source_id, neighbor_id))` is evaluated inside the
   Phase 4b HNSW expansion loop, before any candidate is added to `informs_metadata`.

The dedup pre-filter applies **before** the Phase 5 cap — a pair that is already in the
graph does not consume one of the 25 Informs slots. This is the behavior specified by SR-01
and is preserved unchanged in the refactored flow.

The Phase 5 cap (`MAX_INFORMS_PER_TICK = 25`) is a hard write limit on net-new candidates.
It is not a soft warning. The cap applies after dedup — the 25 slots represent new edges,
not candidates checked.

---

## Contradiction Scan Separation

The contradiction scan in `background.rs` currently has a comment (`// GH #278 fix:`) that
identifies its purpose but does not clearly name it as an independent tick step. crt-039
adds a named section comment that makes explicit:

1. The scan is a separate tick step from structural graph inference.
2. Its condition is `nli_enabled && current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS)`.
3. The behavior is unchanged — only the comment structure changes.

The zero-diff behavioral constraint from SR-07 applies: the only permitted changes to the
contradiction scan block are comment additions and whitespace. No condition reordering, no
bracket changes.

---

## `format_nli_metadata_informs` After Split

Currently this function records `nli_neutral`, `nli_entailment`, and `nli_contradiction`
as metadata on Informs edges. After the split, Informs edges are written without NLI scores.
The function is either removed or simplified to record only structural metadata (cosine,
category pair). A replacement `format_informs_metadata` function takes the relevant
structural fields instead of `NliScores`.

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|------------------|----------------|--------|
| `run_graph_inference_tick` (public async) | `(store: &Store, nli_handle: &NliServiceHandle, vector_index: &VectorIndex, rayon_pool: &RayonPool, config: &InferenceConfig)` — signature **unchanged** | `nli_detection_tick.rs:121` |
| `apply_informs_composite_guard` (private) | `(candidate: &InformsCandidate) -> bool` — nli_scores and config params **removed** | `nli_detection_tick.rs:800` |
| `phase4b_candidate_passes_guards` (private) | `(similarity, source_cat, target_cat, source_ts, target_ts, source_fc, target_fc, config) -> bool` — **unchanged** | `nli_detection_tick.rs:754` |
| `InformsCandidate` struct | All 9 fields non-Option — **unchanged** | `nli_detection_tick.rs:85` |
| `NliCandidatePair` enum | `Informs` variant **removed**; `SupportsContradict` variant retained | `nli_detection_tick.rs:61` |
| `PairOrigin` enum | `Informs` variant **removed**; `SupportsContradict` variant retained | `nli_detection_tick.rs:103` |
| `default_nli_informs_cosine_floor()` | Returns `0.5` (was `0.45`) | `config.rs:783` |
| `InferenceConfig::default()` `nli_informs_cosine_floor` | `0.5` (was `0.45`) | `config.rs:628` |
| Outer gate in `background.rs` | `if inference_config.nli_enabled` block **removed** | `background.rs:760` |
| Contradiction scan block label | Named comment block, behavior zero-diff | `background.rs:661` |
| `format_nli_metadata_informs` | **Replaced** by `format_informs_metadata(cosine: f32, ...) -> String` | `nli_detection_tick.rs:819` |

---

## Test Impact Summary

Tests requiring update (confirmed from SCOPE.md and source review):

| Test | Required Change |
|------|----------------|
| `test_run_graph_inference_tick_nli_not_ready_no_op` | Split into two: (a) assert Informs edges CAN be written when NLI not ready, (b) assert Supports edges NOT written when NLI not ready |
| `test_phase8b_no_informs_when_neutral_exactly_0_5` | Remove — neutral guard is gone. Replace with cosine floor boundary test at 0.499 (excluded) and 0.500 (included) |
| `test_phase8b_writes_informs_when_neutral_just_above_0_5` | Remove — neutral guard is gone |
| All tests asserting `nli_informs_cosine_floor == 0.45` | Update to assert `0.50` |
| `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold` | Update floor boundary: band changes from [0.45, 0.50) to [0.50, supports_threshold). The test scenario must use a cosine at exactly 0.50 to verify inclusive floor. |
| Tests calling `apply_informs_composite_guard` with `NliScores` arg | Update call sites — NliScores parameter removed |

New tests required (SR-04, SR-05 from SCOPE-RISK-ASSESSMENT.md):

| New Test | Assertion |
|----------|-----------|
| `test_structural_graph_tick_writes_informs_when_nli_not_ready` | Phase 4b writes Informs edges even when `get_provider()` returns Err (entries with embeddings, passing cosine floor) |
| `test_nli_supports_tick_no_edges_when_nli_not_ready` | Phase 8 writes no Supports edges when `get_provider()` returns Err |

---

## ADR References

| ADR | Title | File | Unimatrix ID |
|-----|-------|------|--------------|
| ADR-001 | Control flow split in `run_graph_inference_tick` | `ADR-001-control-flow-split.md` | #4017 |
| ADR-002 | `apply_informs_composite_guard` simplification — remove NLI guards | `ADR-002-composite-guard-simplification.md` | #4018 |
| ADR-003 | Raise `nli_informs_cosine_floor` default from 0.45 to 0.50 | `ADR-003-cosine-floor-raise.md` | #4019 |

---

## Open Questions

None. All design decisions resolved in SCOPE.md and confirmed in architecture analysis above.
The SR-02 corpus measurement concern (baseline candidate counts at 0.45 vs 0.50) is a
pre-implementation risk mitigation step for the implementor, not an architectural blocker.
The SR-06 observability log line requirement (Phase 4b: candidates found, dedup-filtered,
cap-applied, edges written) is an implementation AC that the spec writer should formalize —
the architecture provides the correct logging points (after Phase 4b candidate construction
and after the Phase 5 cap).
