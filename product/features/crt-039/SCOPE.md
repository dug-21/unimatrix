# crt-039: Tick Decomposition — Decouple Structural Graph Inference from NLI Gate

## Problem Statement

The background tick pipeline contains two misplaced NLI guards that block structurally
sound inference paths, plus a correctly-gated contradiction scan that is structurally
entangled with the graph inference tick. Together they prevent structural Informs inference
from ever running in production (NLI is disabled by default and has no domain-adapted model
available — GGUF failed ASS-036).

**Guard 1 — Category error in `run_single_tick` (background.rs:760)**
`run_graph_inference_tick` is gated on `inference_config.nli_enabled`. Phase 4b
(structural Informs inference via HNSW cosine floor) does not use the NLI cross-encoder
at all — it is a pure cosine filter with category-pair and temporal guards. Gating Phase
4b on NLI availability is a category error: the gate conflates a model-availability check
with a structural-filter invocation. The result is that Informs edges are never written
in any production deployment where `nli_enabled = false` (the default).

**Guard 2 — NLI score applied to structural filter in Phase 8b (nli_detection_tick.rs:805)**
`apply_informs_composite_guard` requires `nli_scores.neutral > 0.5` as guard 1 of 5.
This check applies an SNLI-trained neutral-zone score to knowledge entries — a corpus
for which the model was not designed (ASS-035 confirmed task mismatch). The neutral score
carries no reliable signal for Unimatrix entry pairs. Raising `nli_informs_cosine_floor`
from 0.45 → 0.5 provides an equivalent structural filter that does not rely on a
task-mismatched NLI score.

**Structural entanglement — `contradiction_scan` shares the NLI guard**
Contradiction detection (`scan_contradictions`) does depend on embedding quality — it is a
legitimate O(N) ONNX-adjacent operation that should remain conditionally gated. Currently
it runs every `CONTRADICTION_SCAN_INTERVAL_TICKS` ticks regardless of `nli_enabled`, but
it lives in the same tick block as graph inference, creating conceptual coupling. The
roadmap prescribes separating it into a named tick step that clearly expresses its own
condition: remains gated on `nli_enabled`, fires on its own interval.

This feature is the prerequisite for Group 3 (Graph Enrichment): structural Informs
inference cannot accumulate edges until the NLI gate is removed.

## Goals

1. Remove the `nli_enabled` guard from `run_graph_inference_tick` in `background.rs` so
   structural graph inference (Phase 4b) always runs regardless of NLI model availability.
2. Remove `nli_scores.neutral > 0.5` from `apply_informs_composite_guard` in
   `nli_detection_tick.rs` (Phase 8b guard 1) and raise the default
   `nli_informs_cosine_floor` from 0.45 → 0.5 to compensate.
3. Separate `contradiction_scan()` into a clearly named tick step that retains its own
   condition (`nli_enabled && current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS)`)
   and makes that condition explicit in code comments.
4. Preserve the ordering invariant: compaction → promotion → graph-rebuild →
   contradiction_scan (if nli_enabled) → extraction_tick → structural_graph_tick (always).
5. Update all affected tests to reflect the new behavior (tick always runs; neutral guard
   gone; cosine floor raised).

## Non-Goals

- Replacing the NLI model with a domain-adapted alternative — blocked on ASS-036 finding,
  no model available.
- Implementing contradiction edge writing to `GRAPH_EDGES` — contradiction detection remains
  a scan-only operation writing to the contradiction cache, not the graph.
- Removing `nli_enabled` from `InferenceConfig` — the flag still gates the NLI cross-encoder
  for Supports edge detection (Phase 8) and contradiction scan. It is not being removed.
- Implementing Group 3 graph enrichment (S1 tag co-occurrence, S2 vocabulary, S8 search
  co-retrieval) — those are Group 3 and depend on crt-039 completing first.
- Changing the Supports (Phase 8) detection path — only Phase 8b (Informs) is modified.
- Adding the PPR expander (Group 4).
- Any behavioral signal infrastructure (Group 5/6).

## Background Research

### Tick Pipeline Current State

`run_single_tick` in `background.rs` executes in this order:

1. **maintenance_tick** — co-access cleanup, confidence refresh, graph compaction (HNSW
   rebuild via `VectorIndex::compact` + `Store::rewrite_vector_map`), session GC.
2. **GRAPH_EDGES orphaned-edge compaction** (lines 511–541) — DELETE FROM graph_edges
   where endpoints no longer exist. Non-fatal.
3. **co_access promotion tick** (line 550) — `run_co_access_promotion_tick`. Must run
   AFTER compaction, BEFORE TypedGraphState rebuild (ADR-005 / crt-034 ordering invariant).
4. **TypedGraphState rebuild** (lines 561–597) — async spawn + TICK_TIMEOUT wrapper.
5. **PhaseFreqTable rebuild** (lines 619–658) — async spawn + TICK_TIMEOUT wrapper.
6. **Contradiction scan** (lines 661–722) — `scan_contradictions` via rayon pool. Gated on
   `current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS)`. No `nli_enabled` gate
   (runs when embed adapter is available regardless of NLI model state). Writes to
   `contradiction_cache`, not `GRAPH_EDGES`.
7. **Extraction tick** (lines 724–757) — with TICK_TIMEOUT.
8. **Graph inference tick** (lines 759–769) — `run_graph_inference_tick`. Gated on
   `inference_config.nli_enabled`. **This is the guard to remove.**

### `run_graph_inference_tick` Structure (nli_detection_tick.rs)

- **Phase 1** (line 128–132): Guard — `nli_handle.get_provider()` returns `Err` when NLI
  not ready. Silent no-op. **This internal guard must also be removed or bypassed for the
  Phase 4b path**, since `get_provider()` is used to obtain the cross-encoder for Phase 8
  (Supports). Phase 4b does not use the cross-encoder at all.
- **Phase 2** (lines 134–180): DB reads — active entries, isolated IDs, existing
  Supports/Informs pairs.
- **Phase 3** (lines 182–200): Source candidate selection — metadata only.
- **Phase 4** (lines 207–265): HNSW expansion for Supports candidates (cosine >
  `supports_candidate_threshold`).
- **Phase 4b** (lines 267–384): HNSW expansion for Informs candidates (cosine >=
  `nli_informs_cosine_floor`). Cross-category, temporal, cross-feature, and dedup guards.
  **Pure structural filter — no NLI model used.**
- **Phase 5** (lines 392–459): Independent caps — Supports capped by
  `max_graph_inference_per_tick`, Informs by `MAX_INFORMS_PER_TICK` (25).
- **Phase 6**: Merge pair_origins with tagged-union `NliCandidatePair`.
- **Phase 7**: Single rayon spawn — NLI `score_batch` call (W1-2 contract). **Required for
  Phase 8 (Supports) only. Phase 4b candidates still enter the batch here.**
- **Phase 8** (lines 597–632): Write Supports edges — uses `nli_scores.entailment`.
- **Phase 8b** (lines 634–676): Write Informs edges — uses `apply_informs_composite_guard`.

### Phase 8b Guard (the neutral zone check)

`apply_informs_composite_guard` (nli_detection_tick.rs:800–812):

```
nli_scores.neutral > 0.5          // guard 1 — THE GUARD TO REMOVE
&& candidate.source_created_at < candidate.target_created_at  // guard 2 — temporal
&& (cross-feature check)          // guard 3 — feature cycle
&& nli_scores.entailment <= config.supports_edge_threshold    // guard 4 — mutual exclusion
&& nli_scores.contradiction <= config.nli_contradiction_threshold  // guard 5 — mutual exclusion
```

Guards 2, 3, 4, 5 are structurally sound. Guard 1 applies a task-mismatched NLI score.

After removal, guards 4 and 5 may also require re-evaluation: they exist to prevent an
Informs edge from being written for a pair that Phase 8 would write as Supports. Once Phase
4b candidates no longer pass through the NLI batch (see Open Questions), these guards
become irrelevant to the Informs path.

### Config Fields Affected

In `InferenceConfig` (`infra/config.rs`):

| Field | Current Default | Change |
|-------|----------------|--------|
| `nli_enabled` | `false` | No change — still gates NLI cross-encoder and contradiction scan |
| `nli_informs_cosine_floor` | `0.45` | Raise to `0.5` |
| `nli_informs_ppr_weight` | `0.6` | No change |
| `nli_contradiction_threshold` | `0.6` | No change |

The `nli_informs_cosine_floor` default change must propagate to:
- `default_nli_informs_cosine_floor()` backing function (config.rs)
- `InferenceConfig::default()` literal (config.rs)
- All tests that assert the 0.45 default value

### Tick Architecture Constraint (ADR-002 Phase 6 comment)

`background.rs:600–605` contains an ordering invariant comment in Phase 8:
> "this break is safe only because Phase 6 appends SupportsContradict pairs before Informs
> pairs in merged_pairs. If Phase 6 ever reorders the merge, this break could fire
> mid-Supports and cause Phase 8b to miss Informs variants silently."

This constraint is internal to `run_graph_inference_tick` and is unaffected by crt-039.

### Tests Requiring Updates

Key tests that test the neutral zone guard (must be removed/updated):
- `test_phase8b_no_informs_when_neutral_exactly_0_5` (nli_detection_tick.rs:1936)
- `test_phase8b_writes_informs_when_neutral_just_above_0_5` (nli_detection_tick.rs:1958)
- `test_run_graph_inference_tick_nli_not_ready_no_op` (nli_detection_tick.rs:1273) —
  tests that tick no-ops when NLI not ready. Semantics change: after crt-039, Phase 4b
  still runs even when NLI not ready. The test's assertion (no edges written when NLI is
  not ready) must be updated or rephrased.
- Config tests asserting `nli_informs_cosine_floor == 0.45` (config.rs).

## Proposed Approach

### Change 1 — Remove outer NLI gate (background.rs:760)

Remove the `if inference_config.nli_enabled` wrapper around `run_graph_inference_tick`.
The tick always runs. No replacement condition.

### Change 2 — Decouple Phase 4b from Phase 7 NLI batch (Option B — confirmed)

Split the tick into two independent code paths:
- **Phase 4b path (structural Informs):** Runs unconditionally. Uses only structural
  guards (cosine floor, category pair, temporal, cross-feature). Does not enter the NLI
  batch. Phase 8b writes Informs edges directly from Phase 4b output — no NLI scores used.
- **Phase 8 path (Supports, NLI-dependent):** Retains the NLI batch. Phase 1 guard
  (`get_provider()`) gates only this path.

Option A was rejected: keeping Phase 4b candidates in the NLI batch means Phase 1's
`get_provider()` early-return still blocks Phase 4b when `nli_enabled = false` (the
production default). Option A removes the neutral zone check but does not unblock Informs
edges in production. The roadmap is explicit: "always runs regardless of NLI availability."

`apply_informs_composite_guard` is simplified: guards 4 and 5 (`nli_scores.entailment ≤
threshold`, `nli_scores.contradiction ≤ threshold`) are **removed**. The mutual-exclusion
concern they addressed is handled by candidate set separation between Phase 4 and Phase 4b,
not by score comparison. The `nli_scores` parameter is removed from the function entirely.
Remaining guards: temporal (guard 2) and cross-feature (guard 3) only.

### Change 3 — Remove neutral zone check (nli_detection_tick.rs:805)

Remove `nli_scores.neutral > 0.5` from `apply_informs_composite_guard`. Raise default
`nli_informs_cosine_floor` from 0.45 → 0.5.

### Change 3b — Rayon pool floor (no change)

Phase 4b is pure structural: HNSW cosine queries and DB reads. It does not invoke the
rayon pool. The pool floor of 6 exists because `CrossEncoderProvider::score_batch` is
CPU-bound ML inference. Raising the floor when `nli_enabled = false` has no basis in
actual workload. Pool floor remains at 4 when `nli_enabled = false`. No code change.

### Change 4 — Separate contradiction scan (background.rs)

Add a named comment block that makes the contradiction scan's own condition explicit.
The behavior is unchanged — but the code comment must clearly separate it from the
structural graph tick. The ordering invariant comment must reflect:
> compaction → promotion → graph-rebuild → contradiction_scan (if nli_enabled, every N ticks) →
> extraction_tick → structural_graph_tick (always)

### Change 5 — Module-level doc comment update (nli_detection_tick.rs)

Rename is **deferred** to Group 3 (after cosine Supports replacement removes NLI from
Phase 8 entirely). For now: update the module-level doc comment to describe the dual
nature of the file — structural Informs path (Phase 4b) + NLI Supports path (Phase 8)
— and note the rename is deferred to Group 3.

## Acceptance Criteria

- AC-01: `run_graph_inference_tick` is called unconditionally on every tick regardless of
  `inference_config.nli_enabled`. The `if inference_config.nli_enabled` guard in
  `background.rs` is removed.
- AC-02: When `nli_enabled = false` and the NLI model is not loaded, Phase 4b (structural
  Informs HNSW scan) still executes and can write Informs edges. Phase 8 (Supports) does
  not write any edges (NLI model required).
- AC-03: `apply_informs_composite_guard` does not reference `nli_scores.neutral` in any
  form. The function signature or body may retain `nli_scores` only if guards 4 and 5
  (mutual-exclusion with Supports) are preserved; otherwise the parameter may be removed.
- AC-04: Default `nli_informs_cosine_floor` is 0.5 in `InferenceConfig::default()` and
  `default_nli_informs_cosine_floor()`.
- AC-05: A candidate pair with cosine similarity 0.499 is excluded by Phase 4b. A pair at
  0.500 is included (inclusive floor, AC-17 / AC-18 semantics preserved).
- AC-06: Contradiction scan continues to be gated on NLI model availability (embed adapter
  check) and `current_tick.is_multiple_of(CONTRADICTION_SCAN_INTERVAL_TICKS)`. The
  condition is unchanged; only its placement and labeling in the tick are clarified.
- AC-07: The ordering invariant is preserved: compaction → promotion → graph-rebuild →
  contradiction_scan (if nli_enabled) → extraction_tick → structural_graph_tick (always).
  The tick position of contradiction_scan does not change.
- AC-08: All tests that previously asserted `nli_informs_cosine_floor == 0.45` are updated
  to assert 0.5.
- AC-09: All tests that previously tested the `neutral > 0.5` guard behavior
  (test_phase8b_no_informs_when_neutral_exactly_0_5, test_phase8b_writes_informs_when_neutral_just_above_0_5)
  are removed or repurposed to test the new guard boundary (cosine floor at 0.5).
- AC-10: `cargo test --workspace` passes with no regressions after all changes.
- AC-11: The eval harness (`product/research/ass-039/harness/scenarios.jsonl`) shows MRR
  >= 0.2913 (no regression from crt-038 baseline).

## Constraints

- **Ordering invariant is non-negotiable.** `run_co_access_promotion_tick` must run before
  `TypedGraphState::rebuild` (ADR-005 / crt-034). Graph inference tick must run after
  TypedGraphState rebuild. Contradiction scan must run after graph inference tick. This
  sequence is preserved in current code and must not be disturbed.
- **W1-2 contract.** All `CrossEncoderProvider::score_batch` calls must go through
  `rayon_pool.spawn()`. `spawn_blocking` is prohibited. Inline async NLI is prohibited.
  If Phase 4b is decoupled from Phase 7, the NLI batch for Phase 8 still respects W1-2.
- **Contradiction scan remains conditional.** `scan_contradictions` uses embedding
  re-computation across all active entries (O(N) ONNX calls). This is expensive and
  appropriately gated. It must not be made unconditional.
- **`nli_enabled` is not being removed.** It still gates: (a) the NLI cross-encoder for
  Supports edge detection, (b) the rayon pool floor of 6 at startup, (c) contradiction
  scan scheduling. Removing it is out of scope.
- **Max 500 lines per file.** `nli_detection_tick.rs` is the primary file being modified.
  Currently ~2,200 lines (large due to extensive tests). Any refactoring must not push
  production code further past this boundary; if Option B structural split is chosen,
  consider splitting Informs-only logic into a submodule.
- **No domain string literals in production code (C-12).** Category pair strings must
  come from config only. This constraint is pre-existing and must be respected in any new
  code paths.
- **`nli_informs_cosine_floor` range validation** in `InferenceConfig::validate()` is
  `(0.0, 1.0)` exclusive. Raising the default to 0.5 is within valid range; no validation
  change needed.

## Design Decisions (Resolved)

| # | Decision | Resolution |
|---|----------|------------|
| D-01 | Option A vs B for Phase 7 decoupling | **Option B — full structural separation.** Option A doesn't unblock Phase 4b in production (default `nli_enabled=false`). Roadmap requirement: "always runs regardless of NLI availability." |
| D-02 | Guards 4 and 5 in `apply_informs_composite_guard` after Option B | **Remove both.** Mutual-exclusion concern is handled by candidate set separation (Phase 4 vs Phase 4b), not score comparison. `nli_scores` parameter removed entirely. |
| D-03 | Rayon pool floor when `nli_enabled=false` | **Keep at 4.** Phase 4b is pure structural (HNSW + DB reads), no rayon pool usage. Pool floor of 6 is justified only by NLI CPU-bound inference. |
| D-04 | Module rename (`nli_detection_tick.rs`) | **Deferred to Group 3.** Phase 8 (Supports) still uses NLI inside this file. Clean rename moment is after cosine Supports replacement removes NLI from Phase 8 entirely. Update module-level doc comment to describe dual nature and note deferred rename. |

## Open Questions

- None. All design questions resolved above.

## Tracking

https://github.com/dug-21/unimatrix/issues/485
