## ADR-002: `handle_subgraph` Lives in `graph_read_subgraph.rs`, Not Inline

### Context

SR-04 from the scope risk assessment flags that `graph_read.rs` is approaching the
500-line file limit and the file-split decision must be made before delivery begins,
not when the limit is hit mid-delivery.

Current line counts (post-vnc-018 delivery, as measured on `feature/vnc-018`):
- `graph_read.rs`: 306 lines (wire types, entry point, validation, dispatch)
- `graph_read_neighbors.rs`: 356 lines (neighbors BFS and SQL logic)
- `graph_read_supersession.rs`: 448 lines (chain/current CTE logic)

The subgraph BFS implementation includes:
- BFS frontier loop (multi-type, multi-direction, resolve_supersessions path): ~80 lines
- Parameter validation: ~40 lines
- Post-BFS batch node hydration: ~20 lines
- Post-BFS metadata batch query + join: ~40 lines
- `SubgraphResponse` construction and error handling: ~20 lines
- Tests: separate `graph_read_subgraph_tests.rs` file

Total estimated implementation: ~200 lines. Adding 200 lines to `graph_read.rs`'s 306
would produce ~506 lines — over the limit.

Two options:
(A) Inline `handle_subgraph` in `graph_read.rs`. Exceeds 500-line limit immediately.
    Requires splitting it back out mid-delivery or leaving a rule violation.

(B) New `graph_read_subgraph.rs` file declared via `#[path]` in `graph_read.rs`, the
    same pattern established by `graph_read_neighbors.rs` and `graph_read_supersession.rs`.
    Consistent with the existing module organization.

Option B is the correct choice. The pattern is already established. There is no
coordination overhead — the sibling file is straightforward to create, and the dispatch
arm in `handle_graph` is a two-line addition.

`SubgraphResponse` is defined in `graph_read.rs` (not in the subgraph submodule)
alongside the other response envelopes (`ChainResult`, `CurrentResponse`,
`NeighborsResponse`). This keeps all wire types co-located, consistent with ADR-004
vnc-018.

### Decision

Create `crates/unimatrix-server/src/mcp/graph_read_subgraph.rs` as a `#[path]`-declared
submodule of `graph_read.rs`:

```rust
// In graph_read.rs:
#[path = "graph_read_subgraph.rs"]
mod graph_read_subgraph;
```

`graph_read_subgraph.rs` contains:
- `handle_subgraph(store, typed_graph_state, params) -> Result<SubgraphResponse, ErrorData>`
- BFS loop, resolve_supersessions logic, edge deduplication, truncation enforcement
- Post-BFS node batch hydration
- Post-BFS metadata batch query

Tests go in `graph_read_subgraph_tests.rs` declared via `#[path]` inside
`graph_read_subgraph.rs`:
```rust
#[cfg(test)]
#[path = "graph_read_subgraph_tests.rs"]
mod tests;
```

`SubgraphResponse` is defined in `graph_read.rs` adjacent to `ChainResult`,
`CurrentResponse`, and `NeighborsResponse`.

`handle_graph` dispatch arm:
```rust
"subgraph" => {
    let result =
        graph_read_subgraph::handle_subgraph(store, typed_graph_state, &params).await?;
    let json = serde_json::to_string(&result).map_err(|e| {
        ErrorData::new(ERROR_INTERNAL, format!("serialization error: {e}"), None)
    })?;
    Ok(CallToolResult::success(vec![rmcp::model::Content::text(json)]))
}
```

### Consequences

Easier: `graph_read.rs` stays within the 500-line limit. File organization is consistent
with the existing vnc-018 pattern. No delivery-time judgment call about when to split.

Harder: One additional file in the module tree. `SubgraphResponse` lives in `graph_read.rs`
while `handle_subgraph` lives in `graph_read_subgraph.rs` — the type and its primary
producer are in sibling files, but this matches the `EdgeRecord` / neighbors pattern.
