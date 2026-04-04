# crt-046 Pseudocode Overview — Behavioral Signal Delivery

## Components and Wave Order

| Wave | Component | File | Dependency |
|------|-----------|------|------------|
| 1 | store-v22 | store-v22.md | none |
| 2 | behavioral_signals | behavioral-signals.md | store-v22 |
| 3 | cycle-review-step-8b | cycle-review-step-8b.md | store-v22, behavioral_signals |
| 4 | briefing-blending | briefing-blending.md | store-v22, behavioral_signals |

Wave 2 may not start until `goal_clusters.rs`, `get_cycle_start_goal_embedding`, and
`InferenceConfig` new fields from Wave 1 compile cleanly.

## What Each Component Does

**store-v22** — Schema v21 to v22 migration. New `goal_clusters` table. Three new async
methods on `SqlxStore`: `get_cycle_start_goal_embedding`, `insert_goal_cluster`,
`query_goal_clusters_by_embedding`. New `GoalClusterRow` struct. Three new `InferenceConfig`
fields. Nine cascade touchpoints updated.

**behavioral_signals** — New module `services/behavioral_signals.rs`. Pure computation
and coordination: parse co-access pairs from `ObservationRow` slices, enumerate pairs,
emit bidirectional `Informs` edges via a `write_graph_edge` helper, write `goal_clusters`
row. Also provides `blend_cluster_entries` (pure function used by briefing handler).

**cycle-review-step-8b** — Insertion point in `context_cycle_review` handler.
Step 8b runs on EVERY call (cache-hit or miss). `parse_failure_count` is a top-level
JSON response field. Memoisation early-return is positioned AFTER step 8b.

**briefing-blending** — Goal-conditioned blending path in `context_briefing` handler.
Two-level guard (Level 1: feature/goal absent; Level 2: no stored embedding). Score-based
interleaving (Option A, ADR-005). Uses `blend_cluster_entries` from behavioral_signals.

## Data Flow Between Components

```
                    [store-v22]
                        |
        goal_clusters table + GoalClusterRow struct
        get_cycle_start_goal_embedding()
        insert_goal_cluster()
        query_goal_clusters_by_embedding()
        InferenceConfig: goal_cluster_similarity_threshold, w_goal_cluster_conf, w_goal_boost
                        |
          ┌─────────────┴──────────────┐
          ▼                            ▼
  [behavioral_signals]          [behavioral_signals]
  (step 8b side)                (blending side)
  collect_coaccess_entry_ids    blend_cluster_entries
  build_coaccess_pairs          (pure function)
  outcome_to_weight
  emit_behavioral_edges
  populate_goal_cluster
          |                            |
          ▼                            ▼
  [cycle-review-step-8b]     [briefing-blending]
  mcp/tools.rs step 8b       mcp/tools.rs briefing path
  parse_failure_count         Level 1 + Level 2 guards
  in JSON response            store.get_by_ids() fetch
                              cluster_score computation
                              blend_cluster_entries call
```

## Shared Types (New or Modified)

### New: `GoalClusterRow` (unimatrix-store/src/goal_clusters.rs)

```
GoalClusterRow {
    id:             i64
    feature_cycle:  String
    goal_embedding: Vec<f32>          -- decoded at query time; not a BLOB here
    phase:          Option<String>
    entry_ids_json: String            -- raw JSON array text from DB
    outcome:        Option<String>
    created_at:     i64               -- Unix millis
    similarity:     f32               -- computed at query time, NOT stored
}
```

### Modified: `InferenceConfig` (unimatrix-server/src/infra/config.rs)

Three new fields (all `#[serde(default)]`):
- `goal_cluster_similarity_threshold: f32` — default 0.80
- `w_goal_cluster_conf: f32` — default 0.35
- `w_goal_boost: f32` — default 0.25

These are read at call time from `Arc<InferenceConfig>` — they are NOT constants in
`behavioral_signals.rs`.

### Existing types consumed

| Type | Source | Usage in crt-046 |
|------|--------|-----------------|
| `ObservationRow` | unimatrix-store/src/observations.rs | input to `collect_coaccess_entry_ids` |
| `AnalyticsWrite::GraphEdge` | unimatrix-store/src/analytics.rs | NOT used in step 8b emit path; see behavioral-signals.md |
| `IndexEntry` | unimatrix-server/src/mcp/response/briefing.rs | input and output of `blend_cluster_entries` |
| `EntryRecord` | unimatrix-store (via unimatrix-core) | holds Wilson-score `confidence`; used for cluster_score |
| `CycleReviewRecord` | unimatrix-store/src/cycle_review_index.rs | NOT modified; parse_failure_count is outside it |
| `SessionState` | unimatrix-server/src/infra/session.rs | `.feature` and `.current_goal` fields in briefing guard |

## Naming Collision — CRITICAL

`IndexEntry.confidence` (f64, raw HNSW cosine, returned by `briefing.index()`)
is NOT the same as `EntryRecord.confidence` (f64, Wilson-score composite, returned by
`store.get()` or individual per-ID fetches).

The `cluster_score` formula uses `EntryRecord.confidence`. Both names compile.
The wrong one silently uses cosine twice. Every pseudocode file that references
`cluster_score` explicitly labels which struct provides `confidence`.

## Sequencing Constraints

1. `goal_clusters.rs` must compile (Wave 1) before `behavioral_signals.rs` can
   reference `GoalClusterRow` or call `insert_goal_cluster`.
2. `InferenceConfig` new fields must be present before the briefing blending path
   can read `config.goal_cluster_similarity_threshold`.
3. `behavioral_signals::blend_cluster_entries` must be in place before the briefing
   handler can call it (Wave 4 depends on Wave 2).
4. The analytics drain test flush pattern (I-02, entry #2148) must be applied to
   every integration test querying `graph_edges`.
