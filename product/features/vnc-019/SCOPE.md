# vnc-019: context_graph — subgraph Mode (W1B-2b)

## Problem Statement

vnc-018 (GH #596) delivers `context_graph` with three modes: `chain`, `current`, `neighbors`.
These handle one-dimensional traversal — linear supersession chains and single-anchor
neighbor retrieval. They cannot answer the class of questions that matter most for structured
knowledge reasoning: "Give me the complete bounded evidence graph around this Goal",
"What is the full support-and-refute web around this Thesis?", "Show me the contradiction
surface reachable from these entries?"

These are multi-hop, multi-seed, multi-edge-type queries. Neighbors mode at depth > 1 gets
partway there but returns only entry records, not the edges between them — making graph
reconstruction at the agent side impossible. The agent cannot tell why two entries are
connected, which direction the edge runs, or what type of relationship is asserted.

vnc-019 adds `subgraph` mode to the existing `context_graph` tool. Subgraph mode performs
bounded BFS from one or more seed entries, following specified edge types, and returns
**both** the discovered entries and the edges between them — enough for a consuming agent
to reconstruct the full subgraph locally. The 200-node / ~290 KB JSON cap keeps results
within LLM context window limits while covering the research domain's Q2/Q3/Q5 traversal
patterns and the SDLC goal-traceability audit graph.

## Goals

1. Add `subgraph` mode to the existing `context_graph` tool (no new tool, no new route).
2. Multi-hop BFS from a caller-supplied `seed_ids` set, following `edge_types` (default:
   all non-Supersedes types, consistent with neighbors mode), respecting `direction`
   (incoming / outgoing / both), bounded by `max_depth` (default 3, max 10) and `max_nodes`
   (default 200, hard cap 200; values > 200 rejected with validation error).
3. Return `(Vec<EntryRecord>, Vec<EdgeRecord>)` — both nodes and edges so the agent can
   reconstruct the full subgraph without additional queries.
4. `resolve_supersessions: bool` (default false) — when true, substitute each deprecated
   node encountered during BFS with its terminal active successor before enqueuing it.
5. Hard cap enforcement: once `max_nodes` is reached, BFS terminates and the response
   includes a `truncated: true` flag in the summary so agents know the graph was bounded.
6. Populate the `metadata` field on `EdgeRecord` from the `GRAPH_EDGES.metadata` column
   when non-null — this was defined as `null` in vnc-018 and is first populated here
   (ADR-004 vnc-018: `metadata` defined now to prevent breaking wire change).
7. Remove the forward-compat error that rejects `seed_ids` on non-subgraph modes in
   `validate_no_unsupported_params` — replace with the `subgraph` mode dispatch arm.
8. Update `test_protocol.py` P-04 (or equivalent) to cover the new mode.

## Non-Goals

- `inverse` mode (antijoin: entries of a category with no incoming edges of a given type) —
  W1B-2c (#598).
- `path` mode (shortest path between two entries) — W1B-2c (#598).
- `filter` mode (property + edge count filters) — W1B-2c (#598).
- Any new `RelationType` enum variants — all 16 variants already exist (vnc-015).
- Adding `metadata: Option<String>` to `RelationEdge` in `unimatrix-engine` — ASS-057
  Track B OQ-B-1 identifies this as optional for topology-only results; subgraph returns
  edge metadata via post-fetch from `GRAPH_EDGES` rather than from the in-memory
  `RelationEdge`, so the engine struct change is not required.
- SQL-only BFS path — subgraph mode uses the in-memory `TypedRelationGraph` for hop
  enumeration (consistent with neighbors depth>1 per ADR-005 vnc-018), with entry
  hydration from the store. A SQL-recursive BFS is out of scope.
- `as_of` timestamp parameter — Phase 3+, deferred per ASS-057 findings.
- Research domain `research-domain.toml` configuration — separate scope item.
- NLI `contradicts_category_pairs` scoping — Wave 3 intelligence enhancement.
- `context_batch_write` — out of roadmap scope (HNSW atomicity open question, OSC-6).

## Background Research

### vnc-018 delivers the scaffolding subgraph builds on

vnc-018 (`graph_read.rs`) defines `GraphParams` with all subgraph fields already present
as forward-compat stubs: `seed_ids: Option<Vec<u64>>`, `max_nodes: Option<u32>`. The
`validate_no_unsupported_params` function rejects these on non-subgraph modes and is
documented to receive the `subgraph` arm in #597. `EdgeRecord` is defined in `graph_read.rs`
and re-exported from `mcp/mod.rs` with a doc comment explicitly saying "#597 imports
`crate::mcp::EdgeRecord` — no type change needed when #597 ships".

As of branch `feature/vnc-018`, `graph_read.rs` is a stub returning
`INTERNAL_ERROR` — agent-7 delivers the full chain/current/neighbors implementation.
vnc-019 depends on that full implementation existing before delivery can begin.

### TypedRelationGraph BFS primitives

`TypedRelationGraph` exposes two methods sufficient for BFS:

- `node_index_for(&self, id: u64) -> Option<NodeIndex>` — added in vnc-018 per ADR-008;
  the cross-crate accessor that keeps BFS in `unimatrix-server` without exposing internals.
- `edges_of_type(node_idx, relation_type, direction) -> impl Iterator<Item = EdgeReference>`
  — the sole filter boundary (SR-01). Returns all edges of the given type from a node.

For multi-type BFS, subgraph mode calls `edges_of_type` for each requested `RelationType`
variant in turn. `RelationEdge` fields available on each edge reference: `relation_type`,
`weight`, `created_at`, `created_by`, `source`, `bootstrap_only`. The `metadata` field is
NOT on `RelationEdge` — it requires a post-BFS `GRAPH_EDGES` lookup per edge triple
`(source_id, target_id, relation_type)`.

### resolve_supersessions helper

ASS-057 Track B (Section 3) describes `follow_to_current`: a Store-layer async helper that
walks `entry.superseded_by` up to 50 hops and returns the terminal id. vnc-018 ADR-005
specifies that the `follow_to_current` helper for `resolve_supersessions=true` at depth>1
uses `Store::get()` via `read_pool()` — NOT the in-memory graph. This design carries
directly into subgraph mode: at each BFS hop, if `resolve_supersessions=true` and the
target entry is deprecated, substitute the terminal active successor before enqueuing.

Supersession resolution must happen BEFORE enqueuing a node (to avoid expanding deprecated
intermediate nodes unnecessarily and to avoid the resolved successor also being in the
frontier). ASS-057 Track B confirms this ordering.

### EdgeRecord metadata field

`EdgeRecord` was defined in vnc-018 with `metadata: Option<serde_json::Value>` set to
`None` unconditionally. The doc comment explicitly states: "populated in #597 when
RelationEdge carries metadata." `GRAPH_EDGES.metadata` is `TEXT` (confirmed in
`migration.rs:340-352`). The post-BFS approach: collect `(source_id, target_id,
relation_type)` triples, issue a single `GRAPH_EDGES` batch query for the full set, and
join metadata into `EdgeRecord`. This avoids N round-trips during BFS inner loop.

### 200-node / ~290 KB size estimate

ASS-057 Track B validates: 200 entries × ~1 KB JSON + 600 edges × ~150 bytes ≈ 290 KB
as JSON. At 3k entries and ~10k edges, BFS at depth 3 with `both` direction and no
edge-type filter would typically stop on the node cap well before exhausting the graph.
The 200-node cap is enforced by the BFS frontier check before enqueuing, not after
(prevents over-collection).

### Dependency on vnc-018 indexes

vnc-018 ADR-007 adds four indexes in schema v27:
- `idx_entries_supersedes` and `idx_entries_superseded_by` — for CTE traversal
- `idx_graph_edges_source_type (source_id, relation_type)` — composite, for outgoing
  neighbor SQL
- `idx_graph_edges_target_type (target_id, relation_type)` — composite, for incoming
  neighbor SQL

These are all required by subgraph mode too: the composite indexes make the post-BFS
metadata batch query efficient; the entries indexes make `follow_to_current` O(log N).
vnc-019 requires schema v27 to already be present — no additional migration needed.

### All 16 RelationType variants are recognized

`RelationType::from_str` in `graph.rs` handles all 16 variants including the 10 added
in vnc-015: Advances, Motivates, Cites, Asserts, Mentions, Refutes, Tests, DerivedFrom,
About, RelatedTo. `build_typed_relation_graph` skips unrecognized `relation_type` strings
with `warn!` — but all types are recognized. Any `edge_types` filter value not matching
a known variant should produce a validation error, not a silent empty result (consistent
with SR-01 from vnc-015 ADR-007).

### Tick-window staleness applies to subgraph mode

The in-memory `TypedRelationGraph` is tick-rebuilt. Edges written within the current tick
interval may not appear. This is the documented behavioral contract from vnc-018 ADR-005
(neighbors depth>1). Subgraph mode inherits the same constraint. The tool description
must include the staleness disclosure text as mandated by ADR-005 — subgraph is also
in-memory BFS and the same limitation applies.

### GraphParams struct is locked

ADR-003 vnc-018 locks the `GraphParams` struct. `seed_ids` and `max_nodes` are present
as forward-compat stubs. No new fields are needed for subgraph mode. `max_depth` is NOT
yet in `GraphParams` — this is an open question (see Open Questions, OQ-01).

## Proposed Approach

Subgraph mode is implemented entirely within `graph_read.rs` as a new `handle_subgraph`
function dispatched from `handle_graph`, mirroring the `handle_chain` / `handle_current`
/ `handle_neighbors` pattern being established by vnc-018.

**BFS loop** (pseudocode, not wire pseudocode):
1. Validate parameters; reject if `seed_ids` is empty, `max_depth` out of [1,10], or any
   `edge_type` string does not parse to a known `RelationType`.
2. Acquire `TypedRelationGraph` read lock once; clone the graph; release lock.
3. Initialize frontier = seed_ids (resolved via `follow_to_current` if
   `resolve_supersessions=true`); visited = HashSet; collected_nodes = Vec; collected_edges
   = Vec.
4. BFS iteration up to `max_depth` levels. At each level:
   a. For each node in current frontier, for each requested `RelationType`, call
      `edges_of_type` (outgoing, incoming, or both per `direction`).
   b. For each edge reference, record `(source_id, target_id, relation_type, depth)` in
      edge collection.
   c. Resolve the neighbor to its current terminal if `resolve_supersessions=true`.
   d. If neighbor not in visited and `collected_nodes.len() < max_nodes`: add to next
      frontier and visited.
   e. If `collected_nodes.len() >= max_nodes`: set `truncated = true`; stop.
5. Post-BFS dangling-edge filter: remove any edge whose source_id or target_id is not in
   `collected_node_ids`. Edges to truncated (uncollected) neighbors are collected during
   BFS before the cap check fires; this pass ensures no `EdgeRecord` references a node
   absent from `nodes`.
6. Hydrate all collected node IDs → `Vec<EntryRecord>` via a single `read_pool` batch
   query.
7. Post-fetch metadata: query `GRAPH_EDGES` for all `(source_id, target_id, relation_type)`
   triples collected; join into `EdgeRecord.metadata`.
7. Format and return.

**Result format**: JSON object `{ nodes: [...], edges: [...], truncated: bool, seed_ids:
[...], depth_reached: u8 }`.

**Rationale for in-memory BFS**: Same as neighbors mode depth>1 (ADR-005): avoids N SQL
round-trips per hop, petgraph is already linked, sub-millisecond for 3k nodes. The
post-BFS hydration is O(nodes) SQL; the post-fetch metadata is O(edges) SQL — both are
single queries, not N+1.

**Rationale against adding `metadata` to `RelationEdge`**: The engine change would require
loading the `metadata` column in `query_graph_edges` and propagating it through
`build_typed_relation_graph`. This changes a shared hot-path type (used by PPR, BFS
expansion, search) to carry a field that only graph read tools use. The post-fetch approach
is 3–5 lines of additional SQL vs. modifying the engine type and all its construction
sites. Post-fetch is the correct separation.

## Acceptance Criteria

- **AC-01**: `context_graph(mode="subgraph", seed_ids=[N], edge_types=["Supports"],
  direction="both", max_depth=2)` returns a JSON result containing `nodes` and `edges`
  arrays and a `truncated` bool.
- **AC-02**: Each entry in `nodes` is a full `EntryRecord` serialized as JSON (same shape
  as other modes return entries).
- **AC-03**: Each entry in `edges` is an `EdgeRecord` with `source_id`, `target_id`,
  `relation_type`, `direction` (always `"outgoing"` in subgraph mode — canonical
  GRAPH_EDGES direction, source_id → target_id), `depth` (1-based hop count from nearest
  seed), and `metadata` (null when `GRAPH_EDGES.metadata` is null for that edge;
  populated otherwise).
- **AC-04**: Seed entries are always included in the `nodes` result, regardless of
  whether they are also discovered as BFS neighbors.
- **AC-05**: `max_nodes=200` hard cap applies to the total node count including seed
  entries. When BFS would exceed `max_nodes`, BFS terminates early and `truncated: true`
  is set in the response.
- **AC-06**: `max_depth` default is 3 when omitted. Valid range [1, 10]; out-of-range
  values produce a validation error with a message stating the allowed range.
- **AC-07**: `seed_ids` empty or absent → validation error: "subgraph mode requires at
  least one entry ID in seed_ids".
- **AC-08**: Unknown `edge_type` string → validation error naming the unrecognized value
  and listing recognized types.
- **AC-09**: `resolve_supersessions=true`: deprecated nodes encountered during BFS are
  replaced by their terminal active successors before enqueuing. The superseded node does
  NOT appear in `nodes`; the terminal node does.
- **AC-10**: `resolve_supersessions=false` (default): deprecated nodes are included in
  `nodes` as-is; edges are returned as stored.
- **AC-11**: Passing `seed_ids` with `mode="neighbors"` or any other non-subgraph mode
  continues to return the existing validation error from `validate_no_unsupported_params`
  (forward-compat guard preserved for non-subgraph modes).
- **AC-12**: `direction="both"` returns incoming and outgoing edges from each node. A
  single edge `A→B` appears once in `edges`, deduplicated by `(source_id, target_id,
  relation_type)`. The `direction` field in `EdgeRecord` is always `"outgoing"` for
  subgraph mode, reflecting the canonical `GRAPH_EDGES` direction (source_id → target_id
  as stored). This is documented in the tool description.
- **AC-13**: Tool description text for subgraph mode includes the staleness disclosure
  (in-memory BFS, tick-window lag). Exact text to be finalized in specification.
- **AC-14**: An integration test in the infra-001 suite writes 5 entries with typed edges
  and calls `context_graph(mode="subgraph", ...)`, asserting the returned nodes and edges
  match the expected subgraph topology.
- **AC-15**: `depth_reached` in the response is the actual maximum BFS depth traversed
  (useful when BFS terminates early due to `max_nodes` cap).

## Constraints

1. **vnc-018 must ship first** — subgraph mode is implemented in `graph_read.rs`, which
   is a stub until vnc-018 agent-7 delivers the full chain/current/neighbors
   implementation. vnc-019 delivery is blocked until vnc-018 PR #596 merges.
2. **Schema v27 required** — vnc-019 requires the four indexes added in vnc-018 migration
   v26→v27. No additional migration is introduced; this feature is index-only at the
   storage layer.
3. **500-line file limit** — `graph_read.rs` will grow. The BFS implementation, result
   types, and formatting may push the file toward the 500-line limit. If it approaches
   that limit, `handle_subgraph` must be split into a sibling module (e.g.,
   `graph_read_subgraph.rs`), consistent with the existing module organization.
4. **`GraphParams` struct is locked (ADR-003 vnc-018)** — no new fields may be added.
   `max_depth` is currently absent from `GraphParams`. This is a pre-delivery gap that
   must be resolved (see OQ-01).
5. **In-memory BFS only** — subgraph mode does not have a SQL-only fallback path for
   depth > 1. Cold-start behavior (empty `TypedRelationGraph`) returns an empty result or
   a documented error — not a fallback to SQL. Consistent with neighbors mode.
6. **No engine changes to `RelationEdge`** — metadata flows through post-BFS SQL, not
   through the engine's in-memory struct. No changes to `unimatrix-engine`.
7. **Capability gate unchanged** — subgraph mode requires `Capability::Read`, checked
   before `handle_graph` is called, per the established `tools.rs` pattern (FR-02).
8. **No new MCP tool** — subgraph is a mode of `context_graph`, not a separate tool.
   Total MCP tool count remains 14 after this delivery.

## Open Questions

**OQ-01 — RESOLVED**: Add `max_depth: Option<u8>` to `GraphParams` during vnc-019
delivery. ADR-003 Consequences section explicitly permits `Option<T>` additions for
backward compatibility — it forbids removing/retyping fields and silent-ignore behavior,
not future extension. Doc comment: "subgraph mode only: BFS max depth 1..=10 (default 3).
Error if passed to chain, current, or neighbors." Update `validate_no_unsupported_params`
to reject `max_depth` on non-subgraph modes, same pattern as `seed_ids`.

**OQ-02 — RESOLVED**: Always include seed entries in `nodes`. An agent reconstructing
the graph needs `EntryRecord` content for the seeds — not just IDs. Agents often discover
seed IDs indirectly and do not hold the full record. Excluding seeds would force
`context_get` calls for every seed, defeating the mode's purpose.

**OQ-03 — RESOLVED**: `direction` in `EdgeRecord` is always `"outgoing"` for subgraph
mode — canonical GRAPH_EDGES direction (source_id → target_id as stored). Dedup by
`(source_id, target_id, relation_type)` when `direction="both"` traversal finds the same
edge from both ends. No ambiguous label, no new field, `EdgeRecord` type unchanged.
Document in tool description.

**OQ-04 — RESOLVED**: JSON wrapper object only. `SubgraphResponse { nodes, edges,
truncated: bool, seed_ids, depth_reached }`. No appended text summary. `truncated` is a
plain `bool` (not per-direction; BFS terminates globally).

**OQ-05 — RESOLVED**: Return empty result `{ nodes: [], edges: [], truncated: false,
seed_ids: [N], depth_reached: 0 }`. Consistent with `query_direct_neighbors` and
`query_supersession_chain` precedents. `depth_reached: 0` signals nothing was traversed.
Document in tool description that unknown seed IDs yield empty results.

**OQ-06 — RESOLVED**: Seeds count toward `max_nodes`. The 200-node / ~290 KB budget
was sized for total node count (validated in ASS-057). A call with 50 seeds +
`max_nodes=200` must not produce 250 nodes. Single counter, no special-casing. Document
explicitly in AC-05 and tool description.

## Tracking

GH Issue #597 (linked from spawn prompt). Will be updated after session opening.
