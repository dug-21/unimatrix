# crt-024: SearchService Pipeline Rewrite — Pseudocode

## Purpose

Rewrite the scoring phase of `SearchService::search()` in
`crates/unimatrix-server/src/services/search.rs` to:

1. Remove `apply_nli_sort` (ADR-002)
2. Change `try_nli_rerank` to return `Option<Vec<NliScores>>` (raw scores, no sort)
3. Add Step 6c: co-access boost_map prefetch (moved earlier in pipeline, fully awaited)
4. Replace Step 7 (NLI sort) and Step 8 (co-access re-sort) with a single fused scoring pass
5. Add `fusion_weights: FusionWeights` field to `SearchService`
6. Update `ScoredEntry.final_score` to use the fused formula
7. Migrate `apply_nli_sort` test coverage to new fused scorer tests

Steps 0–6b and 9–12 of the pipeline are unchanged.

---

## File: `crates/unimatrix-server/src/services/search.rs`

---

## Struct Field Addition: `SearchService.fusion_weights`

Add one field to `SearchService`:

```
pub(crate) struct SearchService {
    // ... all existing fields unchanged ...

    /// crt-024: config-driven fusion weights for the six-term scoring formula (ADR-003).
    /// Constructed from InferenceConfig.{w_sim, w_nli, w_conf, w_coac, w_util, w_prov}
    /// in SearchService::new. Stored here so EvalServiceLayer profile TOMLs can
    /// supply different weights per eval run (FR-14, AC-15).
    fusion_weights: FusionWeights,
}
```

---

## `SearchService::new` Change

Add the `fusion_weights` parameter construction. `InferenceConfig` must be available in `new`.

Two options exist for how to thread `InferenceConfig` into `SearchService::new`:

**Option A** (preferred — minimal signature change): Extract `FusionWeights` before calling
`SearchService::new`, pass it as an additional parameter. The caller (ServiceLayer /
EvalServiceLayer) constructs `FusionWeights::from_config(&inference_config)` and passes it in.

**Option B**: Pass the full `InferenceConfig` into `SearchService::new` and construct
`FusionWeights` inside. This is simpler but requires adding `InferenceConfig` to the signature.

The implementation agent should check how `SearchService::new` is called by `ServiceLayer`
and `EvalServiceLayer` and choose the option that requires fewer changes. The critical invariant
is FR-14 (AC-15): `EvalServiceLayer` must NOT use a default `FusionWeights` — it must construct
from the profile-specific `InferenceConfig`.

Regardless of option, add `fusion_weights` initialization to the struct literal:

```
SearchService {
    // ... existing fields ...
    fusion_weights,  // new
}
```

---

## `try_nli_rerank` — Return Type Change (ADR-002)

The existing function returns `Option<Vec<(EntryRecord, f64)>>` (sorted, truncated candidates).
Change it to return `Option<Vec<NliScores>>` (raw NLI scores, no sort, parallel to input order).

### New signature:

```
/// Attempt NLI scoring of `candidates`.
///
/// Returns `Some(nli_scores)` when scoring succeeded — one NliScores per candidate,
/// in the same index order as `candidates`. Does NOT sort. Does NOT truncate.
/// Caller runs the fused scoring pass using these scores alongside other signals.
///
/// Returns `None` on any failure (provider not ready, rayon timeout, inference error,
/// empty candidates, length mismatch). Caller uses nli_entailment=0.0 for all candidates
/// and calls FusionWeights::effective(nli_available: false).
///
/// W1-2 contract: ALL NLI inference is dispatched via `rayon_pool.spawn_with_timeout`.
/// Never inline in async context. Never via `spawn_blocking`.
async fn try_nli_rerank(
    candidates: &[(EntryRecord, f64)],
    query_text: &str,
    nli_handle: &NliServiceHandle,
    rayon_pool: &RayonPool,
) -> Option<Vec<NliScores>>
```

Note: `penalty_map` and `top_k` parameters are REMOVED — no longer needed since this function
no longer sorts or applies penalties.

### New body (logical pseudocode):

```
async fn try_nli_rerank(
    candidates: &[(EntryRecord, f64)],
    query_text: &str,
    nli_handle: &NliServiceHandle,
    rayon_pool: &RayonPool,
) -> Option<Vec<NliScores>> {
    // Fast check: get provider or return None for fallback.
    let provider = match nli_handle.get_provider().await {
        Ok(p) => p,
        Err(_) => {
            tracing::debug!("NLI provider not ready; NLI term will be 0.0");
            return None;
        }
    };

    if candidates.is_empty() {
        return None;
    }

    // Build owned strings for rayon closure (Send + 'static required).
    let query_owned = query_text.to_string();
    let passages: Vec<String> = candidates.iter()
        .map(|(entry, _)| entry.content.clone())
        .collect();

    // Dispatch to rayon pool with MCP_HANDLER_TIMEOUT (W1-2, FR-16).
    let nli_result = rayon_pool
        .spawn_with_timeout(MCP_HANDLER_TIMEOUT, move || {
            let pairs: Vec<(&str, &str)> = passages.iter()
                .map(|p| (query_owned.as_str(), p.as_str()))
                .collect();
            provider.score_batch(&pairs)
        })
        .await;

    let nli_scores: Vec<NliScores> = match nli_result {
        Ok(Ok(scores)) => scores,
        Ok(Err(e)) => {
            tracing::debug!(error = %e, "NLI score_batch error; NLI term will be 0.0");
            return None;
        }
        Err(e) => {
            tracing::debug!(error = %e, "NLI rayon task failed/timed out; NLI term will be 0.0");
            return None;
        }
    };

    // Length check: scores must be parallel to candidates (EC-07).
    if nli_scores.len() != candidates.len() {
        tracing::debug!(
            nli_len = nli_scores.len(),
            candidates_len = candidates.len(),
            "NLI scores length mismatch; NLI term will be 0.0"
        );
        return None;
    }

    // Return raw scores — no sort, no truncation. Caller handles all of that.
    Some(nli_scores)
}
```

---

## `apply_nli_sort` Removal (ADR-002)

The function `apply_nli_sort` is deleted entirely. Its `pub(crate)` annotation means any direct
test calls fail to compile — those test functions must be replaced by fused scorer tests (R-05).

Migration required:
- Delete the `apply_nli_sort` function body
- Delete all test functions that call `apply_nli_sort` directly
- Add replacement tests in the fused scorer test block (see Key Test Scenarios below)

---

## `SearchService::search()` Pipeline Changes

### Constants to import (additions to existing import block)

```
// New imports needed:
use unimatrix_engine::coaccess::MAX_CO_ACCESS_BOOST;
// Note: UTILITY_BOOST and UTILITY_PENALTY are already imported from effectiveness
// Note: PROVENANCE_BOOST is already used as a local const (re-alias not needed)
// Note: NliScores is already imported from unimatrix_embed
```

### Pre-loop: snapshot `nli_enabled` for the scoring pass

Before Steps 0–6b, capture the NLI availability flag. This flag is computed once before the
loop so that `FusionWeights::effective()` is called once, not per-candidate.

```
// Already exists — nli_enabled is a field on SearchService.
// No new snapshot needed for this; use self.nli_enabled throughout.
```

### Step 6c: Co-access boost_map prefetch (NEW, replaces old Step 8 position)

Move the entire `spawn_blocking_with_timeout` call for `compute_search_boost` from the old
Step 8 location to immediately after Step 6b (supersession injection). The call is fully
`.await`-ed before proceeding to Step 7.

This resolves SR-07. The `boost_map` result is available in the same task context for the
scoring loop.

```
// Step 6c: Co-access boost map prefetch.
//
// Fully await before the scoring pass begins (correctness constraint, not optimization).
// Scoring without co-access data would silently produce coac_norm=0.0 for all candidates.
// Moved earlier from old Step 8 to make boost_map available before fused scoring.
let boost_map: HashMap<u64, f64> = if results_with_scores.len() > 1 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let staleness_cutoff = now.saturating_sub(CO_ACCESS_STALENESS_SECONDS);

    let anchor_count = results_with_scores.len().min(3);
    let anchor_ids: Vec<u64> = results_with_scores.iter()
        .take(anchor_count)
        .map(|(e, _)| e.id)
        .collect();
    let result_ids: Vec<u64> = results_with_scores.iter()
        .map(|(e, _)| e.id)
        .collect();

    // crt-010: deprecated entries excluded from co-access co-occurrence counts.
    let deprecated_ids: HashSet<u64> = results_with_scores.iter()
        .filter(|(e, _)| e.status == Status::Deprecated)
        .map(|(e, _)| e.id)
        .collect();

    let store = Arc::clone(&self.store);
    spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, move || {
        compute_search_boost(&anchor_ids, &result_ids, &store, staleness_cutoff, &deprecated_ids)
    })
    .await
    .unwrap_or_else(|e| {
        tracing::warn!("co-access boost prefetch failed: {e}; coac_norm will be 0.0 for all");
        HashMap::new()
    })
} else {
    HashMap::new()
};
```

### Step 7: NLI scoring + fused score pass (REPLACES old Steps 7 and 8)

```
// Step 7: NLI scoring (if enabled) → fused score computation (single pass) →
//         sort by final_score DESC → truncate to k.
//
// NLI scoring and boost_map prefetch may run concurrently if the implementation
// initiates NLI early, BUT both must be fully resolved before the scoring loop begins.
// The current structure (Step 6c fully awaited, then Step 7 NLI below) is sequential —
// co-access runs first, then NLI. This is correct and simpler than concurrent initiation.

// NLI scoring — returns None on any failure; caller handles the NLI-absent path.
let nli_scores: Option<Vec<NliScores>> = if self.nli_enabled {
    try_nli_rerank(
        &results_with_scores,
        &params.query,
        &self.nli_handle,
        &self.rayon_pool,
    )
    .await
} else {
    None
};

let nli_available = nli_scores.is_some();

// Compute effective weights: if NLI absent, re-normalize the five remaining weights.
// Called once before the loop — NLI availability does not change per-candidate.
let effective_weights = self.fusion_weights.effective(nli_available);

// Single fused scoring pass: one iteration over all candidates.
// Each candidate gets a FusedScoreInputs constructed from its signals.
let mut scored: Vec<(EntryRecord, f64, f64)> = Vec::with_capacity(results_with_scores.len());
// Vec element: (entry, sim, final_score)

for (i, (entry, sim)) in results_with_scores.iter().enumerate() {
    // -- nli_entailment: f32 cast to f64; 0.0 when NLI absent --
    let nli_entailment: f64 = nli_scores
        .as_ref()
        .and_then(|scores| scores.get(i))
        .map(|s| s.entailment as f64)
        .unwrap_or(0.0);

    // -- coac_norm: raw boost / MAX_CO_ACCESS_BOOST --
    // MAX_CO_ACCESS_BOOST imported from unimatrix_engine::coaccess (AC-07, R-08).
    let raw_coac = boost_map.get(&entry.id).copied().unwrap_or(0.0);
    let coac_norm = raw_coac / MAX_CO_ACCESS_BOOST;

    // -- util_norm: shift-and-scale maps [-UTILITY_PENALTY, +UTILITY_BOOST] to [0, 1] --
    // utility_delta function is unchanged; normalization is new (FR-05, R-01, R-11).
    let raw_delta = utility_delta(categories.get(&entry.id).copied());
    let util_norm = (raw_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY);

    // -- prov_norm: divide by PROVENANCE_BOOST; guard zero denominator (R-03) --
    let raw_prov = if self.boosted_categories.contains(&entry.category) {
        PROVENANCE_BOOST
    } else {
        0.0
    };
    let prov_norm = if PROVENANCE_BOOST == 0.0 {
        0.0
    } else {
        raw_prov / PROVENANCE_BOOST
    };

    // -- Construct FusedScoreInputs --
    let inputs = FusedScoreInputs {
        similarity:     *sim,
        nli_entailment,
        confidence:     entry.confidence,
        coac_norm,
        util_norm,
        prov_norm,
    };

    // -- Fused score + status penalty (ADR-004: penalty at call site) --
    let fused = compute_fused_score(&inputs, &effective_weights);
    let penalty = penalty_map.get(&entry.id).copied().unwrap_or(1.0);
    let final_score = fused * penalty;

    scored.push((entry.clone(), *sim, final_score));
}

// Single sort by final_score DESC. No secondary sort after this (AC-04, FR-08).
// Tiebreaker: original candidate index (HNSW order) for determinism (NFR-03).
// Use enumerate index to produce a stable deterministic ordering when scores are equal.
scored.sort_by(|a, b| {
    b.2.partial_cmp(&a.2).unwrap_or(Ordering::Equal)
    // If scores are equal, the sort is not further disambiguated here — Rust's sort_by
    // is stable, so equal-score candidates retain their relative pre-sort order (HNSW order).
    // This satisfies the tiebreaker requirement without an explicit secondary key.
});

// Truncate to requested k.
scored.truncate(params.k);

// Replace results_with_scores for Steps 8–9 (floors, ScoredEntry construction).
// Rebuild as Vec<(EntryRecord, f64)> for the floor steps which only need entry + sim.
results_with_scores = scored.iter().map(|(e, sim, _)| (e.clone(), *sim)).collect();
// Carry final_score separately for ScoredEntry construction below.
let final_scores: Vec<f64> = scored.iter().map(|(_, _, fs)| *fs).collect();
```

### Step 8 — REMOVED

The old Step 8 (co-access re-sort with `boost_map`) is fully deleted. Its logic is absorbed
into Step 6c (prefetch) and the fused scoring loop in Step 7.

### Steps 9–10: Apply floors (unchanged)

Steps 9 and 10 remain unchanged. They operate on `results_with_scores` (the rebuilt vec).
The only change: truncation already happened as part of Step 7, so `truncate(params.k)` in
Step 9 is now a no-op. Keep it for safety; it does not hurt.

```
// Step 9: Truncate to k (now a no-op — Step 7 already truncated, but kept for safety).
results_with_scores.truncate(params.k);

// Step 10: Apply floors (if set) — unchanged.
```

### Note on final_scores alignment after floors

After floors are applied in Step 10, `results_with_scores` may be shorter than `final_scores`.
The floor steps use indices into `results_with_scores` via `retain`. The `final_scores` vector
must be kept parallel. Two approaches:

**Approach A** (simpler): Keep `(EntryRecord, f64, f64)` tuples through to ScoredEntry
construction — do not split into `results_with_scores` + `final_scores`. Filter `retain` on the
triple. This avoids the parallelism problem entirely.

**Approach B** (current code shape): Keep both vecs and apply the same retain logic to both.

The implementation agent should choose Approach A if it reduces code complexity. The pseudocode
above uses Approach B for conceptual clarity, but Approach A is preferred.

### Step 11: Build ScoredEntry (updated formula)

Replace the existing ScoredEntry construction to use the fused final_score:

```
// Step 11: Build ScoredEntry with fused final_score.
// ScoredEntry.final_score = compute_fused_score * status_penalty (already computed).
// Field name 'final_score' is unchanged; formula changes (FR-10, AC-08).
let entries: Vec<ScoredEntry> = results_with_scores.iter()
    .zip(final_scores.iter())
    .map(|((entry, sim), &final_score)| {
        ScoredEntry {
            entry: entry.clone(),
            final_score,               // fused formula value
            similarity: *sim,
            confidence: entry.confidence,
        }
    })
    .collect();
```

---

## `utility_delta` Function

The `utility_delta` function body is unchanged. Its return value is now used differently:
instead of being added directly to the rerank score, it is normalized to [0,1] by the scoring
loop before being placed in `FusedScoreInputs.util_norm`.

The doc comment on `utility_delta` should be updated to remove the reference to the old formula
`(rerank_score + utility_delta + prov_boost + co_access_boost) * status_penalty` and instead
note that the raw delta is used as input to shift-and-scale normalization.

---

## Fallback Path

There is no longer a separate `!used_nli` fallback path that uses `rerank_score`. The single
fused scoring pass handles both the NLI-active and NLI-absent cases via
`FusionWeights::effective(nli_available)`.

The `rerank_score` function itself is NOT removed (FR-12). It may still be called by tests that
verify backward compatibility, and it remains available in `unimatrix-server/src/confidence.rs`
for other uses. The scoring loop in `SearchService::search()` no longer calls it.

---

## Imports to Add / Remove

Add:
```
use unimatrix_engine::coaccess::MAX_CO_ACCESS_BOOST;
```

Remove from the function body (not from imports — other tests may use it):
- `rerank_score` is no longer called in the pipeline body (still needed by existing tests)
- `confidence_weight` snapshot at the top of `search()` is no longer needed for the fused path
  — remove or keep for tests; if tests assert `rerank_score` values they still need it

---

## Error Handling

- `try_nli_rerank` returns `None` on any failure — pipeline continues with `nli_available = false`
- boost_map prefetch failure: `unwrap_or_else(|e| { warn!(...); HashMap::new() })` — pipeline
  continues with `coac_norm = 0.0` for all candidates (FM-03)
- NaN in fused score: prevented by the `prov_norm` zero-guard (R-03). No explicit `is_finite()`
  check in the hot path, but debug builds may add `debug_assert!(fused.is_finite())`.
- Status penalty of 0.0: legal multiplier producing `final_score = 0.0` — no division by penalty

---

## Key Test Scenarios

### Pipeline / Integration Tests

#### T-SP-09: boost_map prefetch sequencing (R-07, IR-01)

Integration test with seeded co-access data. Run a search call and assert that the returned
`ScoredEntry` ordering reflects co-access weighting — a high-coac entry must not score as if
`coac_norm = 0.0`. Confirms Step 6c fully resolves before the scoring loop.

#### T-SP-10: single scoring pass — no secondary sort (AC-04)

With NLI active and co-access data present, run a search call. Assert the pipeline produces
a single sorted result set with no intermediate re-sorts. Code review + behavioral assertion:
a high-NLI entry must rank above a lower-NLI entry even if the latter would have benefited
from the old Step 8 co-access re-sort.

#### T-SP-11: NLI active path — correct final_score values (AC-05 pipeline version)

Integration-level test with known query, known candidate entries, mocked NLI scores, seeded
co-access. Assert `ScoredEntry.final_score` values match hand-computed fused formula values.

#### T-SP-12: NLI absent path — re-normalized weights applied (AC-06, R-09)

Run search with `nli_enabled = false`. Assert results use re-normalized weights (sim dominant).
Assert no panic. Assert scores are in [0, 1].

#### T-SP-13: AC-11 regression test — NLI-high beats co-access-high at system level

End-to-end: entry A (high NLI, no co-access), entry B (low NLI, max co-access), equal sim and
conf. Assert A ranks above B in the returned `ScoredEntry` slice. Mark with comment referencing
AC-11 and any associated GH issue.

### Fused Scorer Unit Tests (migrated from apply_nli_sort, R-05)

#### T-SP-14: NLI entailment dominant ranking (replaces apply_nli_sort sort-key test)

`compute_fused_score` with nli=0.9 vs nli=0.1, equal sim/conf/coac/util/prov, default weights.
Assert first entry scores higher than second.

#### T-SP-15: tiebreak on equal fused scores is deterministic (NFR-03, R-05)

Two entries with identical signal values produce identical fused scores. Assert that calling the
scoring loop twice on the same input yields the same order both times (stable sort).

#### T-SP-16: NLI scores length mismatch → None returned (EC-07, R-05)

In `try_nli_rerank`, simulate NLI returning a Vec of wrong length. Assert the function returns
`None` (not panic, not wrong-index access). Assert the scoring loop uses `nli_entailment = 0.0`.

### Score-Value Update Tests (R-04, AC-08)

#### T-SP-17 through T-SP-24: Updated penalty tests

The existing tests T-SP-01 through T-SP-08 use `penalized_score(sim, conf, penalty)` which
calls `rerank_score`. These tests remain correct as-is (they test penalty arithmetic, not the
scoring formula). No update needed for tests that use only `rerank_score` directly without
asserting `SearchService.search()` output.

Tests that call `SearchService.search()` end-to-end and assert specific `final_score` values
must be updated to reflect the new fused formula. These are the test functions that reference
`ScoredEntry.final_score` in assertions. Update expected values to fused-formula results for
the same inputs.

#### T-SP-25: EvalServiceLayer wiring — w_sim=1.0 profile (R-NEW, AC-15)

Construct `EvalServiceLayer` with a profile containing `w_sim=1.0`, all other weights 0.0.
Score a candidate with known sim=0.6, nli=0.9, conf=0.8. Assert `final_score ≈ 0.6 * penalty`.
If EvalServiceLayer uses default weights instead of the profile's, the actual score would be
materially higher (≈0.6 + 0.315 + 0.12 = higher), causing the assertion to fail and exposing
the wiring bug.

#### T-SP-26: EvalServiceLayer wiring — differential test (R-NEW)

Two EvalServiceLayer instances: one with `old-behavior.toml` (w_nli=0.0), one with
`crt024-weights.toml` (w_nli=0.35). Same candidate, same NLI score. Assert scores differ
by at least `0.35 * nli_score - epsilon`.
