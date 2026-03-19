# crt-021: Typed Relationship Graph (W1-1) — Specification

## Objective

Replace the untyped, in-memory-only `SupersessionGraph` (`StableGraph<u64, ()>`) with a
`TypedRelationGraph` (`StableGraph<u64, RelationEdge>`) that persists all edge types to
`GRAPH_EDGES` in the single SQLite database. Bootstrap the table from existing data sources
on the v12→v13 schema migration. Preserve all existing penalty semantics (Supersedes edges
only drive `graph_penalty`) while establishing the typed graph foundation required by W1-2
(NLI) and W3-1 (GNN).

---

## Functional Requirements

### Data Model

FR-01: Define `RelationType` as an enum with exactly five variants: `Supersedes`,
`Contradicts`, `Supports`, `CoAccess`, `Prerequisite`. Each variant serializes to and
deserializes from its variant name as a UTF-8 string (e.g., `"Supersedes"`). Round-trip
`RelationType → String → RelationType` must succeed for all five variants without loss.

FR-02: Define `RelationEdge` as a struct with fields:
- `relation_type: String` — stores the `RelationType` string encoding
- `weight: f32` — edge strength; validated finite (not NaN, not ±Inf) before any persist
- `created_at: i64` — Unix seconds
- `created_by: String` — agent or system identifier attributing the edge creation
- `source: String` — originating pipeline (e.g., `"bootstrap"`, `"nli"`, `"manual"`)

FR-03: Define `bootstrap_only: bool` on `RelationEdge`. When `true`, the edge is excluded
from confidence scoring and `graph_penalty` computation. Stored as `INTEGER NOT NULL DEFAULT 0`
in `GRAPH_EDGES`. `bootstrap_only=true` indicates the edge was created by the bootstrap
migration heuristic and has not yet been confirmed by W1-2 NLI.

FR-04: `RelationType::Prerequisite` is defined in the enum for W3-1 forward compatibility.
No bootstrap path writes Prerequisite edges in crt-021. No analytics write path creates
Prerequisite edges in crt-021. The variant is reserved.

### GRAPH_EDGES Table

FR-05: Add `GRAPH_EDGES` DDL to `create_tables_if_needed` in `db.rs`. The table must be
created on every fresh database initialization (alongside all other tables). DDL:

```sql
CREATE TABLE IF NOT EXISTS graph_edges (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id     INTEGER NOT NULL,
    target_id     INTEGER NOT NULL,
    relation_type TEXT    NOT NULL,
    weight        REAL    NOT NULL DEFAULT 1.0,
    created_at    INTEGER NOT NULL,
    created_by    TEXT    NOT NULL DEFAULT '',
    source        TEXT    NOT NULL DEFAULT '',
    bootstrap_only INTEGER NOT NULL DEFAULT 0,
    metadata      TEXT    DEFAULT NULL,
    UNIQUE(source_id, target_id, relation_type)
)
```

FR-06: `UNIQUE(source_id, target_id, relation_type)` enforces idempotent inserts. Bootstrap
`INSERT OR IGNORE` statements rely on this constraint. Multiple migration runs do not
produce duplicate edges.

### v12→v13 Schema Migration

FR-07: Add a `v12 → v13` migration block in `run_main_migrations` within `migration.rs`.
The migration must:
1. Execute `CREATE TABLE IF NOT EXISTS graph_edges (...)` as specified in FR-05.
2. Bootstrap `Supersedes` edges from `entries.supersedes` (FR-08).
3. Bootstrap `CoAccess` edges from `co_access` table (FR-09).
4. Write no `Contradicts` edges (FR-10).
5. Increment `CURRENT_SCHEMA_VERSION` to 13.

All bootstrap inserts use `INSERT OR IGNORE INTO graph_edges` keyed on
`(source_id, target_id, relation_type)`. The migration is idempotent.

FR-08: Bootstrap `Supersedes` edges: for each row in `entries` where `supersedes IS NOT NULL`,
insert one edge with `source_id = entry.supersedes`, `target_id = entry.id`,
`relation_type = "Supersedes"`, `weight = 1.0`, `created_by = "bootstrap"`,
`source = "bootstrap"`, `bootstrap_only = 0`. Edge direction follows graph.rs line 12:
`pred_id → entry.id` (outgoing edges point toward newer knowledge). Supersedes edges are
authoritative corrections (not heuristic); `bootstrap_only=0` is correct.

FR-09: Bootstrap `CoAccess` edges: for each row in `co_access` where `count >= CO_ACCESS_BOOTSTRAP_MIN_COUNT`
(constant: 3, defined in the migration or a constants module), insert one edge with
`source_id = entry_id_a`, `target_id = entry_id_b`, `relation_type = "CoAccess"`,
`weight` computed as:
```sql
COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)
```
This normalizes each co-access count against the maximum count in the table, producing a
weight in `(0.0, 1.0]`. When the table is empty or the max count is zero (R-06 guard),
`NULLIF` produces `NULL` and `COALESCE` falls back to `1.0` so the migration never errors.
`created_by = "bootstrap"`, `source = "bootstrap"`, `bootstrap_only = 0`.
Co-access edges bootstrapped from the count table are authoritative (not heuristic);
`bootstrap_only=0` is correct.

FR-10: No `Contradicts` edges are written during the v12→v13 bootstrap migration.
The `shadow_evaluations` table does not store `(entry_id_a, entry_id_b)` pairs and cannot
be used as a Contradicts edge source (confirmed: SR-04, entry #2404). The `bootstrap_only`
flag and `source='bootstrap'` columns exist in the schema for W1-2 NLI use. W1-2 NLI
writes all Contradicts edges at runtime.

### AnalyticsWrite::GraphEdge Variant

FR-11: Add `AnalyticsWrite::GraphEdge` variant to the `#[non_exhaustive]` enum in
`analytics.rs`:

```rust
GraphEdge {
    source_id:      u64,
    target_id:      u64,
    relation_type:  String,
    weight:         f32,
    created_by:     String,
    source:         String,
    bootstrap_only: bool,
}
```

FR-12: Add a `GraphEdge` arm to `execute_analytics_write` in the drain task. The arm
executes:
```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
```
where `created_at` is populated by the drain task at enqueue time. The drain task validates
`weight.is_finite()` before executing; non-finite weights are rejected with a logged ERROR
and the event is dropped (not written, not retried).

FR-13: Add `"GraphEdge"` to `variant_name()` in `AnalyticsWrite`. The catch-all `_ => "Unknown"`
arm remains unchanged.

FR-14: `AnalyticsWrite::GraphEdge` goes through the analytics write queue (bounded, shed
policy). This is the correct path for runtime edge writes (e.g., future W1-2 NLI edges).
The shed risk for W1-2 runtime NLI edge writes is an accepted, documented risk — NLI edges
written by W1-2 are not re-derivable from canonical sources if shed. (SR-02: architect must
address in W1-2 architecture.) Bootstrap edges are safe to shed because the migration is
idempotent.

### TypedGraphState (renamed from SupersessionState)

FR-15: Rename `SupersessionState` to `TypedGraphState` and `SupersessionStateHandle` to
`TypedGraphStateHandle` in `services/supersession.rs` (or a renamed module). All ~20 call
sites in `background.rs`, `main.rs`, `services/mod.rs`, and `services/search.rs` must be
updated. No type aliases are permitted to paper over the rename — compiler enforcement is
required. The rename signals the semantic upgrade.

FR-16: `TypedGraphState` carries:
- `all_entries: Vec<EntryRecord>` — unchanged from `SupersessionState`
- `typed_graph: TypedRelationGraph` — the full in-memory graph loaded from `GRAPH_EDGES`
- `use_fallback: bool` — unchanged cold-start semantics

FR-17: `TypedGraphState::rebuild(store: &Store)` queries both `all_entries` (existing
`store.query_all_entries()`) and all `GRAPH_EDGES` rows from the database, then
reconstructs `TypedRelationGraph` in memory. The rebuild returns `Err(StoreError)` on
failure; the caller retains old state. The rebuilt graph is made available to the search
path under the existing `Arc<RwLock<_>>` write lock pattern.

FR-18: On cold-start (before first tick completes), `TypedGraphState::new()` sets
`use_fallback = true` and `typed_graph` to an empty `TypedRelationGraph`. The search path
applies `FALLBACK_PENALTY` directly when `use_fallback = true`. No regression from
current cold-start behavior.

### TypedRelationGraph (replacing SupersessionGraph)

FR-19: Define `TypedRelationGraph` as a wrapper around `StableGraph<u64, RelationEdge>`
with a `node_index: HashMap<u64, NodeIndex>` for O(1) lookup. `StableGraph` is retained
(ADR-001, entry #1601). `SupersessionGraph` is removed.

FR-20: `TypedRelationGraph` provides a method (or the caller uses a filtered iterator)
to access only edges of a specific `RelationType`. A single `edges_of_type(relation_type)`
boundary is defined to enforce the Supersedes-only filter. Do not scatter ad-hoc
`relation_type == "Supersedes"` checks at each traversal site (SR-01).

FR-21: `graph_penalty` and `find_terminal_active` operate on `TypedRelationGraph` but filter
to Supersedes edges only. Penalty computation semantics are functionally identical to the
current `SupersessionGraph` implementation. The 6-priority dispatch, BFS/DFS traversal,
`MAX_TRAVERSAL_DEPTH = 10` cap, and cycle detection fallback are unchanged.

FR-22: `build_supersession_graph` (or its renamed equivalent `build_typed_relation_graph`)
constructs the `TypedRelationGraph` from `GRAPH_EDGES` rows loaded from the database during
`TypedGraphState::rebuild`. The function signature accepts the edge rows returned from the
store, not `&[EntryRecord]`. The search path reads the pre-built graph from `TypedGraphState`
under a read lock — it does not rebuild the graph on each query (this is the key behavior
change from per-query rebuild to tick-rebuild).

FR-23: `bootstrap_only=1` edges are excluded from confidence scoring. The search path must
not apply any confidence penalty derived from `bootstrap_only=1` edges, regardless of
`relation_type`.

### Background Tick Sequence

FR-24: The background tick sequence after crt-021 is:
1. `maintenance_tick()` — existing (co-access cleanup, confidence refresh, observation retention, session GC)
2. GRAPH_EDGES orphaned-edge compaction (new)
3. VECTOR_MAP compaction — existing
4. `TypedGraphState::rebuild()` — upgraded from `SupersessionState::rebuild()`
5. Contradiction scan — existing

Steps 2, 3, and 4 run sequentially. Steps 2 and 3 must both complete before step 4 fires.
Concurrent execution of compaction and rebuild is prohibited (constraint C-07).

FR-25: GRAPH_EDGES orphaned-edge compaction executes the following SQL directly through
`write_pool` (not the analytics queue — this is a bounded maintenance write):
```sql
DELETE FROM graph_edges
WHERE source_id NOT IN (SELECT id FROM entries)
   OR target_id NOT IN (SELECT id FROM entries)
```
Compaction runs in the maintenance tick. It is not batched in crt-021 (the architect may
choose to impose a per-tick row limit as a performance guard — see open questions).

### Bootstrap-to-Confirmed Edge Promotion (W1-2 contract)

FR-26: The mechanism for W1-2 NLI to promote a `bootstrap_only=1` edge to confirmed
(`bootstrap_only=0`) is: DELETE the existing row and INSERT a new row with
`bootstrap_only=0` via `AnalyticsWrite::GraphEdge`. The `UNIQUE(source_id, target_id,
relation_type)` constraint plus `INSERT OR IGNORE` makes this idempotent. W1-1 must not
implement this promotion logic — but the schema and analytics write path must support it
without modification in W1-2. (SR-07: architect must verify this contract is sufficient.)

### ADR

FR-27: A new ADR superseding entry #1604 (ADR-004, crt-014) must be stored in Unimatrix
before the feature ships. The new ADR must document: (1) typed edge weights replacing `()`
edges, (2) why `graph_penalty` filters to Supersedes edges only (other types not yet scoring
inputs), (3) the `bootstrap_only` exclusion policy for Contradicts edges, and (4) the
tick-rebuild pattern for `TypedRelationGraph` (from `GRAPH_EDGES`, not from canonical
sources each tick).

---

## Non-Functional Requirements

NF-01: `weight: f32` validation — every code path that creates or enqueues a `RelationEdge`
or `AnalyticsWrite::GraphEdge` must call `weight.is_finite()` before proceeding. Non-finite
weights (`NaN`, `+Inf`, `-Inf`) are rejected with a logged ERROR; the edge is not written
and not retried. A NaN weight silently corrupts search rankings.

NF-02: `UNIQUE(source_id, target_id, relation_type)` enforces at the database layer that
duplicate edges cannot be inserted. All insert paths use `INSERT OR IGNORE` to exploit this
constraint for idempotency.

NF-03: Cold-start fallback — the search path must apply `FALLBACK_PENALTY` from startup
until the first background tick populates `TypedGraphState`. No behavioral regression from
the current `SupersessionState` cold-start path.

NF-04: All existing graph.rs unit tests (25+ tests covering 6-priority dispatch, depth-cap
behavior, cycle detection) must pass against `TypedRelationGraph` without modification to
test expectations. The typed graph upgrade must be transparent to penalty semantics.

NF-05: `StableGraph` from petgraph with `stable_graph` feature only (ADR-001, entry #1601).
No other petgraph features are introduced.

NF-06: The `AnalyticsWrite` `#[non_exhaustive]` contract is preserved. Adding `GraphEdge`
is an additive extension. External crates that match on `AnalyticsWrite` with a `_ => {}`
catch-all arm continue to compile without modification.

NF-07: The rename from `SupersessionState`/`SupersessionStateHandle` to
`TypedGraphState`/`TypedGraphStateHandle` is enforced by the compiler. No type aliases.
All ~20 call sites are updated.

NF-08: `sqlx-data.json` must be regenerated via `cargo sqlx prepare` after the v12→v13
schema change and committed. A stale cache silently disables compile-time SQL validation
for all modified queries. (SR-09, product vision §W0-1 medium-severity security requirement.)

NF-09: GRAPH_EDGES compaction in the background tick must not introduce unbounded execution
time. The architect must specify either (a) an accepted worst-case table size for
unbounded DELETE or (b) a per-tick batch limit. This is an open question for the architect.

NF-10: `RelationType` string encoding is the only permitted persistence encoding. Integer
discriminants are prohibited by constraint C-05. This encoding must be consistent across
all insert paths, query paths, and in-memory deserialization.

---

## Acceptance Criteria

### AC-01 — TypedRelationGraph type upgrade
`TypedRelationGraph` (`StableGraph<u64, RelationEdge>`) replaces `SupersessionGraph`
(`StableGraph<u64, ()>`) in `unimatrix-engine/src/graph.rs`. All existing graph.rs unit
tests pass without modification to test expectations.
Verification: `cargo test -p unimatrix-engine` passes; `SupersessionGraph` does not appear
in the compiled source.

### AC-02 — RelationType enum completeness and round-trip
`RelationType` enum defines exactly five variants: `Supersedes`, `Contradicts`, `Supports`,
`CoAccess`, `Prerequisite`. Each serializes to its variant name as a string. Round-trip
`RelationType → String → RelationType` succeeds for all five variants.
Verification: unit test constructs each variant, serializes and deserializes, asserts
round-trip equality.

### AC-03 — RelationEdge struct and weight validation
`RelationEdge` struct carries `relation_type: String`, `weight: f32`, `created_at: i64`,
`created_by: String`, `source: String`, `bootstrap_only: bool`. `weight.is_finite()` is
asserted or checked before any persist path (analytics write enqueue or migration insert).
Verification: unit test passes `NaN`, `+Inf`, `-Inf` to the validation guard and asserts
rejection; valid weights pass through.

### AC-04 — GRAPH_EDGES DDL
`GRAPH_EDGES` table exists in the database schema after running `create_tables_if_needed`
on a fresh database. DDL exactly matches:
```sql
CREATE TABLE IF NOT EXISTS graph_edges (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id     INTEGER NOT NULL,
    target_id     INTEGER NOT NULL,
    relation_type TEXT    NOT NULL,
    weight        REAL    NOT NULL DEFAULT 1.0,
    created_at    INTEGER NOT NULL,
    created_by    TEXT    NOT NULL DEFAULT '',
    source        TEXT    NOT NULL DEFAULT '',
    bootstrap_only INTEGER NOT NULL DEFAULT 0,
    metadata      TEXT    DEFAULT NULL,
    UNIQUE(source_id, target_id, relation_type)
)
```
The `metadata` column stores JSON for per-edge feature vectors used by W3-1 GNN. It is
`NULL` for all edges written in crt-021.
Verification: integration test opens a fresh database, queries `sqlite_master` for the
`graph_edges` table, asserts all columns exist with correct types and constraints,
including `metadata TEXT DEFAULT NULL`.

### AC-05 — v12→v13 migration runs on existing database
A v12→v13 migration runs on an existing v12 database. After migration: `CURRENT_SCHEMA_VERSION`
is 13, `GRAPH_EDGES` exists, and bootstrap `Supersedes` edges from `entries.supersedes` are
present in `GRAPH_EDGES`.
Verification: migration integration test creates a synthetic v12 database with at least one
entry that has a non-null `supersedes` column, runs `migrate_if_needed`, queries
`graph_edges` for `relation_type="Supersedes"` rows, asserts count matches input.

### AC-06 — Supersedes bootstrap edges are authoritative
Bootstrap inserts from `entries.supersedes` produce one `relation_type="Supersedes"` edge
per supersession link. `bootstrap_only=0` for all Supersedes edges.
Verification: after v12→v13 migration on synthetic data, assert all `relation_type="Supersedes"`
rows in `graph_edges` have `bootstrap_only=0` and `source="bootstrap"`.

### AC-07 — CoAccess bootstrap threshold applied and weights normalized
Bootstrap inserts from `co_access` produce `relation_type="CoAccess"` edges for pairs where
`count >= CO_ACCESS_BOOTSTRAP_MIN_COUNT` (3). Pairs with `count < 3` produce no edge.
`bootstrap_only=0` for all bootstrapped CoAccess edges. Edge weights are normalized:
`COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)`, yielding values in
`(0.0, 1.0]`. A fresh-database migration (empty `co_access` table) must succeed without
error (R-06 guard: `NULLIF` prevents division by zero, `COALESCE` provides fallback 1.0).
Verification: migration integration test (a) populates `co_access` with pairs at count=2,
count=3, count=5, runs migration, asserts only count>=3 pairs appear in `graph_edges` as
CoAccess edges with `bootstrap_only=0`, and asserts all CoAccess edge weights are in
`(0.0, 1.0]` with the count=5 pair having a higher weight than the count=3 pair; and (b)
runs migration on a database with an empty `co_access` table and asserts it completes
without error and zero CoAccess rows are inserted.

### AC-08 — No Contradicts edges at bootstrap; schema columns ready for W1-2
No `Contradicts` edges are written during the v12→v13 bootstrap migration. The `bootstrap_only`
column (`INTEGER NOT NULL DEFAULT 0`) and `source` column (`TEXT NOT NULL DEFAULT ''`) exist
in `GRAPH_EDGES` schema for W1-2 NLI use. W1-2 NLI creates all Contradicts edges at runtime
via `AnalyticsWrite::GraphEdge` with `bootstrap_only=true`, `source="nli"`.
Verification: after migration on any synthetic v12 database, assert zero rows in `graph_edges`
have `relation_type="Contradicts"`. Assert `bootstrap_only` and `source` columns exist in
the schema.

### AC-09 — AnalyticsWrite::GraphEdge variant and drain arm
`AnalyticsWrite::GraphEdge { source_id: u64, target_id: u64, relation_type: String,
weight: f32, created_by: String, source: String, bootstrap_only: bool }` variant exists
in `analytics.rs`. The drain task arm executes `INSERT OR IGNORE INTO graph_edges`. The
`variant_name()` method returns `"GraphEdge"` for this variant.
Verification: unit test constructs the variant, calls `variant_name()`, asserts `"GraphEdge"`.
Integration test enqueues a `GraphEdge` event, drains, queries `graph_edges`, asserts row
present.

### AC-10 — Supersedes-only penalty: identical behavior on TypedRelationGraph
`graph_penalty` and `find_terminal_active` produce identical results on a `TypedRelationGraph`
containing only Supersedes edges as they did on the old `SupersessionGraph`. Verified by
running the existing graph.rs unit test suite.
Verification: all 25+ existing graph.rs tests pass without modification to test expectations.

### AC-11 — Non-Supersedes edges do not drive graph_penalty
`graph_penalty` does NOT apply any penalty derived from `Contradicts`, `Supports`, `CoAccess`,
or `Prerequisite` edges. Only Supersedes edge topology drives penalty computation.
Verification: unit test builds a `TypedRelationGraph` with mixed edge types (Supersedes and
Contradicts), asserts `graph_penalty` output is identical to a graph containing only the
Supersedes edges.

### AC-12 — bootstrap_only=1 edges excluded from confidence scoring
`bootstrap_only=1` edges are not passed to `graph_penalty` and do not influence confidence
scoring on the search path. The search path checks the `bootstrap_only` field before applying
any edge to confidence computation.
Verification: unit test builds a `TypedRelationGraph` where a Supersedes edge has
`bootstrap_only=true`, asserts that `graph_penalty` treats that edge as absent (no penalty
applied).

### AC-13 — Background tick rebuilds TypedRelationGraph from GRAPH_EDGES
The background tick rebuilds `TypedRelationGraph` from `GRAPH_EDGES` after `maintenance_tick`
completes and after GRAPH_EDGES orphaned-edge compaction. The in-memory graph is updated
under the existing `Arc<RwLock<_>>` write lock.
Verification: integration test seeds `GRAPH_EDGES`, triggers tick, reads `TypedGraphState`
under read lock, asserts graph contains expected edges.

### AC-14 — Orphaned edge compaction runs before in-memory rebuild
Orphaned edge compaction deletes `GRAPH_EDGES` rows where `source_id` or `target_id` no
longer exists in `entries`. Runs before the in-memory graph rebuild in the tick sequence.
Verification: integration test inserts orphaned edges (referencing deleted entry IDs) into
`graph_edges`, triggers tick, asserts orphaned rows are absent from `graph_edges` after tick.

### AC-15 — Cold-start fallback unchanged
On cold start (before first tick), the search path applies `FALLBACK_PENALTY` as it does
today. `TypedGraphState::new()` sets `use_fallback = true`. No regression in cold-start
behavior.
Verification: unit test asserts `TypedGraphState::new()` has `use_fallback=true` and
`all_entries` empty.

### AC-16 — ADR superseding entry #1604 stored in Unimatrix
New ADR stored in Unimatrix (via `context_store`) before the feature ships. ADR documents:
typed edge weights, Supersedes-only penalty filter rationale, `bootstrap_only` exclusion
policy, tick-rebuild pattern for `TypedRelationGraph`.
Verification: `context_lookup` for the new ADR returns an active entry tagged with
`[crt-021, adr]`. Entry #1604 (ADR-004) is deprecated.

### AC-17 — weight: f32 finite validation on all write paths
`weight: f32` values are validated finite (not NaN, not ±Inf) before any `GraphEdge`
analytics write is enqueued. Invalid weights are rejected with a logged ERROR; the event
is not written.
Verification: unit test calls the weight validation guard with `f32::NAN`, `f32::INFINITY`,
`f32::NEG_INFINITY`, asserts all are rejected. Valid weights (0.0, 1.0, 0.5) pass.

### AC-18 — v12→v13 migration integration test
Migration integration test covers the v12→v13 migration on a synthetic v12 database.
Asserts: `GRAPH_EDGES` table exists, `schema_version` counter is 13, at least one Supersedes
edge present (if test data includes `entries.supersedes`).
Verification: test in `migration.rs` test module, consistent with existing migration
integration test pattern.

### AC-19 — sqlx-data.json regenerated after schema change
`sqlx-data.json` is regenerated via `cargo sqlx prepare` and committed as part of the
crt-021 deliverable. CI compile-time SQL validation is not silently disabled.
Verification: `cargo build` with `SQLX_OFFLINE=true` succeeds after the schema change
and `sqlx-data.json` update.

### AC-20 — Prerequisite variant reserved, no edges written
`RelationType::Prerequisite` exists in the enum. No code path in crt-021 creates a
Prerequisite edge (no bootstrap, no analytics write, no migration). The variant is
reserved for W3-1.
Verification: grep over crt-021 implementation confirms no `Prerequisite` edge insertions.
Unit test asserts round-trip for the variant (covered by AC-02).

### AC-21 — bootstrap_only promotion path specified (W1-2 contract)
The schema and analytics write path support W1-2 NLI promoting a `bootstrap_only=1` edge
to confirmed via DELETE+INSERT (idempotent via UNIQUE constraint). No promotion logic is
implemented in crt-021; the mechanism is documented and the schema enables it.
Verification: integration test demonstrates the promotion pattern: insert edge with
`bootstrap_only=1`, delete it, insert with `bootstrap_only=0`, assert the final row has
`bootstrap_only=0`.

---

## Domain Models

### Entry (existing)
A knowledge artifact stored in `entries`. Has `id: u64`, `supersedes: Option<u64>`,
`superseded_by: Option<u64>`, status, confidence score, and other fields from `EntryRecord`.
The correction chain (`supersedes`/`superseded_by`) is the canonical source for Supersedes
edge bootstrap. Entry is the node identity in `TypedRelationGraph`.

### RelationType (new)
Enum encoding the semantic meaning of a directed graph edge between two entries. Five
variants: `Supersedes` (one entry replaces another), `Contradicts` (entries conflict),
`Supports` (entries corroborate), `CoAccess` (entries are frequently retrieved together),
`Prerequisite` (one entry is required context for another). Persisted as a string.

### RelationEdge (new)
Struct encoding the typed, attributed properties of a directed edge in `TypedRelationGraph`.
Fields: `relation_type`, `weight`, `created_at`, `created_by`, `source`, `bootstrap_only`.
Persisted as a row in `GRAPH_EDGES`. Carried as the edge payload in `StableGraph<u64, RelationEdge>`.

### TypedRelationGraph (new, replaces SupersessionGraph)
Wrapper around `StableGraph<u64, RelationEdge>` with `node_index: HashMap<u64, NodeIndex>`.
In-memory graph of all edges across all types. Penalty logic consults only Supersedes
edges by filtering on `relation_type`. Rebuilt from `GRAPH_EDGES` each tick.

### TypedGraphState (renamed from SupersessionState)
In-memory tick-rebuild cache. Holds `all_entries: Vec<EntryRecord>`, `typed_graph: TypedRelationGraph`,
`use_fallback: bool`. Wrapped in `Arc<RwLock<TypedGraphState>>` as `TypedGraphStateHandle`.
Sole writer: background tick. Reader: search service (read lock only, zero store I/O).

### GRAPH_EDGES (new table)
Persistence layer for typed graph edges. One row per directed edge identified by
`(source_id, target_id, relation_type)`. Source of truth for `TypedRelationGraph` rebuild.
Bootstrap migration populates initial rows from `entries.supersedes` and `co_access`.

---

## User Workflows

### Search path (unchanged semantics, upgraded internals)
1. Search request arrives at `SearchService`.
2. `SearchService` acquires read lock on `TypedGraphStateHandle`.
3. If `use_fallback=true`, apply `FALLBACK_PENALTY` to each result.
4. Otherwise, call `graph_penalty(typed_graph, entry_id)` filtering to Supersedes edges.
5. Release read lock. No store I/O on hot path.

### Background tick (upgraded sequence)
1. `maintenance_tick()` completes.
2. Orphaned-edge compaction: `DELETE FROM graph_edges WHERE ...` via `write_pool`.
3. VECTOR_MAP compaction: existing VectorIndex::compact + Store::rewrite_vector_map.
4. `TypedGraphState::rebuild(store)`: queries all entries + all GRAPH_EDGES rows, builds TypedRelationGraph.
5. Acquire write lock on `TypedGraphStateHandle`, swap in new state, release lock.
6. Contradiction scan: existing.

### Migration (one-time, v12→v13)
1. `migrate_if_needed` detects `current_version=12 < CURRENT_SCHEMA_VERSION=13`.
2. CREATE TABLE `graph_edges`.
3. INSERT OR IGNORE Supersedes edges from `entries.supersedes`.
4. INSERT OR IGNORE CoAccess edges from `co_access` where `count >= 3`, with weight `COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)`.
5. Update `schema_version` counter to 13.

### Edge write (future W1-2 NLI, contract specified here)
1. NLI pipeline produces a confirmed contradiction or support relationship between entry A and entry B.
2. Enqueue `AnalyticsWrite::GraphEdge { source_id, target_id, relation_type, weight, created_by: "nli", source: "nli", bootstrap_only: false }`.
3. Drain task validates `weight.is_finite()`, executes `INSERT OR IGNORE INTO graph_edges`.
4. Next tick: `TypedGraphState::rebuild` picks up the new edge.

### Bootstrap-to-confirmed promotion (W1-2 contract)
1. W1-2 NLI confirms a `bootstrap_only=1` edge (any type).
2. DELETE the existing row (by `source_id`, `target_id`, `relation_type`).
3. Enqueue `AnalyticsWrite::GraphEdge` with identical key fields and `bootstrap_only=false`, `source="nli"`.
4. `INSERT OR IGNORE` inserts the confirmed row. Edge is now active for scoring after next tick.

---

## Constraints

C-01: **Single SQLite file** — `GRAPH_EDGES` goes in the same database as all other tables.
No separate analytics.db file. Pre-decided (entry #2063).

C-02: **Single graph architecture** — One `TypedRelationGraph` replaces `SupersessionGraph`.
Not two parallel graphs. Pre-decided.

C-03: **Supersedes edges only drive penalty scoring** — `graph_penalty` and `find_terminal_active`
filter to Supersedes edges. Non-Supersedes edges do not contribute to `graph_penalty` in
crt-021.

C-04: **No Contradicts bootstrap** — No Contradicts edges are written during the v12→v13
migration. `shadow_evaluations` has no entry ID pairs (confirmed: SR-04, entry #2404).
W1-2 NLI creates all Contradicts edges at runtime.

C-05: **String encoding for RelationType** — Integer discriminants are prohibited. String
encoding allows extension without schema migration or GNN feature vector changes (W3-1).

C-06: **ADR-004 superseded before ship** — Writing the new ADR (FR-27, AC-16) is part of
the feature acceptance criteria, not a post-ship follow-up.

C-07: **Compaction before rebuild, never concurrent** — GRAPH_EDGES orphaned-edge compaction
and VECTOR_MAP compaction must both complete before the in-memory TypedRelationGraph rebuild.
Run sequentially in the tick.

C-08: **Schema version** — Migration bumps `CURRENT_SCHEMA_VERSION` from 12 to 13.

C-09: **petgraph stable_graph feature only** — Per ADR-001 (entry #1601), only
`StableGraph` from petgraph is used. No other petgraph features introduced.

C-10: **AnalyticsWrite #[non_exhaustive] contract** — `GraphEdge` variant added without
breaking external crates. The existing catch-all arm (`_ => {}`) handles it in older builds
during rolling deploys.

C-11: **No type aliases for rename** — The `SupersessionState` → `TypedGraphState` rename
is enforced by the compiler. Type aliases that paper over the rename are prohibited (SR-06).

C-12: **Prerequisite variant reserved** — No code path in crt-021 creates Prerequisite
edges. The variant is forward-compatibility-only for W3-1.

C-13: **bootstrap_only=1 edges excluded from scoring** — bootstrap_only=1 edges must not
penalize valid entries through `graph_penalty` or any confidence score component in crt-021.

C-14: **Graph rebuilt from GRAPH_EDGES, not recomputed from canonical sources each tick** —
The tick queries persisted `GRAPH_EDGES` rows to build the in-memory graph. It does not
recompute edges from `entries.supersedes` or `co_access` each tick. This preserves
attribution and captures runtime-written edges (e.g., future W1-2 NLI edges).

---

## Dependencies

### Rust Crates (existing, no new crate dependencies)
- `petgraph` with `stable_graph` feature — `StableGraph<u64, RelationEdge>` (ADR-001)
- `sqlx` with `sqlite` feature — `GRAPH_EDGES` DDL, migration, analytics drain queries
- `tokio` — background tick, async rebuild
- `unimatrix-engine` — `TypedRelationGraph`, `RelationType`, `RelationEdge`, `graph_penalty`,
  `find_terminal_active`
- `unimatrix-store` — `GRAPH_EDGES` DDL (`db.rs`), migration (`migration.rs`),
  `AnalyticsWrite::GraphEdge` (`analytics.rs`), store read for GRAPH_EDGES rows
- `unimatrix-server` — `TypedGraphState`, `TypedGraphStateHandle`, background tick sequence

### Existing Components Modified
- `unimatrix-engine/src/graph.rs` — `SupersessionGraph` → `TypedRelationGraph`
- `unimatrix-store/src/db.rs` — `create_tables_if_needed`: add GRAPH_EDGES DDL
- `unimatrix-store/src/migration.rs` — v12→v13 block, `CURRENT_SCHEMA_VERSION = 13`
- `unimatrix-store/src/analytics.rs` — `AnalyticsWrite::GraphEdge` variant + drain arm
- `unimatrix-server/src/services/supersession.rs` — rename/upgrade to `TypedGraphState`
- `unimatrix-server/src/background.rs` — tick sequence: add compaction step, upgrade rebuild call
- `unimatrix-server/src/services/search.rs` — consume `TypedRelationGraph` from state handle
- `unimatrix-server/src/main.rs`, `services/mod.rs` — update handle type references

### Unimatrix Knowledge (read before implementation)
- Entry #1601 — ADR-001 (petgraph stable_graph feature only)
- Entry #1602 — ADR-002 (per-query vs. tick rebuild, now superseded by tick-rebuild)
- Entry #1604 — ADR-004 (to be superseded by new ADR)
- Entry #1607 — SupersessionGraph pattern (reference for upgrade)
- Entry #2063 — single SQLite file confirmed
- Entry #2403 — typed graph upgrade path pattern
- Entry #2404 — shadow_evaluations no entry ID pairs (Contradicts bootstrap is empty)

---

## NOT In Scope

- NLI inference or any ML model integration (W1-2).
- Exposing graph edges via any MCP tool. Graph is internal infrastructure only.
- DOT/GraphViz export endpoint (future opportunity).
- Using non-Supersedes edge type weights in `graph_penalty` or confidence scoring (W3-1).
- Changing the `co_access` table schema or removing it.
- Removing `shadow_evaluations`.
- PostgreSQL-specific SQL.
- Automatic GRAPH_EDGES compaction beyond what runs in the maintenance tick.
- Writing Contradicts edges during bootstrap migration (shadow_evaluations has no entry ID pairs).
- Writing Prerequisite edges by any path (variant reserved for W3-1).
- Promotion logic for bootstrap_only edges (W1-2 implements; W1-1 specifies the mechanism only).
- NLI confidence score storage on `RelationEdge` beyond `weight: f32` and the `metadata TEXT`
  JSON column. `metadata` is present in the schema but its JSON structure is defined by W3-1.
- Batching GRAPH_EDGES compaction per tick (open question for architect).

---

## Open Questions for Architect

OQ-01 (SR-07): **Bootstrap-to-confirmed promotion sufficiency** — The specified promotion
path (DELETE + INSERT OR IGNORE with `bootstrap_only=0`) relies on the `UNIQUE(source_id,
target_id, relation_type)` constraint. Is DELETE+INSERT the correct mechanism for W1-2, or
should W1-1 provide an explicit `UPDATE graph_edges SET bootstrap_only=0` path? An UPDATE
path would be simpler but requires a separate analytics write variant or a direct write_pool
call. Confirm the DELETE+INSERT approach is sufficient before architect begins design.

OQ-02 (SR-02): **Analytics queue shed risk for W1-2 runtime NLI edge writes** — W1-1 only
generates bootstrap edges (via migration, not the queue). W1-2 NLI will write runtime
Contradicts/Supports edges through `AnalyticsWrite::GraphEdge`. If the queue is at capacity
(1000 events), these writes are shed and the edge is lost permanently (NLI edges are not
re-derivable from canonical sources). Document the accepted risk window or specify a non-shedding
path for edge writes in W1-2 architecture.

OQ-03 (NF-09, SR-03): **GRAPH_EDGES compaction per-tick row limit** — The orphaned-edge
DELETE is unbounded in crt-021 spec. On large graphs, this may inflate tick cost (SR-03,
entry #1777 precedent). Architect should either (a) accept unbounded DELETE for crt-021
scope and bound it in a follow-up, or (b) specify a per-tick DELETE batch size limit (e.g.,
`LIMIT 500`).

OQ-04 (SR-08): **W3-1 GNN edge feature vector readiness** — RESOLVED. `metadata TEXT DEFAULT NULL`
added to `GRAPH_EDGES` DDL (AC-04, FR-05). The column is `NULL` for all edges written in
crt-021. W3-1 defines the JSON structure stored in this column.

OQ-05 (FR-22 / SR-01): **edges_of_type boundary API** — The spec requires a single
`edges_of_type(relation_type)` method or equivalent to enforce the Supersedes-only filter.
Architect should confirm whether this is a method on `TypedRelationGraph` returning a
filtered iterator, or a standalone function, and whether the filtered view needs to be
a petgraph `EdgeFiltered` wrapper for compatibility with existing BFS/DFS traversal code.

---

## Knowledge Stewardship

Queried: /uni-query-patterns for crt-021 graph, typed edges, migration, analytics write,
supersession state — results: entries #2403 (typed graph upgrade path pattern), #1607
(SupersessionGraph pattern), #1601 (ADR-001 petgraph), #1602 (ADR-002), #1604 (ADR-004
to supersede), #2063 (single SQLite file), #2404 (shadow_evaluations no entry ID pairs).
All relevant prior decisions confirmed and incorporated.
