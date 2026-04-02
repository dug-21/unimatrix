# nli_detection_tick.rs — Tick Implementation Pseudocode
# crt-039: Option Z control-flow split; remove dead enum variants; simplify guard

## Purpose

`nli_detection_tick.rs` owns `run_graph_inference_tick` and all its phases. After crt-039:

- Path A (structural Informs, Phase 4b + 8b): runs unconditionally — no NLI model required.
- Path B (NLI Supports, Phase 6/7/8): conditionally executes via `get_provider()` — returns
  early on Err without any writes.

The function signature is unchanged. All changes are internal.

## Module-Level Doc Comment Change (FR-12)

Replace the existing module-level comment with:

```
//! Background graph inference tick — dual-path architecture (crt-039).
//!
//! # Path A: Structural Informs (Phase 4b)
//! Pure HNSW cosine scan. No NLI cross-encoder. Runs unconditionally on every tick.
//! Writes `Informs` edges directly from cosine similarity + category pair filter.
//! Gated by: cosine >= nli_informs_cosine_floor (0.50 default), informs_category_pairs
//! config, temporal ordering, cross-feature guard.
//!
//! # Path B: NLI Supports (Phase 8)
//! Requires NLI cross-encoder. Gated by get_provider() — skipped entirely if Err.
//! Phase 7 runs the NLI batch via rayon_pool.spawn() (W1-2 contract).
//! Writes `Supports` edges (entailment > threshold).
//!
//! # Module Rename Deferred
//! This module is named `nli_detection_tick` but now hosts structural-only inference
//! as its primary path. Module rename to `graph_inference_tick` is deferred to Group 3
//! when NLI is fully removed from Phase 8.
//!
//! # W1-2 Contract
//! ALL `CrossEncoderProvider::score_batch` calls via `rayon_pool.spawn()`.
//! `spawn_blocking` prohibited. Inline async NLI prohibited.
//! Phase 4b MUST NOT call score_batch.
//!
//! # R-09 Rayon/Tokio Boundary (C-14)
//! The rayon closure in Phase 7 MUST be synchronous CPU-bound only.
//! PROHIBITED: `tokio::runtime::Handle::current()`, `.await`, any async call.
```

## Type Changes

### Remove: `NliCandidatePair::Informs` variant

The `Informs` variant of `NliCandidatePair` is dead code after Option Z. Path A writes
Informs edges directly from `informs_metadata: Vec<InformsCandidate>`. The NLI batch
(Phase 7) only processes Supports candidates.

```
// BEFORE:
enum NliCandidatePair {
    SupportsContradict { source_id: u64, target_id: u64, cosine: f32, nli_scores: NliScores },
    Informs { candidate: InformsCandidate, nli_scores: NliScores },
}

// AFTER:
enum NliCandidatePair {
    SupportsContradict { source_id: u64, target_id: u64, cosine: f32, nli_scores: NliScores },
    // Informs variant removed (crt-039 ADR-001). Path A writes Informs edges directly.
}
```

R-04 mitigation: every match site on `NliCandidatePair` in Phase 8 must be updated.
After removal, Phase 8 iterates `merged_pairs: Vec<NliCandidatePair>` which now contains
only `SupportsContradict` variants — no match arm for `Informs` exists or is needed.
The `for pair in &merged_pairs` loop in Phase 8 uses `if let NliCandidatePair::SupportsContradict { ... } = pair`
— this pattern is exhaustive and no wildcard arm is needed.

### Remove: `PairOrigin::Informs` variant

```
// BEFORE:
enum PairOrigin {
    SupportsContradict { source_id: u64, target_id: u64, cosine: f32 },
    Informs(InformsCandidate),
}

// AFTER:
enum PairOrigin {
    SupportsContradict { source_id: u64, target_id: u64, cosine: f32 },
    // Informs variant removed (crt-039 ADR-001). informs_metadata is separate Vec.
}
```

Phase 6 after removal iterates only `candidate_pairs` (Supports) to build `scored_input`
and `pair_origins`. The `PairOrigin::Informs` push that previously happened in Phase 6
for Informs candidates is deleted entirely.

### Unchanged: `InformsCandidate` struct (all 9 fields)

```
struct InformsCandidate {
    source_id: u64,
    target_id: u64,
    cosine: f32,
    source_created_at: i64,
    target_created_at: i64,
    source_feature_cycle: String,
    target_feature_cycle: String,
    source_category: String,
    target_category: String,
}
```

## New Function: `format_informs_metadata`

Replaces `format_nli_metadata_informs`. No NLI score fields.

```
/// Serialize structural metadata to JSON for Informs edges (crt-039).
///
/// Records cosine similarity and category pair that qualified this edge.
/// No NLI score fields — Informs edges are written via structural path only.
fn format_informs_metadata(cosine: f32, source_category: &str, target_category: &str) -> String {
    serde_json::json!({
        "cosine":           cosine,
        "source_category":  source_category,
        "target_category":  target_category,
    })
    .to_string()
}
```

Delete `format_nli_metadata_informs` entirely (R-08 mitigation).

## Modified Function: `apply_informs_composite_guard`

### Before (5 guards, 3 params)

```
fn apply_informs_composite_guard(
    nli_scores: &NliScores,
    candidate: &InformsCandidate,
    config: &InferenceConfig,
) -> bool {
    nli_scores.neutral > 0.5                                              // guard 1 — NLI neutral
    && candidate.source_created_at < candidate.target_created_at         // guard 2 — temporal
    && (cross-feature check)                                              // guard 3 — feature cycle
    && nli_scores.entailment <= config.supports_edge_threshold            // guard 4 — mutual exclusion
    && nli_scores.contradiction <= config.nli_contradiction_threshold     // guard 5 — mutual exclusion
}
```

### After (2 guards, 1 param — ADR-002)

```
/// Evaluate composite guard predicates for a candidate Informs edge (crt-039, ADR-002).
///
/// Returns `true` only when BOTH guards pass:
///
/// Guard 2 (temporal): source entry must have been created before target entry.
/// Guard 3 (cross-feature): block only when both feature_cycle fields are non-empty AND equal
///   (intra-feature pairs blocked; empty feature_cycle means unknown provenance, allowed through).
///
/// Guards 1, 4, 5 removed (crt-039 ADR-002):
///   Guard 1 (nli neutral) — NLI model not available on this path.
///   Guards 4, 5 (mutual exclusion via NLI scores) — enforced by candidate set separation
///   at Phase 4b via explicit Supports-set subtraction (FR-06, AC-13).
///
/// Module-private. Accessible to the `tests` sub-module via `use super::*`.
fn apply_informs_composite_guard(candidate: &InformsCandidate) -> bool {
    candidate.source_created_at < candidate.target_created_at
        && (candidate.source_feature_cycle.is_empty()
            || candidate.target_feature_cycle.is_empty()
            || candidate.source_feature_cycle != candidate.target_feature_cycle)
}
```

All call sites of `apply_informs_composite_guard` must pass exactly one argument (`candidate`).
Pre-merge grep: `grep -n 'apply_informs_composite_guard' nli_detection_tick.rs` — every
occurrence must show a single-argument call.

## Modified Function: `run_graph_inference_tick`

### Phase 1 — Removed

The existing Phase 1 block is deleted entirely:

```
// DELETED:
// Phase 1 — Guard: silent no-op when NLI not ready
// let provider = match nli_handle.get_provider().await {
//     Ok(p) => p,
//     Err(_) => return,
// };
```

`provider` is no longer bound at function entry. It is bound inside Path B entry.

### Phase 2 — Unchanged (DB reads)

Reads `all_active`, `isolated_ids`, `existing_supports_pairs`, `existing_informs_pairs`.
All four reads happen unconditionally before any path branching. No changes.

### Phase 3 — Unchanged (Source candidate selection)

`select_source_candidates(...)` call unchanged.

### Phase 4 — Unchanged (HNSW Supports candidates)

HNSW expansion for Supports. Produces `candidate_pairs: Vec<(u64, u64, f32)>`. No changes.

Note: Phase 4 uses `similarity > config.supports_candidate_threshold` (strict greater-than).
A pair at exactly 0.50 is EXCLUDED from Phase 4.

### Phase 4b — Modified: explicit Supports-set subtraction required (R-03, FR-06, AC-13)

The existing Phase 4b code already produces `informs_metadata: Vec<InformsCandidate>` via
HNSW scan over `source_candidates`. The modification is to add an explicit subtraction of
Phase 4 `candidate_pairs` from the Informs candidate set.

Architecture note: ARCHITECTURE.md initially claimed disjoint-by-construction, but spec
(FR-06, AC-13, R-03) requires explicit subtraction. The implementation must NOT rely on
threshold arithmetic alone.

```
// Phase 4b — HNSW Informs candidates (crt-037, structural path)
//
// Uses cosine >= nli_informs_cosine_floor (0.50 default — inclusive floor, >= semantics).
// A pair at exactly cosine 0.500 is INCLUDED (>= semantics). A pair at 0.499 is EXCLUDED.
// Domain vocabulary must not appear here — category strings from config only (C-07/C-12).
//
// Dedup ordering: existing_informs_pairs check happens INSIDE this loop (Phase 2 pre-filter,
// before Phase 5 cap). This satisfies the dedup-before-cap invariant (C-04, FR-09).
//
// R-03 / AC-13: After collecting informs_metadata, EXPLICITLY subtract the Phase 4
// candidate_pairs from the Informs set. Do not rely solely on threshold arithmetic.
// The overlap scenario: a pair at cosine exactly 0.50 passes Phase 4b (>= 0.50) and
// is EXCLUDED from Phase 4 (strict > 0.50). However, when supports_candidate_threshold
// is configured lower than 0.50, overlap could occur. Explicit subtraction is always safe.

// Build lookup set from Phase 4 Supports candidates for O(1) subtraction.
// Use directional (source_id, target_id) key to match Phase 4b candidate orientation.
// Phase 4 uses (min, max) symmetric dedup; Phase 4b uses directional.
// Check both directions to be safe.
let supports_candidate_set: HashSet<(u64, u64)> = candidate_pairs
    .iter()
    .flat_map(|(src, tgt, _)| [(*src, *tgt), (*tgt, *src)])
    .collect();

// ... [existing Phase 4b HNSW loop — unchanged: source-category pre-filter,
//      embedding fetch, HNSW search, phase4b_candidate_passes_guards,
//      existing_informs_pairs dedup, seen_informs_pairs dedup,
//      InformsCandidate construction] ...

// After loop: explicit Supports-set subtraction (R-03, FR-06, AC-13).
// Remove any informs_metadata candidate whose (source_id, target_id) appears in
// Phase 4 candidate_pairs (in either direction).
informs_metadata.retain(|c| {
    !supports_candidate_set.contains(&(c.source_id, c.target_id))
        && !supports_candidate_set.contains(&(c.target_id, c.source_id))
});
```

Note: the observability log for Phase 4b (FR-14, AC-17) must record counts at the correct
pipeline stages — see Phase 4b observability section below.

### Phase 4b Observability Log (AC-17 — REQUIRED)

A `tracing::debug!` call is emitted AFTER the Phase 8b write loop completes, when all four
values are fully known. This is the canonical placement (see function skeleton). The log
must be emitted even when `informs_edges_written = 0`.

```
// Count raw candidates before dedup for informs_candidates_found.
// This counter must be incremented INSIDE the Phase 4b loop, before dedup checks.
let mut informs_candidates_found: usize = 0;
// ... (increment inside loop at each candidate that passes phase4b_candidate_passes_guards,
//      before the existing_informs_pairs and seen_informs_pairs dedup checks) ...

// After dedup (including Supports-set subtraction), before Phase 5 truncation:
let informs_candidates_after_dedup = informs_metadata.len();

// Phase 5 truncation runs here (see below).
informs_metadata.truncate(MAX_INFORMS_PER_TICK);
let informs_candidates_after_cap = informs_metadata.len();

// informs_edges_written is counted during Phase 8b write loop (below).
// The observability log is emitted AFTER Phase 8b completes — all four values known.

tracing::debug!(
    informs_candidates_found,
    informs_candidates_after_dedup,
    informs_candidates_after_cap,
    informs_edges_written,    // available after Phase 8b write loop
    "graph inference tick Phase 4b: Informs candidate pipeline"
);
```

R-10 mitigation: `informs_candidates_found` must be the raw count BEFORE any dedup check,
so the four values are independently observable. They must not all collapse to the same
number. The log must be emitted even when `informs_edges_written = 0` (e.g., all candidates
deduped or all failing composite guard) — no early-return before this log.

Placement decision: emit the log after Phase 8b completes (all four values known),
at the end of Path A. If Path B exits early (empty candidate_pairs), the log still fires
because Phase 8b runs before Path B entry.

### Phase 5 — Modified: remove merged early-return; Informs-only section

The existing Phase 5 has this early-return:

```
// EXISTING (remove this):
if candidate_pairs.is_empty() && informs_metadata.is_empty() {
    tracing::debug!("graph inference tick: no candidate pairs after HNSW expansion");
    return;
}
```

This early-return must be removed. After Option Z, `informs_metadata` being non-empty is
sufficient reason to continue to Path A. The early-return only makes sense if both are
empty.

Revised Phase 5 structure:

```
// Phase 5 — Independent caps.
// Supports: sort by priority criteria, truncate to max_graph_inference_per_tick.
// Informs: shuffle, truncate to MAX_INFORMS_PER_TICK.
// Informs budget is independent of Supports (bugfix-473).

// Supports cap (unchanged sort + truncate logic).
candidate_pairs.sort_by(...)
candidate_pairs.truncate(config.max_graph_inference_per_tick);

// Informs cap.
// Record informs_candidates_after_dedup BEFORE shuffle + truncate.
let informs_candidates_after_dedup = informs_metadata.len();
{
    use rand::seq::SliceRandom;
    let mut rng = rand::rng();
    informs_metadata.shuffle(&mut rng);
}
informs_metadata.truncate(MAX_INFORMS_PER_TICK);
let informs_candidates_after_cap = informs_metadata.len();

// If both are empty after caps, log and return now.
// This is the ONLY early return allowed at this stage.
if candidate_pairs.is_empty() && informs_metadata.is_empty() {
    tracing::debug!("graph inference tick: no candidates after HNSW expansion and caps");
    return;
}
```

### Path A — Informs Write Loop (Phase 8b)

Phase 8b is placed BEFORE the Path B entry gate. It runs unconditionally after Phase 5.

```
// === PATH A: Structural Informs write loop ===
// Runs unconditionally. No NLI provider required. No rayon pool usage (NFR-01).
// Hard cap already applied in Phase 5 — write all candidates passing composite guard.
let mut informs_edges_written: usize = 0;
let timestamp = current_timestamp_secs();

for candidate in &informs_metadata {
    // Defense-in-depth: re-evaluate temporal and cross-feature guards at write time.
    // These were already checked in Phase 4b via phase4b_candidate_passes_guards;
    // this re-evaluation catches any future code path that bypasses Phase 4b.
    // (ADR-002: cheap and structurally sound; not redundant at this cost.)
    if !apply_informs_composite_guard(candidate) {
        continue;
    }

    let weight = candidate.cosine * config.nli_informs_ppr_weight;
    if !weight.is_finite() {
        continue;  // guard against NaN/Inf from cosine * weight product
    }

    let metadata_json = format_informs_metadata(
        candidate.cosine,
        &candidate.source_category,
        &candidate.target_category,
    );

    let written = write_nli_edge(
        store,
        candidate.source_id,
        candidate.target_id,
        "Informs",      // must match RelationType::Informs.as_str() exactly
        weight,
        timestamp,
        &metadata_json,
    )
    .await;

    if written {
        informs_edges_written += 1;
    }
    // Non-fatal: write failure for one candidate does not abort the loop.
    // write_nli_edge handles logging internally.
}

// Emit Phase 4b observability log (AC-17, FR-14).
// All four values are now known.
tracing::debug!(
    informs_candidates_found,
    informs_candidates_after_dedup,
    informs_candidates_after_cap,
    informs_edges_written,
    "graph inference tick Phase 4b: Informs candidate pipeline"
);
```

### Path B Entry Gate (R-01)

Path B entry immediately follows the Path A write loop. The `get_provider()` call is
the SOLE entry point to Phase 6/7/8. A conditional `return` on `Err` is structurally
guaranteed to precede any Phase 8 write.

```
// === PATH B entry gate ===
// Informs writes (Path A) are complete above. Path B gates NLI Supports only.

// Fast exit: no Supports candidates — skip NLI batch entirely.
if candidate_pairs.is_empty() {
    tracing::debug!("graph inference tick: no Supports candidates; skipping NLI batch");
    return;
}

// R-01 CRITICAL: get_provider() is the SOLE entry point to Phase 6/7/8.
// Err return here structurally prevents ANY Phase 8 write without a successful provider.
// No code path from get_provider() Err to write_nli_edge for Supports edges exists.
let provider = match nli_handle.get_provider().await {
    Ok(p) => p,
    Err(_) => {
        // Expected behavior when nli_enabled=false (production default).
        // Phase 8b Informs writes already complete above — returning here is not a failure.
        tracing::debug!("graph inference tick: NLI provider not ready; Supports path skipped");
        return;
    }
};
```

### Phase 6 — Modified: Supports only (R-04 mitigation)

After removing `PairOrigin::Informs`, Phase 6 iterates only `candidate_pairs`.
The entire "Fetch Informs pairs" sub-block is deleted.

```
// Phase 6 — Text fetch for Supports candidates only.
// PairOrigin::Informs removed (crt-039 ADR-001) — Informs text not needed (no NLI batch).
let mut scored_input: Vec<(u64, u64, String, String)> = Vec::new();
let mut pair_origins: Vec<PairOrigin> = Vec::new();

// Fetch Supports pairs only.
for (source_id, target_id, cosine) in &candidate_pairs {
    let source_text = match store.get_content_via_write_pool(*source_id).await {
        Ok(text) => text,
        Err(e) => {
            tracing::debug!(entry_id = source_id, error = %e, "graph inference tick: source content fetch failed");
            continue;
        }
    };
    let target_text = match store.get_content_via_write_pool(*target_id).await {
        Ok(text) => text,
        Err(e) => {
            tracing::debug!(entry_id = target_id, error = %e, "graph inference tick: target content fetch failed");
            continue;
        }
    };
    scored_input.push((*source_id, *target_id, source_text, target_text));
    pair_origins.push(PairOrigin::SupportsContradict {
        source_id: *source_id,
        target_id: *target_id,
        cosine: *cosine,
    });
}

if scored_input.is_empty() {
    tracing::debug!("graph inference tick: no Supports pairs with fetchable content");
    return;
}
```

R-04 post-removal check: Phase 6 `pair_origins` now contains only `SupportsContradict`
variants. The `.map(|(origin, scores)| match origin { ... })` in Phase 7 has only one arm:
`PairOrigin::SupportsContradict { ... } => NliCandidatePair::SupportsContradict { ... }`.
No `PairOrigin::Informs` arm. No wildcard arm. The match is exhaustive by construction.

### Phase 7 — Unchanged (W1-2 contract preserved)

Single rayon spawn for `score_batch`. W1-2 contract preserved. Closure body sync-only.
The closure now scores only Supports pairs (fewer items in `nli_pairs` than before).
No structural changes to Phase 7 code.

```
// Phase 7 — W1-2 rayon dispatch (Supports only after crt-039).
// W1-2: ALL score_batch calls via rayon_pool.spawn(). No spawn_blocking. No inline async NLI.
// Phase 4b MUST NOT call score_batch — it runs in Path A above (NFR-01, C-02).
let nli_pairs: Vec<(String, String)> = scored_input.iter().map(|(_, _, s, t)| (s.clone(), t.clone())).collect();
let provider_clone = Arc::clone(&provider);
let nli_result = rayon_pool.spawn(move || {
    // SYNC-ONLY CLOSURE — no .await, no Handle::current()
    let pairs_ref: Vec<(&str, &str)> = nli_pairs.iter().map(|(q, p)| (q.as_str(), p.as_str())).collect();
    provider_clone.score_batch(&pairs_ref)
}).await;
```

### Phase 8 — Unchanged semantics; `merged_pairs` contains SupportsContradict only

After `PairOrigin::Informs` is removed, `pair_origins` never contains an Informs variant.
The `merged_pairs: Vec<NliCandidatePair>` therefore contains only `SupportsContradict` items.
The Phase 8 write loop `if let NliCandidatePair::SupportsContradict { ... } = pair { ... }`
pattern is exhaustive. The ordering-invariant comment about `SupportsContradict` appearing
before `Informs` in `merged_pairs` is no longer relevant and must be removed (it referenced
the old Phase 6 ordering where Informs pairs were appended after Supports).

Phase 8 write loop body is functionally unchanged — same entailment threshold check, same
`write_nli_edge` call with "Supports" relation type.

## Complete Function Skeleton (Option Z)

```
pub async fn run_graph_inference_tick(
    store: &Store,
    nli_handle: &NliServiceHandle,
    vector_index: &VectorIndex,
    rayon_pool: &RayonPool,
    config: &InferenceConfig,
) {
    // Phase 1 removed (crt-039 ADR-001). get_provider() moved to Path B entry below.

    // Phase 2 — DB reads (active entries, isolated, supports pairs, informs pairs) [unchanged]
    let all_active = ...;
    let isolated_ids = ...;
    let existing_supports_pairs = ...;
    let existing_informs_pairs = ...;

    // Phase 3 — Source candidate selection [unchanged]
    let source_candidates = select_source_candidates(...);
    if source_candidates.is_empty() { return; }

    // Phase 4 — HNSW Supports expansion (cosine > supports_candidate_threshold) [unchanged]
    let mut candidate_pairs: Vec<(u64, u64, f32)> = ...;

    // Phase 4b — HNSW Informs expansion (cosine >= nli_informs_cosine_floor)
    let mut informs_candidates_found: usize = 0;
    let mut informs_metadata: Vec<InformsCandidate> = ...;
    // [existing Phase 4b loop — unchanged except informs_candidates_found increment
    //  before dedup checks, and Supports-set subtraction after loop]
    let supports_candidate_set: HashSet<(u64, u64)> = ...; // built from candidate_pairs
    informs_metadata.retain(|c| !supports_candidate_set.contains(...));

    // Phase 5 — Independent caps
    let informs_candidates_after_dedup = informs_metadata.len();
    informs_metadata.shuffle(...);
    informs_metadata.truncate(MAX_INFORMS_PER_TICK);
    let informs_candidates_after_cap = informs_metadata.len();
    candidate_pairs.sort_by(...);
    candidate_pairs.truncate(config.max_graph_inference_per_tick);

    if candidate_pairs.is_empty() && informs_metadata.is_empty() { return; }

    // === PATH A: Structural Informs write loop (unconditional) ===
    let mut informs_edges_written: usize = 0;
    let timestamp = current_timestamp_secs();
    for candidate in &informs_metadata {
        if !apply_informs_composite_guard(candidate) { continue; }
        let weight = candidate.cosine * config.nli_informs_ppr_weight;
        if !weight.is_finite() { continue; }
        let metadata_json = format_informs_metadata(candidate.cosine, &candidate.source_category, &candidate.target_category);
        let written = write_nli_edge(store, candidate.source_id, candidate.target_id, "Informs", weight, timestamp, &metadata_json).await;
        if written { informs_edges_written += 1; }
    }

    // Observability log (AC-17, FR-14) — all four values known here.
    tracing::debug!(informs_candidates_found, informs_candidates_after_dedup, informs_candidates_after_cap, informs_edges_written, "graph inference tick Phase 4b: Informs candidate pipeline");

    // === PATH B entry gate ===
    if candidate_pairs.is_empty() { return; }
    let provider = match nli_handle.get_provider().await {
        Ok(p) => p,
        Err(_) => return,  // R-01: no path from here to Phase 8 write
    };

    // Phase 6 — Text fetch (Supports candidates only)
    let mut scored_input = ...;
    let mut pair_origins: Vec<PairOrigin> = ...;  // SupportsContradict only

    // Phase 7 — W1-2 rayon NLI batch (Supports only)
    let nli_result = rayon_pool.spawn(...).await;

    // Phase 8 — Write Supports edges (entailment > threshold)
    let merged_pairs: Vec<NliCandidatePair> = ...;  // SupportsContradict only
    for pair in &merged_pairs {
        if let NliCandidatePair::SupportsContradict { source_id, target_id, cosine: _, nli_scores } = pair {
            if nli_scores.entailment > config.supports_edge_threshold {
                write_nli_edge(store, *source_id, *target_id, "Supports", ...).await;
            }
        }
    }
}
```

## Error Handling

| Situation | Handling |
|-----------|----------|
| `get_provider()` returns `Err` | Return after Path A completes — no Phase 8 writes (R-01) |
| Phase 2 DB read fails | Degraded mode: proceed with empty set (INSERT OR IGNORE backstop) |
| Phase 4b HNSW search fails for one source | Debug log, skip that source, continue loop |
| `write_nli_edge` fails for one candidate | Non-fatal, continue to next candidate |
| Phase 7 `score_batch` fails | Warn log, return — no Phase 8 writes |
| `weight.is_finite()` false | Skip candidate (guard against NaN/Inf) |
| Zero active entries | Return early after Phase 2 check |
| Zero source candidates | Return after Phase 3 |

## Key Test Scenarios

### Tests to Remove (TR)

| Test | Reason |
|------|--------|
| `test_run_graph_inference_tick_nli_not_ready_no_op` | Semantics invalidated — tick is no longer a no-op when NLI not ready. Replaced by TC-01 + TC-02. Pre-merge grep must return empty. |
| `test_phase8b_no_informs_when_neutral_exactly_0_5` | Neutral guard removed (ADR-002). |
| `test_phase8b_writes_informs_when_neutral_just_above_0_5` | Neutral guard removed. |

### Tests to Add (TC)

**TC-01**: `test_phase4b_writes_informs_when_nli_not_ready` (Integration)
- Setup: real Store with two active entries having embeddings and passing category pair filter.
  Vector index contains both entries at cosine >= 0.50. `NliServiceHandle` in Loading state.
  Configure `supports_candidate_threshold` above test pair cosine (ensure no Supports candidates).
- Execute: `run_graph_inference_tick` with NLI not ready.
- Assert: at least one Informs edge written in GRAPH_EDGES. Zero Supports edges.
- This also covers R-07 (Phase 8b runs even when candidate_pairs is empty).

**TC-02**: `test_phase8_no_supports_when_nli_not_ready` (Integration)
- Setup: real Store. Two entries with cosine above `supports_candidate_threshold`.
  `NliServiceHandle` in Loading state (same as TC-01).
- Execute: `run_graph_inference_tick`.
- Assert: zero Supports edges in GRAPH_EDGES. May contain Informs edges (don't assert absence).
  Specifically assert `score_batch` was NOT called.
- These are two separate tests — not combined (R-02 coverage requirement).

**TC-03**: `test_apply_informs_composite_guard_temporal_guard` (Unit)
- Assert false when `source_created_at >= target_created_at`.
- Assert true when `source_created_at < target_created_at` (all else equal).

**TC-04**: `test_apply_informs_composite_guard_cross_feature_guard` (Unit)
- Assert false when both feature_cycles non-empty AND equal.
- Assert true when source cycle is empty.
- Assert true when target cycle is empty.
- Assert true when both non-empty but different.

**TC-05**: `test_phase4b_cosine_floor_0500_included` (Unit)
- Cosine exactly 0.500 passes Phase 4b cosine guard (`>=` semantics).

**TC-06**: `test_phase4b_cosine_floor_0499_excluded` (Unit)
- Cosine 0.499 excluded by Phase 4b (below floor).

**TC-07**: `test_phase4b_excludes_supports_candidates` (Unit)
- Populate `candidate_pairs` with a pair at cosine 0.68 (above `supports_candidate_threshold`).
- Run Phase 4b with that pair also qualifying on cosine for Informs.
- Assert the pair is absent from `informs_metadata` after explicit subtraction.
- Boundary variant: pair at exactly 0.50 — NOT in candidate_pairs (strict > 0.50 excludes it
  from Phase 4); assert it IS in informs_metadata (>= 0.50 includes it in Phase 4b).

### Tests to Update

| Test | Change |
|------|--------|
| `test_inference_config_default_nli_informs_cosine_floor` | Assert `0.5_f32` (was `0.45`) |
| `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold` | Update band from `[0.45, 0.50)` to `[0.50, supports_threshold)`. Use cosine = 0.50 as inclusive floor. |
| All `apply_informs_composite_guard` call sites in tests | Remove `nli_scores` / `NliScores` argument |
