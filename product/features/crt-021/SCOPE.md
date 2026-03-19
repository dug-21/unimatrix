# crt-021: Typed Relationship Graph (W1-1)

## Problem Statement

The existing `SupersessionGraph` (`StableGraph<u64, ()>`) captures only one edge
type — supersession — and is never persisted. Every restart discards all graph
knowledge. Co-access relationships (already stored in `co_access`) and contradiction
signals (already stored in `shadow_evaluations`) are collected but never promoted to
first-class graph edges, so they cannot participate in topology-derived confidence
scoring or serve as training features for the future GNN (W3-1).

The absence of typed, persisted edges is listed in the product vision under
"Intelligence & Confidence" gaps (severity High for typed relationships, Medium for
persistence and co-access/contradiction formalization). W1-1 closes all three gaps
in a single feature and establishes the graph foundation that W1-2 (NLI) and W3-1
(GNN) depend on.

## Goals

1. Replace `SupersessionGraph` (`StableGraph<u64, ()>`) with `TypedRelationGraph`
   (`StableGraph<u64, RelationEdge>`) in `unimatrix-engine` — single graph, all edge
   types, functionally identical penalty logic.
2. Define `RelationType` enum (Supersedes, Contradicts, Supports, CoAccess,
   Prerequisite) with string persistence encoding.
3. Define `RelationEdge` struct (`relation_type: String, weight: f32, created_at:
   i64, created_by: String, source: String`) — stored in `GRAPH_EDGES` table in the
   single SQLite database.
4. Add `GRAPH_EDGES` DDL to `db.rs` (`create_tables_if_needed`) and a
   `v12 → v13` schema migration in `migration.rs`.
5. Add `AnalyticsWrite::GraphEdge { .. }` variant to `analytics.rs` and a
   corresponding `execute_analytics_write` arm in the drain task.
6. Bootstrap `GRAPH_EDGES` from existing data on first migration: `Supersedes` from
   `entries.supersedes`, `Contradicts` from `shadow_evaluations` (bootstrap-flagged),
   `CoAccess` from high-count `co_access` pairs.
7. Rebuild the in-memory `TypedRelationGraph` from `GRAPH_EDGES` in the background
   tick, following the `Arc<RwLock<_>>` tick-rebuild pattern (crt-014 fix, GH #264).
8. Preserve all existing penalty semantics: `graph_penalty`, `find_terminal_active`,
   cycle detection — unchanged in behavior, operating on the typed graph by filtering
   Supersedes edges.
9. Supersede ADR-004 (crt-014, entry #1604) with a new ADR documenting the typed
   edge weight model before the feature ships.

## Non-Goals

- NLI inference or any ML model integration — that is W1-2. Bootstrap contradiction
  edges come from existing `shadow_evaluations` data only.
- Exposing graph edges via any MCP tool. Graph is internal infrastructure only.
- DOT/GraphViz export endpoint, even though petgraph supports it natively. That is
  a future opportunity, not W1-1 scope.
- Updating confidence scoring to use edge weights from non-Supersedes edge types.
  The `weight: f32` field is persisted and available, but only Supersedes topology
  drives `graph_penalty` in this feature. Other edge types become scoring inputs in
  W3-1 (GNN).
- Changing the `co_access` table schema or removing it. `co_access` remains the
  primary store for co-access affinity; `GRAPH_EDGES` CoAccess entries are a
  promoted, attributed, graph-layer view of high-count pairs.
- Removing `shadow_evaluations`. That table remains the raw contradiction signal
  store; `GRAPH_EDGES` Contradicts entries are a promoted view for bootstrap and
  future NLI confirmation.
- PostgreSQL-specific SQL. GRAPH_EDGES DDL is SQLite-compatible per the W0-1
  single-binary model; PostgreSQL compatibility follows naturally from sqlx's
  unified API.
- Automatic compaction of `GRAPH_EDGES` (orphaned edge pruning beyond what the
  bootstrap migration installs). Compaction runs in the maintenance tick and removes
  edges whose source or target entry IDs no longer exist in `entries`.

## Background Research

### Existing Graph Implementation (unimatrix-engine/src/graph.rs)

- `SupersessionGraph` wraps `StableGraph<u64, ()>` with a `node_index: HashMap<u64,
  NodeIndex>` for O(1) lookup.
- `StableGraph` was chosen over `Graph` for crt-017 forward compatibility — node
  indices remain stable when nodes are removed. This is the correct choice to retain.
- `graph_penalty` dispatches on 6 priority cases using outgoing edge counts and BFS/DFS
  traversal. The priority logic is pure CPU — no I/O. All traversals use Supersedes
  edges exclusively (all edges in the current `()` graph are Supersedes by definition).
- `find_terminal_active` is an iterative DFS capped at `MAX_TRAVERSAL_DEPTH = 10`.
- Both `build_supersession_graph` and `graph_penalty` take `&[EntryRecord]` — they
  are pure functions with no store dependency.
- The upgrade path is: change `StableGraph<u64, ()>` to `StableGraph<u64,
  RelationEdge>`; filter to Supersedes edges when calling penalty logic. The function
  signatures for `graph_penalty` and `find_terminal_active` will need to accept the
  typed graph (or a filtered view).
- The test suite at the bottom of graph.rs is comprehensive — 25+ unit tests covering
  all penalty scenarios, depth-cap behavior, and cycle detection. These must all pass
  against the typed graph.

### SupersessionState Cache Pattern (supersession.rs)

- `SupersessionState { all_entries: Vec<EntryRecord>, use_fallback: bool }` is the
  existing in-memory tick-rebuild cache.
- The background tick calls `SupersessionState::rebuild(&store)` which does a single
  async SQL SELECT of all entries.
- This needs to become `TypedRelationGraphState` (or be extended) to also load
  `GRAPH_EDGES` from the database and reconstruct the full `TypedRelationGraph`.
- The cold-start `use_fallback: true` path applies `FALLBACK_PENALTY` directly —
  this behavior is unchanged.
- The `Arc<RwLock<_>>` + `.unwrap_or_else(|e| e.into_inner())` poison-recovery
  pattern is established convention across all three handles
  (`EffectivenessStateHandle`, `SupersessionStateHandle`, `ContradictionScanCacheHandle`).

### Background Tick (background.rs)

- The supersession state rebuild runs after `maintenance_tick` completes, wrapped in
  `TICK_TIMEOUT`. The new `TypedRelationGraph` rebuild must follow the same sequencing:
  run after `GRAPH_EDGES` compaction (orphaned edge cleanup), then rebuild in-memory.
- Product vision states: "GRAPH_EDGES cleanup and VECTOR_MAP compaction must both
  complete before the tick triggers an in-memory rebuild. Sequence within the tick;
  never run concurrently."
- The tick currently has an explicit sequence: maintenance → supersession rebuild →
  contradiction scan. The GRAPH_EDGES compaction step inserts between maintenance
  and the graph rebuild.

### Analytics Write Queue (analytics.rs)

- `AnalyticsWrite` is `#[non_exhaustive]` with a `_ => {}` catch-all in both
  `variant_name()` and `execute_analytics_write()`. Adding `GraphEdge` is a clean
  additive extension — no breaking changes.
- The `GraphEdge` variant is already stubbed in a comment: `// W1-1 adds: GraphEdge
  { .. }` at line 164.
- The drain task runs in a single `write_pool` transaction per batch (up to 50 events
  or 500ms). `GraphEdge` writes go through this path — under load they can be shed
  (analytics shed policy). This is acceptable because `GRAPH_EDGES` is rebuilt from
  canonical sources (entries.supersedes, co_access, shadow_evaluations) on bootstrap.
- Edge integrity writes (attribution, `created_by`) are carried in the `GraphEdge`
  variant fields themselves — no separate audit-log path is needed for analytics-side
  edge writes, consistent with how `CoAccess` is handled.

### Schema (migration.rs)

- Current `CURRENT_SCHEMA_VERSION = 12` (v11→v12 added `keywords` column on
  `sessions`).
- crt-021 adds `GRAPH_EDGES` table → schema version 13. This is a purely additive
  migration (no column changes, no data rewrites on existing tables except the
  bootstrap INSERT into the new table).
- Migration pattern: `CREATE TABLE IF NOT EXISTS graph_edges (...)` in the `v12 →
  v13` block of `run_main_migrations`, followed by bootstrap SELECTs from entries,
  co_access, and shadow_evaluations.
- The bootstrap is idempotent via `INSERT OR IGNORE` keyed on `(source_id, target_id,
  relation_type)` — running the migration twice does not duplicate edges.

### Co-access Bootstrap Source

- `store.top_co_access_pairs(n, staleness_cutoff)` already exists in `read.rs`
  (line 599). It returns `Vec<((u64, u64), CoAccessRecord)>` sorted by count DESC.
- The bootstrap threshold for CoAccess edges needs to be defined (open question: what
  minimum count qualifies a co-access pair for promotion to a graph edge?). The
  product vision says "high-count co_access pairs" without defining the threshold.

### Shadow Evaluations Bootstrap Source

- `shadow_evaluations` stores NLI shadow evaluation results from the existing
  convention-scorer pipeline. Fields: `timestamp, rule_name, rule_category,
  neural_category, neural_confidence, convention_score, rule_accepted, digest`.
- There is no existing store method to bulk-read shadow_evaluations for bootstrap
  — one needs to be added, or the migration uses raw SQL directly (acceptable for a
  one-time migration).
- Bootstrap Contradicts edges carry `source: "bootstrap"` and a `bootstrap_only:
  true` flag (stored as a column or encoded in source string) to exclude them from
  confidence scoring until W1-2 NLI confirms them.
- The `shadow_evaluations` table does not store `(entry_id_a, entry_id_b)` directly
  — it stores rule/category evaluation results. **The mapping from shadow evaluation
  rows to entry ID pairs needs clarification** (open question below).

### Single-File Topology Confirmation (entry #2063)

- Unimatrix entry #2063 explicitly documents the scope-gap risk: "analytics.db" in
  the product vision means tables in the same SQLite file, not a separate database.
- This is confirmed as a pre-decided constraint: `GRAPH_EDGES` goes in the same
  database (the single `knowledge.db` / `analytics.db` is one file) alongside all
  other tables.

### ADR-004 (entry #1604): Topology-Derived Penalty Scoring

- ADR-004 documents that penalty scoring is topology-derived and `()` edge weights
  are used. W1-1's upgrade to `weight: f32` renders this ADR architecturally
  inconsistent.
- The product vision explicitly states: "ADR #1604 must be explicitly superseded with
  a new ADR before W1-1 ships or the penalty computation is architecturally
  inconsistent."
- The new ADR must document: typed edge weights, why penalty logic filters to
  Supersedes edges only (other edge types are not yet scoring inputs), and the
  bootstrap edge exclusion policy for Contradicts edges.

## Proposed Approach

### Phase 1: Data Model (unimatrix-engine, unimatrix-store)

Define `RelationType` and `RelationEdge` in `unimatrix-engine`. Upgrade
`SupersessionGraph` to `TypedRelationGraph` by changing the edge type from `()` to
`RelationEdge`. Update all functions that construct or traverse the graph. Existing
penalty logic filters to `relation_type == "Supersedes"` edges — behaviorally
identical.

Add `GRAPH_EDGES` DDL to `create_tables_if_needed` in `db.rs`. Add v12→v13
migration in `migration.rs` with bootstrap inserts. Add `AnalyticsWrite::GraphEdge`
variant and drain task arm in `analytics.rs`.

### Phase 2: State Handle Upgrade (unimatrix-server)

Upgrade `SupersessionState` to carry both `all_entries` (for Supersedes graph
construction from live `entries.supersedes`) and the full `TypedRelationGraph` built
from `GRAPH_EDGES`. The rebuild path queries `GRAPH_EDGES` from the database in
addition to all entries. The search path reads the typed graph under the read lock,
unchanged.

Alternatively (preferred): rename `SupersessionState` to `TypedGraphState` and
update all call sites. The rename signals the semantic upgrade without adding a
second cache handle — one graph, all edge types.

### Phase 3: Background Tick Integration

Insert a `GRAPH_EDGES` compaction step (orphaned edge cleanup via `DELETE FROM
graph_edges WHERE source_id NOT IN (SELECT id FROM entries) OR target_id NOT IN
(SELECT id FROM entries)`) before the supersession state rebuild. Compaction runs
through the `write_pool` directly (not the analytics queue — it is a maintenance
write). The rebuild always sees post-compaction consistent state.

### Phase 4: ADR + Tests

Write the new ADR superseding #1604 before or concurrent with implementation. Extend
existing `graph.rs` tests to cover typed edge construction, Supersedes filter
behavior, and bootstrap-edge exclusion from confidence scoring. Add migration test
for v12→v13 (consistent with existing migration integration test pattern).

### Key Design Choices

- **String encoding for RelationType**: `"Supersedes"`, `"Contradicts"`, `"Supports"`,
  `"CoAccess"`, `"Prerequisite"`. Not integer discriminants. Rationale: integer
  encoding locks extensibility; adding a new type would require schema migration AND
  GNN feature vector changes (W3-1). String allows extension without either.
- **Single graph, not two**: One `TypedRelationGraph` supersedes `SupersessionGraph`.
  Penalty logic filters to Supersedes edges internally. Not two separate graphs.
- **Bootstrap Contradicts edges excluded from scoring**: `source: "bootstrap"` edges
  are never passed to `graph_penalty`. The product vision is explicit: bootstrap
  contradiction edges come from cosine heuristics known to produce false positives.
  They must not penalize valid entries until W1-2 NLI confirms them.
- **Analytics queue for edge writes; direct write_pool for compaction**: New
  `GraphEdge` events from runtime (e.g., future W1-2 NLI creating edges) go through
  the analytics queue. Maintenance-tick compaction (orphaned edge DELETE) goes
  directly through write_pool — it is a bounded, tick-scoped operation.
- **`weight: f32` validation**: Validate finite (not NaN, not ±Inf) before
  persisting. A NaN weight propagated into confidence scoring corrupts search
  rankings silently.

## Acceptance Criteria

- AC-01: `TypedRelationGraph` (`StableGraph<u64, RelationEdge>`) replaces
  `SupersessionGraph` (`StableGraph<u64, ()>`) in `unimatrix-engine/src/graph.rs`.
  All existing graph.rs unit tests pass without modification to test expectations.
- AC-02: `RelationType` enum defines exactly five variants: Supersedes, Contradicts,
  Supports, CoAccess, Prerequisite. Each serializes to its variant name as a string
  (e.g., `"Supersedes"`). Round-trip `RelationType → String → RelationType`
  succeeds for all five variants.
- AC-03: `RelationEdge` struct carries `relation_type: String, weight: f32,
  created_at: i64, created_by: String, source: String`. `weight` is validated finite
  before any persist path accepts it.
- AC-04: `GRAPH_EDGES` table exists in the database schema after running
  `create_tables_if_needed` on a fresh database. DDL: `(id INTEGER PRIMARY KEY
  AUTOINCREMENT, source_id INTEGER NOT NULL, target_id INTEGER NOT NULL,
  relation_type TEXT NOT NULL, weight REAL NOT NULL DEFAULT 1.0, created_at INTEGER
  NOT NULL, created_by TEXT NOT NULL DEFAULT '', source TEXT NOT NULL DEFAULT '',
  bootstrap_only INTEGER NOT NULL DEFAULT 0, UNIQUE(source_id, target_id,
  relation_type))`.
- AC-05: A v12→v13 migration runs on an existing v12 database. After migration:
  `CURRENT_SCHEMA_VERSION` is 13, `GRAPH_EDGES` exists, and bootstrap Supersedes
  edges from `entries.supersedes` are present in `GRAPH_EDGES`.
- AC-06: Bootstrap inserts from `entries.supersedes` produce one
  `relation_type="Supersedes"` edge per supersession link. `bootstrap_only=0` for
  Supersedes edges (these are authoritative, not heuristic).
- AC-07: Bootstrap inserts from `co_access` produce `relation_type="CoAccess"` edges
  for pairs where `count >= CO_ACCESS_BOOTSTRAP_MIN_COUNT` (constant to be defined,
  default 3). `bootstrap_only=0` for CoAccess edges bootstrapped from the count
  table.
- AC-08: Bootstrap inserts from `shadow_evaluations` (if the mapping to entry ID
  pairs can be resolved — see Open Questions) produce `relation_type="Contradicts"`
  edges with `bootstrap_only=1` and `source="bootstrap"`.
- AC-09: `AnalyticsWrite::GraphEdge { source_id: u64, target_id: u64,
  relation_type: String, weight: f32, created_by: String, source: String,
  bootstrap_only: bool }` variant exists in `analytics.rs`. The drain task arm
  executes an `INSERT OR IGNORE INTO graph_edges` statement.
- AC-10: `graph_penalty` and `find_terminal_active` produce identical results on a
  `TypedRelationGraph` containing only Supersedes edges as they did on the old
  `SupersessionGraph`. Verified by running the existing graph.rs test suite.
- AC-11: `graph_penalty` does NOT apply any penalty derived from Contradicts, Supports,
  CoAccess, or Prerequisite edges. Only Supersedes edge topology drives penalty
  computation.
- AC-12: Bootstrap Contradicts edges (`bootstrap_only=1`) are excluded from
  confidence scoring. The search path does not apply contradiction penalties from
  bootstrap-only edges.
- AC-13: The background tick rebuilds the `TypedRelationGraph` from `GRAPH_EDGES`
  after `maintenance_tick` completes and after GRAPH_EDGES orphaned-edge compaction.
  The in-memory graph is updated under the existing `Arc<RwLock<_>>` write lock.
- AC-14: Orphaned edge compaction (background tick maintenance step) deletes
  `GRAPH_EDGES` rows where `source_id` or `target_id` no longer exists in `entries`.
  Runs before the in-memory graph rebuild.
- AC-15: On cold start (before first tick), the search path falls back to
  `FALLBACK_PENALTY` as it does today. No regression in cold-start behavior.
- AC-16: New ADR (superseding entry #1604) is stored in Unimatrix before the feature
  ships. ADR documents typed edge weights and why penalty logic filters to Supersedes
  edges only.
- AC-17: `weight: f32` values are validated finite (not NaN, not ±Inf) before any
  `GraphEdge` analytics write is enqueued. Invalid weights are rejected with a
  logged error; the entry is not written.
- AC-18: Migration integration test covers v12→v13 migration on a synthetic v12
  database, asserting `GRAPH_EDGES` table exists and schema_version is 13 afterward.

## Constraints

1. **Single SQLite file**: `GRAPH_EDGES` goes in the same database as all other
   tables. No separate analytics.db file is created. This is a pre-decided
   architectural constraint (entry #2063, human decision).
2. **Single graph architecture (Option A)**: One `TypedRelationGraph` replaces
   `SupersessionGraph`. Not two parallel graphs. Pre-decided by the human.
3. **Supersedes edges only drive penalty scoring**: The existing `graph_penalty` logic
   is preserved functionally; it filters to Supersedes edges. Non-Supersedes edges do
   not contribute to `graph_penalty` or `find_terminal_active` in this feature.
4. **Bootstrap Contradicts excluded from scoring**: `shadow_evaluations`-derived
   edges carry `bootstrap_only=1` and must not influence confidence until W1-2 NLI
   confirms them. This is a non-negotiable safety constraint.
5. **String encoding for RelationType**: Integer discriminants are explicitly
   prohibited by the product vision (extensibility + GNN forward compatibility).
6. **ADR-004 must be superseded before ship**: Writing the new ADR is part of the
   feature's acceptance criteria, not a post-ship follow-up.
7. **Compaction before rebuild, never concurrent**: GRAPH_EDGES orphaned edge
   compaction and VECTOR_MAP compaction must both complete before the in-memory graph
   rebuild fires. They run sequentially in the tick.
8. **Schema version**: Migration bumps `CURRENT_SCHEMA_VERSION` from 12 to 13.
9. **petgraph `stable_graph` feature only**: Per ADR-001 (crt-014, entry #1601),
   only the `stable_graph` feature of petgraph is used. `StableGraph` is the correct
   type.
10. **`AnalyticsWrite` `#[non_exhaustive]` contract**: The new `GraphEdge` variant
    is added without breaking external crates that match on the enum — the existing
    catch-all arm (`_ => {}`) handles it gracefully in older server builds during
    rolling deploys.

## Open Questions

1. **Shadow evaluations → entry ID pair mapping**: The `shadow_evaluations` table
   stores `(rule_name, rule_category, neural_category, neural_confidence,
   convention_score, rule_accepted, digest)`. It does not appear to store
   `(entry_id_a, entry_id_b)` directly. How are shadow evaluations correlated to
   specific entry pairs? Is there an implied entry context from the `digest` column
   or from a join with another table? If the mapping does not exist, the bootstrap
   for Contradicts edges may need to be empty (no edges from shadow_evaluations) and
   Contradicts edges will only be created by W1-2 NLI at runtime. **This must be
   resolved before the implementer begins AC-08.**

2. **CoAccess bootstrap minimum count**: What is the threshold count for promoting a
   co-access pair to a `GRAPH_EDGES` CoAccess edge? The product vision says
   "high-count pairs" without specifying. Suggested default: `count >= 3`, but this
   needs confirmation. Too low (1) floods the graph with noise; too high (10+)
   leaves real signal on the table for deployments with moderate usage.

3. **`TypedGraphState` rename vs. `SupersessionState` extension**: Should the state
   handle be renamed to `TypedGraphState` (signals semantic upgrade, cleaner API) or
   should `SupersessionState` be extended in-place (smaller diff, less churn in call
   sites)? There are approximately 20 call sites in `background.rs`, `main.rs`,
   `services/mod.rs`, and `services/search.rs` that reference `SupersessionState` or
   `SupersessionStateHandle`. A rename is cleaner architecturally but more diff.

4. **Graph rebuild from GRAPH_EDGES vs. recompute from entries**: The tick rebuild
   could either (a) query `GRAPH_EDGES` from the database and build the graph from
   persisted edge rows, or (b) recompute all edges from canonical sources (entries,
   co_access) each tick and skip querying GRAPH_EDGES entirely. Option (a) is the
   correct approach — it respects attribution and source provenance on each edge.
   Option (b) loses any runtime-written edges (e.g., future W1-2 NLI edges) that
   were written since the last migration. This should be confirmed as Option (a).

5. **Prerequisite edge source**: No existing data source produces Prerequisite edges.
   The `RelationType::Prerequisite` variant is included in the enum for forward
   compatibility (W3-1 GNN training) but no bootstrap path writes Prerequisite edges.
   Is this intentional for W1-1 scope? (Assumed yes — include variant, write no
   bootstrap path.)

## Tracking

https://github.com/dug-21/unimatrix/issues/315
