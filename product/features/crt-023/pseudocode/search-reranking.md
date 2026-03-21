# Search Re-ranking — Pseudocode

**File**: `crates/unimatrix-server/src/services/search.rs` (modified)

**Purpose**: Extend `SearchService` with an `nli_handle` field and insert the NLI re-ranking
step into the search pipeline. When NLI is Ready, HNSW candidate pool is expanded to `nli_top_k`,
all candidates are batch-scored via the rayon pool (spawn_with_timeout), sorted by entailment
descending (stable sort), then truncated to top-K. When NLI is not ready, disabled, or times out,
falls back to the existing `rerank_score`-based sort path unchanged (ADR-002).

**Critical constraint (W1-2)**: NLI scoring runs via `rayon_pool.spawn_with_timeout(
MCP_HANDLER_TIMEOUT, ...)`. Never inline in async context. Never via `spawn_blocking`.

---

## `SearchService` Struct Extension

Add one field to the existing struct. All existing fields unchanged.

```
// Add to SearchService struct (after rayon_pool field):

/// crt-023: NLI cross-encoder handle for search re-ranking (ADR-002).
/// When Ready and nli_enabled=true, replaces rerank_score sort step.
/// When Loading/Failed/disabled, pipeline falls back to rerank_score unchanged.
nli_handle: Arc<NliServiceHandle>,
```

---

## `SearchService::new` Extension

Add `nli_handle` parameter to the existing `new` function. Pass it through to the struct.

```
// Add parameter to existing fn new(...):
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
    nli_handle: Arc<NliServiceHandle>,   // new
    nli_top_k: usize,                    // new — from InferenceConfig
    nli_enabled: bool,                   // new — from InferenceConfig
) -> Self
    SearchService {
        // ... existing fields ...
        nli_handle,
        nli_top_k,
        nli_enabled,
    }
```

Add `nli_top_k` and `nli_enabled` as fields to the struct as well:

```
// Additional fields in SearchService struct:
nli_top_k:   usize,   // from config, used to expand HNSW candidate pool
nli_enabled: bool,    // from config, fast check before get_provider()
```

---

## Modified Search Pipeline

The search pipeline is modified at Steps 5 and 7. All other steps (0-4, 6, 6a, 6b, 8+) are
unchanged from the existing implementation.

### Step 5 modification: HNSW candidate count

```
// BEFORE (existing):
// self.vector_store.search(embedding.clone(), params.k, EF_SEARCH)

// AFTER:
// When NLI is enabled and potentially ready, expand candidate pool to nli_top_k.
// The fallback check at Step 7 handles the case where NLI is not actually ready.
let hnsw_k = if self.nli_enabled {
    // Expand: fetch more candidates for NLI to score and then truncate to params.k
    self.nli_top_k.max(params.k)  // never retrieve fewer than requested
} else {
    params.k
};

// Then use hnsw_k in the vector_store.search call:
if let Some(ref filter) = params.filters:
    self.vector_store.search_filtered(embedding.clone(), hnsw_k, EF_SEARCH, allowed_ids)
else:
    self.vector_store.search(embedding.clone(), hnsw_k, EF_SEARCH)
```

### Step 7 replacement: NLI re-ranking sort (replaces existing rerank_score sort)

This replaces the existing `results_with_scores.sort_by(...)` block entirely.

```
// --- NLI re-ranking attempt ---
//
// Try to get NLI provider and score all candidates.
// If NLI is available: sort by entailment DESC (ADR-002 — pure replacement).
// If NLI unavailable for any reason: fall back to rerank_score sort (unchanged behavior).
//
// Status penalty interaction (ADR-002): status penalty is applied as a multiplier on the
// HNSW base_score (similarity), NOT on the NliScores values. This means NLI scores are
// raw (unmodified by status). The penalty affects the final ranking position but not
// what is stored in GRAPH_EDGES metadata.

let used_nli_ranking = if self.nli_enabled {
    // Attempt NLI path
    try_nli_rerank(
        &results_with_scores,
        &params.query,
        &self.nli_handle,
        &self.rayon_pool,
        &mut results_with_scores,  // in-place replacement
    ).await
} else {
    false
}

if !used_nli_ranking {
    // Fallback path: existing rerank_score sort (unchanged from current implementation)
    fallback_rerank_sort(&mut results_with_scores, confidence_weight, &self.boosted_categories, &penalty_map)
}
```

### `try_nli_rerank` helper (private async fn)

```
/// Attempt NLI re-ranking. Returns true if NLI ranking was applied; false if fallback needed.
///
/// W1-2 contract: ALL NLI inference dispatched via rayon_pool.spawn_with_timeout.
/// Never inline in async context.
async fn try_nli_rerank(
    candidates: &[(EntryRecord, f64)],   // (entry, base_similarity)
    query_text: &str,
    nli_handle: &NliServiceHandle,
    rayon_pool: &RayonPool,
) -> Option<Vec<(EntryRecord, f64, NliScores)>>
    // Returns None on any failure (caller falls back to rerank_score)

    // Step 1: Get provider (fast async check)
    let provider = match nli_handle.get_provider().await:
        Ok(p)  -> p
        Err(_) -> return None  // Loading, Failed, or disabled

    if candidates.is_empty():
        return None  // No candidates to score (edge case: all filtered out)

    // Step 2: Build (query, passage) pairs. Query text is re-used for all pairs.
    // Collect owned strings for Send bound across rayon spawn.
    let query_owned: String = query_text.to_string()
    let passages: Vec<String> = candidates.iter()
        .map(|(entry, _)| entry.content.clone())
        .collect()

    // Step 3: Dispatch to rayon pool with MCP_HANDLER_TIMEOUT (W1-2, FR-16)
    let provider_clone = Arc::clone(&provider)
    let nli_result = rayon_pool.spawn_with_timeout(
        MCP_HANDLER_TIMEOUT,
        move || {
            // Build &[(&str, &str)] from owned strings inside the rayon closure
            let pairs: Vec<(&str, &str)> = passages.iter()
                .map(|p| (query_owned.as_str(), p.as_str()))
                .collect()
            provider_clone.score_batch(&pairs)
        }
    ).await

    // Step 4: Handle rayon result
    match nli_result:
        Ok(Ok(scores)) -> Some(scores)
        Ok(Err(_))     -> None  // score_batch returned Err (inference error)
        Err(_)         -> None  // RayonError::Cancelled (panic) or TimedOut (timeout)
        // On timeout: rayon task continues running (no cooperative cancel).
        // The Mutex<Session> is held until the task completes, then released.
        // This is acceptable per ADR-001: subsequent calls queue behind the running task.
```

### Sorting and truncation after NLI scores

```
// After try_nli_rerank returns Some(scores):
//
// NLI sort step:
//
// Sort key: nli_scores.entailment DESCENDING.
// Must be STABLE sort (R-03): when entailment scores are equal, secondary sort by original
// HNSW rank (position in results_with_scores) to maintain deterministic ordering.
//
// Status penalty interaction (ADR-002): penalty is applied as a multiplier on the base
// score BEFORE NLI scoring. The penalty_map (from Step 6a) contains multipliers [0,1].
// Apply: effective_entailment = nli_scores.entailment * penalty_map.get(entry.id).unwrap_or(1.0)
// This depresses penalized entries below less-relevant active entries, per ADR-002.
// NOTE: The raw NliScores are used for GRAPH_EDGES metadata (not the penalized value).

// Annotate each candidate with its NLI score and applied penalty
annotated: Vec<(EntryRecord, f64, f32, usize)> = candidates.iter()
    .zip(nli_scores.iter())
    .enumerate()
    .map(|(original_rank, ((entry, base_sim), nli_scores))| {
        let penalty = penalty_map.get(&entry.id).copied().unwrap_or(1.0)
        // Effective entailment for sorting: raw entailment × status penalty
        let effective_entailment = nli_scores.entailment * penalty as f32
        (entry.clone(), *base_sim, effective_entailment, original_rank)
    })
    .collect()

// Stable sort by effective_entailment DESC, then by original_rank ASC (deterministic tiebreak)
annotated.sort_by(|a, b| {
    b.2.partial_cmp(&a.2)
       .unwrap_or(Ordering::Equal)  // NaN-safe: treat NaN as equal
       .then_with(|| a.3.cmp(&b.3)) // tiebreak: original HNSW rank ascending
})

// Truncate to params.k
annotated.truncate(params.k)

// Rebuild results_with_scores from annotated (dropping the NLI and rank fields)
results_with_scores = annotated.into_iter()
    .map(|(entry, sim, _, _)| (entry, sim))
    .collect()
```

### Fallback sort path (unchanged behavior)

```
fn fallback_rerank_sort(
    results_with_scores: &mut Vec<(EntryRecord, f64)>,
    confidence_weight: f64,
    boosted_categories: &HashSet<String>,
    penalty_map: &HashMap<u64, f64>,
)
    // Existing rerank_score sort from Step 7 — no changes to this logic.
    // Called when NLI is not available, disabled, or timed out.
    results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
        // ... existing rerank_score calculation with penalty_map, boosted_categories ...
    })
```

---

## Post-NLI Steps (Steps 8+): Unchanged

After the sort step, the pipeline continues as before:
- Co-access boost
- Floors (similarity_floor, confidence_floor)
- Result construction → SearchResults

The MCP response schema does not change (FR-17). NLI scores are internal only.

---

## `coaccess` Module Note

The existing `compute_search_boost` and co-access boost are applied AFTER NLI sort and
truncation. This is correct: co-access boost measures session context (what was accessed
together this session), not query relevance. Post-sort position is final before boost.

---

## Error Handling

| Condition | Behavior |
|-----------|----------|
| `nli_handle.get_provider()` → Err | Fall back to `rerank_score` sort, no error to caller |
| `rayon_pool.spawn_with_timeout` → `Err(TimedOut)` | Fall back to `rerank_score` sort, no error to caller |
| `rayon_pool.spawn_with_timeout` → `Err(Cancelled)` | Fall back to `rerank_score` sort, no error to caller |
| `score_batch` → Err (inference error) | Fall back to `rerank_score` sort, no error to caller |
| Empty candidates after filters | NLI path returns `None` (no pairs to score); fallback sort on empty vec |
| `nli_enabled = false` | HNSW uses `params.k` (not expanded); fallback sort path used |
| All candidates have equal entailment scores | Stable sort by original HNSW rank (R-03 determinism) |
| NaN in NliScores.entailment | `partial_cmp` returns `Equal`; NaN-safe tiebreak by original rank |

**Note**: Fallback is always silent — no error code, no log above debug level. The caller
(MCP handler) receives valid results regardless of NLI state. A `tracing::debug!` noting
the fallback reason is acceptable.

---

## Key Test Scenarios

1. **AC-08 / NLI active ordering**: With NLI ready and a mock provider returning known scores, assert result ordering matches entailment descending (not rerank_score formula).
2. **AC-08 / NLI disabled fallback**: With `nli_enabled = false`, assert existing `rerank_score` path is used and result count equals `params.k`.
3. **AC-20 / rerank_score not called on NLI-active path**: When NLI is active, `rerank_score()` function is not called as part of the sort step.
4. **R-03 / stable sort**: Mock provider returns identical entailment=0.33 for all candidates; assert result order is deterministic across 10 repeated identical calls.
5. **FR-16 / timeout fallback**: Mock provider delays 35s; MCP_HANDLER_TIMEOUT=30s fires; assert response uses `rerank_score` fallback and returns within timeout.
6. **R-04 / timeout then available**: After timeout, call search again; assert NliServiceHandle remains Ready (not Failed); second call may succeed with NLI.
7. **R-17 / status penalty position**: Deprecated entry with high entailment still ranks lower than active entry with lower entailment when penalty depresses effective_entailment below the active entry's score.
8. **R-01 / concurrent load**: 3 concurrent NLI search calls complete without timeout propagating to embedding step.
9. **AC-19 / nli_top_k independence**: `nli_top_k` controls HNSW expansion for search; does not affect `nli_post_store_k` in StoreService.
10. **Empty candidate pool**: All HNSW candidates filtered by quarantine; `score_batch(&[])` not called; result is empty SearchResults without error.
