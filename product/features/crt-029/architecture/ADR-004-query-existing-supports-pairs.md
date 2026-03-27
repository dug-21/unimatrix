## ADR-004: Separate `query_existing_supports_pairs()` Store Helper for Pre-Filter

### Context

The tick needs a `HashSet<(u64, u64)>` of already-written Supports pairs to skip before
NLI scoring (per-pair pre-filter, AC-06b). Two approaches are available:

1. **Reuse `Store::query_graph_edges()`** — returns `Vec<GraphEdgeRow>` with all edge types
   and all non-metadata columns. The tick would filter in Rust to `relation_type == "Supports"
   && !bootstrap_only`. This loads all edge types into memory, including Supersedes,
   Contradicts, CoAccess rows that are not needed.

2. **New `Store::query_existing_supports_pairs()`** — targeted SQL returning only
   `(source_id, target_id)` for non-bootstrap Supports edges:

   ```sql
   SELECT source_id, target_id
   FROM graph_edges
   WHERE relation_type = 'Supports'
     AND bootstrap_only = 0
   ```

   Returns `HashSet<(u64, u64)>` directly.

SR-04 notes: "at large graph sizes [query_graph_edges] is a full table scan loaded into
memory each tick." A targeted query avoids loading irrelevant edge types, and returns only
the two columns needed for HashSet construction rather than all eight columns.

The GRAPH_EDGES `UNIQUE(source_id, target_id, relation_type)` index is a covering index for
the `relation_type` filter (SQLite will use it). The query stays bounded and indexed.

### Decision

Add `Store::query_existing_supports_pairs()` to `unimatrix-store/src/read.rs`:

```rust
pub async fn query_existing_supports_pairs(&self) -> Result<HashSet<(u64, u64)>> {
    let rows = sqlx::query(
        "SELECT source_id, target_id \
         FROM graph_edges \
         WHERE relation_type = 'Supports' AND bootstrap_only = 0",
    )
    .fetch_all(self.read_pool())
    .await
    ...
    // map rows to HashSet<(u64, u64)>
}
```

Uses `read_pool()` — this is a read-only query with no write-path involvement. Returns a
`HashSet` directly rather than a `Vec` to avoid a second allocation in the caller.

If the implementation agent judges that the graph is small enough that a Rust-side filter
over `query_graph_edges()` is simpler and no performance concern exists, that is acceptable.
The interface to `run_graph_inference_tick` remains `HashSet<(u64, u64)>` regardless of how
it is built.

### Consequences

The new method is a clean, single-purpose store query that follows the col-029 pattern
(ADR-004 in that feature: bounded SQL, no Cartesian products).

`query_graph_edges()` is not modified; `TypedGraphState::rebuild` continues to use it for
the full edge set.

The implementation agent must add tests for `query_existing_supports_pairs()` covering:
empty table, only bootstrap rows (should return empty set), mixed bootstrap and non-bootstrap
Supports rows.
