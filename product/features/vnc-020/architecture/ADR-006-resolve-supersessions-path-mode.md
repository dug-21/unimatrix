## ADR-006: resolve_supersessions in path Mode — Endpoint Resolution Before BFS, Per-Hop Reuses Existing Infrastructure

### Context

`resolve_supersessions: bool` (default false) is supported in neighbors and subgraph modes.
When true, deprecated entries encountered during traversal are substituted with their
terminal active successors via `follow_to_current` before being enqueued.

SCOPE.md OQ-01 resolved that path mode supports `resolve_supersessions` for endpoint
resolution before BFS begins — a caller passing a deprecated `from_id` almost certainly
wants the path from its active successor; `resolve_supersessions=false` is the explicit
audit mode.

SCOPE.md §path mode also states "in addition to per-hop intermediate resolution" — the
question (SR-05) is whether per-hop intermediate resolution in path mode is new
infrastructure or reuses what neighbors/subgraph already provide.

Inspection of the delivered `graph_read_subgraph.rs` (vnc-019) confirms that per-hop
`follow_to_current` calls already occur at lines 227-233 for every BFS neighbor. The
`graph_read_neighbors.rs` BFS path does the same at lines 302-309. The
`follow_to_current` helper is `pub(super)` since vnc-019 delivery (the vnc-019
architecture mandated this as the first delivery action).

SR-05 conclusion: per-hop intermediate resolution is NOT new — it is the existing
pattern copied from `graph_read_subgraph.rs`. Zero new infrastructure is required.

### Decision

path mode supports `resolve_supersessions: bool` (default false):

1. **Endpoint resolution** (new for path mode): Before BFS begins, call
   `follow_to_current(store, from_id)` and `follow_to_current(store, to_id)`. Use the
   resolved IDs for all subsequent BFS operations. If either resolves to `None`
   (orphaned deprecated terminal — 50-hop cap reached or no active successor), fall back
   to the original ID (same fallback as neighbors and subgraph — ADR-005 vnc-018 R-10).
   The resolved IDs are reflected in `PathResponse.from_id` and `PathResponse.to_id`.

2. **Per-hop intermediate resolution** (reused pattern): At each BFS hop, call
   `follow_to_current(store, neighbor_id)` for each discovered neighbor before the
   visited-set check. Same as `graph_read_subgraph.rs` lines 227-233 and
   `graph_read_neighbors.rs` lines 302-309. No new helper function.

When `resolve_supersessions=false` (default): deprecated endpoints and intermediate nodes
are used as-is. This is audit mode — the caller explicitly wants to find paths through
or to deprecated entries.

`follow_to_current` is imported from `graph_read_neighbors.rs` via
`super::graph_read_neighbors::follow_to_current` — the same import path used by
`graph_read_subgraph.rs`.

### Consequences

Easier: Consistent behavior across neighbors, subgraph, and path modes. Zero new
helper infrastructure. The 50-hop safety cap in `follow_to_current` applies universally.
Deprecated-endpoint callers get useful behavior by default (active successor path) while
audit mode (false default) is still reachable.

Harder: Per-hop `follow_to_current` calls are async — the BFS loop must be async,
consistent with subgraph mode. Each deprecated intermediate node adds one `Store::get`
call in the hot BFS path. At `max_depth=5` and the typical graph, this is bounded by
~50 resolver calls in the worst case — acceptable for a read-only path.
