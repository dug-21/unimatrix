# vnc-018: context_graph — Initial Traversal Modes (chain, current, neighbors)

## Problem Statement

Unimatrix's typed knowledge graph has a write surface (W1B-1, vnc-015/017: `context_store`/`context_correct` with `edges`, `context_edge` for standalone lifecycle management) and a graph store (`GRAPH_EDGES`, 16 `RelationType` variants). What it lacks is a read surface. Agents cannot today walk supersession chains, identify the current version of a deprecated entry, or retrieve one-hop neighbors by edge type — all without touching the semantic search pipeline.

Three concrete gaps arise immediately:

1. **Supersession history audits** — when an agent wants to see the full correction history of an entry (all prior versions in order), no tool supports this. `context_get` retrieves a single entry; `context_lookup` filters by status. Neither walks the `entries.supersedes`/`entries.superseded_by` chain.

2. **Current-version lookup** — given any ID in a supersession chain (possibly deprecated), no tool returns the live terminal entry. Agents either call `context_correct` to find the correction or search semantically — neither is a precise lookup.

3. **Graph neighbor retrieval** — given an entry and one or more edge types, no tool returns its connected neighbors. This blocks agents from using typed edges for structured retrieval: "What ADRs does this decision depend on?", "What Claims support this Thesis?", "What Findings have been tagged as advancing this Goal?"

These gaps make the graph write path (W1B-1) only half-useful: agents can declare relationships but cannot navigate them. vnc-018 delivers the first three traversal modes of the planned `context_graph` tool (GH #596), which is the 14th MCP tool in the server.

This is delivery issue W1B-2a in the product roadmap. Issues W1B-2b (#597) and W1B-2c (#598) deliver the remaining four modes (subgraph, inverse, path, filter) after this.

## Goals

1. Add a `context_graph` MCP tool (14th tool) with three initial modes: `chain`, `current`, `neighbors`.
2. `chain` mode: walk the supersession history in both directions from a given entry ID using a recursive SQL CTE on `entries.supersedes`/`entries.superseded_by`, returning the full ordered chain. Safety cap: 50 hops. `resolve_supersessions=false` is the only supported behavior for this mode (it IS the supersession query; resolving within it is semantically circular).
3. `current` mode: given any entry ID, follow `superseded_by` to the terminal active entry. Returns the live entry record. Safety cap: 50 hops.
4. `neighbors` mode: given an entry ID, edge types, direction, and depth, return connected neighbor entries (with edge metadata). Depth 1 = direct neighbors; depth > 1 = shallow multi-hop BFS. When `resolve_supersessions=true`, substitute deprecated terminal nodes with their live successors at each hop. Safety cap: 50 hops per supersession walk.
5. Add two composite indexes on `GRAPH_EDGES` that are missing and required for efficient single-type neighbor queries: `(source_id, relation_type)` and `(target_id, relation_type)`.
6. Add two indexes on `entries.supersedes` and `entries.superseded_by` that are missing and required for efficient CTE steps.
7. Update `test_protocol.py` P-03 to assert 14 tools (currently 13).
8. Add `Advances` and `Motivates` to the PPR positive edge types and BFS expansion (completing the write-only deferral from W1B-1).

## Non-Goals

- `subgraph` mode (multi-hop BFS returning both node and edge sets, 200-node cap) — W1B-2b (#597).
- `inverse` mode (antijoin: entries of a category with no incoming edges of a given type) — W1B-2c (#598).
- `path` mode (shortest path between two entries) — W1B-2c (#598).
- `filter` mode (property + edge count filter) — W1B-2c (#598).
- Adding `resolve_supersessions` to `chain` mode (it is the supersession chain; applying resolve_supersessions within it is circular).
- A `metadata` field on `RelationEdge` (needed for returning edge properties in `subgraph` mode — deferred to W1B-2b).
- Any new `RelationType` enum variants — W1B-1 (vnc-015) already added all 10 new variants (`Advances`, `Cites`, `Asserts`, `Mentions`, `Refutes`, `Tests`, `DerivedFrom`, `Motivates`, `About`, `RelatedTo`).
- `context_batch_write` — out of roadmap scope due to HNSW atomicity open question.
- Research domain configuration (`research-domain.toml`, category provisioning) — separate from this delivery.
- NLI `contradicts_category_pairs` scoping — Wave 3 intelligence enhancement.
- `revision_reason` accessibility through supersession chain traversal — `GRAPH_EDGES` Supersedes rows are skip-loaded by the in-memory graph; `revision_reason` is accessible via direct SQL only. Not addressed here.

## Background Research

### W1B-1 is complete and confirms the foundation

`context_edge` (13th tool, vnc-015, PR #600) is shipped. The `RelationType` enum now has 16 variants (6 original + 10 new). All variants have `as_str()` and `from_str()` arms in `graph.rs`. `GRAPH_EDGES` is string-typed for `relation_type` — the storage layer already handles all 16 types. `write_graph_edge` (in `nli_detection.rs`) accepts `relation_type: &str` and supports all types immediately.

Current `RelationType` variants (confirmed `graph.rs:86-121`): `Supersedes`, `Contradicts`, `Supports`, `CoAccess`, `Prerequisite`, `Informs`, `Advances`, `Motivates`, `Cites`, `Asserts`, `Mentions`, `Refutes`, `Tests`, `DerivedFrom`, `About`, `RelatedTo`.

### Supersession model is entries-table, not GRAPH_EDGES

ASS-057 Track B confirmed exhaustively: `context_correct` does NOT write a `GRAPH_EDGES` Supersedes row. `entries.supersedes` and `entries.superseded_by` are the canonical supersession fields. The in-memory `TypedRelationGraph` derives Supersedes edges from `entries.supersedes` in Pass 2a, explicitly skipping `GRAPH_EDGES` Supersedes rows in Pass 2b (`graph.rs:294-296`).

Implication: `chain` and `current` modes must query `entries.supersedes`/`entries.superseded_by` directly (recursive SQL CTE), not GRAPH_EDGES.

### `find_terminal_active` exists but is not MCP-exposed

`graph.rs:523` implements `find_terminal_active(node_id, graph, entries)` — traverses outgoing Supersedes edges to find the terminal active node. It operates on the in-memory graph. It is called by the search hot path (`TypedGraphState`) but is not exposed through any MCP tool. `current` mode wraps this functionality, but a SQL CTE implementation is preferred (avoids the read-lock dependency on the typed graph state cache, handles cold-cache gracefully, and is consistent with vnc-017 ADR-001 which chose `new_entry.id` directly over cache traversal for the same reason).

### Indexes missing from GRAPH_EDGES

Confirmed from `migration.rs:360-385` and `db.rs:956-963`: three single-column indexes exist — `idx_graph_edges_source_id`, `idx_graph_edges_target_id`, `idx_graph_edges_relation_type`. No composite indexes exist. For `neighbors` mode queries like `WHERE source_id = ?1 AND relation_type IN (?, ?, ...)`, the query uses `idx_graph_edges_source_id` and then applies `relation_type` filtering in memory. The composite `(source_id, relation_type)` and `(target_id, relation_type)` indexes collapse these into single-range scans. Both are required for `inverse` (W1B-2c) and `filter` (W1B-2c) modes as well.

### Indexes missing from entries table (supersession fields)

Confirmed from `db.rs:572-583`: no indexes on `entries.supersedes` or `entries.superseded_by`. Recursive CTE steps do a full O(N) scan per hop without them. Acceptable at 3k entries (< 2ms per chain of 10); required at 30k+. Adding `idx_entries_supersedes` and `idx_entries_superseded_by` in this migration is the right time — they serve `chain` and `current` modes directly.

### MCP tool registration pattern

From `tools.rs` analysis: new tools are added as `#[tool(...)]` attributed `async fn` methods on the `McpServerImpl` struct. Tool handler functions in this file delegate to helpers in sibling modules under `mcp/`. The `context_cycle` (12th tool) and `context_edge` (13th tool) are the reference implementations for mode-dispatched and direct tools respectively. Pattern #4436 (Unimatrix entry): "When wiring a new MCP tool handler in tools.rs to call functions in a sibling module, every call to the sibling module function must be qualified with the full module path."

The `context_cycle` tool uses `type` parameter for mode dispatch; `context_graph` will use `mode` for the same purpose (per ASS-057 Section 3 recommendation). The 500-line per-file limit means graph traversal logic must go in a new sibling module: `mcp/graph_read.rs`. `tools.rs` is already 9,610 lines — the 500-line rule governs new modules, not `tools.rs` which predates it.

### In-memory TypedRelationGraph for neighbors mode

For `neighbors` with depth > 1, BFS over the in-memory `TypedRelationGraph` is the correct path (avoids SQL round-trips per hop, petgraph already linked). `TypedRelationGraph.edges_of_type(node_idx, relation_type, direction)` provides type-filtered traversal. `TypedRelationGraph.node_index: HashMap<u64, NodeIndex>` provides O(1) entry-ID-to-petgraph lookup. For depth = 1, a direct SQL query on GRAPH_EDGES is equivalent and avoids the lock.

The in-memory graph is tick-rebuilt. Edges written within the last tick interval may not appear. For depth > 1, this staleness window is relevant — documented as a known behavioral constraint (OQ-B-4 from ASS-057).

### resolve_supersessions implementation

ASS-057 Track B designed the store-layer helper:

```rust
async fn follow_to_current(store: &Store, id: u64) -> Option<u64> {
    let mut current = id;
    for _ in 0..50 {
        let entry = store.get(current).await.ok()?;
        match entry.superseded_by {
            None => return Some(current),
            Some(next_id) => current = next_id,
        }
    }
    None  // chain too long
}
```

This is a Store-layer helper (~20 lines), applied at each hop in the `neighbors` BFS expansion when `resolve_supersessions=true`. Threading it into `neighbors` mode is ~30 additional lines. Total cost confirmed at 1-2 engineering days once the traversal tools exist.

### Advances and Motivates PPR deferral

W1B-1 explicitly deferred `Advances` and `Motivates` from PPR positive types ("write-only in this phase"). Product vision W1B-2 states these must be added as part of W1B-2. The PPR change is in `graph_ppr.rs:168-187` (the `edges_of_type` calls and `positive_out_degree_weight` function). `graph_expand.rs:62` (BFS expansion) also needs the same addition. This is approximately 16 lines across two files.

### Tool count: currently 13

`test_protocol.py` P-03 currently asserts exactly 13 `context_*` tools (last updated for vnc-015). Adding `context_graph` makes it 14. This test must be updated as part of delivery (lesson #4437 confirmed: gate 3b catches missing protocol test updates).

## Proposed Approach

### New module: `mcp/graph_read.rs`

All `context_graph` dispatch logic and mode handlers go in a new `mcp/graph_read.rs` module. `tools.rs` contains only the `#[tool]` annotated handler function that calls into `graph_read.rs`. This follows the `edge_write.rs` precedent (ADR-005, vnc-015).

### Parameter struct

```
GraphParams {
    mode: String,                          // "chain" | "current" | "neighbors"
    agent_id: Option<String>,
    format: Option<String>,
    // chain, current, neighbors:
    id: Option<u64>,
    // chain:
    direction: Option<String>,             // "forward" (descendants) | "backward" (ancestors) | "both" (default)
    // neighbors:
    edge_types: Option<Vec<String>>,       // absent or [] = all types (Supersedes always excluded)
    depth: Option<u8>,                     // 1..=10, default 1
    resolve_supersessions: Option<bool>,   // default false
    // forward-compat fields (ignored by chain/current/neighbors; validated to error if misused):
    seed_ids: Option<Vec<u64>>,            // subgraph mode (#597): multi-seed BFS
    from_id: Option<u64>,                  // path mode (#598): path source
    to_id: Option<u64>,                    // path mode (#598): path target
    max_nodes: Option<u32>,                // subgraph mode (#597): node cap (default 200)
}
```

Forward-compat fields are real typed fields with validation: if passed to an unsupported mode they return a clear error ("seed_ids is not supported in neighbors mode — use subgraph mode"). They are NOT accepted silently.

### EdgeRecord type

Define now so subgraph mode (#597) can reuse without a type change:

```
EdgeRecord {
    source_id: u64,
    target_id: u64,
    relation_type: String,
    direction: String,    // "incoming" | "outgoing" (relative to the traversal anchor)
    depth: u8,
    metadata: Option<serde_json::Value>,  // None until W1B-2b extends RelationEdge
}
```

`neighbors` mode returns `Vec<EdgeRecord>` (flat list per OQ-02 decision). The `metadata` field is always `None` in vnc-018 — the field is defined to avoid a breaking type change when W1B-2b populates it.

Mode dispatch: a `match params.mode.as_str()` block in `graph_read.rs` routes to `handle_chain`, `handle_current`, `handle_neighbors`.

### chain mode

SQL recursive CTE on `entries.supersedes`/`entries.superseded_by`. Two sub-queries depending on `direction`:

- `direction="forward"`: returns descendants — entries that supersede the seed (walking toward newer knowledge)
- `direction="backward"`: returns ancestors — entries the seed supersedes (walking toward older knowledge)
- `direction="both"` (default): union both CTEs, dedup

Direction naming aligns with the timeline, not field-pointer direction. Tool description must say: "forward: returns descendants (entries that supersede X); backward: returns ancestors (entries X supersedes)." The parenthetical descriptions "newest-first" / "oldest-first" are misleading — they describe ordering, not traversal direction; avoid them in docs.

Safety cap: `WHERE depth < 50` in the CTE recursive step, applied independently to each direction branch. **When `direction="both"` and the cap fires on one branch but not the other, the response must include a `truncated: bool` field indicating cap-firing occurred.** Agents must be able to detect truncation — a union result with no indication that it was capped is insufficient. Returns `Vec<EntryRecord>` ordered by chain position plus `truncated: bool`. If the ID does not exist: empty result (not an error).

Note: `resolve_supersessions` is not a parameter on `chain` mode — this mode IS the supersession audit; applying it would be circular.

### current mode

Single recursive CTE following `superseded_by` until a terminal is found:

```sql
WITH RECURSIVE chain(id, depth) AS (
    SELECT id, 0 FROM entries WHERE id = ?1
    UNION ALL
    SELECT e.superseded_by, c.depth + 1
    FROM entries e JOIN chain c ON e.id = c.id
    WHERE e.superseded_by IS NOT NULL AND c.depth < 50
)
SELECT e.* FROM entries e
JOIN chain c ON e.id = c.id
WHERE e.superseded_by IS NULL
LIMIT 1;
```

Returns the terminal `EntryRecord`. If the input ID is already the current version: returns that same entry. If no active terminal exists (cycle or orphaned chain): returns an informative error message.

### neighbors mode

For `depth = 1`: SQL query against GRAPH_EDGES using the composite index. This is the live-database path — freshly written edges appear immediately.

For `depth > 1`: BFS over the in-memory `TypedRelationGraph`. At each frontier node, call `edges_of_type` for each requested type and direction. Track visited set to prevent re-expansion. Apply `follow_to_current` at each hop if `resolve_supersessions=true`.

**Behavioral split must be documented in the tool description**: "depth=1 queries the live database and reflects all committed writes; depth>1 queries the in-memory graph, which may lag recent writes by up to one tick interval." This asymmetry is intentional: depth=1 is the common precision case where staleness matters; depth>1 is exploratory traversal where a tick-window of lag is acceptable. Crucially, depth=1 is always at least as fresh as depth>1 — the asymmetry goes in the expected direction.

`edge_types` accepts strings that must parse via `RelationType::from_str()`. Unknown strings return an error before any traversal. Empty or absent `edge_types` means traverse all types **excluding `Supersedes`**. If `Supersedes` is explicitly specified in `edge_types`, reject with: "Supersedes edges are not traversable via neighbors mode — use chain or current modes for supersession navigation." `Supersedes` is also silently excluded from the "all types" default expansion (same as `query_incoming_edges` in vnc-017).

Returns: `Vec<EdgeRecord>` flat list (per OQ-02) where each item carries `{source_id, target_id, relation_type, direction, depth, metadata: None}`. The `metadata` field is always `None` in vnc-018 — it is defined on `EdgeRecord` for W1B-2b forward compatibility.

### Schema migration

Add to the migration sequence:
1. `CREATE INDEX IF NOT EXISTS idx_entries_supersedes ON entries(supersedes)`
2. `CREATE INDEX IF NOT EXISTS idx_entries_superseded_by ON entries(superseded_by)`
3. `CREATE INDEX IF NOT EXISTS idx_graph_edges_source_type ON graph_edges(source_id, relation_type)`
4. `CREATE INDEX IF NOT EXISTS idx_graph_edges_target_type ON graph_edges(target_id, relation_type)`

Indexes 3 and 4 are also used by `inverse` and `filter` modes (W1B-2c) — adding them now avoids a second migration.

### PPR and BFS expansion additions

Add `Advances` and `Motivates` to:
- `graph_ppr.rs`: the `edges_of_type` calls in `personalized_pagerank` and in `positive_out_degree_weight`
- `graph_expand.rs`: the positive-type filter in the BFS expansion

## Acceptance Criteria

- AC-01: `context_graph(mode="chain", id=X)` returns all entries in the supersession chain containing X, ordered from oldest to newest. The result includes both ancestors and descendants of X in the chain.
- AC-02: `context_graph(mode="chain", id=X, direction="forward")` returns only X and its descendants (entries that supersede X). `direction="backward"` returns only X and its ancestors.
- AC-03: `context_graph(mode="chain", id=X)` on a chain of length > 50 hops returns at most 50 entries from the seed outward (safety cap enforced at the CTE level). The response includes `truncated: true` when the cap fires on either direction branch.
- AC-03b: `context_graph(mode="chain", id=X, direction="both")` where only the forward branch hits the cap returns `truncated: true`; the backward branch (if under cap) is returned in full. The agent can distinguish which direction was capped.
- AC-04: `context_graph(mode="chain", id=X)` where X does not exist returns an empty result, not an error.
- AC-05: `context_graph(mode="current", id=X)` where X is an active entry returns X.
- AC-06: `context_graph(mode="current", id=X)` where X is deprecated returns the terminal active entry at the end of the `superseded_by` chain.
- AC-07: `context_graph(mode="current", id=X)` where the chain depth exceeds 50 hops returns an error response indicating the chain is too long (safety cap).
- AC-08: `context_graph(mode="neighbors", id=X, edge_types=["Prerequisite"], direction="outgoing", depth=1)` returns all entries with a Prerequisite edge from X.
- AC-09: `context_graph(mode="neighbors", id=X, edge_types=["Supports"], direction="incoming", depth=1)` returns all entries with a Supports edge pointing at X.
- AC-10: `context_graph(mode="neighbors", id=X, edge_types=[], direction="both", depth=1)` returns all neighbors across all edge types in both directions (empty `edge_types` = all types).
- AC-11: `context_graph(mode="neighbors", id=X, depth=2)` returns both direct neighbors (depth 1) and their neighbors (depth 2), with hop depth included in the result.
- AC-12: `context_graph(mode="neighbors", ..., resolve_supersessions=true)` substitutes deprecated neighbor entries with their terminal active successors. If a hop's endpoint is deprecated and has a `superseded_by` chain, the terminal active entry is returned instead.
- AC-13: `context_graph(mode="neighbors", ..., resolve_supersessions=false)` (or default) returns edges as stored, including deprecated endpoints without substitution.
- AC-14: `context_graph` with an unrecognized `mode` value returns an error with a clear message listing supported modes.
- AC-15: `context_graph(mode="neighbors", edge_types=["UnknownType"])` returns an error before any traversal.
- AC-16: `test_protocol.py` P-03 asserts exactly 14 `context_*` tools (updated from 13).
- AC-17: `Advances` and `Motivates` participate in PPR expansion (personalized PageRank positive types). A unit test confirms these types appear in the positive-type set.
- AC-18: `Advances` and `Motivates` participate in BFS graph expansion (`graph_expand`). A unit test confirms these types are traversed.
- AC-19: Four new indexes are present in the schema after migration: `idx_entries_supersedes`, `idx_entries_superseded_by`, `idx_graph_edges_source_type`, `idx_graph_edges_target_type`. A migration test confirms their presence.
- AC-20: All three modes are covered by at least one integration test in the infra-001 Python suite.

## Constraints

### Technical constraints

- `tools.rs` is 9,610 lines. All `context_graph` logic must go in `mcp/graph_read.rs` (new module). `tools.rs` contains only the `#[tool]` dispatch point. This is mandatory — not optional.
- The `context_graph` handler requires `Capability::Read` (not `Capability::Write`) — all three modes are read-only operations.
- `chain` and `current` modes query `entries.supersedes`/`entries.superseded_by` via SQL recursive CTE — NOT the in-memory `TypedRelationGraph`. The CTE path avoids the tick-staleness window for supersession data and eliminates the read-lock dependency.
- `neighbors` depth > 1 uses the in-memory `TypedRelationGraph`. The staleness window (edges written within the last tick may not appear) is a documented behavioral constraint, not a defect.
- `RelationEdge` in the in-memory graph does NOT carry `metadata`. Neighbors mode returns edge type and direction but not edge metadata (e.g., `strength`, `contribution_kind`). This is a stated limitation until W1B-2b extends `RelationEdge`.
- Safety cap of 50 hops applies to: supersession chain depth in `chain`/`current` modes, `follow_to_current` helper depth in `neighbors` mode. This cap was established in ASS-057 and confirmed in the GH #596 issue.
- `Capability::Read` gate applies. The standard `require_cap` check must be called before any traversal.
- SQLite recursive CTEs support depth limiting via `WHERE depth < N` in the recursive step — this is the correct implementation. Not loop-based Rust code.
- `build_typed_relation_graph` Pass 2b silently drops unrecognized `relation_type` strings with `tracing::warn!`. All 16 current variants are registered in `from_str()`. This invariant must be maintained — no new variants are added in this feature.
- `write_pool_server()` and `read_pool()` currently alias the same underlying pool (`db.rs:294`). Use the canonical accessor names per C-07 (vnc-017): read operations use `read_pool()`.

### Dependency

- W1B-1 (`vnc-015`, PR #600) must be merged. It is the stated dependency. Per the git status, the current branch is `feature/vnc-017` — vnc-017 (auto-redirect) is also a dependency and must be merged first. Neither `context_graph`'s chain/current/neighbors modes depend on vnc-017 functionality directly, but the codebase state (graph.rs with 16 variants, `edge_write.rs`, `query_incoming_edges`, etc.) must be the post-vnc-017 state.

### Testing constraints

- Test infrastructure is cumulative: extend `product/test/infra-001/` fixtures and the Python suite. Do not create isolated scaffolding.
- Gate 3b requires `test_protocol.py` P-03 to be updated — this was caught as a gate failure for vnc-015 (lesson #4437). The AC-16 update is mandatory and must not be missed.

## Open Questions — RESOLVED

**OQ-01 — RESOLVED: depth=1 SQL, depth>1 in-memory graph**
SQL for depth=1 using the composite index (live database, no staleness). In-memory graph for depth>1 (tick-window staleness acceptable for exploratory use). The asymmetry is documented in the tool description and is intentional: depth=1 is always at least as fresh as depth>1. See "neighbors mode" in Proposed Approach for the required tool description text.

**OQ-02 — RESOLVED: flat list**
Flat `Vec<EdgeRecord>` where each item carries `{source_id, target_id, relation_type, direction, depth, metadata}`. The `depth` field on each item gives callers all grouping information without pre-grouped structure. Option B (grouped) is appropriate for pagination/streaming, not for a synchronous MCP tool.

**OQ-03 — RESOLVED: absent or [] = all types (excluding Supersedes)**
`edge_types=[]` or absent means traverse all types. `Supersedes` is always excluded from the "all types" default expansion. If `Supersedes` is explicitly specified, return an error directing the caller to `chain`/`current` modes.

**OQ-04 — RESOLVED: forward = descendants, backward = ancestors**
`direction="forward"` = descendants (entries that supersede X, toward newer). `direction="backward"` = ancestors (entries X supersedes, toward older). Convention aligns with the timeline. Avoid "newest-first"/"oldest-first" in documentation — those describe ordering, not traversal direction.

**OQ-05 — RESOLVED: default false for both modes**
`resolve_supersessions=false` is the default for both `chain` and `neighbors`. `chain` mode is an audit view; `neighbors` substitution is a semantic transformation agents must opt into explicitly.

**OQ-06 — RESOLVED: Supersedes not traversable in neighbors mode**
Reject with error if explicitly specified; silently exclude from "all types" default. Error message: "Supersedes edges are not traversable via neighbors mode — use chain or current modes for supersession navigation." Reasons: (1) chain/current are the correct surface with SQL CTEs and proper ordering; (2) in-memory graph Supersedes direction (predecessor→newer) is opposite to agent expectation; (3) including Supersedes in the "all types" expansion would silently mix supersession traversal into neighbor results.

## Tracking

https://github.com/dug-21/unimatrix/issues/608

GH #596 is the parent issue for W1B-2; vnc-018 corresponds to W1B-2a (neighbors+subgraph as defined in the roadmap, but scoped here to chain+current+neighbors per the spawn prompt for issue #596).
