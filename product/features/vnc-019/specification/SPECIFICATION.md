# vnc-019: context_graph subgraph Mode — Specification

## Objective

vnc-019 adds `subgraph` mode to the existing `context_graph` MCP tool (14 tools total,
unchanged). Subgraph mode performs bounded breadth-first search from one or more caller-
supplied seed entries, following specified edge types in a requested direction, and returns
both the discovered entry records and the typed edges between them — enough for a consuming
agent to reconstruct the full subgraph locally without additional queries. The feature
builds entirely on vnc-018 infrastructure (`TypedRelationGraph`, `GraphParams`, `EdgeRecord`,
schema v27 indexes) with no new MCP tool, no new crate, no new database table, and no new
migration.

---

## Functional Requirements

**FR-01** — Subgraph mode dispatch.
`context_graph` must accept `mode="subgraph"` and route to `graph_read_subgraph::handle_subgraph`
via the existing `handle_graph` dispatch. All other modes (`chain`, `current`, `neighbors`)
continue to behave as defined in vnc-018.

**FR-02** — Capability gate.
The `Capability::Read` check in `tools.rs` must be applied before `handle_graph` is called,
identical to the existing gate for all `context_graph` modes. No new gate, no change to
the gate location.

**FR-03** — Parameter: `seed_ids` (required).
`seed_ids: Vec<u64>` must be non-empty. An absent or empty `seed_ids` must produce a
validation error with message: `"subgraph mode requires at least one entry ID in seed_ids"`.
On non-subgraph modes, `seed_ids` continues to be rejected by `validate_no_unsupported_params`
(forward-compat guard preserved).

**FR-04** — Parameter: `edge_types` (optional).
`edge_types: Vec<String>` is optional. When absent or empty, all recognized `RelationType`
variants **except `Supersedes`** are traversed — i.e., the 15 non-structural edge types
returned by `all_non_supersedes_types()`, consistent with the `neighbors` mode default.
Callers who want supersession-chain edges must request `"Supersedes"` explicitly in
`edge_types`. When present, each string is validated via `RelationType::from_str`; any
unrecognized value produces a validation error naming the unrecognized value and listing
all recognized type names.

**FR-05** — Parameter: `direction` (default `"both"`).
`direction` must be one of `"incoming"`, `"outgoing"`, `"both"`. Default `"both"` when absent.
Any other value produces a validation error. Both directions are traversed when `"both"`.

**FR-06** — Parameter: `max_depth` (default 3, range [1, 10]).
`max_depth: Option<u8>` is added to `GraphParams` (ADR-001). When absent, default 3. When
present, must be in the range `[1, 10]`; out-of-range produces a validation error with message:
`"max_depth must be in range 1..=10, got {depth}"`. `max_depth` passed to `chain`, `current`,
or `neighbors` modes must be rejected by `validate_no_unsupported_params` with message:
`"max_depth is not supported in {mode} mode — use subgraph mode"`.

**FR-07** — Parameter: `max_nodes` (default 200, hard cap 200).
`max_nodes: Option<u32>` is present as a forward-compat stub from vnc-018. Default 200; hard
cap 200. Seeds count toward the cap. A value above 200 is **rejected** with a validation
error: `"max_nodes must be in range 1..=200, got {value}"`. No result may contain more than
200 nodes. Silent clamping is prohibited — the 200-node limit is a hard architectural
constraint (JSON response size budget), not a preference, and inconsistent with the
`max_depth` out-of-range pattern.

**FR-08** — Parameter: `resolve_supersessions` (default `false`).
When `resolve_supersessions=true`, each deprecated node encountered during BFS is substituted
by its terminal active successor via `follow_to_current` (50-hop guard) before enqueuing.
The deprecated node does not appear in `nodes`; the terminal active node does. Substitution
happens before the visited-set check to prevent double-enqueuing when the same successor is
reachable via multiple paths.

**FR-09** — BFS traversal via in-memory `TypedRelationGraph`.
BFS uses the in-memory `TypedRelationGraph` for hop enumeration. The read lock is acquired
once, the graph is cloned, and the lock is released before any async work. `edges_of_type`
is called per-hop per-type per-direction. No SQL is issued during the BFS inner loop.

**FR-10** — Seed entries always included in `nodes`.
All seed entries must appear in `nodes` regardless of BFS traversal depth or whether they
are also discovered as BFS neighbors. Seeds are hydrated via the same batch store query as
BFS-discovered nodes and count toward the `max_nodes` cap.

**FR-11** — `max_nodes` cap enforcement (pre-enqueue).
The cap is checked before enqueuing a node. When `collected_node_ids.len() >= max_nodes`,
BFS terminates immediately and `truncated = true` is set. No partial frontier is processed
after the cap is hit. If seeds alone reach the cap, BFS does not execute and
`truncated = true, depth_reached = 0`.

**FR-12** — Edge deduplication.
When `direction="both"`, the same physical edge `A → B` may be discovered from both the
A-outgoing and B-incoming traversal. Edges are deduplicated by canonical triple
`(source_id, target_id, relation_type)`. Canonical direction is always the stored GRAPH_EDGES
direction (`source_id → target_id`). The `direction` field in every returned `EdgeRecord`
is always `"outgoing"`.

**FR-13** — Post-BFS batch node hydration.
All collected node IDs are hydrated via a single batch store query after BFS completes.
No per-node individual queries.

**FR-14** — Post-BFS metadata batch query (ADR-003).
After BFS, all collected `(source_id, target_id, relation_type)` triples are fetched from
`GRAPH_EDGES` in a single SQL query using a dynamically built OR-chain. The result is joined
into `EdgeRecord.metadata`. When the collected-edges set is empty, the query is skipped and
all `EdgeRecord.metadata` values are `None`. The query uses `store.read_pool_server()`.

**FR-15** — Missing seed ID behavior.
A seed ID absent from the in-memory graph (cold-start or genuinely missing entry) produces
an empty result: `{ nodes: [], edges: [], truncated: false, seed_ids: [N], depth_reached: 0 }`.
This is not an error.

**FR-16** — `depth_reached` computation.
`depth_reached` is the actual maximum BFS depth traversed, computed as the maximum `depth`
value across all collected edges. When no edges were discovered (isolated seeds or empty
graph), `depth_reached = 0`.

**FR-17** — `SubgraphResponse` wire type.
The response is serialized as a JSON object:

```
{
  "nodes":         Vec<EntryRecord>,   // full EntryRecord per node
  "edges":         Vec<EdgeRecord>,    // full EdgeRecord per edge
  "truncated":     bool,               // true if max_nodes cap was hit
  "seed_ids":      Vec<u64>,           // echo of input seed_ids
  "depth_reached": u8                  // actual max BFS depth traversed
}
```

`SubgraphResponse` is defined in `graph_read.rs` alongside the other response envelopes.

**FR-18** — File placement (ADR-002).
`handle_subgraph` lives in `graph_read_subgraph.rs` declared as a `#[path]`-submodule of
`graph_read.rs`. `SubgraphResponse` is defined in `graph_read.rs`. Tests live in
`graph_read_subgraph_tests.rs` declared via `#[path]` inside `graph_read_subgraph.rs`.

**FR-19** — Tool description staleness disclosure (ADR-004).
The `context_graph` tool description in `tools.rs` must include, in the subgraph mode
section, the following text (or equivalent preserving all facts). The `direction` field
semantics and the staleness warning MUST appear in the **first two sentences** — not
buried in a closing note. Agents read the opening of a tool description and stop; a
`direction="both"` caller who sees `EdgeRecord.direction: "outgoing"` on every edge will
be confused if the explanation is at the end.

> "**subgraph** mode: All returned EdgeRecords have `direction: \"outgoing\"` regardless of
> the `direction` parameter you pass — this reflects the canonical stored edge direction
> (source_id → target_id). A `direction=\"both\"` traversal includes edges pointing TO
> your seeds, but those edges are still labeled `outgoing` (i.e., they exist as A→seed in
> storage). Use source_id / target_id to determine actual graph direction.
>
> BFS uses the in-memory graph cache, rebuilt each tick (typically 30-60 seconds). Edges
> written within the current tick interval may not appear. Same staleness contract as
> neighbors mode at depth>1.
>
> `depth_reached`: actual max depth traversed. `truncated: true`: the `max_nodes` cap was
> hit before BFS completed — retry with a smaller `max_depth` or a specific `edge_types`
> filter. Seed IDs absent from the graph return an empty result, not an error."

**FR-20** — `validate_no_unsupported_params` subgraph arm.
A new `"subgraph"` arm is added that permits `seed_ids`, `max_nodes`, and `max_depth`.
`from_id` and `to_id` remain rejected on subgraph mode (path mode only). The unrecognized-
mode error message is updated to include `subgraph` in the supported-modes list.

**Delivery note — vnc-018 test breakage**: Adding the `"subgraph"` arm causes the vnc-018
test `test_validate_unrecognized_mode_fires_before_field_check` to fail — it asserts that
`mode="subgraph"` returns `"unrecognized mode"`. After vnc-019 ships, `"subgraph"` is a
recognized mode and that assertion is wrong. The delivery agent must update this test as
part of FR-20 delivery: remove the `"subgraph"` case from the unrecognized-mode test and
add it to the recognized-mode test suite instead.

**FR-21** — No engine changes.
`unimatrix-engine` (`TypedRelationGraph`, `RelationEdge`, `graph.rs`) must not be modified.
Metadata flows through the post-BFS SQL path, not through `RelationEdge`.

**FR-22** — No new migration.
vnc-019 requires schema v27 (delivered by vnc-018) and adds no new migration. Total table
count unchanged.

**FR-23** — Integration test.
An integration test in the infra-001 suite must write at least 5 entries with typed edges,
call `context_graph(mode="subgraph", ...)`, and assert that the returned `nodes` and `edges`
match the expected subgraph topology.

---

## Non-Functional Requirements

**NFR-01** — BFS latency.
At 3,000 in-memory nodes, depth 3, `both` direction, and no edge-type filter (worst-case
traversal), BFS must complete in under 50 ms (excluding post-BFS I/O). The petgraph
adjacency traversal is sub-millisecond per hop; this bound is architectural, not
benchmarked in this feature, but must not be regressed.

**NFR-02** — Post-BFS I/O bounded queries.
The total number of SQL round-trips for a single `subgraph` call is bounded at 3:
(1) batch node hydration, (2) post-BFS metadata batch query, (3) optional per-deprecated-
node `follow_to_current` calls (capped at `max_nodes = 200`). No N+1 patterns.

**NFR-03** — Metadata batch query scale.
The OR-chain metadata query is bounded by the `max_nodes = 200` node cap, which bounds
edges to approximately 600 at depth 3 with `both` direction on an average-degree graph.
The query must not be issued when `collected_edges` is empty.

**NFR-04** — Lock hold time.
The `TypedRelationGraph` read lock is held only for the duration of the clone operation.
No async I/O may occur while the lock is held. This preserves tick-rebuild availability.

**NFR-05** — Memory.
The `SubgraphResponse` peak payload at the 200-node / ~600-edge cap is approximately
290 KB JSON (200 entries × ~1 KB + 600 edges × ~150 bytes). This must fit within standard
LLM context window limits and must not require streaming.

**NFR-06** — No regressions on existing modes.
The `chain`, `current`, and `neighbors` mode implementations must not be functionally
changed. All existing tests must pass after delivery.

**NFR-07** — Compatibility.
`GraphParams` additions (`max_depth`) use `Option<T>` to preserve backward compatibility.
Callers that omit `max_depth` receive the default behavior (depth 3). No existing
serialized `GraphParams` JSON is invalidated.

**NFR-08** — SR-05 supersession I/O bound.
When `resolve_supersessions=true`, the 50-hop guard on `follow_to_current` limits each
individual chain walk. The total number of `follow_to_current` calls within one BFS
invocation is bounded by `max_nodes = 200`. The worst-case I/O is 200 sequential
`Store::get()` calls, which is acceptable for a read-only path. No batch supersession
pre-resolution is required in this feature.

---

## Acceptance Criteria

**AC-01** — Subgraph mode returns typed response.
`context_graph(mode="subgraph", seed_ids=[N], edge_types=["Supports"], direction="both",
max_depth=2)` returns a JSON object containing `nodes` (array), `edges` (array), and
`truncated` (bool).
Verification: integration test, `test_protocol.py` P-04.

**AC-02** — Nodes are full EntryRecords.
Each entry in `nodes` is a full `EntryRecord` serialized as JSON with the same shape as
entries returned by other `context_graph` modes.
Verification: integration test field-shape assertion.

**AC-03** — Edges are full EdgeRecords.
Each entry in `edges` is an `EdgeRecord` with fields: `source_id` (u64), `target_id` (u64),
`relation_type` (string), `direction` (always `"outgoing"`), `depth` (u8, 1-based hop from
nearest seed), and `metadata` (`null` when `GRAPH_EDGES.metadata` is null; populated
otherwise).
Verification: integration test field-shape and value assertion.

**AC-04** — Seeds always in nodes.
Seed entries are present in `nodes` regardless of whether they are also BFS-discovered
neighbors. A call with a single seed and no matching edges must still return the seed entry
in `nodes`.
Verification: unit test with isolated seed node.

**AC-05** — `max_nodes` cap and truncation flag.
`max_nodes = 200` is the hard cap including seeds. When BFS would exceed `max_nodes`, BFS
terminates early and `truncated: true` is returned. A call with 201 seeds must return
`truncated: true` with exactly 200 nodes.
Verification: integration test with seed list exceeding cap.

**AC-06** — `max_depth` default and range validation.
`max_depth` defaults to 3 when absent. Passing `max_depth=0` or `max_depth=11` must produce
a validation error with message indicating the allowed range `1..=10`.
Verification: unit test for each boundary value (0, 1, 10, 11).

**AC-07** — `seed_ids` empty/absent validation.
`seed_ids` absent or empty produces a validation error with message:
`"subgraph mode requires at least one entry ID in seed_ids"`.
Verification: unit test.

**AC-08** — Unknown `edge_type` validation and default exclusion of Supersedes.
An unrecognized `edge_type` string produces a validation error naming the unrecognized value
and listing all recognized type names. When `edge_types` is absent or empty, the traversal
uses `all_non_supersedes_types()` (15 types — excludes Supersedes). Passing
`edge_types=["Supersedes"]` explicitly traverses supersession edges.
Verification: unit test with a synthetic unknown type string; unit test confirming Supersedes
absent from default traversal; unit test confirming Supersedes present when explicitly requested.

**AC-09** — `resolve_supersessions=true` substitution.
When `resolve_supersessions=true`, deprecated nodes are replaced by their terminal active
successors before enqueuing. The deprecated node does NOT appear in `nodes`; the terminal
node does.
Verification: integration test with a deprecated node in the traversal path.

**AC-10** — `resolve_supersessions=false` behavior.
When `resolve_supersessions=false` (default), deprecated nodes are included in `nodes`
as-is; edges are returned as stored.
Verification: integration test confirming deprecated node present in `nodes`.

**AC-11** — `seed_ids` and `max_depth` rejected on non-subgraph modes.
Passing `seed_ids` or `max_depth` to `chain`, `current`, or `neighbors` mode returns the
existing `validate_no_unsupported_params` validation error. The forward-compat guard for
`seed_ids` must be preserved after the subgraph arm is added; `max_depth` must be newly
rejected on all three existing modes.
Verification: unit tests for `seed_ids` on each of chain/current/neighbors, and `max_depth`
on each of chain/current/neighbors.

**AC-12** — Edge deduplication.
When `direction="both"`, a single stored edge `A → B` appears exactly once in `edges`. The
`direction` field on that `EdgeRecord` is always `"outgoing"`.
Verification: integration test with `direction="both"` asserting no duplicate triples.

**AC-13** — Tool description staleness disclosure.
The `context_graph` tool description text for subgraph mode includes: (a) in-memory BFS
and tick-window staleness, (b) `depth_reached` and `truncated` semantics, (c) unknown seed
ID behavior (empty result, not error), (d) `EdgeRecord.direction` always `"outgoing"`.
Verification: code review of `tools.rs` description string.

**AC-14** — Integration test: subgraph topology.
An integration test in the infra-001 suite writes 5+ entries with typed edges, calls
`context_graph(mode="subgraph", ...)`, and asserts the returned nodes and edges match the
expected subgraph topology (node IDs, edge triples, depths).
Verification: test run in CI.

**AC-15** — `depth_reached` accuracy.
`depth_reached` in the response equals the actual maximum BFS depth traversed. When BFS
terminates early due to the `max_nodes` cap, `depth_reached` reflects the depth at which
truncation occurred, not `max_depth`. When no edges are traversed, `depth_reached = 0`.
Verification: integration test with depth assertions.

**AC-16** — `max_depth` on non-subgraph modes validation error.
Passing `max_depth` to `chain`, `current`, or `neighbors` produces a validation error with
message: `"max_depth is not supported in {mode} mode — use subgraph mode"`.
Verification: unit tests (extends SR-07 coverage from SCOPE-RISK-ASSESSMENT).

**AC-17** — Missing seed ID returns empty result.
A call where all seed IDs are absent from the in-memory graph returns
`{ nodes: [], edges: [], truncated: false, seed_ids: [...], depth_reached: 0 }`.
No error is returned.
Verification: unit test with a non-existent seed ID.

**AC-18** — `EdgeRecord.metadata` populated.
When `GRAPH_EDGES.metadata` is non-null for an edge in the result, the corresponding
`EdgeRecord.metadata` is a parsed JSON value (not a string). When null, `EdgeRecord.metadata`
is JSON `null`.
Verification: integration test with a graph edge that has non-null metadata.

**AC-19** — No metadata query on empty edge set.
When BFS produces no edges (isolated seed, cold-start), the post-BFS metadata batch query
is not issued.
Verification: unit test confirming zero SQL queries for the metadata fetch when edge list
is empty.

---

## Domain Models

### Entities

**Entry** — the core knowledge unit stored in ENTRIES. Identified by `id: u64`. Has a
`status` field (`Active` | `Deprecated`). Deprecated entries have a `superseded_by: u64`
pointer to a successor. `EntryRecord` is the serialized wire form.

**Edge** — a typed, directed relationship between two entries stored in GRAPH_EDGES.
Fields: `source_id`, `target_id`, `relation_type` (string mapping to `RelationType`),
`metadata` (nullable TEXT). The canonical direction is source → target. `EdgeRecord` is
the serialized wire form.

**RelationType** — the 16-variant enum governing allowed relationship semantics. All 16
variants are recognized and traversable. Subgraph mode validates that caller-supplied
`edge_types` strings parse to known variants.

**TypedRelationGraph** — the in-memory petgraph structure built by the background tick.
Provides `node_index_for(id) -> Option<NodeIndex>` and
`edges_of_type(node_idx, rel_type, direction) -> Iterator<EdgeReference>`. The sole BFS
traversal surface. Rebuilt from `GRAPH_EDGES` on each tick; tick interval is typically
30-60 seconds (tick-window staleness contract).

**BFS Frontier** — a `VecDeque<(NodeIndex, u64, u8)>` of (graph index, entry id, current
depth) tuples. Processes nodes level by level up to `max_depth`.

**Visited Set** — a `HashSet<u64>` keyed by entry ID. Prevents cycles and double-enqueuing
when the same node is reachable via multiple paths.

**SubgraphResponse** — the wire envelope returned for subgraph mode. Fields: `nodes`
(Vec<EntryRecord>), `edges` (Vec<EdgeRecord>), `truncated` (bool), `seed_ids` (Vec<u64>),
`depth_reached` (u8).

**GraphParams** — the shared wire struct for all `context_graph` modes, locked by ADR-003
vnc-018. Extended in vnc-019 with `max_depth: Option<u8>`. All additions are `Option<T>`
for backward compatibility.

### Ubiquitous Language

- **seed** — an entry ID supplied as a starting point for BFS traversal.
- **hop** — one edge traversal step in BFS; depth increments by 1 per hop from the seed.
- **depth_reached** — actual maximum hop count traversed in a BFS call (not the requested
  maximum).
- **truncated** — `true` when BFS was stopped by the `max_nodes` cap before exhausting the
  reachable subgraph.
- **canonical direction** — the direction edges are stored in GRAPH_EDGES: source → target.
  `EdgeRecord.direction` is always `"outgoing"` in subgraph mode.
- **terminal active successor** — the result of `follow_to_current`: the non-deprecated
  entry at the end of the supersession chain from a deprecated entry.
- **tick-window staleness** — the interval between graph rebuilds during which new edges
  are present in GRAPH_EDGES but absent from the in-memory `TypedRelationGraph`.
- **forward-compat stub** — an `Option<T>` field present in `GraphParams` for a future
  mode, rejected on existing modes by `validate_no_unsupported_params`.

---

## User Workflows

### W1 — Agent reconstructs evidence subgraph for a Goal entry

1. Agent holds `goal_id` (seed).
2. Agent calls `context_graph(mode="subgraph", seed_ids=[goal_id], direction="both",
   max_depth=3)`.
3. Server returns `SubgraphResponse` with full entry records and edge records.
4. Agent reconstructs the subgraph locally from `nodes` and `edges`.
5. If `truncated=true`, agent may re-query with a smaller `max_depth` or targeted
   `edge_types` filter to stay within the cap.

This is Q2 (Goal full subgraph) from the research domain traversal patterns.

### W2 — Agent surfaces thesis evidence chain with typed edges

1. Agent holds `thesis_id`.
2. Agent calls `context_graph(mode="subgraph", seed_ids=[thesis_id],
   edge_types=["Supports", "Refutes", "Cites"], direction="both", max_depth=2)`.
3. Server returns supporting and refuting nodes with typed edge records.
4. Agent uses `relation_type` in `edges` to reason about evidence polarity.

This is Q3 (Thesis evidence chain) from the research domain traversal patterns.

### W3 — Agent surfaces contradiction network

1. Agent holds one or more `entry_ids` suspected of contradiction.
2. Agent calls `context_graph(mode="subgraph", seed_ids=[...],
   edge_types=["Refutes"], direction="both", max_depth=2)`.
3. Server returns contradiction-adjacent entries and edges.
4. Agent inspects edge `relation_type` to map contradiction surface.

This is Q5 (Contradiction surface) from the research domain traversal patterns.

### W4 — Supersession-clean traversal

1. Agent calls `context_graph(mode="subgraph", seed_ids=[id],
   resolve_supersessions=true, direction="both")`.
2. Server substitutes deprecated BFS-discovered nodes with their terminal active
   successors.
3. Returned `nodes` contain only active entries; agent does not need to filter by status.

---

## Constraints

**C-01** — vnc-018 must merge first (SR-06).
vnc-019 delivery is hard-blocked on vnc-018 PR #596 merging. `graph_read.rs`,
`graph_read_neighbors.rs`, and `graph_read_supersession.rs` are stubs until vnc-018
delivers the full chain/current/neighbors implementation. Delivery must not begin
against a stub.

**C-02** — Schema v27 required; no new migration.
vnc-019 requires the four indexes added in schema v26→v27 migration (vnc-018):
`idx_entries_supersedes`, `idx_entries_superseded_by`, `idx_graph_edges_source_type`,
`idx_graph_edges_target_type`. No additional migration is introduced by vnc-019.

**C-03** — `GraphParams` struct lock.
The `GraphParams` struct layout is a wire contract (ADR-003 vnc-018). Field removal and
retyping are prohibited. Adding `max_depth: Option<u8>` is permitted as a backward-
compatible `Option<T>` extension (ADR-001 vnc-019).

**C-04** — 500-line file limit.
`graph_read.rs` must not exceed 500 lines. `handle_subgraph` is placed in
`graph_read_subgraph.rs` (ADR-002 vnc-019) to maintain this constraint.

**C-05** — In-memory BFS only; no SQL fallback.
Subgraph mode uses only the in-memory `TypedRelationGraph` for hop enumeration. Cold-start
behavior (empty graph) returns an empty result, not a SQL fallback query. Consistent with
neighbors mode (ADR-005 vnc-018).

**C-06** — No engine changes.
`unimatrix-engine` (`TypedRelationGraph`, `RelationEdge`) must not be modified. Edge
metadata is sourced from the post-BFS SQL batch, not from the in-memory struct.

**C-07** — No new MCP tool.
`subgraph` is a mode of the existing `context_graph` tool. Total MCP tool count is 14
after delivery (unchanged from vnc-018 target).

**C-08** — No `graph_rebuilt_at` in response.
`SubgraphResponse` does not include a staleness timestamp (ADR-004 vnc-019). The tool
description text is the sole staleness disclosure mechanism.

**C-09** — SR-02: `truncated` bool is sufficient.
Structured truncation reason (e.g., "seed saturation" vs. "BFS expansion") is explicitly
deferred to W1B-2c (#598). `truncated: bool` is the only signal in this feature.

**C-10** — SR-05: supersession resolution is inline per-hop.
`follow_to_current` is called inline during BFS when `resolve_supersessions=true`.
Batch pre-resolution before BFS is out of scope. The 50-hop guard on `follow_to_current`
is the depth circuit-breaker; the `max_nodes = 200` cap bounds total calls to 200 max.

---

## Dependencies

### Crates

- `unimatrix-server` — the only crate modified. New file: `graph_read_subgraph.rs`.
- `unimatrix-engine` — consumed read-only. `TypedRelationGraph`, `node_index_for`,
  `node_id_for_index`, `edges_of_type`, `RelationType`, `Direction`. No changes.
- `unimatrix-store` — consumed read-only via `store.read_pool_server()` and
  `Store::get_many` (or equivalent batch). No changes.
- `petgraph` — already a dependency of `unimatrix-engine`. No new dependency.
- `sqlx` — already used in `unimatrix-server` for direct SQL queries.
- `serde_json` — already used for `EdgeRecord.metadata` type.
- `rmcp 0.16.0` — unchanged; `CallToolResult::success` pattern used as in all other modes.

### Existing Components

- `GraphParams` (`graph_read.rs`) — extended with `max_depth: Option<u8>`.
- `EdgeRecord` (`graph_read.rs`) — unchanged structurally; `metadata` field populated for
  the first time.
- `validate_no_unsupported_params` (`graph_read.rs`) — extended with `"subgraph"` arm.
- `handle_graph` (`graph_read.rs`) — extended with `"subgraph"` dispatch arm.
- `follow_to_current` (`graph_read_neighbors.rs`) — re-used with `pub(super)` or private
  copy in `graph_read_subgraph.rs`.
- `all_non_supersedes_types` (`graph_read_neighbors.rs`) — re-used for default `edge_types`
  expansion.
- `TypedGraphStateHandle` (`Arc<RwLock<TypedGraphState>>`) — lock pattern identical to
  `graph_read_neighbors.rs`.

### External Services

None. No external network calls.

### Schema

- `GRAPH_EDGES` table (existing): `source_id`, `target_id`, `relation_type`, `metadata`
  columns consumed by the post-BFS metadata batch query.
- `ENTRIES` table (existing): consumed by batch node hydration.
- Indexes `idx_graph_edges_source_type` and `idx_graph_edges_target_type` (schema v27,
  vnc-018): required for efficient OR-chain metadata batch query per clause.

---

## NOT in Scope

The following are explicitly excluded from vnc-019 to prevent scope creep:

- `inverse` mode — antijoin queries (entries with no incoming edges of a given type). W1B-2c (#598).
- `path` mode — shortest path between two entries. W1B-2c (#598).
- `filter` mode — property + edge count filters. W1B-2c (#598).
- Any new `RelationType` enum variants — all 16 exist (vnc-015).
- Adding `metadata: Option<String>` to `RelationEdge` in `unimatrix-engine`.
- SQL-only BFS fallback path for depth > 1.
- `as_of` timestamp parameter for historical subgraph queries.
- `graph_rebuilt_at` or `graph_age_ms` field in `SubgraphResponse` (ADR-004).
- Structured truncation reason (seed saturation vs. BFS expansion) — deferred to W1B-2c.
- Batch supersession pre-resolution before BFS — inline per-hop is sufficient.
- `research-domain.toml` configuration.
- NLI `contradicts_category_pairs` scoping.
- `context_batch_write` MCP tool.
- Any change to `unimatrix-engine`, `unimatrix-store`, `unimatrix-vector`, or `unimatrix-embed`.
- Any new MCP tool or route.
- Any database migration.
- `max_nodes` values above 200 (clamped or rejected; hard cap is not adjustable upward).

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned 12 entries. Key entries confirmed:
  #4490 (ADR-001 max_depth), #4491 (ADR-002 file split), #4492 (ADR-003 metadata batch),
  #4493 (ADR-004 staleness disclosure), #4486 (post-BFS batch pattern), #4482 (BFS
  primitives in neighbors mode), #4479 (in-memory BFS rationale). All architecture
  decisions are consistent with indexed knowledge.
