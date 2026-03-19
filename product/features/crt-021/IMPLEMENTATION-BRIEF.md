# crt-021: Typed Relationship Graph (W1-1) — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-021/SCOPE.md |
| Architecture | product/features/crt-021/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-021/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-021/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-021/ALIGNMENT-REPORT.md |
| ADR-001 | product/features/crt-021/architecture/ADR-001-typed-edge-weight-model.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| engine-types | pseudocode/engine-types.md | test-plan/engine-types.md |
| store-schema | pseudocode/store-schema.md | test-plan/store-schema.md |
| store-migration | pseudocode/store-migration.md | test-plan/store-migration.md |
| store-analytics | pseudocode/store-analytics.md | test-plan/store-analytics.md |
| server-state | pseudocode/server-state.md | test-plan/server-state.md |
| background-tick | pseudocode/background-tick.md | test-plan/background-tick.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Replace the untyped, ephemeral `SupersessionGraph` (`StableGraph<u64, ()>`) with a typed,
persisted `TypedRelationGraph` (`StableGraph<u64, RelationEdge>`) backed by a new
`GRAPH_EDGES` SQLite table. Bootstrap the table from existing `entries.supersedes` and
`co_access` data via a v12→v13 schema migration, preserving all existing penalty semantics
(Supersedes edges only drive `graph_penalty`) while establishing the typed graph foundation
that W1-2 (NLI) and W3-1 (GNN) depend on.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Single SQLite file vs separate analytics.db | Single file — GRAPH_EDGES in the same database alongside all other tables | SCOPE.md C-01, entry #2063 | architecture/ADR-001-typed-edge-weight-model.md |
| Single `TypedRelationGraph` vs parallel graphs (Option A vs B) | Option A — one typed graph replaces `SupersessionGraph`; penalty logic filters by edge type internally | SCOPE.md C-02, ARCHITECTURE.md §Overview | architecture/ADR-001-typed-edge-weight-model.md |
| Supersedes-only penalty scoring | `graph_penalty` and `find_terminal_active` filter exclusively to Supersedes edges via `edges_of_type` boundary method | ADR-001 §Decision 2, SPECIFICATION C-03 | architecture/ADR-001-typed-edge-weight-model.md |
| No Contradicts bootstrap | `shadow_evaluations` has no `(entry_id_a, entry_id_b)` pairs (entry #2404, SR-04); AC-08 is a dead path; W1-2 NLI creates all Contradicts edges at runtime | SPECIFICATION C-04, ARCHITECTURE §AC-08 Status | architecture/ADR-001-typed-edge-weight-model.md |
| RelationType string encoding | String variant names (e.g., `"Supersedes"`) — integer discriminants prohibited for GNN and extensibility forward-compatibility | SCOPE.md C-05, ADR-001 §Decision 1 | architecture/ADR-001-typed-edge-weight-model.md |
| CoAccess bootstrap threshold | `CO_ACCESS_BOOTSTRAP_MIN_COUNT = 3`; weight = `COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)` | SPECIFICATION FR-09, ARCHITECTURE §2b | architecture/ADR-001-typed-edge-weight-model.md |
| TypedGraphState rename (~20 call sites) | Rename — no type aliases; compiler enforces complete rename | SPECIFICATION FR-15/NF-07, ARCHITECTURE §3a | architecture/ADR-001-typed-edge-weight-model.md |
| Graph rebuild source | Tick queries persisted `GRAPH_EDGES` rows (not recomputed from canonical sources); Supersedes edges derived from `entries.supersedes` during `build_typed_relation_graph` node pass | SPECIFICATION C-14, ARCHITECTURE §3c | architecture/ADR-001-typed-edge-weight-model.md |
| Prerequisite variant included, no bootstrap | Variant defined for W3-1 forward compatibility; no code path writes Prerequisite edges in crt-021 | SPECIFICATION FR-04/C-12, AC-20 | architecture/ADR-001-typed-edge-weight-model.md |
| `metadata TEXT DEFAULT NULL` column | Added to GRAPH_EDGES v13 DDL for W3-1 GNN per-edge feature vectors; NULL for all edges written in crt-021 | ARCHITECTURE §2a, SPECIFICATION FR-05/AC-04 | architecture/ADR-001-typed-edge-weight-model.md |
| Supersedes edge direction | `source_id = entry.supersedes` (old/replaced entry), `target_id = entry.id` (new/correcting entry) — architecture migration SQL governs; SPECIFICATION FR-08 had it reversed (VARIANCE 1, confirmed by human) | ALIGNMENT-REPORT.md VARIANCE 1, ARCHITECTURE §2b | architecture/ADR-001-typed-edge-weight-model.md |
| TypedGraphState holds pre-built graph | Struct holds `typed_graph: TypedRelationGraph` (not raw `Vec<GraphEdgeRow>`); no per-query rebuild — SPECIFICATION FR-16/FR-22 governs over ARCHITECTURE §3a/3b discrepancy | ALIGNMENT-REPORT.md VARIANCE 2, SPECIFICATION FR-22 | architecture/ADR-001-typed-edge-weight-model.md |
| bootstrap_only=1 promotion path for W1-2 | DELETE + INSERT in a single transaction on direct `write_pool` (not analytics queue); attribution reset to NLI agent | ARCHITECTURE §SR-07, ADR-001 §Decision 4 | architecture/ADR-001-typed-edge-weight-model.md |
| Runtime NLI edge write routing (W1-2 contract) | Direct `write_pool` path — NOT analytics queue — for confirmed (bootstrap_only=false) edges; analytics queue is shed-safe only for bootstrap-path writes | ARCHITECTURE §2c SR-02, ADR-001 Consequences | architecture/ADR-001-typed-edge-weight-model.md |
| ADR-004 (entry #1604) superseded | New ADR stored in Unimatrix (entry #2417, supersedes #1604) before ship | SCOPE.md Goal 9, SPECIFICATION AC-16/FR-27 | architecture/ADR-001-typed-edge-weight-model.md |

---

## Files to Create / Modify

### unimatrix-engine

| File | Change |
|------|--------|
| `crates/unimatrix-engine/src/graph.rs` | Add `RelationType` enum and `as_str`/`from_str` methods; add `RelationEdge` struct with `bootstrap_only` field; define `TypedRelationGraph` wrapping `StableGraph<u64, RelationEdge>`; add `edges_of_type` method; add `build_typed_relation_graph(entries, edges)` function; update `graph_penalty` and `find_terminal_active` signatures to accept `&TypedRelationGraph`; remove `SupersessionGraph` and `build_supersession_graph` |

### unimatrix-store

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/db.rs` | Add `GRAPH_EDGES` DDL (with three indexes) to `create_tables_if_needed` |
| `crates/unimatrix-store/src/migration.rs` | Add `v12 → v13` block: CREATE TABLE, Supersedes bootstrap INSERT, CoAccess bootstrap INSERT, schema_version update to 13; define `CO_ACCESS_BOOTSTRAP_MIN_COUNT = 3` constant |
| `crates/unimatrix-store/src/analytics.rs` | Add `AnalyticsWrite::GraphEdge { .. }` variant; add `"GraphEdge"` to `variant_name()`; add drain task arm with `INSERT OR IGNORE INTO graph_edges` and `weight.is_finite()` guard |
| `crates/unimatrix-store/src/read.rs` | Add `GraphEdgeRow` struct; add `Store::query_graph_edges() -> Result<Vec<GraphEdgeRow>, StoreError>` |
| `sqlx-data.json` (workspace root) | Regenerate via `cargo sqlx prepare` after schema changes |

### unimatrix-server

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/services/supersession.rs` | Rename file to `typed_graph.rs`; rename `SupersessionState` → `TypedGraphState`, `SupersessionStateHandle` → `TypedGraphStateHandle`; change struct to hold `typed_graph: TypedRelationGraph` (not raw rows); update `rebuild` to query `GRAPH_EDGES` via `store.query_graph_edges()` and call `build_typed_relation_graph` |
| `crates/unimatrix-server/src/services/mod.rs` | Update module declaration from `supersession` to `typed_graph` |
| `crates/unimatrix-server/src/background.rs` | Insert GRAPH_EDGES orphaned-edge compaction step (direct `write_pool`); update tick sequence to call `TypedGraphState::rebuild`; update all `SupersessionState`/`SupersessionStateHandle` references |
| `crates/unimatrix-server/src/services/search.rs` | Update to read `typed_graph` from `TypedGraphStateHandle` under read lock; call `graph_penalty` on pre-built graph (no per-query rebuild); update handle type reference |
| `crates/unimatrix-server/src/main.rs` | Update `SupersessionState::new_handle()` → `TypedGraphState::new_handle()`; update all handle type references |
| `crates/unimatrix-server/src/server.rs` | Rename `supersession_handle` field → `typed_graph_handle` in `ServiceLayer` |

---

## Data Structures

### RelationType (unimatrix-engine)

```rust
pub enum RelationType {
    Supersedes,
    Contradicts,
    Supports,
    CoAccess,
    Prerequisite,  // reserved for W3-1; no write path in crt-021
}

impl RelationType {
    pub fn as_str(&self) -> &'static str { ... }
    pub fn from_str(s: &str) -> Option<Self> { ... }
}
```

String values: `"Supersedes"`, `"Contradicts"`, `"Supports"`, `"CoAccess"`, `"Prerequisite"`.

### RelationEdge (unimatrix-engine)

```rust
pub struct RelationEdge {
    pub relation_type:  String,  // RelationType::as_str() value
    pub weight:         f32,     // finite-validated; Supersedes=1.0, CoAccess=count/MAX(count)
    pub created_at:     i64,     // unix epoch seconds
    pub created_by:     String,  // "bootstrap" | agent_id
    pub source:         String,  // "entries.supersedes" | "co_access" | "nli" | "bootstrap"
    pub bootstrap_only: bool,    // true → excluded from TypedRelationGraph during rebuild
}
```

### TypedRelationGraph (unimatrix-engine)

```rust
pub struct TypedRelationGraph {
    pub(crate) inner:      StableGraph<u64, RelationEdge>,
    pub(crate) node_index: HashMap<u64, NodeIndex>,
}

impl TypedRelationGraph {
    /// Single filter-boundary method. All traversal uses this; no direct .edges_directed() calls
    /// in graph_penalty, find_terminal_active, or their helpers.
    pub fn edges_of_type(
        &self,
        node_idx:      NodeIndex,
        relation_type: RelationType,
        direction:     Direction,
    ) -> impl Iterator<Item = EdgeReference<'_, RelationEdge>>;
}
```

### GraphEdgeRow (unimatrix-store)

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
```

### TypedGraphState (unimatrix-server)

```rust
pub struct TypedGraphState {
    /// Pre-built in-memory graph. Rebuilt by tick; never rebuilt per search query.
    pub typed_graph:  TypedRelationGraph,
    /// Entry snapshot at last rebuild. Used by graph_penalty / find_terminal_active.
    pub all_entries:  Vec<EntryRecord>,
    /// Cold-start or cycle-detection fallback. When true, search applies FALLBACK_PENALTY.
    pub use_fallback: bool,
}

pub type TypedGraphStateHandle = Arc<RwLock<TypedGraphState>>;
```

### GRAPH_EDGES DDL

```sql
CREATE TABLE IF NOT EXISTS graph_edges (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id      INTEGER NOT NULL,
    target_id      INTEGER NOT NULL,
    relation_type  TEXT    NOT NULL,
    weight         REAL    NOT NULL DEFAULT 1.0,
    created_at     INTEGER NOT NULL,
    created_by     TEXT    NOT NULL DEFAULT '',
    source         TEXT    NOT NULL DEFAULT '',
    bootstrap_only INTEGER NOT NULL DEFAULT 0,
    metadata       TEXT    DEFAULT NULL,
    UNIQUE(source_id, target_id, relation_type)
);
CREATE INDEX IF NOT EXISTS idx_graph_edges_source_id    ON graph_edges(source_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_target_id    ON graph_edges(target_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_relation_type ON graph_edges(relation_type);
```

---

## Function Signatures

```rust
// unimatrix-engine/src/graph.rs

pub fn build_typed_relation_graph(
    entries: &[EntryRecord],
    edges:   &[GraphEdgeRow],   // from GRAPH_EDGES; bootstrap_only=true rows are skipped
) -> Result<TypedRelationGraph, GraphError>

pub fn graph_penalty(
    node_id: u64,
    graph:   &TypedRelationGraph,
    entries: &[EntryRecord],
) -> f64  // filters to Supersedes edges only; identical semantics to old SupersessionGraph

pub fn find_terminal_active(
    node_id: u64,
    graph:   &TypedRelationGraph,
    entries: &[EntryRecord],
) -> Option<u64>  // filters to Supersedes edges only

// unimatrix-store/src/read.rs

impl Store {
    pub async fn query_graph_edges(&self) -> Result<Vec<GraphEdgeRow>, StoreError>
}

// unimatrix-store/src/analytics.rs

pub enum AnalyticsWrite {
    // ... existing variants ...
    GraphEdge {
        source_id:      u64,
        target_id:      u64,
        relation_type:  String,
        weight:         f32,    // validated finite before enqueue; rejected with ERROR if not
        created_by:     String,
        source:         String,
        bootstrap_only: bool,
    },
}

// unimatrix-server/src/services/typed_graph.rs

impl TypedGraphState {
    pub fn new_handle() -> TypedGraphStateHandle
    pub async fn rebuild(store: &Store) -> Result<TypedGraphState, StoreError>
}
```

---

## Constraints

1. `GRAPH_EDGES` goes in the single SQLite database — no separate file.
2. `TypedRelationGraph` is the only graph; `SupersessionGraph` is removed.
3. `graph_penalty` and `find_terminal_active` use Supersedes edges exclusively.
4. No Contradicts edges written at v12→v13 migration; `shadow_evaluations` has no entry ID pairs.
5. `RelationType` persisted as string — no integer discriminants.
6. ADR-004 (entry #1604) must be superseded (new ADR entry #2417) before ship.
7. GRAPH_EDGES compaction → VECTOR_MAP compaction → TypedGraphState rebuild — strictly sequential; never concurrent.
8. `CURRENT_SCHEMA_VERSION` bumps from 12 to 13.
9. `StableGraph` only — no other petgraph features.
10. `AnalyticsWrite` `#[non_exhaustive]` contract preserved; `GraphEdge` is additive.
11. No type aliases for the SupersessionState → TypedGraphState rename; compiler enforces ~20 call sites.
12. `Prerequisite` variant defined; no write paths in crt-021.
13. `bootstrap_only=1` edges excluded structurally in `build_typed_relation_graph` (not conditionally at traversal sites).
14. `TypedGraphState` holds `typed_graph: TypedRelationGraph` — no per-query rebuild; spec FR-22 governs over architecture §3b discrepancy.
15. Supersedes edge direction: `source_id = entry.supersedes` (old), `target_id = entry.id` (new) — architecture migration SQL governs over SPECIFICATION FR-08 (ALIGNMENT-REPORT VARIANCE 1).
16. `sqlx-data.json` must be regenerated via `cargo sqlx prepare` and committed.
17. `weight: f32` validated finite before every write path; NaN/Inf rejected with logged ERROR.
18. Bootstrap migration inserts use direct SQL — NOT routed through `AnalyticsWrite` queue.
19. Runtime NLI edge writes (W1-2) must use direct `write_pool`, not the analytics queue (shed-safe boundary: analytics queue only for bootstrap-origin writes).

---

## Dependencies

### Crates (no new additions)

| Crate | Feature / Use |
|-------|--------------|
| `petgraph` | `stable_graph` feature — `StableGraph<u64, RelationEdge>` (ADR-001, entry #1601) |
| `sqlx` | `sqlite` feature — `GRAPH_EDGES` DDL, migration, `query_graph_edges`, drain arm |
| `tokio` | `Arc<RwLock<_>>` handle, async tick, `query_graph_edges` call |
| `unimatrix-engine` | `RelationType`, `RelationEdge`, `TypedRelationGraph`, `graph_penalty`, `find_terminal_active` |
| `unimatrix-store` | `GRAPH_EDGES` DDL, migration, `AnalyticsWrite::GraphEdge`, `GraphEdgeRow`, `query_graph_edges` |
| `unimatrix-server` | `TypedGraphState`, `TypedGraphStateHandle`, background tick, search service |

### Unimatrix Knowledge (read before implementation)

| Entry | Content |
|-------|---------|
| #1601 | ADR-001 (petgraph stable_graph feature only) |
| #1604 | ADR-004 (superseded by #2417 — read for penalty constant values and cycle detection contract) |
| #1607 | SupersessionGraph pattern (upgrade path reference) |
| #2063 | Single SQLite file confirmed |
| #2403 | Typed graph upgrade path pattern |
| #2404 | shadow_evaluations has no entry ID pairs (Contradicts bootstrap dead) |
| #2417 | New ADR (typed edge weight model, supersedes #1604) |

---

## NOT in Scope

- NLI inference or any ML model integration — W1-2.
- Exposing graph edges via any MCP tool — graph is internal infrastructure only.
- DOT/GraphViz export endpoint.
- Using non-Supersedes edge weights in `graph_penalty` or confidence scoring — W3-1.
- Changing or removing the `co_access` table.
- Removing `shadow_evaluations`.
- Writing Contradicts edges at bootstrap migration.
- Writing Prerequisite edges by any path.
- bootstrap_only edge promotion logic — W1-2 implements; W1-1 specifies the mechanism (DELETE + INSERT) and schema enables it.
- Batching GRAPH_EDGES compaction per tick — deferred; indexes mitigate cost; re-evaluate post-ship.
- `metadata` JSON structure definition — W3-1 owns; column is NULL for all crt-021 writes.
- PostgreSQL-specific SQL.

---

## Alignment Status

Overall status: **PASS with two resolved variances and one confirmed warning.**

| Item | Status | Detail |
|------|--------|--------|
| Vision alignment | PASS | Closes three High/Medium vision gaps: typed relationships, graph persistence, co-access/contradiction formalization |
| Milestone fit (W1-1) | PASS | W0-1 sqlx prerequisite confirmed complete; no Wave 2/3 scope pulled in |
| Scope additions | WARN (approved) | `metadata TEXT DEFAULT NULL` column added beyond SCOPE.md; pre-known human decision; forward-compat with W3-1 GNN; treated as resolved |
| VARIANCE 1: Supersedes edge direction | RESOLVED (confirmed) | ARCHITECTURE.md migration SQL (`source_id = entry.supersedes`, `target_id = entry.id`) governs over SPECIFICATION FR-08 (which had it reversed). Implementer must follow the architecture SQL direction. |
| VARIANCE 2: TypedGraphState field definition | RESOLVED (spec governs) | SPECIFICATION FR-16/FR-22 governs: struct holds `typed_graph: TypedRelationGraph` (pre-built), not `all_edges: Vec<GraphEdgeRow>`. ARCHITECTURE §3a/3b pseudocode is incorrect and should not be followed. No per-query graph rebuild. |
| VARIANCE 3 (WARN): FR-26 vs ARCHITECTURE SR-07 write path for W1-2 promotion | WARN (W1-2 concern) | SPECIFICATION FR-26 routes W1-2 promotion through `AnalyticsWrite::GraphEdge`; ARCHITECTURE §SR-07 specifies direct `write_pool`. Architecture governs for non-shed-safe writes. W1-2 must not use the analytics queue for confirmed edge writes. W1-1 implementer: no action required; document this boundary in W1-2 handoff. |
| R-15 false alarm in RISK-TEST-STRATEGY | WARN (informational) | RISK-TEST-STRATEGY R-15 incorrectly states SPECIFICATION FR-09 specifies `weight=1.0` (flat). FR-09 correctly specifies the `COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)` formula. Implementer: ignore R-15 description text; implement FR-09 as written. The test assertion in R-15 (count=5 weight > count=3 weight) is still valid and should be exercised. |
| ADR-004 supersession | CONFIRMED | Unimatrix entry #2417 confirmed as the new ADR, superseding #1604. AC-16 verification: `context_lookup` for #2417 must return an active entry; #1604 must be deprecated. |
