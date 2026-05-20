# vnc-020: context_graph — inverse, filter, path Modes (W1B-2c)

## Problem Statement

vnc-018 (GH #596) delivered `context_graph` with `chain`, `current`, and `neighbors` modes.
vnc-019 (GH #597) adds `subgraph` mode. Together these cover one-dimensional traversal and
multi-hop bounded BFS from known seed entries.

Three query patterns remain unreachable:

**Gap 1 — Antijoin detection (inverse mode)**: No tool can answer "which entries of a given
category have no incoming edges of a specified type?" The canonical example is Q9: Sources
with no incoming `Cites` edges — uncited sources that may be orphaned or redundant. Neither
`context_lookup` nor any traversal mode can express antijoin semantics. Application-side
filtering requires O(N) MCP calls vs. one SQL query.

**Gap 2 — Combined property + edge-count filter (filter mode)**: No tool can combine a
category constraint with a property filter (e.g., created before a threshold date) AND an
edge count constraint (e.g., has zero outgoing `Advances` edges). This prevents queries like
"Goals with no Advances edges that are more than 30 days old" (Q10 stale Goal detection) or
"entries with more than one outgoing Advances edge" (Q11 multi-Goal advancement tracking).
`context_lookup` handles category/tags/status but not correlated edge counts.

**Gap 3 — Shortest path between two entries (path mode)**: No tool can find the shortest
typed-edge path between two known entries. This prevents "how is entry A connected to entry
B through Supports/Advances chains?" — a fundamental structural question for auditing goal
traceability and ADR dependency chains.

These three modes are the final delivery in the `context_graph` tool series. All three were
explicitly deferred from vnc-018 (#598) and vnc-019 (#597) with forward-compat stubs in
`GraphParams` already in place for `from_id` and `to_id` (path mode).

## Goals

1. Add `inverse` mode: SQL LEFT JOIN antijoin returning entries of a given category with no
   incoming edges of specified types, bounded by a caller-supplied `limit`.
2. Add `filter` mode: combined category + optional property filter + optional edge count
   filter in a single SQL correlated subquery.
3. Add `path` mode: shortest-path BFS from `from_id` to `to_id` over the in-memory
   `TypedRelationGraph`, following caller-specified edge types up to `max_depth`.
4. Add any new `GraphParams` fields needed by `filter` mode (category, limit for inverse;
   property filter and edge count filter params for filter mode) following the
   backward-compatible `Option<T>` extension pattern (ADR-003 vnc-018).
5. Update `validate_no_unsupported_params` to recognize `inverse`, `filter`, and `path` arms
   and update the unrecognized-mode error to list all six modes.
6. Maintain full backward compatibility — no changes to existing mode behavior or wire types.

## Non-Goals

- Any new `RelationType` enum variants — all 16 variants already exist (vnc-015).
- `subgraph` mode — shipped in vnc-019.
- `chain`, `current`, `neighbors` mode behavior changes — no modifications to existing modes.
- `as_of` timestamp support — Phase 3+, deferred per ASS-057 findings.
- `context_batch_write` — out of roadmap scope (HNSW atomicity open question, OSC-6 in ASS-057).
- NLI `contradicts_category_pairs` scoping — Wave 3 intelligence enhancement.
- Adding `metadata: Option<String>` to `RelationEdge` in `unimatrix-engine` — not required
  by any of the three modes (inverse and filter return entry records, not edge records;
  path returns node IDs and hop counts, not edge metadata).
- Multi-hop path enumeration (all paths, not shortest path) — only shortest path is in scope.
- `resolve_supersessions` support in `inverse` or `filter` modes — not applicable to SQL-only
  modes. `path` mode may inherit it consistent with neighbors/subgraph, but this is deferred
  (see Open Questions OQ-01).
- Research-domain configuration (`research-domain.toml`) — separate scope item.

## Background Research

### Existing infrastructure (vnc-018, vnc-019)

`GraphParams` (locked per ADR-003 vnc-018) already carries forward-compat stubs for path
mode: `from_id: Option<u64>` and `to_id: Option<u64>`. These are the only pre-placed
forward-compat stubs. Fields for `inverse` mode (`category`, `limit`, `missing_edge_types`)
and `filter` mode (`category`, `limit`, `where_clause`/property filter, `edge_count_filter`)
are not present — they must be added as backward-compatible `Option<T>` extensions per
ADR-003 Consequences ("Option<T> additions are backward-compatible").

After vnc-019 ships, `GraphParams` will contain: `mode`, `agent_id`, `format`, `id`,
`direction`, `edge_types`, `depth`, `resolve_supersessions`, `seed_ids`, `max_nodes`,
`max_depth`, `from_id`, `to_id`.

`validate_no_unsupported_params` will recognize `chain`, `current`, `neighbors`, `subgraph`
arms. `inverse`, `filter`, and `path` are still in the unrecognized-mode catchall, which
means callers get a helpful "unrecognized mode" error until vnc-020 ships.

### Composite indexes already present (schema v27, vnc-018)

ADR-007 vnc-018 added four indexes in migration v26→v27:
- `idx_graph_edges_source_type ON graph_edges(source_id, relation_type)` — composite
- `idx_graph_edges_target_type ON graph_edges(target_id, relation_type)` — composite
- `idx_entries_supersedes ON entries(supersedes)`
- `idx_entries_superseded_by ON entries(superseded_by)`

The composite indexes were explicitly added early ("inverse and filter modes (#598) —
adding now avoids a second migration" — ADR-007 rationale). Both `inverse` and `filter`
modes can use these indexes without any additional migration. Schema version stays at 27;
vnc-020 introduces no migration.

### inverse mode — SQL antijoin

ASS-057 Track B Section 3 confirms the antijoin pattern:

```sql
SELECT e.id, e.title, e.topic, e.confidence, e.created_at
FROM entries e
LEFT JOIN graph_edges g
    ON e.id = g.target_id AND g.relation_type = 'Cites'
WHERE e.category = 'source'
  AND e.status = 0
  AND g.target_id IS NULL
LIMIT 100;
```

For multiple `missing_edge_types`: multiple LEFT JOINs, one per type, with ALL null checks
ANDed — AND semantics ("entries missing ALL specified types"). This is the more useful
interpretation for gap detection: "Sources with neither Cites nor Supports incoming edges"
is narrower and more actionable than OR. Callers wanting OR behavior issue two separate
inverse queries. Index analysis: `idx_graph_edges_target_type (target_id, relation_type)`
makes each LEFT JOIN a single composite index range scan — sub-millisecond per join.

Application-side alternative is infeasible: 301 MCP calls vs. 1 SQL query.

### filter mode — correlated SQL subquery

ASS-057 Track B Section 3 confirms the correlated subquery pattern:

```sql
SELECT e.*
FROM entries e
WHERE e.category = ?1
  AND e.status = 0
  AND <property_where_clauses>
  AND (
      SELECT COUNT(*)
      FROM graph_edges g
      WHERE g.source_id = e.id
        AND g.relation_type = ?2
  ) >= ?3
LIMIT ?4
```

Query plan at 3k entries: outer scan bounded by `idx_entries_category`; 300 correlated
subquery evaluations each using `idx_graph_edges_source_type` composite index.
Estimated: ~4,500 operations, well under 10ms.

Property filters map to `entries` columns: `created_at` (age threshold), `topic` (prefix
or exact match), `confidence` (minimum floor). These are safe to express via parameterized
SQL — no user-supplied raw SQL is accepted.

### path mode — petgraph BFS in-memory

ASS-057 Track B Section 3 confirms the petgraph approach:

`TypedRelationGraph.inner` is `StableGraph<u64, RelationEdge>`. `petgraph::algo` is already
imported at `graph.rs:21` (`use petgraph::algo::is_cyclic_directed`). `petgraph::algo`
also provides `astar` and `dijkstra` on `StableGraph`. However, for unweighted shortest
path with edge-type filtering, a BFS with a visited set is simpler and equally correct.

`node_index_for(id: u64) -> Option<NodeIndex>` provides O(1) lookup (added in vnc-018,
ADR-008). `edges_of_type(node_idx, relation_type, direction) -> impl Iterator<Item =
EdgeReference>` is the sole filter boundary (SR-01). Multi-type BFS calls `edges_of_type`
for each requested `RelationType` in turn at each hop.

Performance at `max_depth=5`, 3k nodes, 10k edges: worst-case BFS visits at most
avg_degree^max_depth = 5^5 = 3,125 nodes. Sub-millisecond in practice. In-memory petgraph
is strictly better than SQL recursive CTE for this graph size.

Path result: the sequence of entry IDs from `from_id` to `to_id` inclusive, each hop's
`relation_type`, and the total path length. Empty result (no path found within `max_depth`)
returns `{ found: false, path: [], length: 0 }`.

### All 16 RelationType variants recognized

`RelationType::from_str` in `graph.rs` handles all 16 variants (6 original + 10 added in
vnc-015: Advances, Motivates, Cites, Asserts, Mentions, Refutes, Tests, DerivedFrom, About,
RelatedTo). The wildcard arm MUST remain last (ADR-007 vnc-018). All `edge_types` values
passed to `inverse`, `filter`, or `path` modes must be validated against `from_str`.

### Tick-window staleness (path mode only)

`path` mode uses the in-memory `TypedRelationGraph`, which is tick-rebuilt. Edges written
within the last tick interval may not appear. This is the documented behavioral contract
from ADR-005 vnc-018 (inherited by subgraph mode in vnc-019). The tool description for
`path` mode must include the staleness disclosure text consistent with neighbors and subgraph.

`inverse` and `filter` modes execute SQL against the live database — no staleness concern.

## Proposed Approach

All three modes are implemented in `graph_read.rs` (or a sibling module if the 500-line
limit is approached) as handler functions dispatched from `handle_graph`, mirroring the
established `handle_chain` / `handle_current` / `handle_neighbors` / `handle_subgraph`
pattern.

### inverse mode

**Handler**: `handle_inverse` in `graph_read.rs` (or `graph_read_inverse.rs` if size
requires splitting).

**New `GraphParams` fields** (backward-compatible `Option<T>` additions):
- `category: Option<String>` — required for inverse mode; the entry category to scan
- `missing_edge_types: Option<Vec<String>>` — required; one or more relation types whose
  absence defines the antijoin
- `limit: Option<u32>` — max entries to return (default 100, max 500)

**Execution**:
1. Validate: `category` required; `missing_edge_types` non-empty; each element parses via
   `RelationType::from_str`; `limit` in [1, 500].
2. Build parameterized SQL LEFT JOIN antijoin using `idx_graph_edges_target_type` composite
   index. For multiple `missing_edge_types`, use one LEFT JOIN per type (each null-checked).
3. Return `Vec<EntryRecord>` plus a `total_returned: usize` field.

**Validation rejection on other modes**: `category`, `missing_edge_types`, and `limit` are
rejected on `chain`, `current`, `neighbors`, `subgraph`, `path` modes via
`validate_no_unsupported_params` with a message naming the correct mode.

### filter mode

**Handler**: `handle_filter` in `graph_read.rs` (or `graph_read_filter.rs`).

**New `GraphParams` fields** (backward-compatible `Option<T>` additions; `category` already
added for inverse, `limit` already added for inverse):
- `min_age_days: Option<u32>` — entry `created_at` older than N days (Q10 stale Goal)
- `max_confidence: Option<f64>` — upper confidence bound
- `min_confidence: Option<f64>` — lower confidence bound
- `min_edge_count: Option<u32>` — correlated subquery: count of outgoing edges of given
  `edge_types` must be >= this value
- `max_edge_count: Option<u32>` — correlated subquery: count of outgoing edges of given
  `edge_types` must be <= this value

Note: `category`, `limit`, and `edge_types` (already in `GraphParams`) are reused.

**Execution**:
1. Validate: `category` required; if `min_edge_count` or `max_edge_count` present, at least
   one `edge_types` entry required; `limit` in [1, 500]; all `edge_types` parse via
   `RelationType::from_str`.
2. Build parameterized correlated subquery. Property filter clauses built from non-null
   filter params — all via parameterized SQL (no raw SQL injection surface).
3. Return `Vec<EntryRecord>` plus `total_returned: usize`.

**Rationale for no raw WHERE clause**: ASS-057 Track B proposed `where_clause` as a
free-form SQL string. This is an injection surface and cannot be accepted via MCP where
callers are AI agents. Property filters are expressed as typed parameters.

### path mode

**Handler**: `handle_path` in `graph_read.rs` (or `graph_read_path.rs`).

**GraphParams fields consumed** (forward-compat stubs already present):
- `from_id: Option<u64>` — required; start node
- `to_id: Option<u64>` — required; destination node
- `edge_types: Option<Vec<String>>` — filter (absent/[] = all non-Supersedes types)
- `depth: Option<u8>` — max BFS depth (default 5, range 1..=10); reuse existing field

Note: `max_depth` is owned by subgraph mode. `depth` is reused for path mode (identical
semantics: hop limit, no behavioral difference warrants a new field). `validate_no_unsupported_params`
is updated: the `path` arm accepts `depth`; all other modes now explicitly reject `depth`
(correcting the existing soft inconsistency where `depth` was silently ignored on
non-neighbors modes). The `path` arm also rejects `seed_ids`, `max_nodes`, `max_depth`
(subgraph params), and `id` (single-anchor modes).

**Execution**:
1. Validate: `from_id` required; `to_id` required; `depth` in [1, 10] (default 5); all
   `edge_types` parse via `RelationType::from_str`.
2. Acquire `TypedRelationGraph` read lock; clone the graph; release lock.
3. Resolve `from_id` and `to_id` to `NodeIndex` via `node_index_for`. Either absent →
   `{ found: false, path: [], length: 0 }` with a note that the entry may not be in the
   current tick's in-memory snapshot.
4. BFS from `from_id` using a frontier of `(NodeIndex, path_so_far)`. At each hop, call
   `edges_of_type` for each requested `RelationType` in `Direction::Outgoing`. When
   `to_id`'s `NodeIndex` is first reached, return the path.
5. If frontier exhausted or `max_depth` reached without finding `to_id`:
   `{ found: false, path: [], length: 0 }`.
6. Return `PathResponse { found: bool, path: Vec<PathHop>, length: u8 }` where
   `PathHop { entry_id: u64, relation_type: String }` describes each step.

**Direction**: Outgoing only (source → target as stored). No `direction` parameter is
accepted. Bidirectional BFS is a materially different implementation — deferring avoids
under-designing a parameter that would only accept one value. The wire contract
(`GraphParams` `Option<T>` extension) can absorb a `direction` field in a future release
without breaking callers.

**Path response format**:
```json
{
  "found": true,
  "from_id": 123,
  "to_id": 789,
  "hops": [
    { "entry_id": 456, "relation_type": "Advances" },
    { "entry_id": 789, "relation_type": "Supports" }
  ],
  "length": 2
}
```
`from_id` is a top-level field, not an element of `hops`. Each hop describes "the entry
arrived at and the edge traversed to get here." `length = hops.len()`. Agents reconstruct
the full node sequence as `[from_id] + hops.map(|h| h.entry_id)`. No null relation types;
no ambiguity about what the first element means.

**resolve_supersessions in path mode**: Supported (`resolve_supersessions: bool`, default
false). When `true`, resolve `from_id` and `to_id` to their terminal active successors
BEFORE BFS begins (in addition to per-hop intermediate resolution). A caller passing a
deprecated `from_id` almost certainly wants the path from its active successor; passing
`resolve_supersessions=false` is the explicit audit mode for finding a path to/from a
deprecated node. Consistent with neighbors and subgraph modes. ASS-057 explicitly
recommends building this in from day one: retrofit cost exceeds build-in cost.

## Acceptance Criteria

- **AC-01**: `context_graph(mode="inverse", category="source", missing_edge_types=["Cites"])` returns
  a JSON array of `EntryRecord` objects for `source` entries with no incoming `Cites` edges.
- **AC-02**: `inverse` mode with `missing_edge_types` containing an unrecognized string produces
  a validation error naming the unrecognized value and listing recognized edge types.
- **AC-03**: `inverse` mode with `missing_edge_types` absent or empty produces a validation error:
  "inverse mode requires at least one edge type in missing_edge_types".
- **AC-04**: `inverse` mode with `category` absent produces a validation error:
  "inverse mode requires category".
- **AC-05**: `inverse` mode `limit` default is 100 when omitted. Valid range [1, 500]; out-of-range
  values produce a validation error stating the allowed range.
- **AC-06**: `inverse` mode response includes a `total_returned` field with the count of entries
  returned.
- **AC-07**: `context_graph(mode="filter", category="goal", min_age_days=30, max_edge_count=0,
  edge_types=["Advances"])` returns `goal` entries older than 30 days with zero outgoing
  `Advances` edges (Q10 stale Goal pattern).
- **AC-08**: `context_graph(mode="filter", category="decision", min_edge_count=2,
  edge_types=["Advances"])` returns `decision` entries with two or more outgoing `Advances`
  edges (Q11 multi-Goal advancement pattern).
- **AC-09**: `filter` mode with `min_edge_count` or `max_edge_count` present but `edge_types`
  absent or empty produces a validation error: "filter mode requires edge_types when
  edge_count constraints are specified".
- **AC-10**: `filter` mode with `category` absent produces a validation error: "filter mode
  requires category".
- **AC-11**: `filter` mode `limit` default is 100 when omitted. Valid range [1, 500].
- **AC-12**: `filter` mode response includes `total_returned`.
- **AC-13**: `context_graph(mode="path", from_id=A, to_id=B, edge_types=["Supports","Advances"],
  depth=5)` returns `{ found: true, from_id: A, to_id: B, hops: [...], length: N }` where
  `hops` contains N entries (no null relation_types) and `from_id` is NOT in `hops`.
- **AC-14**: `path` mode returns `{ found: false, from_id: A, to_id: B, hops: [], length: 0 }`
  when no path exists between `from_id` and `to_id` within `depth` hops.
- **AC-15**: `path` mode returns `{ found: false, ... }` (not an error) when `from_id` or
  `to_id` is not found in the current in-memory graph snapshot.
- **AC-16**: `path` mode with `from_id` absent produces a validation error: "path mode requires
  from_id".
- **AC-17**: `path` mode with `to_id` absent produces a validation error: "path mode requires
  to_id".
- **AC-18**: `path` mode `depth` default is 5 when omitted. Valid range [1, 10]; out-of-range
  values produce a validation error.
- **AC-19**: `path` mode tool description includes the staleness disclosure (in-memory BFS,
  tick-window lag). Exact text to be finalized in specification.
- **AC-20**: `path` mode with `resolve_supersessions=true`: if `from_id` or `to_id` is a
  deprecated entry, the endpoint is resolved to its terminal active successor before BFS
  begins. The `from_id` field in the response reflects the resolved ID, not the original.
- **AC-21**: `path` mode with `resolve_supersessions=false` (default): deprecated endpoints
  and intermediate nodes are used as-is (audit mode).
- **AC-22**: Passing `from_id` or `to_id` to `chain`, `current`, `neighbors`, `subgraph`, or
  `filter` modes produces the forward-compat validation error from
  `validate_no_unsupported_params` naming the correct mode.
- **AC-23**: Passing `inverse`-only params (`missing_edge_types`) to non-inverse modes produces
  a validation error naming the correct mode.
- **AC-24**: Passing `filter`-only params (`min_age_days`, `max_edge_count`, etc.) to non-filter
  modes produces a validation error naming the correct mode.
- **AC-25**: Passing `depth` to `chain`, `current`, `subgraph`, `inverse`, or `filter` modes
  produces a validation error naming the correct mode (corrects the existing silent-ignore
  behavior on non-neighbors modes).
- **AC-26**: The unrecognized-mode error lists all seven supported modes: "chain, current,
  neighbors, subgraph, inverse, filter, path".
- **AC-27**: An integration test in the infra-001 suite covers `inverse` mode: writes entries
  of a given category where some have incoming edges of a specified type and others do not;
  asserts the mode returns only the entries with no incoming edges of that type.
- **AC-28**: An integration test in the infra-001 suite covers `inverse` mode with two
  `missing_edge_types`: asserts AND semantics — only entries missing ALL specified types
  are returned.
- **AC-29**: An integration test in the infra-001 suite covers `filter` mode with
  `max_edge_count=0`: writes goal entries where some have no outgoing Advances edges and
  some have one or more; asserts the mode returns only the zero-edge entries. The `= 0`
  boundary is explicitly validated (COUNT(*) = 0 vs. COUNT(*) >= N are structurally
  different in SQL).
- **AC-30**: An integration test in the infra-001 suite covers `filter` mode with
  `min_edge_count >= 2`: asserts only entries with two or more outgoing edges of the
  specified type are returned.
- **AC-31**: An integration test in the infra-001 suite covers `path` mode: writes entries
  connected by a known typed-edge chain; asserts the returned hops match the expected
  sequence and `from_id` is not in the `hops` array.

## Constraints

1. **vnc-018 must be merged first** — `graph_read.rs` requires the full chain/current/neighbors
   implementation and schema v27 (four indexes). vnc-020 delivery is blocked until PR #596
   merges.
2. **vnc-019 must be merged first** — vnc-019 adds `max_depth: Option<u8>` to `GraphParams`
   and the `subgraph` arm in `validate_no_unsupported_params`. vnc-020 delivery is blocked
   until PR #597 merges. Starting vnc-020 design before vnc-019 delivers is safe; delivery
   implementation is blocked.
3. **No schema migration** — schema v27 (from vnc-018) already contains both composite
   indexes required by `inverse` and `filter` modes. vnc-020 introduces no migration, no
   new tables, no new columns. `CURRENT_SCHEMA_VERSION` stays at 27.
4. **GraphParams struct lock (ADR-003 vnc-018)** — field removal and retyping are prohibited.
   New fields added by vnc-020 must be `Option<T>` only.
5. **500-line file limit** — `graph_read.rs` already contains the core wire types, dispatch
   logic, and validation. With subgraph (vnc-019) and three new handlers (vnc-020), the
   file will approach or exceed 500 lines. Handlers for `inverse`, `filter`, and `path` must
   each be implemented in sibling modules (`graph_read_inverse.rs`, `graph_read_filter.rs`,
   `graph_read_path.rs`) following the existing `#[path = "..."]` include pattern.
6. **Capability gate unchanged** — all three modes require `Capability::Read`, enforced in
   `tools.rs` before `handle_graph` is called. No new capability is introduced.
7. **No new MCP tool** — all three are modes of `context_graph`. Total MCP tool count
   remains at 14 after vnc-019 and 14 after vnc-020.
8. **In-memory BFS for path only** — `inverse` and `filter` modes execute SQL against the
   live database (no staleness). `path` mode uses the in-memory `TypedRelationGraph` (tick
   staleness applies). No SQL fallback for `path` depth > 1.
9. **No raw SQL injection surface in filter mode** — property filters must be expressed as
   typed `GraphParams` fields, not a free-form `where_clause` string. The ASS-057 Track B
   proposal for `where: String` is rejected on security grounds.

## Open Questions

All open questions resolved before design phase began.

**OQ-01 — RESOLVED**: `resolve_supersessions` is included in `path` mode. Consistent with
neighbors and subgraph. ASS-057 recommends build-in over retrofit. Resolution applies to
endpoints (`from_id`, `to_id`) before BFS begins, in addition to intermediate nodes.

**OQ-02 — RESOLVED**: Reuse `depth` field. Semantics are identical (hop limit). New field
would be struct pollution without behavioral benefit. Rejection guards added on all modes
that do not accept `depth` — corrects the existing silent-ignore inconsistency.

**OQ-03 — RESOLVED**: AND semantics — entries missing ALL specified types. More useful
for gap detection; OR behavior achievable via two separate queries. Maps naturally to the
LEFT JOIN pattern (all null checks ANDed).

**OQ-04 — RESOLVED**: Outgoing only for vnc-020; no `direction` parameter. Bidirectional
BFS is a materially different implementation — no stub parameter that accepts only one value.
`GraphParams` `Option<T>` extension can absorb `direction` in a future release without
breaking callers.

**Path response format — RESOLVED**: `from_id` is a top-level field; `hops` array contains
only traversed hops (no null relation_type for the start node). `length = hops.len()`.

**filter max_edge_count=0 — RESOLVED**: Explicitly validated in integration tests (AC-29).
The `= 0` boundary (COUNT(*) = 0) is structurally different from COUNT(*) >= N and is the
primary Q10 use case.

## Tracking

GH Issue #598 (linked from spawn prompt). Will be updated after session opening.
