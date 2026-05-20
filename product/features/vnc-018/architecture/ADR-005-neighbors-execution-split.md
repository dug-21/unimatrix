## ADR-005: neighbors Mode Execution Split — SQL at depth=1, In-Memory BFS at depth>1

### Context

`neighbors` mode must return entries connected by typed edges. Two execution paths
are available:

**SQL path**: queries `GRAPH_EDGES` directly. Reflects all committed writes
immediately. Requires one SQL round-trip per hop at depth>1 (N hops = N queries),
which is acceptable at small depth but multiplies DB load at depth=10 with wide
graphs.

**In-memory BFS path**: operates on `TypedRelationGraph`, the tick-rebuilt petgraph
instance held in `Arc<RwLock<TypedGraphState>>`. Zero SQL round-trips for the BFS
itself. The staleness window is at most one tick interval (typically 30–60 seconds) —
edges written since the last tick rebuild are not visible.

SCOPE.md OQ-01 resolved this: SQL at depth=1, in-memory at depth>1. The rationale
is that the asymmetry goes in the "expected direction": depth=1 is always at least as
fresh as depth>1. An agent that writes an edge and immediately queries depth=1 sees
the new edge. An agent that queries depth=2 immediately after a write may not — and
this behavior is documented, not surprising.

The alternative (in-memory for all depths) would make depth=1 queries stale within
the tick window — exactly the case where agents most expect freshness (point lookup
after a write). The reverse (SQL for all depths) eliminates the tick-window issue but
adds N SQL round-trips at depth>1, and the composite indexes only help with the WHERE
clause, not with the join fan-out at each hop.

### Decision

- `depth=1`: SQL query on `GRAPH_EDGES` using composite indexes
  `idx_graph_edges_source_type` (outgoing) and `idx_graph_edges_target_type`
  (incoming). Live database, reflects all committed writes immediately.

- `depth>1`: BFS over `TypedRelationGraph`. Lock acquired once at the start of BFS
  (`Arc<RwLock<TypedGraphState>>::read()`). Released after BFS completes.
  Tick-window staleness applies: edges written within the last tick interval may not
  be visible. This is a documented behavioral constraint, not a defect.

The handoff boundary is `params.depth == 1`. The implementation dispatches to either
`handle_neighbors_sql` or `handle_neighbors_bfs` at the start of `handle_neighbors`.

**Tool description text** (exact, mandatory in `#[tool(description = "...")]`):

> "depth=1 queries the live database and reflects all committed writes immediately.
> depth>1 queries the in-memory graph cache, which may lag recent writes by up to
> one tick interval (typically 30–60 seconds). This asymmetry is intentional:
> depth=1 is the precise lookup case where freshness matters; depth>1 is exploratory
> multi-hop traversal where a tick-window lag is acceptable."

**Staleness test requirement** (SR-02): the infra-001 integration suite must include
a test that writes an edge and immediately queries `depth=2`. The test asserts that
the new edge does NOT appear (expected staleness behavior), confirming the constraint
is tested and not accidentally fixed into a false "always fresh" behavior.

`follow_to_current` helper for `resolve_supersessions=true` at depth>1:

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
    None  // chain too long — caller treats as: no substitution, use original id
}
```

This is a `Store`-layer helper using `read_pool()` (C-07). It does NOT use the
in-memory graph for supersession resolution — consistent with ADR-001.

### Consequences

Easier: depth=1 queries are always fresh, matching agent expectations for point
lookups after writes. depth>1 BFS avoids N SQL round-trips per hop, keeping
multi-hop queries responsive. The asymmetry is explicitly documented in the tool
description, preventing agents from building incorrect freshness models (Pattern
#4474).

Harder: two code paths for what agents perceive as "the same operation at different
depths." The behavioral split must be tested explicitly (SR-02 staleness test). Any
future change that makes depth>1 use SQL must be treated as a behavioral change (not
a refactor) and requires a new ADR.
