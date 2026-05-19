## ADR-004: EdgeRecord Type Location — graph_read.rs with Re-Export

### Context

`EdgeRecord` is the per-hop result type for `neighbors` mode. It will also be used
by `subgraph` mode (#597) which returns a `(Vec<EntryRecord>, Vec<EdgeRecord>)` pair.
The question is where to define the canonical type:

**Option A — `mcp/graph_read.rs`**
Defined in the new module as `pub(crate)`, re-exported from `mcp/mod.rs`. #597
imports from `mcp::graph_read::EdgeRecord` (or via `mcp::EdgeRecord`).

**Option B — `mcp/types.rs` (new shared types module)**
A new module for types shared across MCP tool handlers. Cleaner conceptually but
adds a module that currently has only one type.

**Option C — `unimatrix-engine` or `unimatrix-core`**
Defined in the engine or core crate for maximum reuse. But `EdgeRecord` contains
MCP-layer fields (`direction: String` relative to traversal anchor, `depth: u8`,
`metadata: Option<serde_json::Value>`) that are not engine concerns — they are
presentation layer annotations added for tool consumers. Placing it in core or engine
would pollute those crates with MCP wire-protocol fields.

### Decision

`EdgeRecord` is defined in `mcp/graph_read.rs` and re-exported from `mcp/mod.rs`.
`#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]` enables serialization
and schema generation.

```rust
/// A single edge result from neighbors or subgraph mode traversal.
#[derive(Debug, Serialize, Deserialize, JsonSchema, Clone)]
pub struct EdgeRecord {
    /// Source entry ID (graph-level; not relative to traversal anchor).
    pub source_id: u64,
    /// Target entry ID (graph-level; not relative to traversal anchor).
    pub target_id: u64,
    /// Edge type string (e.g., "Supports", "Prerequisite").
    pub relation_type: String,
    /// Direction relative to the traversal anchor: "incoming" | "outgoing".
    pub direction: String,
    /// Hop depth from the seed entry (1-based).
    pub depth: u8,
    /// Edge metadata. Always None in vnc-018. Populated by W1B-2b (#597)
    /// when RelationEdge gains a metadata field.
    pub metadata: Option<serde_json::Value>,
}
```

`mcp/mod.rs` re-export:
```rust
pub use graph_read::EdgeRecord;
```

#597 imports `crate::mcp::EdgeRecord` directly. No type change needed when #597
ships — it uses the type as-is, just adds population of `metadata` from the extended
`RelationEdge`.

**Why not `mcp/types.rs`**: a single-type module is premature abstraction. If a
second MCP-layer shared type emerges in #597 or #598, creating `mcp/types.rs` at
that point and moving `EdgeRecord` is a one-line change with no wire impact.

**`metadata` field rationale**: defined now as `Option<serde_json::Value>` so that
when W1B-2b extends `RelationEdge` with edge metadata (strength, contribution_kind,
etc.), the `EdgeRecord` wire format does not change — callers already handle a
nullable `metadata` field. In vnc-018, the field is always serialized as `null`.
Removing a field later would be a breaking change; adding it now is not.

### Consequences

Easier: #597 has a clear import path (`crate::mcp::EdgeRecord`). No new module to
create. The type is co-located with its primary producer (`handle_neighbors`).

Harder: `graph_read.rs` contains both logic and a shared wire type. If graph_read.rs
grows toward the 500-line limit, the type definition contributes ~15 lines that could
have been in a separate types module. This is acceptable given the single-type
situation at vnc-018 delivery time.
