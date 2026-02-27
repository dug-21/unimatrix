# Pseudocode Overview: crt-005 Coherence Gate

## Components

| # | Component | Crate | New/Modified | Tier |
|---|-----------|-------|-------------|------|
| C1 | schema-migration | unimatrix-store | migration.rs, schema.rs | 1 |
| C2 | f64-scoring | server, store, vector, core | confidence.rs, coaccess.rs, index.rs, traits.rs, write.rs, tools.rs | 1 |
| C3 | vector-compaction | unimatrix-vector, unimatrix-core | index.rs, traits.rs | 1 |
| C4 | coherence-module | unimatrix-server | coherence.rs (new), lib.rs | 1 |
| C5 | confidence-refresh | unimatrix-server | tools.rs | 2 |
| C6 | status-extension | unimatrix-server | response.rs | 1 |
| C7 | maintenance-parameter | unimatrix-server | tools.rs | 2 |
| C8 | compaction-integration | unimatrix-server | tools.rs | 2 |

## Build Order

1. **C1 (schema-migration)** -- Must land first so EntryRecord.confidence is f64.
2. **C2 (f64-scoring)** -- Depends on C1 (schema change). Updates all scoring constants and signatures.
3. **C3 (vector-compaction)** -- Depends on C2 (SearchResult.similarity is now f64). Adds compact method.
4. **C4 (coherence-module)** -- Independent of C1-C3 at the type level but uses f64 types.
5. **C6 (status-extension)** -- Depends on C4 (coherence types in StatusReport).
6. **C7 (maintenance-parameter)** -- Depends on C6 (StatusParams extended).
7. **C5 (confidence-refresh)** -- Depends on C2 (compute_confidence returns f64) and C7 (maintain flag).
8. **C8 (compaction-integration)** -- Depends on C3 (compact method), C5 (refresh logic), C7 (maintain flag).

## Data Flow

```
Store (EntryRecord.confidence: f64)
  |
  +-- compute_confidence(&entry, now) -> f64   [C2: confidence.rs]
  |     |
  |     +-- update_confidence(id, f64)          [C1: schema change, C2: signature change]
  |
  +-- confidence_freshness_score(&entries, now, threshold) -> (f64, stale_count) [C4: coherence.rs]
  |
  +-- SearchResult.similarity: f64              [C2: index.rs cast at boundary]
        |
        +-- rerank_score(f64, f64) -> f64       [C2: confidence.rs]
              + co_access_boost: HashMap<u64, f64> [C2: coaccess.rs]

VectorIndex
  |
  +-- stale_count() / point_count()             [existing]
  +-- compact(Vec<(u64, Vec<f32>)>)             [C3: index.rs new method]
  |
  +-- graph_quality_score(stale, total) -> f64  [C4: coherence.rs]

context_status handler
  |
  +-- dimension scores from C4
  +-- confidence refresh from C5 (when maintain=true)
  +-- compaction trigger from C8 (when maintain=true + stale ratio > 10%)
  +-- lambda + recommendations from C4
  +-- StatusReport with coherence fields from C6
  +-- maintain parameter from C7
```

## Shared Types

### Modified Types
- `EntryRecord.confidence`: f32 -> f64 (C1, all crates)
- `SearchResult.similarity`: f32 -> f64 (C2, vector+core+server)
- `compute_confidence` return: f32 -> f64 (C2)
- `rerank_score` params/return: f32 -> f64 (C2)
- `co_access_affinity` params/return: f32 -> f64 (C2)
- `update_confidence` param: f32 -> f64 (C2)
- `compute_search_boost`/`compute_briefing_boost` return: HashMap<u64, f32> -> HashMap<u64, f64> (C2)

### New Types (C4: coherence.rs)
- `CoherenceWeights { confidence_freshness: f64, graph_quality: f64, embedding_consistency: f64, contradiction_density: f64 }`
- `DEFAULT_WEIGHTS: CoherenceWeights` (const)

### Extended Types (C6: response.rs)
- `StatusReport` gains 10 new fields (coherence section)

### Extended Types (C7: tools.rs)
- `StatusParams` gains `maintain: Option<bool>`
