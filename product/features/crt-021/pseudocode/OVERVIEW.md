# crt-021: Typed Relationship Graph — Pseudocode Overview

## Components Involved

| Component | File | Role |
|-----------|------|------|
| engine-types | `crates/unimatrix-engine/src/graph.rs` | Core types, graph builder, penalty traversal |
| store-schema | `crates/unimatrix-store/src/db.rs` | GRAPH_EDGES DDL in create_tables_if_needed |
| store-migration | `crates/unimatrix-store/src/migration.rs` | v12→v13 block: DDL + bootstrap INSERTs |
| store-analytics | `crates/unimatrix-store/src/analytics.rs` + `read.rs` | AnalyticsWrite::GraphEdge variant + drain arm; GraphEdgeRow + query_graph_edges |
| server-state | `crates/unimatrix-server/src/services/typed_graph.rs` | TypedGraphState + TypedGraphStateHandle; rebuild; ~20 call-site rename |
| background-tick | `crates/unimatrix-server/src/background.rs` | GRAPH_EDGES compaction; tick sequence update; TypedGraphState rebuild call |

---

## Data Flow

```
STARTUP (v12→v13 migration):
  entries.supersedes ──► INSERT OR IGNORE graph_edges (Supersedes, bootstrap_only=0)
  co_access count >= 3 ──► INSERT OR IGNORE graph_edges (CoAccess, weight=count/MAX, bootstrap_only=0)
  Contradicts: EMPTY (AC-08 dead — shadow_evaluations has no entry ID pairs)

BACKGROUND TICK (every 15 min, sequential):
  Step 1: maintenance_tick() [existing]
  Step 2: DELETE orphaned graph_edges via write_pool [NEW]
  Step 3: VectorIndex compact + Store::rewrite_vector_map [existing]
  Step 4: Store::query_all_entries + Store::query_graph_edges
          → build_typed_relation_graph(entries, edges)
          → write lock → swap TypedGraphState [UPGRADED]
  Step 5: contradiction scan [existing]

SEARCH HOT PATH (per query, zero store I/O):
  read lock on TypedGraphStateHandle
  → clone typed_graph, all_entries, use_fallback
  → release lock
  → if use_fallback: return FALLBACK_PENALTY
  → graph_penalty(node_id, &typed_graph, &all_entries)
     [internally calls edges_of_type(..., Supersedes, ...) only]

RUNTIME EDGE WRITE (W1-2 only, NOT crt-021):
  NLI confirmed edge → direct write_pool (NOT analytics queue)
```

---

## Shared Types Introduced / Modified

### New in unimatrix-engine (graph.rs)

```
RelationType          — enum: Supersedes | Contradicts | Supports | CoAccess | Prerequisite
                        as_str() -> &'static str; from_str(s: &str) -> Option<Self>

RelationEdge          — struct: relation_type: String, weight: f32, created_at: i64,
                        created_by: String, source: String, bootstrap_only: bool
                        NOTE: bootstrap_only is on RelationEdge (in-memory) and on
                        GraphEdgeRow (persisted). Both reflect the same field.

TypedRelationGraph    — wraps StableGraph<u64, RelationEdge> + HashMap<u64, NodeIndex>
                        edges_of_type(node_idx, RelationType, Direction) -> Iterator
                        Replaces SupersessionGraph entirely.
```

### New in unimatrix-store (read.rs)

```
GraphEdgeRow          — struct: source_id: u64, target_id: u64, relation_type: String,
                        weight: f32, created_at: i64, created_by: String,
                        source: String, bootstrap_only: bool
                        Passed from store layer to engine layer at rebuild time.
```

### Renamed in unimatrix-server (services/typed_graph.rs)

```
TypedGraphState       — was SupersessionState; adds typed_graph: TypedRelationGraph field
TypedGraphStateHandle — was SupersessionStateHandle; type alias Arc<RwLock<TypedGraphState>>
```

---

## Build Sequencing Constraints

1. `engine-types` has no dependencies on other crt-021 components. Implement first.
2. `store-schema` and `store-migration` both depend on the GRAPH_EDGES DDL being defined. The DDL is identical in both files — define it in both, do not cross-reference.
3. `store-analytics` depends on `GraphEdgeRow` definition (for drain arm SQL column order).
4. `server-state` depends on `TypedRelationGraph` (engine-types), `GraphEdgeRow` and `query_graph_edges` (store-analytics).
5. `background-tick` depends on `TypedGraphState::rebuild` (server-state) and `query_graph_edges` (store-analytics).
6. Run `cargo sqlx prepare` after store-schema and store-analytics are implemented. sqlx-data.json must be committed before CI.

---

## Constants

| Name | Value | Location | Used By |
|------|-------|----------|---------|
| `CO_ACCESS_BOOTSTRAP_MIN_COUNT` | `3_u64` | `migration.rs` | v12→v13 CoAccess bootstrap WHERE clause |
| `CURRENT_SCHEMA_VERSION` | `13` | `migration.rs` | migrate_if_needed guard; final UPDATE |
| `ORPHAN_PENALTY` | `0.75` | `graph.rs` | graph_penalty priority 1 |
| `CLEAN_REPLACEMENT_PENALTY` | `0.40` | `graph.rs` | graph_penalty priority 4/5 |
| `HOP_DECAY_FACTOR` | `0.60` | `graph.rs` | graph_penalty priority 5 |
| `PARTIAL_SUPERSESSION_PENALTY` | `0.60` | `graph.rs` | graph_penalty priority 3 |
| `DEAD_END_PENALTY` | `0.65` | `graph.rs` | graph_penalty priority 2/6 |
| `FALLBACK_PENALTY` | `0.70` | `graph.rs` | cold-start; cycle-detected fallback |
| `MAX_TRAVERSAL_DEPTH` | `10` | `graph.rs` | find_terminal_active DFS depth cap |
