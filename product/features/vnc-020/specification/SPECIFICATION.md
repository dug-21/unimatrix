# Specification: vnc-020 — context_graph inverse, filter, path Modes

**Feature ID**: vnc-020
**GH Issue**: #598
**Schema Version**: 27 (no migration introduced by this feature)
**Blocked on**: vnc-018 (PR #596) and vnc-019 (PR #597) merged

---

## Objective

vnc-020 completes the `context_graph` tool series by adding three modes that address structural
query patterns unreachable by any existing tool: `inverse` (SQL antijoin — entries of a given
category with no incoming edges of specified types), `filter` (combined category + property +
edge-count filter via correlated subquery), and `path` (shortest BFS path between two entries
over the in-memory `TypedRelationGraph`). All three are modes of the existing `context_graph`
MCP tool (tool count stays at 14); no new tool is introduced. All prior mode behavior, wire
types, and GraphParams field semantics are preserved unchanged.

---

## Functional Requirements

### FR-01 — inverse mode: antijoin query

`context_graph(mode="inverse")` executes a SQL LEFT JOIN antijoin against the live `entries`
and `graph_edges` tables, returning only entries of the specified `category` that have no
incoming edges of ALL the specified `missing_edge_types` (AND semantics). The query uses
`idx_graph_edges_target_type` composite index for each JOIN arm. The handler is
`handle_inverse` implemented in `graph_read_inverse.rs`.

### FR-02 — inverse mode: required parameters

`category` (String) and `missing_edge_types` (Vec<String>, non-empty) are both required.
Absence of either produces a distinct validation error with exact wording (see AC-03, AC-04).
Each element of `missing_edge_types` must parse via `RelationType::from_str`; unrecognized
values produce a validation error naming the unrecognized value and listing all 16 recognized
types (AC-02).

### FR-03 — inverse mode: AND semantics for multiple missing_edge_types

When `missing_edge_types` contains more than one type, the antijoin returns only entries that
have no incoming edges of ANY of the specified types — i.e., missing ALL of them. This is
implemented as one LEFT JOIN per type, all null checks ANDed in the WHERE clause. Entries
missing only a subset of the specified types are excluded. Callers wanting OR behavior issue
two separate `inverse` queries.

### FR-04 — inverse mode: limit and response envelope

`limit: Option<u32>` defaults to 100 when absent. Valid range is [1, 500]; values outside
this range produce a validation error stating the allowed range (AC-05). The response is a
JSON object containing `entries: Vec<EntryRecord>` and `total_returned: usize` (AC-06). Only
active entries (`status = 0`) are included.

### FR-05 — filter mode: correlated subquery

`context_graph(mode="filter")` executes a parameterized correlated subquery against the live
database. The outer query filters `entries` by `category` and optional property constraints
(`min_age_days`, `min_confidence`, `max_confidence`). When `min_edge_count` or
`max_edge_count` are present, a correlated subquery counts outgoing edges of the specified
`edge_types`. No raw SQL is accepted from the caller; all constraints are expressed as typed
`GraphParams` fields (Constraint C9). The handler is `handle_filter` in
`graph_read_filter.rs`.

### FR-06 — filter mode: required and conditional parameters

`category` is required (AC-10). If `min_edge_count` or `max_edge_count` is present, at least
one `edge_types` entry is required (AC-09). All `edge_types` elements must parse via
`RelationType::from_str`. The `limit` field (shared with `inverse` mode) applies: default
100, range [1, 500] (AC-11). The response contains `entries: Vec<EntryRecord>` and
`total_returned: usize` (AC-12). Only active entries are included.

### FR-07 — filter mode: property filter parameters

Three optional property filters map to `entries` columns:

- `min_age_days: Option<u32>`: entries where `created_at <= (NOW - N days)`. `created_at`
  is `INTEGER NOT NULL` (Unix epoch seconds). Implemented as
  `entries.created_at < (strftime('%s','now') - ? * 86400)` in parameterized SQL,
  where `?` is bound to `min_age_days`. The `datetime()` text form is incorrect for this
  column type.
- `min_confidence: Option<f64>`: entries where `confidence >= min_confidence`.
- `max_confidence: Option<f64>`: entries where `confidence <= max_confidence`.

All clauses are combined with AND. A query with none of the property filters present and no
edge count filters is a valid category-only filter (equivalent to a bounded category scan).

### FR-08 — filter mode: edge count filter parameters

Two optional edge-count filters apply a correlated subquery on `graph_edges`:

- `min_edge_count: Option<u32>`: `(SELECT COUNT(*) FROM graph_edges WHERE source_id = e.id
  AND relation_type IN (...)) >= min_edge_count`.
- `max_edge_count: Option<u32>`: same subquery `<= max_edge_count`.

The `max_edge_count = 0` boundary (COUNT(*) = 0) is explicitly supported and validated in
integration tests (AC-29). Both may be present simultaneously to express a count range.
`edge_types` is required when either is present (FR-06).

### FR-09 — filter mode: deprecated endpoint exclusion

`filter` mode executes SQL against the live database and includes only active entries
(`status = 0`). Deprecated entry records are excluded at the SQL level. There is no
staleness concern for `filter` mode.

### FR-10 — path mode: BFS over TypedRelationGraph

`context_graph(mode="path")` finds the shortest outgoing-edge path from `from_id` to
`to_id` using BFS over the in-memory `TypedRelationGraph`. The graph is acquired via read
lock, cloned, and the lock is released before BFS begins. When `edge_types` is absent or
empty, all non-Supersedes edge types are followed. When `edge_types` is present, only those
types are followed. The handler is `handle_path` in `graph_read_path.rs`.

### FR-11 — path mode: required parameters and depth

`from_id` (u64) and `to_id` (u64) are both required (AC-16, AC-17). `depth: Option<u8>`
is reused from `GraphParams`; default is 5 when absent, valid range [1, 10] (AC-18). BFS
terminates when `to_id` is first reached (shortest path) or when the frontier is exhausted
at `depth` hops without finding `to_id`.

### FR-12 — path mode: response when path found

When a path is found, the response is `PathResponse` with `found: true`, `from_id` (the
resolved or original start ID), `to_id` (the resolved or original destination ID), `hops:
Vec<PathHop>`, and `length: u8` equal to `hops.len()`. `from_id` is a top-level field
and is NOT included in `hops`. Each `PathHop` has `entry_id: u64` and `relation_type:
String` (no null relation types). The full node sequence is reconstructable as
`[from_id] + hops.map(h => h.entry_id)`.

### FR-13 — path mode: response when no path found

When no path exists within `depth` hops, or when `from_id` or `to_id` is absent from the
current in-memory graph snapshot, the response is `PathResponse` with `found: false`,
`from_id: A`, `to_id: B`, `hops: []`, `length: 0`. This is not an error (AC-14, AC-15).

### FR-14 — path mode: resolve_supersessions

`resolve_supersessions: Option<bool>` (default false, already in `GraphParams`) applies to
`path` mode. When `true`: `from_id` and `to_id` are resolved to their terminal active
successors before BFS begins, and intermediate nodes encountered during BFS are also
resolved. When `false` (audit mode): deprecated endpoints and intermediate nodes are used
as-is. The `from_id` field in the response reflects the resolved ID when resolution occurs
(AC-20, AC-21). Consistent with neighbors and subgraph mode behavior.

### FR-15 — path mode: outgoing direction only

`path` mode follows edges in `Direction::Outgoing` only (source → target as stored).
No `direction` parameter is accepted by `path` mode. Bidirectional path search is deferred
to a future release; the `GraphParams` `Option<T>` extension contract can absorb a
`direction` field without breaking callers.

### FR-16 — validate_no_unsupported_params: new mode arms

`validate_no_unsupported_params` gains three new arms (`inverse`, `filter`, `path`) and
the unrecognized-mode error lists all seven supported modes: "chain, current, neighbors,
subgraph, inverse, filter, path" (AC-26). Each new arm explicitly accepts its own params
and rejects all params that belong to other modes. See Param/Mode Rejection Matrix.

### FR-17 — depth rejection on non-neighbors, non-path modes

`depth` is accepted only by `neighbors` and `path` modes. Passing `depth` to `chain`,
`current`, `subgraph`, `inverse`, or `filter` modes produces a validation error naming the
correct mode (AC-25). This corrects the existing silent-ignore behavior on non-`neighbors`
modes; it is a behavior change for callers currently passing `depth` to those modes.

### FR-18a — path mode: self-path (from_id == to_id)

When `from_id == to_id`, path mode returns `{ found: false, from_id: A, to_id: A, hops: [], length: 0 }`.
A self-path is defined as "not found" — zero hops are required to be "at" the destination,
which is not a meaningful traversal. This is consistent with BFS: the frontier starts at
`from_id`, and the destination check fires only when a neighbor is reached, never on the
seed itself.

### FR-18 — no new MCP tool, no schema migration

All three modes are dispatched through the existing `context_graph` tool. Total MCP tool
count remains 14. `CURRENT_SCHEMA_VERSION` stays at 27. No new tables, columns, or indexes.

---

## Non-Functional Requirements

### NFR-01 — inverse mode query performance

At 3,000 entries and 10,000 graph edges, `inverse` mode must return in sub-millisecond wall
time. The `idx_graph_edges_target_type (target_id, relation_type)` composite index (added in
schema v26→v27, vnc-018) provides a single range scan per LEFT JOIN arm. Multiple
`missing_edge_types` each get one JOIN arm; N=4 missing types = 4 range scans, still
sub-millisecond at this scale.

### NFR-02 — filter mode query performance

At 3,000 entries and 10,000 graph edges, `filter` mode must return in under 10ms wall time.
Query plan: outer scan bounded by `idx_entries_category`; ~300 correlated subquery
evaluations each using `idx_graph_edges_source_type (source_id, relation_type)`. Estimated
4,500 operations at this scale — well under the 10ms budget. No additional indexes required.

### NFR-03 — path mode BFS performance

At 3,000 nodes and 10,000 edges, `path` mode with `depth=5` must return in sub-millisecond
wall time. Worst-case BFS at average degree 5 explores at most 5^5 = 3,125 nodes. In-memory
petgraph BFS is strictly faster than SQL recursive CTE at this graph size.

### NFR-04 — freshness contract per mode

`inverse` and `filter` modes query the live SQLite database directly — no staleness. Results
reflect all committed writes at query time. `path` mode uses the in-memory
`TypedRelationGraph` rebuilt each background tick (approximately 30–60 seconds). Edges
written within the current tick interval may not appear in `path` results. This asymmetry is
disclosed in the `path` mode tool description (AC-19; see Staleness Disclosure Text section).

### NFR-05 — backward compatibility

No existing mode behavior, response shape, or GraphParams field semantics change. New
GraphParams fields are `Option<T>` only (ADR-003 vnc-018). Callers passing no new fields
receive identical behavior to the current implementation.

### NFR-06 — no SQL injection surface

`filter` mode SQL is constructed entirely from typed parameters — never from caller-supplied
string fragments. Property filter clauses are built programmatically from non-null typed
fields bound as query parameters. Raw SQL input (`where_clause: String`) is explicitly
rejected by the design.

### NFR-07 — capability gate

All three new modes require `Capability::Read`, enforced in `tools.rs` before `handle_graph`
is called. No new capability is introduced.

---

## Acceptance Criteria

All ACs from SCOPE.md are reproduced verbatim below with added verification method.

| AC-ID | Statement | Verification Method |
|-------|-----------|---------------------|
| AC-01 | `context_graph(mode="inverse", category="source", missing_edge_types=["Cites"])` returns a JSON array of `EntryRecord` objects for `source` entries with no incoming `Cites` edges. | Integration test (infra-001 suite): write source entries with and without incoming Cites edges, assert only entries without are returned. |
| AC-02 | `inverse` mode with `missing_edge_types` containing an unrecognized string produces a validation error naming the unrecognized value and listing recognized edge types. | Unit test: call `handle_inverse` with `missing_edge_types=["NotAType"]`, assert error message contains "NotAType" and all 16 type names. |
| AC-03 | `inverse` mode with `missing_edge_types` absent or empty produces a validation error: "inverse mode requires at least one edge type in missing_edge_types". | Unit test: call with `missing_edge_types=None` and with `missing_edge_types=[]`, assert exact error text. |
| AC-03a | `inverse` mode rejects `edge_types` with a validation error naming `missing_edge_types` as the correct parameter. `inverse` uses `missing_edge_types` exclusively; `edge_types` has no meaning in the antijoin context. | Unit test: call with `mode="inverse"` and `edge_types=["Cites"]`; assert validation error. |
| AC-04 | `inverse` mode with `category` absent produces a validation error: "inverse mode requires category". | Unit test: call `handle_inverse` with no category field, assert exact error text. |
| AC-05 | `inverse` mode `limit` default is 100 when omitted. Valid range [1, 500]; out-of-range values produce a validation error stating the allowed range. | Unit test: omit limit, assert 100 applied. Pass limit=0 and limit=501, assert validation error. |
| AC-06 | `inverse` mode response includes a `total_returned` field with the count of entries returned. | Integration test: assert `total_returned == entries.len()` in response. |
| AC-07 | `context_graph(mode="filter", category="goal", min_age_days=30, max_edge_count=0, edge_types=["Advances"])` returns `goal` entries older than 30 days with zero outgoing `Advances` edges (Q10 stale Goal pattern). | Integration test (infra-001): write goal entries with varying ages and outgoing Advances edges, assert only old entries with 0 Advances edges are returned. |
| AC-08 | `context_graph(mode="filter", category="decision", min_edge_count=2, edge_types=["Advances"])` returns `decision` entries with two or more outgoing `Advances` edges (Q11 multi-Goal advancement pattern). | Integration test (infra-001): write decision entries with 0, 1, 2, 3 outgoing Advances edges; assert only entries with >=2 are returned. |
| AC-09 | `filter` mode with `min_edge_count` or `max_edge_count` present but `edge_types` absent or empty produces a validation error: "filter mode requires edge_types when edge_count constraints are specified". | Unit test: set `min_edge_count=1` with `edge_types=None`, assert exact error text. Repeat with `edge_types=[]`. |
| AC-10 | `filter` mode with `category` absent produces a validation error: "filter mode requires category". | Unit test: call `handle_filter` with no category, assert exact error text. |
| AC-11 | `filter` mode `limit` default is 100 when omitted. Valid range [1, 500]. | Unit test: same as AC-05 pattern for filter mode. |
| AC-12 | `filter` mode response includes `total_returned`. | Integration test: assert `total_returned == entries.len()` in response. |
| AC-13 | `context_graph(mode="path", from_id=A, to_id=B, edge_types=["Supports","Advances"], depth=5)` returns `{ found: true, from_id: A, to_id: B, hops: [...], length: N }` where `hops` contains N entries (no null relation_types) and `from_id` is NOT in `hops`. | Integration test (infra-001): write entries connected by known typed-edge chain; assert response shape, hops content, and that `from_id` is absent from hops array. |
| AC-14 | `path` mode returns `{ found: false, from_id: A, to_id: B, hops: [], length: 0 }` when no path exists between `from_id` and `to_id` within `depth` hops. | Integration test: write disconnected entries; assert `found: false` and empty hops. |
| AC-15 | `path` mode returns `{ found: false, ... }` (not an error) when `from_id` or `to_id` is not found in the current in-memory graph snapshot. | Unit test: call `handle_path` with a `from_id` not in the graph; assert `found: false` and no error code. |
| AC-16 | `path` mode with `from_id` absent produces a validation error: "path mode requires from_id". | Unit test: call with no `from_id`, assert exact error text. |
| AC-17 | `path` mode with `to_id` absent produces a validation error: "path mode requires to_id". | Unit test: call with no `to_id`, assert exact error text. |
| AC-18 | `path` mode `depth` default is 5 when omitted. Valid range [1, 10]; out-of-range values produce a validation error. | Unit test: omit depth, assert 5 applied. Pass depth=0 and depth=11, assert validation error. |
| AC-19 | `path` mode tool description includes the staleness disclosure (in-memory BFS, tick-window lag). Exact text defined in this specification. | Manual inspection of tool description string in `tools.rs`. Code review gate. |
| AC-20 | `path` mode with `resolve_supersessions=true`: if `from_id` or `to_id` is a deprecated entry, the endpoint is resolved to its terminal active successor before BFS begins. The `from_id` field in the response reflects the resolved ID, not the original. | Integration test: write deprecated entry with active successor; call path mode with deprecated entry as from_id and resolve_supersessions=true; assert response from_id is the successor's ID. |
| AC-21 | `path` mode with `resolve_supersessions=false` (default): deprecated endpoints and intermediate nodes are used as-is (audit mode). | Integration test: same setup as AC-20 but with resolve_supersessions=false; assert response from_id matches the original deprecated ID. |
| AC-22 | Passing `from_id` or `to_id` to `chain`, `current`, `neighbors`, `subgraph`, or `filter` modes produces the forward-compat validation error from `validate_no_unsupported_params` naming the correct mode. | Unit test: one test per affected mode passing from_id; assert error names "path" mode. |
| AC-23 | Passing `inverse`-only params (`missing_edge_types`) to non-inverse modes produces a validation error naming the correct mode. | Unit test: pass `missing_edge_types` with mode="filter"; assert error names "inverse" mode. Repeat for chain, current, neighbors, subgraph, path. |
| AC-24 | Passing `filter`-only params (`min_age_days`, `max_edge_count`, etc.) to non-filter modes produces a validation error naming the correct mode. | Unit test: one test per filter-only param passed to mode="inverse"; assert error names "filter" mode. |
| AC-25 | Passing `depth` to `chain`, `current`, `subgraph`, `inverse`, or `filter` modes produces a validation error naming the correct mode (corrects the existing silent-ignore behavior on non-neighbors modes). | Unit test: one test per affected mode; assert validation error is returned rather than silent continuation. |
| AC-26 | The unrecognized-mode error lists all seven supported modes: "chain, current, neighbors, subgraph, inverse, filter, path". | Unit test: call `context_graph(mode="unknown")`; assert error message contains all seven mode names. |
| AC-27 | An integration test in the infra-001 suite covers `inverse` mode: writes entries of a given category where some have incoming edges of a specified type and others do not; asserts the mode returns only the entries with no incoming edges of that type. | Integration test (infra-001 suite, named `test_context_graph_inverse_single_type` or equivalent). |
| AC-28 | An integration test in the infra-001 suite covers `inverse` mode with two `missing_edge_types`: asserts AND semantics — only entries missing ALL specified types are returned. | Integration test (infra-001 suite): write entries in 4 states (missing both, missing only first, missing only second, missing neither); assert only entries missing both are returned. |
| AC-29 | An integration test in the infra-001 suite covers `filter` mode with `max_edge_count=0`: writes goal entries where some have no outgoing Advances edges and some have one or more; asserts the mode returns only the zero-edge entries. The `= 0` boundary is explicitly validated. | Integration test (infra-001 suite, named `test_context_graph_filter_max_edge_count_zero` or equivalent). |
| AC-30 | An integration test in the infra-001 suite covers `filter` mode with `min_edge_count >= 2`: asserts only entries with two or more outgoing edges of the specified type are returned. | Integration test (infra-001 suite): entries with 0, 1, 2, 3 edges; assert only 2 and 3 returned. |
| AC-31 | An integration test in the infra-001 suite covers `path` mode: writes entries connected by a known typed-edge chain; asserts the returned hops match the expected sequence and `from_id` is not in the `hops` array. | Integration test (infra-001 suite, named `test_context_graph_path_found` or equivalent). |
| AC-32 | `path` mode with `from_id == to_id` returns `{ found: false, hops: [], length: 0 }`. A self-path is not a meaningful traversal; the BFS never fires the destination check on the seed node. | Unit test: call handle_path with from_id == to_id; assert found: false and empty hops. |

---

## Param/Mode Rejection Matrix (SR-08)

This table is a required artifact per the scope risk assessment. Every new `GraphParams`
field added by vnc-020 is listed as a row. All seven modes are columns. Cells indicate
whether the field is accepted, rejected with a validation error, or inherited from a prior
feature and not applicable as a new field.

Key:
- **accept** — mode accepts and uses this field
- **reject** — mode rejects with validation error naming the owning mode
- **n/a** — field existed before vnc-020 (not a new addition); pre-existing behavior applies

| Field | chain | current | neighbors | subgraph | inverse | filter | path |
|-------|-------|---------|-----------|----------|---------|--------|------|
| `category` (new) | reject (inverse/filter) | reject (inverse/filter) | reject (inverse/filter) | reject (inverse/filter) | accept | accept | reject (inverse/filter) |
| `missing_edge_types` (new) | reject (inverse) | reject (inverse) | reject (inverse) | reject (inverse) | accept | reject (inverse) | reject (inverse) |
| `limit` (new) | reject (inverse/filter) | reject (inverse/filter) | reject (inverse/filter) | reject (inverse/filter) | accept | accept | reject (inverse/filter) |
| `min_age_days` (new) | reject (filter) | reject (filter) | reject (filter) | reject (filter) | reject (filter) | accept | reject (filter) |
| `min_confidence` (new) | reject (filter) | reject (filter) | reject (filter) | reject (filter) | reject (filter) | accept | reject (filter) |
| `max_confidence` (new) | reject (filter) | reject (filter) | reject (filter) | reject (filter) | reject (filter) | accept | reject (filter) |
| `min_edge_count` (new) | reject (filter) | reject (filter) | reject (filter) | reject (filter) | reject (filter) | accept | reject (filter) |
| `max_edge_count` (new) | reject (filter) | reject (filter) | reject (filter) | reject (filter) | reject (filter) | accept | reject (filter) |
| `depth` (n/a — existing) | reject (neighbors/path) | reject (neighbors/path) | accept | reject (neighbors/path) | reject (neighbors/path) | reject (neighbors/path) | accept |
| `from_id` (n/a — existing stub) | reject (path) | reject (path) | reject (path) | reject (path) | reject (path) | reject (path) | accept |
| `to_id` (n/a — existing stub) | reject (path) | reject (path) | reject (path) | reject (path) | reject (path) | reject (path) | accept |

Notes:
1. `edge_types` (existing field) is accepted by `filter` (validated when edge count filters
   present) and `path` (optional filter). `inverse` mode does NOT accept `edge_types` —
   `inverse` uses `missing_edge_types` exclusively; passing `edge_types` to `inverse` mode
   must produce a validation error. `chain` and `current` already reject it; `neighbors`
   and `subgraph` already accept it. No change from vnc-020 for those modes.
2. `seed_ids` and `max_nodes` (subgraph params, vnc-019) are already rejected by all
   non-subgraph modes. No change from vnc-020 for these fields.
3. `resolve_supersessions` (existing field) is accepted by `path` mode. For `inverse` and
   `filter` modes (SQL-only, no graph traversal), `resolve_supersessions` is silently
   ignored — it has no applicable semantics and passing it is not a user error. For
   `chain` and `current`, it continues to be rejected (pre-existing behavior). For
   `neighbors` and `subgraph`, it continues to be accepted (pre-existing behavior).
4. `depth` was previously silently ignored on `chain`, `current`, and `subgraph`. vnc-020
   corrects this to a validation error (FR-17, AC-25). This is a deliberate behavior change.

---

## Wire Formats

### InverseResponse

```json
{
  "entries": [
    { "id": 42, "title": "...", "topic": "...", "category": "source", "confidence": 0.72, ... }
  ],
  "total_returned": 1
}
```

`entries` contains full `EntryRecord` objects (all fields). `total_returned` equals
`entries.len()`. Only active entries are included (`status = 0`).

### FilterResponse

```json
{
  "entries": [
    { "id": 99, "title": "...", "topic": "...", "category": "goal", "confidence": 0.61, ... }
  ],
  "total_returned": 1
}
```

Same envelope as `InverseResponse`. `entries` contains full `EntryRecord` objects. Only
active entries.

### PathResponse (path found)

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

`from_id` is a top-level field and is NOT an element of `hops`. `length` equals
`hops.len()`. Each `PathHop` has `entry_id: u64` and `relation_type: String` (never null).
The full node sequence is `[from_id] + hops.map(h => h.entry_id)`. When
`resolve_supersessions=true`, `from_id` and `to_id` in the response are the resolved IDs.

### PathResponse (no path found)

```json
{
  "found": false,
  "from_id": 123,
  "to_id": 789,
  "hops": [],
  "length": 0
}
```

Used when: (a) no path exists within `depth` hops, or (b) `from_id` or `to_id` is absent
from the current in-memory graph snapshot. Not an error — `found: false` is a valid result.

---

## Staleness Disclosure Text for path Mode (SR-01)

The following exact text must appear in the `context_graph` tool description for `path` mode.
This satisfies AC-19 and SR-01. It is modeled on ADR-004 vnc-019 (#4493), which establishes
the disclosure pattern for in-memory BFS modes.

> **path mode** uses the in-memory graph cache for BFS traversal. The cache is rebuilt each
> tick (typically 30–60 seconds). Edges written within the current tick interval may not
> appear in path results. This is the same staleness contract as `neighbors` mode at depth>1
> and `subgraph` mode. If `from_id` or `to_id` is not present in the current snapshot, the
> response is `{ found: false, hops: [], length: 0 }` — not an error. Use `inverse` or
> `filter` mode when freshness is required, as those modes query the live database directly.

This text must be included verbatim (or with only whitespace adjustments) in the tool
description string in `tools.rs`.

---

## Domain Models

### GraphParams fields (complete post-vnc-020 state)

Fields existing before vnc-020 (locked per ADR-003 vnc-018, updated by vnc-019):

| Field | Type | Owner mode(s) | Notes |
|-------|------|---------------|-------|
| `mode` | `String` | all | Required. One of: chain, current, neighbors, subgraph, inverse, filter, path. |
| `agent_id` | `Option<String>` | all | Optional caller identity. |
| `format` | `Option<String>` | all | Response format hint. |
| `id` | `Option<u64>` | chain, current, neighbors | Single entry anchor. |
| `direction` | `Option<String>` | neighbors | Edge traversal direction. |
| `edge_types` | `Option<Vec<String>>` | neighbors, subgraph, inverse, filter, path | Relation type filter; all elements validated via `RelationType::from_str`. |
| `depth` | `Option<u8>` | neighbors, path | Max hop depth. neighbors default 1 range [1,5]; path default 5 range [1,10]. |
| `resolve_supersessions` | `Option<bool>` | neighbors, subgraph, path | Default false. |
| `seed_ids` | `Option<Vec<u64>>` | subgraph | Forward-compat stub for vnc-019. |
| `max_nodes` | `Option<u32>` | subgraph | Forward-compat stub for vnc-019. |
| `from_id` | `Option<u64>` | path | Forward-compat stub placed in vnc-018. Start node. |
| `to_id` | `Option<u64>` | path | Forward-compat stub placed in vnc-018. Destination node. |
| `max_depth` | `Option<u8>` | subgraph | Added by vnc-019. subgraph BFS depth default 3 range [1,10]. |

New fields added by vnc-020 (`Option<T>` additions, backward-compatible):

| Field | Type | Owner mode(s) | Constraint | Default |
|-------|------|---------------|------------|---------|
| `category` | `Option<String>` | inverse, filter | Required for both modes. Must be a valid entry category string. | None (required) |
| `missing_edge_types` | `Option<Vec<String>>` | inverse | Required, non-empty. Each element validated via `RelationType::from_str`. | None (required) |
| `limit` | `Option<u32>` | inverse, filter | Range [1, 500]. | 100 |
| `min_age_days` | `Option<u32>` | filter | Maps to `created_at <= NOW - N days`. | None (no constraint) |
| `min_confidence` | `Option<f64>` | filter | Maps to `confidence >= N`. | None (no constraint) |
| `max_confidence` | `Option<f64>` | filter | Maps to `confidence <= N`. | None (no constraint) |
| `min_edge_count` | `Option<u32>` | filter | Requires edge_types when present. | None (no constraint) |
| `max_edge_count` | `Option<u32>` | filter | Requires edge_types when present. | None (no constraint) |

### PathHop struct

```
PathHop {
  entry_id: u64,           // The entry arrived at by this hop
  relation_type: String,   // The relation type of the edge traversed to reach entry_id
}
```

Constraints: `relation_type` is never null or empty; always one of the 16 recognized
`RelationType` variant names as a string.

### PathResponse struct

```
PathResponse {
  found: bool,             // true if a path was found within depth hops
  from_id: u64,            // Start node (resolved ID when resolve_supersessions=true)
  to_id: u64,              // Destination node (resolved ID when resolve_supersessions=true)
  hops: Vec<PathHop>,      // Empty when found=false
  length: u8,           // Equals hops.len()
}
```

### InverseResponse struct

```
InverseResponse {
  entries: Vec<EntryRecord>,  // Active entries with no incoming edges of missing_edge_types
  total_returned: usize,      // Equals entries.len()
}
```

### FilterResponse struct

```
FilterResponse {
  entries: Vec<EntryRecord>,  // Active entries matching all filter constraints
  total_returned: usize,      // Equals entries.len()
}
```

---

## User Workflows

### W1 — Orphaned source detection (inverse mode, Q9)

1. Agent calls `context_graph(mode="inverse", category="source", missing_edge_types=["Cites"])`.
2. System executes LEFT JOIN antijoin against live DB using `idx_graph_edges_target_type`.
3. Response returns all active `source` entries with no incoming `Cites` edges.
4. Agent identifies uncited sources for cleanup or promotion.

### W2 — Stale goal detection (filter mode, Q10)

1. Agent calls `context_graph(mode="filter", category="goal", min_age_days=30, max_edge_count=0, edge_types=["Advances"])`.
2. System executes correlated subquery: outer scan of `goal` entries older than 30 days;
   inner subquery counts outgoing `Advances` edges per entry.
3. Response returns only goals with COUNT = 0 and age > 30 days.
4. Agent flags these as stale goals for review.

### W3 — Multi-goal advancement tracking (filter mode, Q11)

1. Agent calls `context_graph(mode="filter", category="decision", min_edge_count=2, edge_types=["Advances"])`.
2. Response returns decision entries with 2+ outgoing `Advances` edges.
3. Agent audits whether multi-goal advancement is intentional or a modeling error.

### W4 — Goal traceability audit (path mode)

1. Agent calls `context_graph(mode="path", from_id=42, to_id=99, edge_types=["Supports","Advances"], depth=5)`.
2. System acquires TypedRelationGraph read lock, clones, releases lock.
3. BFS traverses outgoing Supports and Advances edges from node 42.
4. On first reaching node 99, path is returned with hops sequence.
5. Agent verifies the traceability chain between an action and a goal.

### W5 — ADR dependency chain audit (path mode with resolve_supersessions)

1. Agent calls `context_graph(mode="path", from_id=10, to_id=50, resolve_supersessions=true)`.
2. System resolves node 10 to its terminal active successor (e.g., node 11) before BFS.
3. BFS traverses outgoing edges from node 11.
4. Response contains `from_id: 11` (resolved), `to_id: 50`, hops sequence.

---

## Constraints

Each SCOPE.md constraint mapped to a verifiable requirement:

| SCOPE Constraint | Spec Mapping | Verifiable As |
|------------------|--------------|---------------|
| C1 — vnc-018 must merge first (schema v27 + graph_read.rs) | FR-01, FR-05, FR-10 all depend on schema v27 indexes and existing infrastructure. | Delivery is blocked at CI if vnc-018 is not merged; integration tests will fail. |
| C2 — vnc-019 must merge first (max_depth, subgraph arm) | FR-16 adds arms to validate_no_unsupported_params which must already contain subgraph arm. | Delivery blocked at CI if vnc-019 not merged. |
| C3 — No schema migration; version stays at 27 | FR-18. No new tables, columns, or indexes. | Confirmed by grep for `CURRENT_SCHEMA_VERSION` and migration count. |
| C4 — GraphParams field removal/retyping prohibited (ADR-003) | All new fields are `Option<T>` only. Existing fields unchanged. | Code review gate; struct diff confirms only additions. |
| C5 — 500-line file limit on graph_read.rs | Handlers split to `graph_read_inverse.rs`, `graph_read_filter.rs`, `graph_read_path.rs`. | Enforced by line-count check in code review gate. |
| C6 — Capability gate unchanged (Read required) | FR-18, NFR-07. Enforced in `tools.rs` before dispatch. | Unit test: unenrolled agent rejected with CapabilityDenied. |
| C7 — No new MCP tool | FR-18. Total tool count = 14. | Manual inspection of tool registration. |
| C8 — In-memory BFS for path only | FR-04, FR-09 (SQL live), FR-10 (BFS in-memory). | NFR-04 freshness contract. Architecture review. |
| C9 — No raw SQL injection surface in filter mode | FR-05, NFR-06. All filter clauses built from typed params. No `where_clause: String`. | Code review gate; no string interpolation in SQL construction. |

---

## Dependencies

### Crates

- `petgraph` — `TypedRelationGraph.inner` is `StableGraph<u64, RelationEdge>`. BFS uses the
  graph's adjacency structure. `petgraph::algo` is already imported (`is_cyclic_directed`).
  No new petgraph features required beyond what is already in use.
- `sqlx` 0.8 with SQLite — parameterized query execution for `inverse` and `filter` modes.
  The existing `Store` database connection pool is used. No new SQLx features required.

### Existing Components

- `TypedRelationGraph` in `unimatrix-server/src/services/` — path mode BFS. `node_index_for`
  (O(1) NodeIndex lookup, added vnc-018) and `edges_of_type` are required.
- `graph_read.rs` — dispatch (`handle_graph`), `validate_no_unsupported_params`, `GraphParams`
  struct. All three new handlers are dispatched from here.
- `RelationType::from_str` — validation of all caller-supplied edge type strings.
- Schema v27 indexes: `idx_graph_edges_target_type`, `idx_graph_edges_source_type` — required
  by `inverse` and `filter` SQL queries respectively.
- `EntryRecord` — return type for `inverse` and `filter` response entries.
- infra-001 integration test suite — new tests for AC-27 through AC-31 are added here.

---

## NOT in Scope

The following items are explicitly excluded to prevent scope creep:

- Any new `RelationType` enum variants — all 16 exist (vnc-015).
- `subgraph` mode — shipped in vnc-019.
- `chain`, `current`, `neighbors` behavior changes — no modifications.
- `as_of` timestamp support — Phase 3+, deferred per ASS-057.
- `context_batch_write` — out of roadmap scope.
- NLI `contradicts_category_pairs` scoping — Wave 3.
- `metadata: Option<String>` on `RelationEdge` — not required by any of the three new modes.
- Multi-hop path enumeration (all paths) — only shortest path is in scope.
- `resolve_supersessions` in `inverse` or `filter` modes — SQL-only, not applicable.
- Bidirectional path search — deferred; `path` mode is outgoing only.
- `graph_rebuilt_at` timestamp field on any response — staleness is disclosed via tool
  description text only (ADR-004 vnc-019).
- Research-domain configuration (`research-domain.toml`) — separate scope item.
- Per-hop intermediate `resolve_supersessions` in neighbors/subgraph — not modified; only
  `path` mode endpoint resolution is in scope. Architect should confirm whether intermediate
  resolution during path BFS requires new infrastructure (SR-05 open question; see below).

---

## Open Questions for Architect

**OQ-A1 — RESOLVED** (from SR-05): `follow_to_current` is already `pub(super)` and used
per-hop in both `graph_read_subgraph.rs` and `graph_read_neighbors.rs`. Path mode reuses
it for intermediate resolution; only adds endpoint resolution before BFS begins. Zero new
infrastructure required (confirmed by architect Phase 2a).

**OQ-A2 — RESOLVED** (from SR-03): `validate_no_unsupported_params` stays in `graph_read.rs`
(one function, all modes). Each sibling module performs its own parameter-level validation
(`category` required, `missing_edge_types` non-empty, etc.). Confirmed in ADR-001.

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned 10 entries including ADR-004 vnc-019
  (#4493: staleness disclosure via tool description text), ADR-003 vnc-018 (#4477: GraphParams
  struct lock and forward-compat validation), ADR-001 vnc-019 (#4490: max_depth Option<u8>
  addition pattern), and ADR-007 vnc-018 (#4481: four schema v27 composite indexes). All four
  were directly applicable to specification decisions. Also retrieved entry #4493 in full
  (staleness disclosure text model) and entry #4490 (GraphParams locked field list).
