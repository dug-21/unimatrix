# crt-024: Ranking Signal Fusion — Pseudocode Overview

## What Changed and Why

crt-024 replaces the two-pass sequential ranking pipeline (Step 7: NLI sort, Step 8: co-access
re-sort) with a single linear combination. The structural defect: `apply_nli_sort` sorts by
entailment, then the Step 8 co-access re-sort re-orders those results using a formula that omits
NLI — meaning a high-co-access entry can overtake a high-entailment entry. The fix is one pass
where every signal is a named, weighted, normalized term.

## Components Covered

| Component | File | Wave |
|-----------|------|------|
| `InferenceConfig` weight fields + validation | `infra/config.rs` | 1 |
| `FusedScoreInputs` + `FusionWeights` structs | `services/search.rs` | 2 |
| `compute_fused_score` pure function | `services/search.rs` | 2 |
| `SearchService` pipeline rewrite | `services/search.rs` | 2 |

Wave 1 (config.rs) has no dependencies on Wave 2 and can compile and test independently.
Wave 2 depends on Wave 1 for the `InferenceConfig` fields that supply `FusionWeights`.

## Data Flow Between Components

```
UnimatrixConfig (loaded at startup)
  └── InferenceConfig.{w_sim, w_nli, w_conf, w_coac, w_util, w_prov}
        │
        ▼  (passed into SearchService::new via fusion_weights field)
SearchService.fusion_weights: FusionWeights
        │
        ▼  (at each search call)
[Step 6c] boost_map: HashMap<u64, f64>   ← spawn_blocking, fully awaited
[Step 7]  nli_scores: Option<Vec<NliScores>>  ← rayon_pool, or None
        │
        ▼  (per candidate in scoring loop)
FusedScoreInputs {
    similarity, nli_entailment, confidence,
    coac_norm, util_norm, prov_norm
}
        │
        ▼
compute_fused_score(&inputs, &weights.effective(nli_available)) -> f64
        │
        ▼
final_score = fused_score * status_penalty
        │
        ▼
ScoredEntry.final_score  (field name unchanged; formula changes)
```

## Shared Types Introduced

All defined in `services/search.rs` (or a sub-module within the same file):

### `FusedScoreInputs`

Six f64 fields, all in [0.0, 1.0] by the time they enter `compute_fused_score`:

| Field | Source | Normalization |
|-------|--------|---------------|
| `similarity` | HNSW cosine | identity (already [0,1]) |
| `nli_entailment` | `NliScores.entailment as f64` | identity (already [0,1]); 0.0 when NLI absent |
| `confidence` | `EntryRecord.confidence` | identity (already [0,1]) |
| `coac_norm` | `boost_map.get(id).unwrap_or(0.0)` | `raw / MAX_CO_ACCESS_BOOST` |
| `util_norm` | `utility_delta(category)` | `(raw + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)` |
| `prov_norm` | `PROVENANCE_BOOST` if boosted else `0.0` | `raw / PROVENANCE_BOOST` (guarded) |

WA-2 extension: add `phase_boost_norm: f64` here when WA-2 is implemented.

### `FusionWeights`

Six f64 fields, each in [0.0, 1.0], sum <= 1.0:

| Field | Default |
|-------|---------|
| `w_sim` | 0.25 |
| `w_nli` | 0.35 |
| `w_conf` | 0.15 |
| `w_coac` | 0.10 |
| `w_util` | 0.05 |
| `w_prov` | 0.05 |

Default sum = 0.95, leaving 0.05 headroom for WA-2's `w_phase`.

Constructed from `InferenceConfig` in `SearchService::new`. Not stored in `InferenceConfig`
itself — `InferenceConfig` holds the six raw f64 fields; `FusionWeights` is a derived struct
assembled from those fields.

## Sequencing Constraints

1. `InferenceConfig` fields must exist before `SearchService::new` can construct `FusionWeights`.
   Wave 1 (config.rs) must compile before Wave 2 starts.

2. Within the pipeline, `boost_map` prefetch (Step 6c) must be fully `.await`-ed before the
   candidate scoring loop begins. The NLI scoring future (rayon pool) may be initiated concurrently
   with the boost_map prefetch, but both must be resolved before the loop starts.

3. `apply_nli_sort` removal (ADR-002): the function is deleted in Wave 2. All its existing unit
   tests are migrated to fused scorer tests in the same commit — no intermediate state where
   `apply_nli_sort` is absent but its coverage is not replaced.

## What Is NOT Changed

- Engine crates (`unimatrix_engine::coaccess`, `unimatrix_engine::confidence`) — read-only imports
- `BriefingService` and `MAX_BRIEFING_CO_ACCESS_BOOST = 0.01` — untouched
- `rerank_score` in `unimatrix-server/src/confidence.rs` — retained for fallback path and tests
- `ScoredEntry` field names and MCP response shape — `final_score` field name unchanged
- Database schema — no migration, schema version unchanged
- `utility_delta` function signature — unchanged; normalization happens at the call site in the
  scoring loop, not inside `utility_delta` itself
