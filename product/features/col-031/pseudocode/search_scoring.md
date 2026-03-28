# col-031: search.rs Scoring Wire-Up — Pseudocode

File: `crates/unimatrix-server/src/services/search.rs`
Status: MODIFIED

---

## Purpose

Wire `PhaseFreqTableHandle` into `SearchService` and integrate the
phase affinity signal into the fused scoring loop. The key invariants are:

1. Lock acquired once before the scoring loop — released before the first iteration.
2. `use_fallback` guard fires BEFORE `phase_affinity_score` is called (ADR-003).
3. `current_phase = None` → lock never acquired; `phase_explicit_norm = 0.0`.
4. Cold-start scores are bit-for-bit identical to pre-col-031 (NFR-04).

---

## Change 1: Import

Add to existing imports at top of file:

```
use crate::services::phase_freq_table::PhaseFreqTableHandle;
```

---

## Change 2: `ServiceSearchParams` — New Field

Add `current_phase` to the `ServiceSearchParams` struct.
Insert after the `category_histogram` field (to follow the WA-2 fields pattern):

```
/// col-031: Workflow phase at query time, used for phase-conditioned scoring.
///
/// Set by MCP transport from tool call `current_phase` parameter.
/// Set by eval runner from `record.context.phase` (AC-16).
///
/// When None:
///   - Lock on PhaseFreqTableHandle is never acquired.
///   - phase_explicit_norm = 0.0 for all candidates.
///   - Fused score is bit-for-bit identical to pre-col-031 (NFR-04).
///
/// When Some(phase) and use_fallback = true:
///   - use_fallback guard fires; phase_explicit_norm = 0.0.
///   - phase_affinity_score is NOT called (ADR-003, R-03).
pub current_phase: Option<String>,
```

All existing `ServiceSearchParams` construction sites must add:
```
current_phase: None,
```
These sites are: `replay.rs`, test helpers in `server.rs`, `shutdown.rs`,
`test_support.rs`, `listener.rs`, `eval/profile/layer.rs`, and any tool
call sites in MCP handlers. Adding `current_phase: None` is a no-op for all
non-eval paths. Only `replay.rs` sets a non-None value (via AC-16 fix).

---

## Change 3: `SearchService` Struct — New Field

Add `phase_freq_table` field to the `SearchService` struct.
Insert after the `fusion_weights` field:

```
/// col-031: phase-conditioned frequency table for phase_explicit_norm signal.
///
/// Arc clone received from ServiceLayer (created once in with_rate_config).
/// Background tick is sole writer. Search path acquires short read lock,
/// extracts phase snapshot, releases before scoring loop (NFR-02).
/// Non-optional — missing wiring is a compile error (ADR-005).
phase_freq_table: PhaseFreqTableHandle,
```

---

## Change 4: `SearchService::new()` — New Parameter

Add `phase_freq_table: PhaseFreqTableHandle` as the last parameter:

```
pub(crate) fn new(
    store: Arc<Store>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    gateway: Arc<SecurityGateway>,
    confidence_state: ConfidenceStateHandle,
    effectiveness_state: EffectivenessStateHandle,
    typed_graph_handle: TypedGraphStateHandle,
    boosted_categories: HashSet<String>,
    rayon_pool: Arc<RayonPool>,
    nli_handle: Arc<NliServiceHandle>,
    nli_top_k: usize,
    nli_enabled: bool,
    fusion_weights: FusionWeights,
    phase_freq_table: PhaseFreqTableHandle,   // col-031: required, non-optional (ADR-005)
) -> Self {
    SearchService {
        store,
        vector_store,
        entry_store,
        embed_service,
        adapt_service,
        gateway,
        confidence_state,
        effectiveness_state,
        cached_snapshot: EffectivenessSnapshot::new_shared(),
        typed_graph_handle,
        boosted_categories,
        rayon_pool,
        nli_handle,
        nli_top_k,
        nli_enabled,
        fusion_weights,
        phase_freq_table,   // col-031
    }
}
```

---

## Change 5: `search()` Method — Pre-Loop Phase Snapshot

This is the critical change. Insert the phase snapshot extraction block
AFTER the co-access boost map prefetch (step 6c) and BEFORE the NLI scoring
call (step 7). The lock must be released before the scoring loop begins.

The insertion point is immediately after `boost_map` is populated and before
the line: `let nli_scores: Option<Vec<NliScores>> = ...`

```
// col-031: Pre-loop phase snapshot extraction (ADR-003, NFR-02).
//
// LOCK ORDER CONTEXT: At this point, EffectivenessStateHandle read lock has
// already been acquired and released (step above). TypedGraphStateHandle read
// lock has been acquired and released (step 609-624 above). Now we acquire
// PhaseFreqTableHandle read lock — this is the third in the chain.
//
// Lock acquired once before the scoring loop. Lock MUST be released before
// the loop body executes (NFR-02: no lock held across scoring loop).
//
// Three cases:
//   1. current_phase = None:
//      -> phase_snapshot = None; lock never acquired.
//   2. current_phase = Some(phase) AND use_fallback = true:
//      -> Guard fires; phase_snapshot = None; phase_explicit_norm = 0.0 for all.
//      -> Scores bit-for-bit identical to pre-col-031 (NFR-04).
//      -> phase_affinity_score is NOT called (ADR-003 fused-scoring contract).
//   3. current_phase = Some(phase) AND use_fallback = false:
//      -> Clone the phase's bucket data out of the guard. Release lock.
//      -> phase_explicit_norm computed per-entry from cloned snapshot.
//
// Snapshot type: HashMap<String, Vec<(u64, f32)>>
//   key = entry_category, value = sorted (entry_id, rank_score) pairs for this phase
let phase_snapshot: Option<HashMap<String, Vec<(u64, f32)>>> =
    match &params.current_phase {
        None => None,  // lock never acquired
        Some(phase) => {
            // Acquire read lock once
            let guard = self
                .phase_freq_table
                .read()
                .unwrap_or_else(|e| e.into_inner());

            if guard.use_fallback {
                // GUARD FIRES: cold-start; do NOT call phase_affinity_score.
                // phase_explicit_norm = 0.0 for all candidates (score identity).
                None
                // guard drops here — lock released
            } else {
                // Extract all (category -> Vec<(entry_id, score)>) entries for
                // this specific phase. Clone out before dropping the guard.
                //
                // We need all categories for this phase because the scoring loop
                // iterates over diverse entries with different categories.
                let snapshot: HashMap<String, Vec<(u64, f32)>> = guard
                    .table
                    .iter()
                    .filter(|((p, _cat), _)| p == phase)
                    .map(|((_p, cat), bucket)| (cat.clone(), bucket.clone()))
                    .collect();
                Some(snapshot)
                // guard drops here — lock released BEFORE scoring loop
            }
        }
    };
// PhaseFreqTableHandle read lock is now released. Scoring loop may begin.
```

---

## Change 6: Scoring Loop — `phase_explicit_norm` Assignment

Replace the existing line:

```rust
// crt-026: ADR-003 placeholder — always 0.0 in crt-026; W3-1 will populate this field
phase_explicit_norm: 0.0,
```

With:

```
// col-031: phase_explicit_norm from pre-built snapshot (no lock in loop body).
//
// phase_snapshot = None when:
//   - current_phase is None (no phase provided)
//   - use_fallback = true (cold-start; guard fired pre-loop)
//   In both cases: 0.0 (pre-col-031 score identity preserved, NFR-04).
//
// phase_snapshot = Some(snapshot) when:
//   - Phase history exists and use_fallback = false.
//   Lookup entry_id in the snapshot's category bucket:
//     - Entry found: return its rank_score as f64.
//     - Bucket absent or entry absent: 1.0 (neutral, no suppression).
//   NOTE: 1.0 neutral return from snapshot is consistent with
//   phase_affinity_score absent-entry contract.
let phase_explicit_norm: f64 = match &phase_snapshot {
    None => 0.0,
    Some(snapshot) => {
        // Look up this entry's category bucket in the phase snapshot.
        // snapshot is HashMap<category, Vec<(entry_id, score)>>.
        match snapshot.get(&entry.category) {
            None => 1.0,  // no history for (phase, category) -> neutral
            Some(bucket) => {
                // Linear scan within bucket for this entry_id.
                // Buckets are small; linear scan is appropriate.
                match bucket.iter().find(|(id, _)| *id == entry.id) {
                    Some((_, score)) => *score as f64,
                    None => 1.0,  // entry not in bucket -> neutral
                }
            }
        }
    }
};
```

The `FusedScoreInputs` construction then becomes:

```
let inputs = FusedScoreInputs {
    similarity: *sim,
    nli_entailment,
    confidence: entry.confidence,
    coac_norm,
    util_norm,
    prov_norm,
    phase_histogram_norm,     // crt-026: unchanged
    phase_explicit_norm,      // col-031: now populated from phase snapshot
};
```

---

## Wiring Site Checklist (ADR-005)

These `ServiceSearchParams` construction sites must add `current_phase: None`:

| Site | File | Action |
|------|------|--------|
| `run_single_profile` | `eval/runner/replay.rs` | Set `current_phase: record.context.phase.clone()` (AC-16 fix — see replay_fix.md) |
| Test helper | `tests/server.rs` (or similar) | Add `current_phase: None` |
| Test helper | `tests/shutdown.rs` | Add `current_phase: None` |
| Test helper | `tests/test_support.rs` | Add `current_phase: None` |
| Test helper | `tests/listener.rs` | Add `current_phase: None` |
| Eval profile | `eval/profile/layer.rs` | Add `current_phase: None` |
| MCP handler | Any `context_search` tool handler | Add `current_phase: params.current_phase` (forwarded from tool call) |

All sites not in scope for col-031 get `current_phase: None`. The eval site gets
the non-None value (replay_fix.md).

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Lock poisoned on PhaseFreqTableHandle | `.unwrap_or_else(|e| e.into_inner())` recovers; may see stale data but no panic |
| `use_fallback = true` at query time | Guard fires pre-loop; `phase_explicit_norm = 0.0`; scores unchanged from pre-col-031 |
| `current_phase = None` | Lock never acquired; `phase_explicit_norm = 0.0` |
| `entry_id` absent from snapshot bucket | Returns 1.0 (neutral); no suppression |
| Snapshot clone fails (OOM) | Would panic; not expected given small bucket sizes |

---

## Key Test Scenarios

### AC-11 Test 1: `current_phase = None`, populated table -> 0.0

```
// Construct SearchService with populated PhaseFreqTableHandle (use_fallback=false).
// Call search with current_phase = None.
// Assert: phase_explicit_norm = 0.0 for all candidates.
// Assert: final scores bit-for-bit identical to scores with w_phase_explicit=0.0.
```

### AC-11 Test 2: `current_phase = Some(...)`, `use_fallback = true` -> guard fires

```
// Construct SearchService with cold-start PhaseFreqTableHandle (use_fallback=true).
// Call search with current_phase = Some("delivery").
// Assert: phase_explicit_norm = 0.0 for all candidates (guard fired).
// Assert: phase_affinity_score was NOT called (verify via code inspection or spy).
// Assert: scores bit-for-bit identical to pre-col-031 baseline.
```

### AC-06: Lock released before scoring loop

```
// Code review: confirm PhaseFreqTableHandle.read() call site is before the
// `for (i, (entry, sim)) in results_with_scores.iter().enumerate()` loop.
// Confirm guard is dropped (goes out of scope) before the loop begins.
// No lock held across the loop body.
```

### R-06: Lock not held per-entry

```
// Concurrent test: start write task holding write lock for 100ms.
// Concurrently call SearchService::search with current_phase = Some("delivery").
// Assert search completes without blocking on write lock during scoring.
```
