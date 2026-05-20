# vnc-020 Pseudocode Overview
# context_graph — inverse, filter, path Modes

## Components Involved

| Component | File | Wave | Role |
|-----------|------|------|------|
| `graph_read.rs` | `crates/unimatrix-server/src/mcp/graph_read.rs` | 1 | Wire types, dispatch, validation (modified) |
| `tools.rs` | `crates/unimatrix-server/src/mcp/tools.rs` | 1 | Tool description string update (modified) |
| `graph_read_inverse.rs` | `crates/unimatrix-server/src/mcp/graph_read_inverse.rs` | 2 | Antijoin SQL handler (new file) |
| `graph_read_filter.rs` | `crates/unimatrix-server/src/mcp/graph_read_filter.rs` | 2 | Correlated subquery handler (new file) |
| `graph_read_path.rs` | `crates/unimatrix-server/src/mcp/graph_read_path.rs` | 2 | BFS shortest-path handler (new file) |

## Wave Structure Rationale

Wave 1 establishes the foundation that Wave 2 agents compile against in parallel:
- Wave 1 declares the three sibling modules via `#[path]` (so the compiler can find them).
- Wave 1 adds all new `GraphParams` fields that the handlers read.
- Wave 1 adds all new response types that the handlers return.
- Wave 1 adds dispatch arms so `handle_graph` calls the handlers.
- Wave 1 updates validation so errors are surfaced correctly before dispatch.

Wave 2 files `graph_read_inverse.rs`, `graph_read_filter.rs`, and `graph_read_path.rs`
can be written in parallel because they share no direct dependencies on each other —
they all import from `graph_read.rs` (via `super::`) and from `graph_read_neighbors.rs`
(via `super::graph_read_neighbors::{follow_to_current, all_non_supersedes_types}`).

## Data Flow

```
MCP caller
  → tools.rs context_graph()
      require_cap(Read)                        [tools.rs]
      handle_graph(store, typed_graph_state, params, ctx)   [graph_read.rs]
        validate_no_unsupported_params(&params) [graph_read.rs, centralized]
        match params.mode {
          "inverse" →
            graph_read_inverse::handle_inverse(store, &params)
              params: GraphParams.{category, missing_edge_types, limit}
              SQL: N LEFT JOIN antijoin via store.read_pool_server()
              returns: InverseResponse { entries: Vec<EntryRecord>, total_returned }
                                                             [graph_read.rs type]

          "filter" →
            graph_read_filter::handle_filter(store, &params)
              params: GraphParams.{category, edge_types, limit,
                                   min_age_days, min_confidence, max_confidence,
                                   min_edge_count, max_edge_count}
              SQL: correlated subquery via store.read_pool_server()
              returns: FilterResponse { entries: Vec<EntryRecord>, total_returned }
                                                             [graph_read.rs type]

          "path" →
            graph_read_path::handle_path(store, typed_graph_state, &params)
              params: GraphParams.{from_id, to_id, depth, edge_types,
                                   resolve_supersessions}
              resolve endpoints via follow_to_current        [graph_read_neighbors.rs]
              clone TypedRelationGraph, release RwLock
              BFS outgoing only, path-carrying frontier
              returns: PathResponse { found, from_id, to_id,
                                     hops: Vec<PathHop>, length }
                                                             [graph_read.rs types]
        }
        serialize response → CallToolResult::success(text(json))
```

## Shared Types (New — defined in graph_read.rs)

These types are defined in `graph_read.rs` and imported by the sibling modules via
`use super::{GraphParams, InverseResponse, FilterResponse, PathHop, PathResponse}`.

```
InverseResponse {
    entries: Vec<EntryRecord>,   // active entries with no incoming edges of missing_edge_types
    total_returned: usize,       // always == entries.len()
}

FilterResponse {
    entries: Vec<EntryRecord>,   // active entries matching all filter constraints
    total_returned: usize,       // always == entries.len()
}

PathHop {
    entry_id: u64,               // entry arrived at by this hop
    relation_type: String,       // edge type traversed to reach entry_id (never null)
}

PathResponse {
    found: bool,                 // true iff path found within depth hops
    from_id: u64,                // start node (resolved ID when resolve_supersessions=true)
    to_id: u64,                  // dest node  (resolved ID when resolve_supersessions=true)
    hops: Vec<PathHop>,          // empty when found=false
    length: u8,                  // always == hops.len()
}
```

New `GraphParams` fields (added to the locked struct per ADR-002):

```
category: Option<String>            -- inverse (required), filter (required), others reject
missing_edge_types: Option<Vec<String>>  -- inverse (required, non-empty)
limit: Option<u32>                  -- inverse and filter (default 100, range [1,500])
min_age_days: Option<u32>           -- filter only
min_confidence: Option<f64>         -- filter only
max_confidence: Option<f64>         -- filter only
min_edge_count: Option<u32>         -- filter only (requires edge_types when present)
max_edge_count: Option<u32>         -- filter only (requires edge_types when present)
```

Pre-existing fields reused by new modes (unchanged):
- `from_id: Option<u64>`, `to_id: Option<u64>` — path mode endpoints
- `depth: Option<u8>` — path mode hop limit (default 5, range [1,10])
- `edge_types: Option<Vec<String>>` — path and filter type filter; REJECTED on inverse
- `resolve_supersessions: Option<bool>` — path mode; silently ignored on inverse/filter

## Integration Surface (Consumed by Wave 2)

| Item | Location | Used By |
|------|----------|---------|
| `GraphParams` (extended) | `graph_read.rs` | all three handlers |
| `InverseResponse` | `graph_read.rs` | `graph_read_inverse.rs` |
| `FilterResponse` | `graph_read.rs` | `graph_read_filter.rs` |
| `PathHop`, `PathResponse` | `graph_read.rs` | `graph_read_path.rs` |
| `follow_to_current` (pub(super)) | `graph_read_neighbors.rs` | `graph_read_path.rs` |
| `all_non_supersedes_types` (pub(super)) | `graph_read_neighbors.rs` | `graph_read_path.rs` |
| `TypedGraphState` | `services/typed_graph.rs` | `graph_read_path.rs` |
| `store.read_pool_server()` | `unimatrix-core` Store | `graph_read_inverse.rs`, `graph_read_filter.rs` |
| `RelationType::from_str` | `unimatrix-engine/graph.rs` | all three handlers |
| `EntryRecord` | `unimatrix-core` | all three handlers (response type) |

## Sequencing Constraints

1. vnc-018 (PR #596) merged — provides `GraphParams`, `validate_no_unsupported_params`,
   `handle_graph`, schema v27 indexes.
2. vnc-019 (PR #597) merged — provides `follow_to_current` and `all_non_supersedes_types`
   as `pub(super)`, `max_depth` field, subgraph arm in validation.
3. Wave 1 (`graph_read.rs`, `tools.rs`) must compile successfully before Wave 2 agents
   begin, because Wave 2 modules are declared via `#[path]` in `graph_read.rs` — the
   compiler requires the file to exist (even if empty) once the `mod` declaration is present.
   Wave 2 agents must create their files immediately on spawn (pattern #4509).
