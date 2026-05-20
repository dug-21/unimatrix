# vnc-020 Architecture: context_graph — inverse, filter, path Modes

## System Overview

vnc-020 completes the `context_graph` MCP tool series by delivering the three remaining
query modes deferred from vnc-018 (#596) and vnc-019 (#597):

- **inverse**: SQL LEFT JOIN antijoin — entries of a given category with no incoming edges
  of specified types. Executes against the live database (no staleness).
- **filter**: SQL correlated subquery — entries matching a category + optional property
  filters + optional edge-count constraints. Executes against the live database (no staleness).
- **path**: BFS shortest-path over the in-memory `TypedRelationGraph` from `from_id` to
  `to_id`, optionally constrained by edge types and hop depth. Tick-window staleness applies
  (same contract as neighbors depth>1 and subgraph modes).

The feature adds no new MCP tool, no new table, no schema migration, and no new crate.
All code lands in `unimatrix-server/src/mcp/`. MCP tool count remains 14 after delivery.
`CURRENT_SCHEMA_VERSION` stays at 27 — the four composite indexes required by `inverse`
and `filter` were added in vnc-018 ADR-007.

## Component Breakdown

### 1. `graph_read.rs` — Wire types, entry point, centralized validation

Owns `GraphParams` (wire contract, locked per ADR-003 vnc-018) and all response envelope
types. Extended in vnc-020:

**New `GraphParams` fields** (all `Option<T>`, backward-compatible per ADR-003):

| Field | Type | Used By |
|-------|------|---------|
| `category` | `Option<String>` | inverse, filter |
| `missing_edge_types` | `Option<Vec<String>>` | inverse |
| `limit` | `Option<u32>` | inverse, filter |
| `min_age_days` | `Option<u32>` | filter |
| `min_confidence` | `Option<f64>` | filter |
| `max_confidence` | `Option<f64>` | filter |
| `min_edge_count` | `Option<u32>` | filter |
| `max_edge_count` | `Option<u32>` | filter |

`from_id: Option<u64>` and `to_id: Option<u64>` are already present as forward-compat
stubs from vnc-018. `depth: Option<u8>` and `edge_types: Option<Vec<String>>` are already
present and reused by path mode.

**New response envelopes** (defined here alongside existing envelopes):

```rust
pub struct InverseResponse {
    pub entries: Vec<EntryRecord>,
    pub total_returned: usize,
}

pub struct FilterResponse {
    pub entries: Vec<EntryRecord>,
    pub total_returned: usize,
}

pub struct PathHop {
    pub entry_id: u64,
    pub relation_type: String,
}

pub struct PathResponse {
    pub found: bool,
    pub from_id: u64,
    pub to_id: u64,
    pub hops: Vec<PathHop>,
    pub length: u8,
}
```

**`validate_no_unsupported_params` expansion** — gains three new arms:
- `"inverse"` arm: permits `category`, `missing_edge_types`, `limit`; rejects all other
  mode-specific params.
- `"filter"` arm: permits `category`, `limit`, `edge_types`, `min_age_days`,
  `min_confidence`, `max_confidence`, `min_edge_count`, `max_edge_count`; rejects
  mode-specific params from other modes.
- `"path"` arm: permits `from_id`, `to_id`, `edge_types`, `depth`,
  `resolve_supersessions`; rejects `seed_ids`, `max_nodes`, `max_depth`, `id`,
  `category`, `missing_edge_types`, `limit`, and all filter-only fields.

The existing `chain`, `current`, `neighbors`, and `subgraph` arms are updated to also
reject the eight new fields (see Param/Mode Rejection Matrix below).

The unrecognized-mode fallthrough error is updated to list all seven modes:
`"unrecognized mode '{x}' — supported modes: chain, current, neighbors, subgraph, inverse, filter, path"`.

**`handle_graph` dispatch** — gains three new arms delegating to sibling modules:
- `"inverse"` → `graph_read_inverse::handle_inverse`
- `"filter"` → `graph_read_filter::handle_filter`
- `"path"` → `graph_read_path::handle_path`

**Current line count**: `graph_read.rs` is 387 lines post-vnc-019. Adding 8 new
`GraphParams` fields (~16 lines), 3 new response envelopes (~25 lines), 3 dispatch arms
(~15 lines), and validation expansion (~60 lines) projects the file to approximately
500 lines. The 500-line limit is respected: handler logic does NOT go in `graph_read.rs`;
it lives entirely in the three sibling modules.

### 2. `graph_read_inverse.rs` — Antijoin handler (new file)

SQL LEFT JOIN antijoin for entries of a category with no incoming edges of specified types.

**Single responsibility**: validate inverse-mode params, build parameterized antijoin
SQL, return `InverseResponse`.

**No in-memory graph access** — pure SQL, live database, no staleness.

**Key SQL pattern** (AND semantics, one LEFT JOIN per `missing_edge_type`):

```sql
SELECT e.id, e.title, e.topic, e.category, e.content, e.confidence,
       e.status, e.tags, e.created_at, e.updated_at, e.supersedes,
       e.superseded_by, e.agent_id, e.feature_cycle, e.helpful_count,
       e.unhelpful_count
FROM entries e
LEFT JOIN graph_edges g1
    ON e.id = g1.target_id AND g1.relation_type = ?
[ LEFT JOIN graph_edges g2
    ON e.id = g2.target_id AND g2.relation_type = ?  -- one per additional type ]
WHERE e.category = ?
  AND e.status = 0
  AND g1.target_id IS NULL
[ AND g2.target_id IS NULL ]
LIMIT ?
```

The `idx_graph_edges_target_type (target_id, relation_type)` composite index (schema v27,
ADR-007 vnc-018) covers each LEFT JOIN as a single composite index range scan.

**Execution flow**:
1. Validate `category` present; `missing_edge_types` non-empty; each parses via
   `RelationType::from_str`; `limit` in [1, 500] (default 100).
2. Build parameterized SQL dynamically (number of LEFT JOINs = `missing_edge_types.len()`).
3. Execute via `store.read_pool_server()`.
4. Return `InverseResponse { entries, total_returned }`.

**No dynamic SQL injection surface**: all values are bound as parameters.

### 3. `graph_read_filter.rs` — Combined property + edge-count filter handler (new file)

SQL correlated subquery for entries matching category + property filters + edge-count
constraints.

**Single responsibility**: validate filter-mode params, build parameterized correlated
subquery, return `FilterResponse`.

**No in-memory graph access** — pure SQL, live database, no staleness.

**Key SQL pattern**:

```sql
SELECT e.*
FROM entries e
WHERE e.category = ?
  AND e.status = 0
  [ AND e.created_at < (strftime('%s','now') - ? * 86400) ]  -- min_age_days
  [ AND e.confidence >= ? ]                                   -- min_confidence
  [ AND e.confidence <= ? ]                                   -- max_confidence
  [ AND (
      SELECT COUNT(*) FROM graph_edges g
      WHERE g.source_id = e.id AND g.relation_type IN (?, ...)
  ) >= ? ]                                                    -- min_edge_count
  [ AND (
      SELECT COUNT(*) FROM graph_edges g
      WHERE g.source_id = e.id AND g.relation_type IN (?, ...)
  ) <= ? ]                                                    -- max_edge_count
LIMIT ?
```

The `idx_entries_category` index bounds the outer scan. The
`idx_graph_edges_source_type (source_id, relation_type)` composite index (schema v27)
covers the correlated subquery inner loop.

**Special case — `max_edge_count = 0`**: The COUNT(*) = 0 case is structurally equivalent
to the general `<= 0` bound but is the primary Q10 use case (stale Goals). The SQL
correctly handles this via `<= ?` binding — no special-casing needed.

**Execution flow**:
1. Validate `category` present; if `min_edge_count` or `max_edge_count` present then
   `edge_types` must be non-empty; `limit` in [1, 500] (default 100); all `edge_types`
   parse via `RelationType::from_str`.
2. Build parameterized SQL: one WHERE clause fragment per non-null filter field. Two
   correlated subqueries when both `min_edge_count` and `max_edge_count` are set.
3. Execute via `store.read_pool_server()`.
4. Return `FilterResponse { entries, total_returned }`.

**No free-form SQL**: The ASS-057 Track B `where_clause: String` proposal is rejected
on injection grounds (SCOPE.md Constraint 9). All property filters are typed params.

### 4. `graph_read_path.rs` — BFS shortest-path handler (new file)

In-memory BFS over `TypedRelationGraph` from `from_id` to `to_id`.

**Single responsibility**: validate path-mode params, acquire graph snapshot, BFS to
find shortest path, optionally resolve endpoints via `follow_to_current`, return
`PathResponse`.

**In-memory BFS** — tick-window staleness applies (see Staleness Disclosure section).

**Reused infrastructure from `graph_read_neighbors.rs`**:
- `follow_to_current(store, id) -> Option<u64>` — `pub(super)` (established in vnc-019).
- `all_non_supersedes_types() -> Vec<RelationType>` — `pub(super)`.

Both are available as `pub(super)` in `graph_read_neighbors.rs` since vnc-019 (the
vnc-019 architecture mandated adding `pub(super)` as the first delivery action). The
path module imports them via `super::graph_read_neighbors::{follow_to_current, all_non_supersedes_types}`,
the same pattern used by `graph_read_subgraph.rs`.

**SR-05 Resolution — per-hop intermediate `resolve_supersessions`**: This does NOT require
new infrastructure. `graph_read_subgraph.rs` already implements per-hop supersession
resolution via `follow_to_current` at each BFS step. The path mode applies the same
pattern: at each BFS hop, if `resolve_supersessions=true`, call `follow_to_current` on
the neighbor before deciding to enqueue it. The only addition specific to path mode is
resolving the two endpoints (`from_id`, `to_id`) before BFS begins — subgraph mode
resolves its seeds before BFS, which is directly analogous. `follow_to_current` is
therefore **reused**, not new.

**BFS execution flow**:
1. Validate `from_id` present; `to_id` present; `depth` in [1, 10] (default 5); all
   `edge_types` parse via `RelationType::from_str`.
2. If `resolve_supersessions=true`: resolve `from_id` and `to_id` via
   `follow_to_current`. If either resolves to `None` (orphaned deprecated terminal),
   use the original ID (same fallback as subgraph and neighbors modes).
3. Acquire `TypedRelationGraph` read lock once, clone, release (same pattern as
   `graph_read_neighbors::neighbors_bfs` and `graph_read_subgraph::handle_subgraph`).
4. Resolve `from_id` to `NodeIndex` via `graph.node_index_for(from_id)`. If absent:
   return `PathResponse { found: false, from_id, to_id, hops: [], length: 0 }` (not
   an error — AC-15).
5. Resolve `to_id` to `NodeIndex`. If absent: same empty not-found response.
6. BFS frontier: `VecDeque<(NodeIndex, u64, Vec<PathHop>)>` — each entry carries the
   full path-so-far to enable path reconstruction on first arrival at target.
   Visited set: `HashSet<u64>` keyed by node_id only (same invariant as neighbors BFS).
7. At each hop: call `edges_of_type` for each requested `RelationType` in
   `Direction::Outgoing` only. If `resolve_supersessions=true`, apply
   `follow_to_current` to each neighbor before visited-check.
8. First time `to_id`'s `NodeIndex` appears as a neighbor: path found. Return
   `PathResponse` with `found: true`, `from_id` (resolved), `to_id` (resolved),
   `hops` = path-so-far + final hop, `length = hops.len()`.
9. Frontier exhausted or `depth` hops reached without finding target:
   `PathResponse { found: false, ... }`.

**Direction**: Outgoing only. No `direction` parameter accepted for path mode (SCOPE.md
§path mode, OQ-04 resolved). Bidirectional BFS is deferred; `GraphParams` `Option<T>`
extension can absorb a `direction` field in a future release without breaking callers.

**Memory bound**: BFS frontier carries path-so-far for each frontier node. With
`max_depth=10` and average degree 5, worst-case frontier is ~10M path reconstructions.
In practice the default `depth=5` and the 3k node / 10k edge graph bounds are well under
1K frontier entries. The path-carrying frontier is the simplest correct approach; no
path-reconstruction back-pointer table is needed given these bounds.

### 5. `tools.rs` — Tool description update only

The `context_graph` tool description is extended to cover the three new modes. No logic
changes. The staleness disclosure for `path` mode (see below) is mandatory per ADR-005
vnc-018 and ADR-004 vnc-019 precedents.

### 6. `unimatrix-store` — No changes

No new SQL functions exposed via the store crate. Antijoin and correlated subquery SQL
is issued via `sqlx::query` directly from the handler modules using
`store.read_pool_server()`, following the same pattern as `graph_read_subgraph.rs`.

### 7. `unimatrix-engine` — No changes

`TypedRelationGraph`, `node_index_for`, `node_id_for_index`, `edges_of_type`, and
`RelationType` are all sufficient. No struct or trait changes.

## Component Interactions

```
tools.rs
  context_graph()
    require_cap(Read)                          ← tools.rs, before handle_graph
    handle_graph(store, typed_graph_state, params, ctx)
      validate_no_unsupported_params(params)   ← graph_read.rs, centralized (ADR-003)
      match params.mode {
        "inverse" =>
          graph_read_inverse::handle_inverse(store, &params)
            1. Validate params (category, missing_edge_types, limit)
            2. Build antijoin SQL (N LEFT JOINs, idx_graph_edges_target_type)
            3. Execute via store.read_pool_server()
            4. Return InverseResponse { entries, total_returned }

        "filter" =>
          graph_read_filter::handle_filter(store, &params)
            1. Validate params (category, edge_types, limits, filter fields)
            2. Build correlated subquery SQL (idx_graph_edges_source_type)
            3. Execute via store.read_pool_server()
            4. Return FilterResponse { entries, total_returned }

        "path" =>
          graph_read_path::handle_path(store, typed_graph_state, &params)
            1. Validate params (from_id, to_id, depth, edge_types)
            2. Resolve endpoints if resolve_supersessions=true
               (follow_to_current from graph_read_neighbors — reused)
            3. Clone TypedRelationGraph (lock → clone → release)
            4. BFS outgoing traversal, path-carrying frontier
            5. Return PathResponse { found, from_id, to_id, hops, length }
      }
```

### Lock Discipline (path mode)

Identical to neighbors BFS and subgraph BFS: `std::sync::RwLock` (not tokio), acquired
once with `.read().unwrap_or_else(|e| e.into_inner())`, graph cloned, lock released
before any async work (including `follow_to_current` Store calls).

## Technology Decisions

| Decision | Choice | ADR | Unimatrix ID |
|----------|--------|-----|--------------|
| Module split strategy | Three new sibling modules (graph_read_inverse.rs, graph_read_filter.rs, graph_read_path.rs) | ADR-001 | #4502 |
| GraphParams field additions and backward compat | 8 new Option<T> fields, extension-only | ADR-002 | #4503 |
| inverse mode AND vs OR semantics | AND semantics (entries missing ALL specified types) | ADR-003 | #4504 |
| depth field reuse for path mode | Reuse existing `depth: Option<u8>` | ADR-004 | #4505 |
| path response format | from_id top-level; hops array; no null relation_type | ADR-005 | #4506 |
| resolve_supersessions in path mode | Supported; resolves endpoints before BFS; per-hop reuses follow_to_current | ADR-006 | #4507 |
| No raw SQL in filter mode | Typed params only; ASS-057 where_clause proposal rejected | ADR-007 | #4508 |

## Integration Points

### Depends On (must be present before delivery)

- vnc-018 PR #596 merged: `GraphParams`, `EdgeRecord`, `validate_no_unsupported_params`,
  `handle_graph` dispatch, schema v27 (four composite indexes).
- vnc-019 PR #597 merged: `follow_to_current` as `pub(super)`, `all_non_supersedes_types`
  as `pub(super)`, `max_depth` in `GraphParams`, subgraph arm in validation.
- `TypedRelationGraph::node_index_for`, `node_id_for_index`, `edges_of_type` —
  delivered in vnc-018.

### Produces

- `InverseResponse`, `FilterResponse`, `PathHop`, `PathResponse` — new wire types.
- Three new handler modules: `graph_read_inverse.rs`, `graph_read_filter.rs`,
  `graph_read_path.rs`.

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|----------------|--------|
| `GraphParams.category` | `Option<String>` | `graph_read.rs` |
| `GraphParams.missing_edge_types` | `Option<Vec<String>>` | `graph_read.rs` |
| `GraphParams.limit` | `Option<u32>` | `graph_read.rs` |
| `GraphParams.min_age_days` | `Option<u32>` | `graph_read.rs` |
| `GraphParams.min_confidence` | `Option<f64>` | `graph_read.rs` |
| `GraphParams.max_confidence` | `Option<f64>` | `graph_read.rs` |
| `GraphParams.min_edge_count` | `Option<u32>` | `graph_read.rs` |
| `GraphParams.max_edge_count` | `Option<u32>` | `graph_read.rs` |
| `InverseResponse` | `{ entries: Vec<EntryRecord>, total_returned: usize }` | `graph_read.rs` |
| `FilterResponse` | `{ entries: Vec<EntryRecord>, total_returned: usize }` | `graph_read.rs` |
| `PathHop` | `{ entry_id: u64, relation_type: String }` | `graph_read.rs` |
| `PathResponse` | `{ found: bool, from_id: u64, to_id: u64, hops: Vec<PathHop>, length: u8 }` | `graph_read.rs` |
| `handle_inverse` | `async fn(store: &Store, params: &GraphParams) -> Result<InverseResponse, ErrorData>` | `graph_read_inverse.rs` |
| `handle_filter` | `async fn(store: &Store, params: &GraphParams) -> Result<FilterResponse, ErrorData>` | `graph_read_filter.rs` |
| `handle_path` | `async fn(store: &Store, typed_graph_state: &Arc<RwLock<TypedGraphState>>, params: &GraphParams) -> Result<PathResponse, ErrorData>` | `graph_read_path.rs` |
| `follow_to_current` | `pub(super) async fn(store: &Store, id: u64) -> Option<u64>` | `graph_read_neighbors.rs` (reused) |
| `all_non_supersedes_types` | `pub(super) fn() -> Vec<RelationType>` | `graph_read_neighbors.rs` (reused) |
| `TypedRelationGraph::node_index_for` | `fn(&self, id: u64) -> Option<NodeIndex>` | `unimatrix-engine/graph.rs` |
| `TypedRelationGraph::node_id_for_index` | `fn(&self, idx: NodeIndex) -> Option<u64>` | `unimatrix-engine/graph.rs` |
| `TypedRelationGraph::edges_of_type` | `fn(&self, NodeIndex, RelationType, Direction) -> impl Iterator<Item = EdgeReference>` | `unimatrix-engine/graph.rs` |
| `validate_no_unsupported_params` | Extended: 3 new arms; 4 existing arms updated to reject 8 new fields | `graph_read.rs` |

## Param/Mode Rejection Matrix

Rows = parameters; columns = modes. A = accepted, R = rejected with named-mode hint.
Blank = was not present before vnc-020 (no change to existing arms).

| Parameter | chain | current | neighbors | subgraph | inverse | filter | path |
|-----------|-------|---------|-----------|----------|---------|--------|------|
| `id` | A | A | A | R→chain/current/neighbors | R | R | R |
| `seed_ids` | R→subgraph | R→subgraph | R→subgraph | A | R→subgraph | R→subgraph | R→subgraph |
| `max_nodes` | R→subgraph | R→subgraph | R→subgraph | A | R→subgraph | R→subgraph | R→subgraph |
| `max_depth` | R→subgraph | R→subgraph | R→subgraph | A | R→subgraph | R→subgraph | R→subgraph |
| `from_id` | R→path | R→path | R→path | R→path | R→path | R→path | A |
| `to_id` | R→path | R→path | R→path | R→path | R→path | R→path | A |
| `depth` | R→neighbors/path | R→neighbors/path | A | R→neighbors/path | R→neighbors/path | R→neighbors/path | A |
| `edge_types` | R | R | A | A | R | A | A |
| `resolve_supersessions` | R (chain IS the audit) | A | A | A | — | — | A |
| `direction` | R | R | A | A | R | R | R |
| `category` | R→inverse/filter | R→inverse/filter | R→inverse/filter | R→inverse/filter | A | A | R→inverse/filter |
| `missing_edge_types` | R→inverse | R→inverse | R→inverse | R→inverse | A | R→inverse | R→inverse |
| `limit` | R→inverse/filter | R→inverse/filter | R→inverse/filter | R→inverse/filter | A | A | R→inverse/filter |
| `min_age_days` | R→filter | R→filter | R→filter | R→filter | R→filter | A | R→filter |
| `min_confidence` | R→filter | R→filter | R→filter | R→filter | R→filter | A | R→filter |
| `max_confidence` | R→filter | R→filter | R→filter | R→filter | R→filter | A | R→filter |
| `min_edge_count` | R→filter | R→filter | R→filter | R→filter | R→filter | A | R→filter |
| `max_edge_count` | R→filter | R→filter | R→filter | R→filter | R→filter | A | R→filter |

Notes:
- `depth` rejection on `chain`, `current`, `subgraph`, `inverse`, `filter` corrects the
  existing silent-ignore behavior (AC-25). Error message: "depth is not supported in
  {mode} mode — use neighbors or path mode".
- `resolve_supersessions` on `inverse` and `filter` is silently ignored (no in-memory
  graph; SQL reads live DB regardless).
- The `chain` arm's existing `resolve_supersessions=Some(true)` rejection is preserved
  unchanged.

## Staleness Disclosure (SR-01)

The `path` mode tool description MUST include the following text verbatim (modeled on
ADR-004 vnc-019, entry #4493):

> "path mode uses the in-memory graph cache for BFS traversal. The cache is rebuilt each
> tick (typically 30-60 seconds). Edges written within the current tick interval may not
> appear in the result. This is the same staleness contract as neighbors mode at depth>1
> and subgraph mode. If from_id or to_id is not present in the current graph snapshot, the
> result is { found: false } — not an error. Use resolve_supersessions=true to have
> deprecated endpoints resolved to their active successors before BFS begins."

`inverse` and `filter` modes read the live database directly via SQL; they have no
staleness concern and their tool descriptions must NOT include staleness language.

## SR Disposition

| Risk ID | Resolution |
|---------|------------|
| SR-01 (path staleness, two freshness contracts) | Exact disclosure text produced above; SQL modes explicitly noted as live-DB. Tool description update required at delivery. |
| SR-02 (filter dynamic SQL double-count risk) | Architecture specifies two independent correlated subqueries (one for min, one for max) rather than a single subquery with AND bounds — eliminates double-count risk. Spec must trace against explicit data scenarios (all-active, deprecated entries present). |
| SR-03 (module split decisions at design time) | Decided here: three sibling modules. validate_no_unsupported_params stays in graph_read.rs and is the single cross-mode rejection point. Handlers contain no validation of other modes' params. |
| SR-04 (depth field rejection behavior change) | Every mode's depth stance captured in rejection matrix above. Delivery spec must enumerate AC per affected mode. |
| SR-05 (per-hop resolve_supersessions new vs. reused) | Reused — see SR-05 Resolution below. |
| SR-06 (AND semantics non-obvious default) | ADR-003 records rationale; tool description must include AND example explicitly. |
| SR-07 (vnc-019 sequencing) | Design can proceed; delivery blocked on vnc-019 PR #597 merge. |
| SR-08 (combinatorial rejection surface) | Rejection matrix above is the authoritative spec input. Tester must validate at least one wrong-mode rejection per new field (8 new fields × at least 1 test each minimum). |

## SR-05 Resolution: per-hop resolve_supersessions — Reused, Not New

The SCOPE.md §path-mode phrase "in addition to per-hop intermediate resolution" is
fully covered by existing `follow_to_current` infrastructure. Evidence:

1. `graph_read_subgraph.rs` (vnc-019, delivered and merged) calls `follow_to_current`
   at every BFS hop for each neighbor before enqueuing (lines 227-233 in the delivered
   file). This is per-hop intermediate resolution.
2. `graph_read_neighbors.rs` performs the same per-hop call (lines 302-309).
3. `follow_to_current` is already `pub(super)` since vnc-019.
4. Path mode applies `follow_to_current` at two additional points not present in
   subgraph: resolving `from_id` and `to_id` endpoints before BFS begins. This is the
   only path-specific addition.

**Conclusion**: path mode needs zero new infrastructure for supersession resolution.
The implementation agent calls `follow_to_current` on endpoints (new) and on each
BFS neighbor (existing pattern copied from subgraph). No new helper functions.

## Open Questions for Spec Writer

None. All SCOPE.md OQs are resolved. The following implementation-time decisions are
pre-resolved by this architecture:

1. Path mode uses a path-carrying BFS frontier (not back-pointer reconstruction) given
   the small graph bounds. Spec writer may confirm this or substitute back-pointer if
   preferred — both are correct.
2. `resolve_supersessions` on `inverse` and `filter` modes: silently ignored (SQL reads
   live DB regardless of this flag). Spec writer may choose to explicitly reject it
   with an error message for clarity; the architecture permits either.
3. The `depth` rejection error message wording: "depth is not supported in {mode} mode
   — use neighbors or path mode". Spec writer should confirm this wording is consistent
   with the existing AC-25 test plan.
