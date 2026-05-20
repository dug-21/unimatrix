## ADR-003: Post-BFS Metadata Batch Query from `GRAPH_EDGES`

### Context

`EdgeRecord.metadata` was defined in vnc-018 as `Option<serde_json::Value>`, always
serialized as JSON `null`. ADR-004 vnc-018 explicitly deferred population to subgraph
mode (#597). The `GRAPH_EDGES.metadata` column is `TEXT`, nullable.

`RelationEdge` (the engine's in-memory edge type, populated at tick-rebuild time)
does NOT carry the `metadata` field. This is a deliberate design choice: adding
`metadata` to `RelationEdge` would require reading the `metadata` column in
`query_graph_edges` and propagating it through `build_typed_relation_graph`, modifying
a shared hot-path struct used by PPR, graph_expand, graph_ppr, and search. The
Non-Goals section of SCOPE.md explicitly excludes this engine change.

Three approaches to populate `EdgeRecord.metadata` during subgraph BFS:

(A) Per-hop SQL: for each edge discovered during BFS, issue a SQL query for its
    metadata. O(edges) round-trips during the BFS inner loop. At depth 3, `both`
    direction, 200-node cap, this can be ~600 queries within a single tool call.
    Unacceptable latency.

(B) Add `metadata` to `RelationEdge` in `unimatrix-engine`. Requires modifying the
    engine struct and its construction path — explicitly excluded as Non-Goal in
    SCOPE.md. Pollutes a shared hot-path type with a presentation-layer field.

(C) Post-BFS batch query: collect all `(source_id, target_id, relation_type)` triples
    during BFS, then issue a single SQL query against `GRAPH_EDGES` for the full set
    after BFS completes. O(1) round-trips regardless of edge count.

Option C is the correct approach. It is also already a named pattern in Unimatrix
(entry #4486).

### Decision

After BFS completes and the edge list is finalized, issue a single batch query to
`GRAPH_EDGES` for all collected `(source_id, target_id, relation_type)` triples.
Build a `HashMap<(u64, u64, String), Option<serde_json::Value>>` from the results.
Populate `EdgeRecord.metadata` from this map before constructing the response.

SQLite does not support tuple-IN syntax. Use a dynamically built OR-chain:

```sql
SELECT source_id, target_id, relation_type, metadata
FROM graph_edges
WHERE (source_id = ?1 AND target_id = ?2 AND relation_type = ?3)
   OR (source_id = ?4 AND target_id = ?5 AND relation_type = ?6)
   -- repeated for each collected edge
```

Bind parameters are generated in a loop at construction time. The query is issued via
`store.read_pool_server()` using `sqlx::query`.

Edge count bound: the `max_nodes = 200` hard cap and depth-3 default bound the edge
count in practice. At 200 nodes with average degree 3 and `both` direction, the upper
bound is ~600 edges. The OR-chain SQL at this scale is well within SQLite's query
complexity limits. This bound is documented here to prevent future callers from
removing the `max_nodes` cap without reconsidering this query.

When `collected_edges` is empty (no edges were discovered), skip the batch query and
return all `EdgeRecord.metadata` as `None`.

The composite indexes from schema v27 (`idx_graph_edges_source_type` on
`(source_id, relation_type)`, `idx_graph_edges_target_type` on
`(target_id, relation_type)`) make individual `(source_id, target_id, relation_type)`
point lookups efficient. The OR-chain will use these indexes per clause.

### Consequences

Easier: One SQL round-trip regardless of graph size (within the cap). Engine types are
not modified. The batch query is simple to reason about — all I/O happens after the
BFS loop completes, not interleaved with it.

Harder: The OR-chain SQL must be dynamically constructed at runtime. The empty-edges
case must be handled explicitly to avoid an empty WHERE clause (which would be a syntax
error or would return all rows). Metadata for edges that exist in the BFS result but
not in `GRAPH_EDGES` (e.g., edges added to the in-memory graph but not yet flushed)
will have `metadata = None` — acceptable given the tick-window staleness contract.
