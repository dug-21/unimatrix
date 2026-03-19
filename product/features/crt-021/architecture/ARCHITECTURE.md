# crt-021: Typed Relationship Graph — Architecture

## System Overview

`crt-021` (W1-1) upgrades `unimatrix-engine`'s single-edge-type supersession DAG
into a typed, persisted relationship graph. It closes three product vision gaps
simultaneously: typed relationships, graph persistence, and co-access/contradiction
formalization as first-class edges.

The feature spans three crates:

| Crate | Change |
|-------|--------|
| `unimatrix-engine` | `SupersessionGraph` → `TypedRelationGraph`; new types `RelationType`, `RelationEdge`; filter-boundary method `edges_of_type` |
| `unimatrix-store` | New `GRAPH_EDGES` table (DDL + v12→v13 migration with bootstrap inserts); new `AnalyticsWrite::GraphEdge` variant; new store read method `query_graph_edges()` |
| `unimatrix-server` | `SupersessionState` renamed to `TypedGraphState`; handle stores pre-built `TypedRelationGraph`; background tick builds graph once and stores result; search path reads pre-built graph under read lock with no per-query rebuild; background tick gains GRAPH_EDGES compaction step |

This feature is a pure infrastructure upgrade — no MCP tool signatures change,
no external behavior changes. The existing 25+ `graph.rs` unit tests must pass
unchanged against the typed graph.

---

## Component Breakdown

### 1. Engine Types (`unimatrix-engine/src/graph.rs`)

**Responsibility**: Define typed edge types; provide the in-memory graph structure
and all pure traversal/penalty functions.

**New types introduced:**

```rust
/// Five edge types covering the full relationship taxonomy.
/// Stored as strings in GRAPH_EDGES — NOT integer discriminants.
/// String encoding allows extension without schema migration or GNN retraining.
pub enum RelationType {
    Supersedes,
    Contradicts,
    Supports,
    CoAccess,
    Prerequisite,
}

impl RelationType {
    pub fn as_str(&self) -> &'static str { ... }
    pub fn from_str(s: &str) -> Option<Self> { ... }
}

/// Typed edge weight carried by StableGraph<u64, RelationEdge>.
/// All five fields are persisted in GRAPH_EDGES.
pub struct RelationEdge {
    pub relation_type: String, // RelationType::as_str() value
    pub weight: f32,           // finite-validated; default 1.0
    pub created_at: i64,       // unix epoch seconds
    pub created_by: String,    // agent_id or "bootstrap"
    pub source: String,        // "entries.supersedes" | "co_access" | "bootstrap" | "nli"
}

/// Typed relationship graph. Replaces SupersessionGraph.
/// StableGraph chosen for crt-017 forward compatibility (node indices stable on removal).
pub struct TypedRelationGraph {
    pub(crate) inner: StableGraph<u64, RelationEdge>,
    pub(crate) node_index: HashMap<u64, NodeIndex>,
}
```

**Filter boundary — SR-01 mitigation:**

A single method enforces the edge-type boundary. All traversal functions call this;
no ad-hoc type checks are scattered at call sites:

```rust
impl TypedRelationGraph {
    /// Iterator over outgoing edges of the specified type from a given node.
    /// This is the ONLY way penalty logic and traversal functions filter by edge type.
    /// Direct calls to .edges_directed() outside of this method are prohibited in
    /// graph_penalty, find_terminal_active, and their private helpers.
    pub fn edges_of_type(
        &self,
        node_idx: NodeIndex,
        relation_type: RelationType,
        direction: Direction,
    ) -> impl Iterator<Item = EdgeReference<'_, RelationEdge>> {
        self.inner
            .edges_directed(node_idx, direction)
            .filter(move |e| e.weight().relation_type == relation_type.as_str())
    }
}
```

**Function signature changes:**

```rust
// OLD
pub fn build_supersession_graph(entries: &[EntryRecord]) -> Result<SupersessionGraph, GraphError>
pub fn graph_penalty(node_id: u64, graph: &SupersessionGraph, entries: &[EntryRecord]) -> f64
pub fn find_terminal_active(node_id: u64, graph: &SupersessionGraph, entries: &[EntryRecord]) -> Option<u64>

// NEW
pub fn build_typed_relation_graph(
    entries: &[EntryRecord],
    edges: &[GraphEdgeRow],   // loaded from GRAPH_EDGES by the tick
) -> Result<TypedRelationGraph, GraphError>

pub fn graph_penalty(node_id: u64, graph: &TypedRelationGraph, entries: &[EntryRecord]) -> f64
pub fn find_terminal_active(node_id: u64, graph: &TypedRelationGraph, entries: &[EntryRecord]) -> Option<u64>
```

`graph_penalty` and `find_terminal_active` use `edges_of_type(..., RelationType::Supersedes, ...)`
exclusively. Non-Supersedes edges are present in the graph but invisible to all existing
penalty logic. This preserves AC-10 and AC-11 with zero test expectation changes.

`build_typed_relation_graph` has two passes:
1. Node-insertion from `entries` (unchanged from `build_supersession_graph`).
2. Edge-insertion from `edges: &[GraphEdgeRow]` — each row becomes a `RelationEdge`.
   Supersedes edges continue to be derived from `entries.supersedes` (authoritative), not
   from `GRAPH_EDGES` rows, to preserve the existing cycle-detection path on the
   authoritative source. See "Supersedes bootstrap strategy" note below.

### 2. Store Layer (`unimatrix-store`)

**Responsibility**: Schema DDL for `GRAPH_EDGES`, v12→v13 migration with bootstrap inserts,
`AnalyticsWrite::GraphEdge` variant, and `query_graph_edges()` read method.

#### 2a. GRAPH_EDGES DDL (`db.rs` — `create_tables_if_needed`)

```sql
CREATE TABLE IF NOT EXISTS graph_edges (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id       INTEGER NOT NULL,
    target_id       INTEGER NOT NULL,
    relation_type   TEXT    NOT NULL,
    weight          REAL    NOT NULL DEFAULT 1.0,
    created_at      INTEGER NOT NULL,
    created_by      TEXT    NOT NULL DEFAULT '',
    source          TEXT    NOT NULL DEFAULT '',
    bootstrap_only  INTEGER NOT NULL DEFAULT 0,
    metadata        TEXT    DEFAULT NULL,
    UNIQUE(source_id, target_id, relation_type)
);
CREATE INDEX IF NOT EXISTS idx_graph_edges_source_id ON graph_edges(source_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_target_id ON graph_edges(target_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_relation_type ON graph_edges(relation_type);
```

The `UNIQUE(source_id, target_id, relation_type)` constraint enables `INSERT OR IGNORE`
idempotency throughout — bootstrap, compaction re-runs, and W1-2 promotion all use this.

The `bootstrap_only` column is `INTEGER` (0/1 SQLite boolean). `bootstrap_only=1` means the
edge was created from heuristic bootstrap data and must not influence confidence scoring
until W1-2 NLI confirms it.

The `metadata TEXT DEFAULT NULL` column is included in v13 to avoid a v14 migration when
W3-1 (GNN) needs per-edge feature vectors (NLI confidence scores, etc.). Adding it now costs
one DDL line while the migration is already being written. W1-1 writes no metadata values;
consumers must treat NULL as "no metadata." SR-08 is resolved by this column.

#### 2b. v12→v13 Migration (`migration.rs`)

Added to `run_main_migrations` as the `v12 → v13` block:

1. `CREATE TABLE IF NOT EXISTS graph_edges (...)` — idempotent DDL.
2. Bootstrap Supersedes edges from `entries.supersedes`:
   ```sql
   INSERT OR IGNORE INTO graph_edges
       (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
   SELECT
       supersedes AS source_id,
       id         AS target_id,
       'Supersedes',
       1.0,
       strftime('%s','now'),
       'bootstrap',
       'entries.supersedes',
       0   -- authoritative; not heuristic
   FROM entries
   WHERE supersedes IS NOT NULL;
   ```
3. Bootstrap CoAccess edges from `co_access` (count >= CO_ACCESS_BOOTSTRAP_MIN_COUNT = 3):
   ```sql
   INSERT OR IGNORE INTO graph_edges
       (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
   SELECT
       entry_id_a,
       entry_id_b,
       'CoAccess',
       COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0),
       strftime('%s','now'),
       'bootstrap',
       'co_access',
       0   -- promoted co-access is authoritative at threshold >= 3
   FROM co_access
   WHERE count >= 3;
   ```
   **R-06 mitigation**: `COALESCE(..., 1.0)` guards against empty `co_access` table.
   On a clean install with no co-access history, the subquery window returns NULL (no rows
   match `count >= 3`), so the INSERT selects zero rows and the COALESCE is never reached.
   On a populated table, `NULLIF(MAX(count) OVER (), 0)` guards against a theoretical
   all-zero count table (division by zero → NULL → COALESCE → 1.0). The result is a
   normalized weight in `(0.0, 1.0]` with 1.0 reserved for the most-accessed pair.
4. No Contradicts bootstrap. AC-08 is a dead path — `shadow_evaluations` does not store
   `(entry_id_a, entry_id_b)` pairs (confirmed by entry #2404, SR-04). All Contradicts
   edges are created at runtime by W1-2 NLI. The migration creates zero Contradicts rows.
5. Update schema version:
   ```sql
   UPDATE counters SET value = 13 WHERE name = 'schema_version';
   ```

#### 2c. `AnalyticsWrite::GraphEdge` variant (`analytics.rs`)

```rust
/// Table: `graph_edges` — idempotent INSERT OR IGNORE via UNIQUE constraint.
///
/// SHEDDING POLICY: This variant is shed-safe when bootstrap_only=true
/// (migration re-creates these rows on next startup). When bootstrap_only=false,
/// the write represents a W1-2 NLI-confirmed edge — see SR-02 note below.
GraphEdge {
    source_id:     u64,
    target_id:     u64,
    relation_type: String,   // RelationType::as_str()
    weight:        f32,      // must be finite; validated before enqueue
    created_by:    String,
    source:        String,
    bootstrap_only: bool,
},
```

The drain task arm executes:
```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
```

**SR-02 — Write routing boundary (W1-2 concern, documented here):**

Bootstrap-path `GraphEdge` writes (migration-time bootstrap inserts) are idempotent via
`INSERT OR IGNORE` and are therefore shed-safe — if dropped under queue pressure, the
migration re-creates them on next startup.

Runtime `GraphEdge` writes created by W1-2 NLI (confirmed edges, `bootstrap_only=false`)
are NOT shed-safe. They cannot be re-derived from canonical sources. **W1-2 must not route
NLI-confirmed edge writes through the analytics queue.** W1-2 must instead use a direct
`write_pool` path for these writes, bypassing the shed queue. This architectural decision
is documented now so W1-2 does not inherit the risk silently. The current `GraphEdge`
variant is acceptable for W1-1 (only bootstrap writes occur); W1-2 must introduce either:
- A separate `write_pool`-direct call in `store.rs`, or
- A flag on `GraphEdge` (e.g. `shed_safe: bool`) that the drain task uses to error rather
  than silently discard on queue saturation.

The preferred approach is a **direct `write_pool` path** for W1-2 confirmed edges,
keeping the analytics queue for fire-and-forget bootstrap writes only. This avoids
complicating the drain task with per-event shedding semantics.

#### 2d. `GraphEdgeRow` struct and `query_graph_edges()` (`read.rs`)

The tick rebuild needs to SELECT all GRAPH_EDGES rows:

```rust
pub struct GraphEdgeRow {
    pub source_id:      u64,
    pub target_id:      u64,
    pub relation_type:  String,
    pub weight:         f32,
    pub created_at:     i64,
    pub created_by:     String,
    pub source:         String,
    pub bootstrap_only: bool,
}

impl Store {
    pub async fn query_graph_edges(&self) -> Result<Vec<GraphEdgeRow>, StoreError> {
        // SELECT all rows from graph_edges via read_pool
    }
}
```

### 3. Server State Handle (`unimatrix-server/src/services/`)

**Responsibility**: Replace `SupersessionState`/`SupersessionStateHandle` with
`TypedGraphState`/`TypedGraphStateHandle`; extend the rebuild to also query `GRAPH_EDGES`.

#### 3a. Rename (`supersession.rs` → `typed_graph.rs`)

File renamed. The type `SupersessionState` becomes `TypedGraphState`:

```rust
pub struct TypedGraphState {
    /// Pre-built typed relation graph. Rebuilt by the background tick once from
    /// GRAPH_EDGES and the entries snapshot, then stored here. The search hot path
    /// reads this pre-built graph under a short read lock — it does NOT rebuild per query.
    /// This is the crt-014 fix pattern: no spawn_blocking or graph construction on the
    /// hot path.
    pub typed_graph: TypedRelationGraph,

    /// Snapshot of all entries at last rebuild time.
    /// Kept alongside the pre-built graph for graph_penalty / find_terminal_active calls.
    pub all_entries: Vec<EntryRecord>,

    /// Cold-start or cycle-detection fallback flag.
    pub use_fallback: bool,
}

pub type TypedGraphStateHandle = Arc<RwLock<TypedGraphState>>;
```

**Call site rename surface (~20 sites):**

| File | Old symbol | New symbol |
|------|-----------|-----------|
| `services/supersession.rs` | entire file | renamed to `services/typed_graph.rs` |
| `services/mod.rs` | `supersession` module | `typed_graph` module |
| `background.rs` | `SupersessionState`, `SupersessionStateHandle` | `TypedGraphState`, `TypedGraphStateHandle` |
| `main.rs` | `SupersessionState::new_handle()` | `TypedGraphState::new_handle()` |
| `services/search.rs` | `SupersessionStateHandle`, `use_fallback`, `all_entries` | updated to use `typed_graph` + `all_entries` |
| `server.rs` (ServiceLayer) | `supersession_handle` field | `typed_graph_handle` field |

The compiler enforces the complete rename — no type aliases permitted.

#### 3b. Search path usage

The search path reads the pre-built `TypedRelationGraph` from `TypedGraphState` under
a short read lock, then calls `graph_penalty` directly on the pre-built graph.
`build_typed_relation_graph` is NOT called on the search hot path — FR-22 requires
it to never rebuild the graph per query.

```rust
// In SearchService::apply_graph_penalty (pseudocode):
let (graph, entries, use_fallback) = {
    let guard = self.typed_graph_handle
        .read()
        .unwrap_or_else(|e| e.into_inner());
    (guard.typed_graph.clone(), guard.all_entries.clone(), guard.use_fallback)
};

if use_fallback {
    return FALLBACK_PENALTY;
}

graph_penalty(node_id, &graph, &entries)
```

The read lock is held only for the duration of the clone. All graph traversal
(`graph_penalty`, `find_terminal_active`) runs outside the lock on the cloned snapshot.

**AC-12 (bootstrap_only exclusion)**: `build_typed_relation_graph` (called by the
background tick, not the search path) filters out `bootstrap_only=true` edges from the
in-memory graph entirely. They are never added to `TypedRelationGraph.inner`. This means
no code path in `graph_penalty` or `find_terminal_active` can ever encounter a
bootstrap-only edge — the exclusion is structural, not conditional.

#### 3c. Background tick rebuild

The background tick is the sole site that calls `build_typed_relation_graph`. After
compaction, the tick queries both tables, builds the graph once, and stores the result
in the handle under a write lock:

```rust
// In background tick (pseudocode), after GRAPH_EDGES compaction:
let all_entries = store.query_all_entries().await?;
let all_edges   = store.query_graph_edges().await?;

let new_state = match build_typed_relation_graph(&all_entries, &all_edges) {
    Ok(typed_graph) => TypedGraphState { typed_graph, all_entries, use_fallback: false },
    Err(GraphError::CycleDetected) => {
        // Preserve last known good graph; set fallback flag.
        let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
        guard.use_fallback = true;
        return;
    }
};

{
    let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
    *guard = new_state;
}
```

The write lock is held only for the final swap. Graph construction (the expensive step)
runs before the lock is acquired.

### 4. Background Tick (`unimatrix-server/src/background.rs`)

**Responsibility**: Insert GRAPH_EDGES compaction step before the typed graph rebuild;
ensure strict sequencing per product vision constraint.

**New tick sequence (updated `maintenance_tick` flow):**

```
1. maintenance_tick (existing: co-access cleanup, confidence refresh, etc.)
2. GRAPH_EDGES orphaned-edge compaction (NEW — direct write_pool, not analytics queue)
3. Background tick queries entries + GRAPH_EDGES, calls build_typed_relation_graph, writes result into TypedGraphStateHandle (write lock)
4. Contradiction scan (unchanged)
```

**GRAPH_EDGES compaction (step 2):**

```rust
// Direct write_pool — maintenance writes bypass analytics queue.
// Bounded: deletes only orphaned edges (entries deleted since last tick).
// On large graphs this is a table scan; see SR-03 note.
sqlx::query(
    "DELETE FROM graph_edges
     WHERE source_id NOT IN (SELECT id FROM entries)
        OR target_id NOT IN (SELECT id FROM entries)"
)
.execute(&store.write_pool)
.await?;
```

**SR-03 — Compaction cost mitigation:**

The orphaned-edge DELETE is a full-table scan against `entries`. On large deployments
this may be non-trivial. Mitigation: the existing indexes on `source_id` and `target_id`
make the NOT IN lookup efficient for most cases. The compaction runs through
`TICK_TIMEOUT` along with other maintenance work. If profiling shows tick budget
exhaustion, the implementer may limit the DELETE to a batched form (DELETE WHERE id IN
(SELECT id FROM graph_edges WHERE ... LIMIT 500)) and run every N ticks. This is a
post-ship optimization unless testing reveals a regression against `TICK_TIMEOUT`.

**Compaction and rebuild must never run concurrently.** The existing tick is single-threaded
sequential — steps 2 and 3 above execute serially within the same tick iteration.

---

## Data Flow

```
STARTUP (v12→v13 migration):
  entries.supersedes ──────────────┐
  co_access (count >= 3) ──────────┼──► INSERT OR IGNORE INTO graph_edges
  Contradicts: EMPTY (AC-08 dead) ─┘

BACKGROUND TICK (every 15 min):
  write_pool ──► DELETE orphaned graph_edges (compaction)
                 │
                 ▼
  read_pool ──► SELECT * FROM entries     ─┐
  read_pool ──► SELECT * FROM graph_edges ─┼──► build_typed_relation_graph()
                                            │   [bootstrap_only=1 edges excluded]
                                            │              │
                                            │              ▼
                                            └──► TypedGraphState { typed_graph, all_entries }
                                                              │
                                                              ▼ (write lock)
                                                  TypedGraphStateHandle updated

SEARCH HOT PATH (per query):
  TypedGraphStateHandle (read lock) ──► clone typed_graph, all_entries
                                             │
                                             ▼ (outside lock — NO graph rebuild)
                                     graph_penalty()
                                  [filters to Supersedes edges only]

RUNTIME EDGE WRITE (W1-2, NOT this feature):
  NLI inference result ──► direct write_pool (NOT analytics queue) ──► graph_edges row
```

---

## SR-07: Bootstrap-Only Promotion Path

SR-07 requires the promotion mechanism to be designed in W1-1 so W1-2 is not blocked.

**Mechanism: DELETE + INSERT OR IGNORE via direct `write_pool`**

W1-2 promotes a `bootstrap_only=1` edge to a confirmed edge using:

```sql
-- Step 1: Remove the bootstrap-only edge
DELETE FROM graph_edges
WHERE source_id = ?1 AND target_id = ?2 AND relation_type = ?3 AND bootstrap_only = 1;

-- Step 2: Insert the confirmed edge (same UNIQUE key, bootstrap_only=0)
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'nli', 0);
```

These two statements execute in the same transaction on the direct `write_pool` path.

**Why DELETE + INSERT rather than UPDATE:**

`UPDATE graph_edges SET bootstrap_only=0 WHERE ...` would work mechanically, but it leaves
`created_by` and `source` pointing to "bootstrap" when the edge is now NLI-confirmed.
DELETE + INSERT correctly attributes the confirmed edge to the NLI agent with `source='nli'`
and resets `created_at` to the confirmation timestamp. The `UNIQUE` constraint prevents
duplicates.

**W1-1 deliverable**: The `GRAPH_EDGES` schema supports this pattern with zero additional
columns. No promotion API needs to be added to W1-1 — W1-2 uses the direct `write_pool`
path defined above. W1-1 must not ship any UPDATE path that sets `bootstrap_only=0`, as
that would allow MCP tool callers to promote unconfirmed edges.

---

## Integration Points

### Existing components — unchanged behavior

| Component | Interaction | Change |
|-----------|-------------|--------|
| `context_search` MCP tool | Calls `SearchService::search()` | No change |
| `SearchService` | Calls `graph_penalty()` | Accepts `TypedRelationGraph` instead of `SupersessionGraph`; results identical |
| `EffectivenessStateHandle` | Background tick | No change |
| `ContradictionScanCacheHandle` | Background tick | No change |
| `VectorIndex::compact()` | Runs in maintenance tick before graph rebuild | No change; VECTOR_MAP compaction still precedes rebuild |
| `shadow_evaluations` table | Raw contradiction signal store | No change; Contradicts edges are NOT bootstrapped from it |
| `co_access` table | Primary co-access affinity store | No change; GRAPH_EDGES CoAccess entries are a promoted view only |

### New dependencies introduced

| Dependency | Direction | Notes |
|-----------|-----------|-------|
| `store.query_graph_edges()` | `background.rs` → `unimatrix-store` | New read method; async |
| `AnalyticsWrite::GraphEdge` | caller → `unimatrix-store` | New variant; shed-safe in W1-1 only |
| `GraphEdgeRow` | `unimatrix-store` → `unimatrix-engine` | New struct in store; passed to engine |
| `build_typed_relation_graph` | `unimatrix-server` → `unimatrix-engine` | Replaces `build_supersession_graph` |

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|----------------|--------|
| `RelationType` | `pub enum` with `as_str() -> &'static str` and `from_str(s: &str) -> Option<Self>` | `unimatrix-engine/src/graph.rs` |
| `RelationEdge` | `pub struct { relation_type: String, weight: f32, created_at: i64, created_by: String, source: String }` | `unimatrix-engine/src/graph.rs` |
| `TypedRelationGraph` | `pub struct { inner: StableGraph<u64, RelationEdge>, node_index: HashMap<u64, NodeIndex> }` | `unimatrix-engine/src/graph.rs` |
| `TypedRelationGraph::edges_of_type` | `fn(&self, NodeIndex, RelationType, Direction) -> impl Iterator<Item = EdgeReference<'_, RelationEdge>>` | `unimatrix-engine/src/graph.rs` |
| `build_typed_relation_graph` | `fn(entries: &[EntryRecord], edges: &[GraphEdgeRow]) -> Result<TypedRelationGraph, GraphError>` | `unimatrix-engine/src/graph.rs` |
| `graph_penalty` | `fn(node_id: u64, graph: &TypedRelationGraph, entries: &[EntryRecord]) -> f64` | `unimatrix-engine/src/graph.rs` |
| `find_terminal_active` | `fn(node_id: u64, graph: &TypedRelationGraph, entries: &[EntryRecord]) -> Option<u64>` | `unimatrix-engine/src/graph.rs` |
| `GraphEdgeRow` | `pub struct { source_id: u64, target_id: u64, relation_type: String, weight: f32, created_at: i64, created_by: String, source: String, bootstrap_only: bool }` | `unimatrix-store/src/read.rs` |
| `Store::query_graph_edges` | `async fn(&self) -> Result<Vec<GraphEdgeRow>, StoreError>` | `unimatrix-store/src/read.rs` |
| `AnalyticsWrite::GraphEdge` | `GraphEdge { source_id: u64, target_id: u64, relation_type: String, weight: f32, created_by: String, source: String, bootstrap_only: bool }` | `unimatrix-store/src/analytics.rs` |
| `TypedGraphState` | `pub struct { typed_graph: TypedRelationGraph, all_entries: Vec<EntryRecord>, use_fallback: bool }` | `unimatrix-server/src/services/typed_graph.rs` |
| `TypedGraphStateHandle` | `Arc<RwLock<TypedGraphState>>` | `unimatrix-server/src/services/typed_graph.rs` |
| `GRAPH_EDGES` DDL | `UNIQUE(source_id, target_id, relation_type)`, `metadata TEXT DEFAULT NULL` — see §2a for full DDL | `unimatrix-store/src/db.rs` |
| `CO_ACCESS_BOOTSTRAP_MIN_COUNT` | `const: u64 = 3` | `unimatrix-store/src/migration.rs` |

---

## Technology Decisions

See `ADR-001-typed-edge-weight-model.md` for the primary decision superseding ADR-004.

**petgraph `StableGraph`**: Retained unchanged per ADR-001 (crt-014, entry #1601).
`StableGraph<u64, RelationEdge>` — the only change is the edge weight type from `()` to
`RelationEdge`.

**String encoding for RelationType**: Per product vision constraint and SCOPE.md §Constraints #5.
Integer discriminants are prohibited. Strings allow new types without schema migration or
GNN feature vector changes.

**Single graph, all types**: Option A per locked design decision #2. `TypedRelationGraph`
replaces `SupersessionGraph`; there is no parallel graph for non-Supersedes edges.

**`edges_of_type` filter pattern**: Centralizes edge-type filtering at one method rather
than ad-hoc checks in each traversal function. Enforces SR-01 mitigation at the API level.

---

## AC-08 Status: Dead Path

AC-08 (bootstrap Contradicts edges from `shadow_evaluations`) is a dead acceptance criterion.

`shadow_evaluations` stores `(rule_name, rule_category, neural_category, neural_confidence,
convention_score, rule_accepted, digest)`. It does not store `(entry_id_a, entry_id_b)` pairs.
Entry #2404 and SR-04 confirm this. There is no feasible join that produces entry ID pairs
from this data.

**Resolution**: The v12→v13 migration creates zero Contradicts rows. All Contradicts edges
are created at runtime by W1-2 NLI. AC-08 must be closed by the spec writer as:
"Empty bootstrap — no Contradicts edges at migration. W1-2 NLI creates all Contradicts
edges at runtime via direct write_pool."

---

## Open Questions for Spec Writer and Implementer

1. **`build_typed_relation_graph` — Supersedes edge source strategy**: The design calls for
   Supersedes edges to be derived from `entries.supersedes` (authoritative) during graph
   construction, not from `GRAPH_EDGES` rows, to preserve the cycle-detection path. This
   means `GRAPH_EDGES` Supersedes rows are written at migration for persistence/attribution
   but are not the source of truth for graph construction. The implementer should confirm this
   is the intended behavior — it simplifies cycle detection but means `GRAPH_EDGES` Supersedes
   rows are not the sole source of graph topology.

2. **`sqlx-data.json` regeneration (SR-09)**: The v12→v13 migration and new `GRAPH_EDGES`
   queries require `sqlx-data.json` regeneration via `cargo sqlx prepare`. A CI check
   must validate the cache is current. This should be an explicit AC in the spec.

~~SR-08 (W3-1 GNN per-edge feature storage): RESOLVED — `metadata TEXT DEFAULT NULL` added
to `GRAPH_EDGES` DDL in v13. See §2a.~~

~~R-06 (CoAccess bootstrap weight NULL on empty table): RESOLVED — `COALESCE(..., 1.0)`
guard with window-function normalization. See §2b step 3.~~
