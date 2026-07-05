# Pseudocode Overview — vnc-043

context_graph subgraph: Class-1 doc fix + live depth-1 read (GH #903).

NARROW feature. Two co-equal components, one delivery wave, one file of Rust logic + two files
of doc text. No wire/struct/hot-path change. All names below are traced to
ARCHITECTURE.md § Integration Surface and the existing code — none invented.

## Components

| Component | File(s) touched | What changes |
|-----------|-----------------|--------------|
| `subgraph-depth1-dispatch` | `mcp/graph_read_subgraph.rs` | (a) exact `max_depth == 1` early return to `subgraph_via_db` inserted before the lock block; (b) presentation-only uniform ordering sort added as the final assembly step in BOTH `subgraph_via_db` and `handle_subgraph`'s warm-BFS path |
| `doc-surfaces` | `mcp/graph_read.rs`, `mcp/tools.rs` | 4 edit points: 2 schemars field docs + the twin description literals (const + `#[tool]` attr), edited identically; substring assertions extended; byte-equality guard #869 stays green |

The two components are independent at the code level (one touches handler logic, the other touches
doc strings + tests) and can be implemented in parallel. They share only the semantic contract that
the docs *describe* and the dispatch *implements* — depth-1 = live, depth>1 = cache; `edge_types`/
`direction` honored on subgraph.

## Data flow (unchanged shapes)

```
tools.rs::context_graph  (#[tool] handler)
  → require_cap(Read)
  → validate_no_unsupported_params   (subgraph arm unchanged)
  → handle_subgraph(store, typed_graph_state, params)
       ├── validate → seed_ids, max_depth, max_nodes, petgraph_dirs, edge_types, resolve_supersessions
       ├── IF max_depth == 1  → subgraph_via_db(...).await        ← NEW live path (no lock)
       ├── { read lock → clone graph + use_fallback → release }   ← depth>1 ONLY
       ├── IF use_fallback     → subgraph_via_db(...).await        ← unchanged cold-start
       ├── warm in-memory BFS over TypedRelationGraph              ← unchanged
       └── dangling-filter → hydrate → metadata → SORT → SubgraphResponse
```

`subgraph_via_db` internally: seed → BFS via `query_direct_neighbors` → R-02 dedup → R-05 dangling
filter → `fetch_nodes_batch` → `fetch_edge_metadata` → SORT → `SubgraphResponse`.

## Reused function surface (do NOT invent new names)

| Name | Signature (as-is) | Role |
|------|-------------------|------|
| `handle_subgraph` | `async fn(store: &Store, typed_graph_state: &Arc<RwLock<TypedGraphState>>, params: &GraphParams) -> Result<SubgraphResponse, ErrorData>` — `graph_read_subgraph.rs:68` | entry point; receives the dispatch insertion + warm-path sort |
| `subgraph_via_db` | `async fn(store: &Store, seed_ids: &[u64], max_depth: u8, max_nodes: u32, petgraph_dirs: &[PetgraphDirection], edge_types: &[RelationType], resolve_supersessions: bool) -> Result<SubgraphResponse, ErrorData>` — `graph_read_subgraph.rs:395` | REUSED as the depth-1 path; receives the sort |
| `query_direct_neighbors`, `fetch_nodes_batch`, `fetch_edge_metadata`, `follow_to_current`, `all_non_supersedes_types` | as-is | unchanged; inherited via `subgraph_via_db` |

## Shared types (unchanged — NFR-1 / AC-10)

- `GraphParams` — wire-locked; fields `seed_ids`, `edge_types: Option<Vec<String>>`,
  `direction: Option<String>`, `max_nodes: Option<u32>`, `max_depth: Option<u8>`,
  `resolve_supersessions: Option<bool>` all already present. Doc-only edits to `direction`/`edge_types`.
- `SubgraphResponse` — `{ nodes: Vec<EntryRecord>, edges: Vec<EdgeRecord>, truncated: bool, seed_ids: Vec<u64>, depth_reached: u8 }`. No `graph_rebuilt_at` (ADR-004 vnc-019).
- `EdgeRecord` — `{ source_id: u64, target_id: u64, relation_type: String, direction: "outgoing", depth: u8, metadata: Option<Value> }`. Sort key = `(source_id, target_id, relation_type)`.
- `EntryRecord` — `{ id, title, content, status, kind, tags, ... }`. Sort key = ascending `id`.

## Shared invariants (both components must honor)

1. **Exact `== 1` dispatch, before the lock.** Never `<= 1`/range; placed after `resolve_supersessions`
   is computed (`graph_read_subgraph.rs:162`), before the lock block (`:164/:169`). Depth-1 acquires
   NO `TypedGraphState` lock (A3 / NFR-2 / AC-10). Depth>1 + its `use_fallback` cold-start branch are
   byte-for-byte unchanged at the SET level (AC-02 / SR-07).
2. **Ordering is presentation-only + set-preserving.** Runs AFTER the R-05 dangling filter; nodes by
   ascending `id`, edges by canonical `(source_id, target_id, relation_type)`; applied UNIFORMLY to
   both depths so callers see one ordering contract (FR-9 / ADR-003). Never adds/removes members;
   `truncated`/`depth_reached`/per-edge `depth` unaffected.
3. **Twin literals stay byte-identical.** Both `CONTEXT_GRAPH_DESCRIPTION` (`tools.rs:76`) and the
   live `#[tool(description=…)]` literal (`tools.rs:~3944`) edited identically;
   `test_graph_tool_attr_description_matches_const` (#869, `tools.rs:6263`) stays green (SR-01 / ADR-002).
4. **No wire/struct/hot-path change.** No `GraphParams`/`SubgraphResponse`/`RelationEdge` shape change.

## Sequencing

No inter-component ordering constraint. Both components are additive to existing tested code. The
tester sweep for a pre-existing fixed-order depth>1 assertion (ADR-003 consequence) is the only
cross-cutting follow-up and belongs to the test plan, not the pseudocode.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-001/003 vnc-043 (#5448/#5450), lesson
  #4562 (`use_fallback` must be read under the same lock guard — respected by the insertion-point
  placement), ADR-005 vnc-018 #4479 (neighbors depth-1-live/depth>1-cache asymmetry this feature
  mirrors), ADR-003 vnc-018 #4490 (GraphParams wire lock), and #4526 (stale in-memory BFS after
  `context_edge` — the exact freshness gap depth-1-live closes). No conflicting prior convention.
- Deviations from established patterns: none.
- Stored: nothing novel — the dual-path parity, byte-equality drift-guard (#5396/#869), and
  depth-asymmetry (ADR-005 vnc-018) patterns are already recorded; no 2+-feature pattern emerges
  that isn't already captured.
