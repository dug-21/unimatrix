## ADR-008: `node_index_for` Accessor on `TypedRelationGraph`

### Context

The depth>1 BFS traversal in `context_graph` neighbors mode is implemented in
`unimatrix-server/src/mcp/graph_read.rs`. BFS requires converting an entry ID (`u64`)
to a petgraph `NodeIndex` to call `edges_of_type(node_idx, rel_type, direction)` on
`TypedRelationGraph`.

`TypedRelationGraph.node_index` (the internal `HashMap<u64, NodeIndex>`) is
`pub(crate)` within `unimatrix-engine`. From `unimatrix-server`, this field is
invisible. Two resolution paths were evaluated:

**Option A — `pub fn node_index_for` accessor on `TypedRelationGraph`**
Add a small public method to `TypedRelationGraph` in `unimatrix-engine`:
```rust
pub fn node_index_for(&self, id: u64) -> Option<NodeIndex> {
    self.node_index.get(&id).copied()
}
```
Approximately 3–5 lines. BFS traversal logic stays in `unimatrix-server` (the
MCP-specific layer). The accessor exposes a reusable primitive that any future
graph consumer in any crate can use.

**Option B — BFS implemented inside `unimatrix-engine`**
Move the BFS traversal function into `unimatrix-engine` and expose it as a public
function that `graph_read.rs` calls. This eliminates the cross-crate visibility
issue entirely but pushes MCP-specific traversal semantics (depth capping, direction
filtering per edge type, `EdgeRecord` construction, `resolve_supersessions` logic)
into the engine layer. `unimatrix-engine` would then need to depend on or replicate
MCP-layer types. This inverts the layering: the engine should not contain
server-layer concerns.

### Decision

Add `pub fn node_index_for(&self, id: u64) -> Option<NodeIndex>` to
`TypedRelationGraph` in `unimatrix-engine/src/graph.rs`.

BFS traversal logic — depth capping, direction filtering, `EdgeRecord` construction,
`resolve_supersessions` substitution — stays entirely in `unimatrix-server/src/mcp/graph_read.rs`.

The accessor is a reusable, zero-logic primitive. It exposes the minimum necessary
surface: a single ID-to-index lookup with an `Option` return for missing entries.
It does not expose the internal `HashMap` or any mutable access.

### Consequences

Easier: BFS traversal is implemented once, in the correct layer (`unimatrix-server`),
with full access to MCP-specific types. Future graph consumers (e.g., a subgraph mode
handler or a path-finding mode) can use the same accessor without duplicating the
node-lookup pattern.

Harder: `TypedRelationGraph`'s public API grows by one method. This is a stable,
additive change — it does not break existing callers and is straightforward to
document.

The `unimatrix-engine` crate does not take on any `unimatrix-server` dependencies.
The layering constraint (engine has no knowledge of MCP concerns) is fully preserved.
