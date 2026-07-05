# vnc-044 Pseudocode — OVERVIEW

`context_graph` two-axis output: split `format` (serialization) from a new `detail`
(verbosity) axis, add a lean node/edge summary projection. Projection + parameter-
resolution change only — traversal, BFS, DB reads unchanged (ADR-002 §6).

Read ADR-001 (#5509 suite contract) and ADR-002 (#5510 graph adoption) first; this file
wires them to the four component pseudocode files.

## Components (build order)

| # | Component | File | New/Mod | Depends on |
|---|-----------|------|---------|-----------|
| 1 | Shared verbosity primitives | `mcp/response/verbosity.rs` | new | — |
| 2 | Graph projection | `mcp/graph_read_projection.rs` | new | 1 |
| 3 | Output resolver + seam threading | `mcp/graph_read.rs` | modify | 1, 2 |
| 4 | Tool description | `mcp/tools.rs` (twin literals) | modify | 3 |

Build 1 → 2 → 3 → 4. 1 has no deps; 2 imports 1 (`content_preview`) and the envelope
types from 3's file (`super::…`); 3 imports both 1 and 2; 4 is prose only.

Also touched (documentation only, no logic):
- `mcp/graph_read_validation.rs` — a comment noting `detail` is a **universal** field with
  **no** per-mode rejection arm (covered in graph_read.md §Validation).

Explicitly **UNTOUCHED** (constraints C-1..C-4, C-7):
- `unimatrix-store/src/schema.rs` — `EntryRecord` / `EdgeRecord` / `Status`, no
  `skip_serializing_if`.
- `mcp/response/mod.rs` shared `ResponseFormat` / `parse_format` — graph never calls them;
  `status_str` reused read-only.
- `mcp/graph_read_subgraph.rs` (742 lines, pre-existing over-limit debt) — `fetch_nodes_batch`
  still reads `content`; nothing added here.

## Data flow (the serialization seam)

```
context_graph(params)                              [tools.rs — cap check, ctx build]
  └─ handle_graph(store, graph_state, params, _ctx) [graph_read.rs]
       1. let (detail, _serialization) = resolve_graph_output(&params)?   ← NEW; rejects
             markdown / legacy-conflict / bad values for ALL SEVEN modes, PRE-dispatch
       2. validate_no_unsupported_params(&params)?                        [unchanged; detail universal]
       3. mode dispatch → traversal → typed envelope                      [unchanged]
       4. serialize at the seam:
            node-bearing (subgraph/chain/current/inverse/filter):
              Detail::Full    → serde_json::to_string(&result)            ← byte-identical to today
              Detail::Summary → serde_json::to_string(&result.to_summary_json())
            edge-only (neighbors/path):
              always serde_json::to_string(&result)                       ← detail accepted, ignored
```

The `graph_read.rs:251` parse-and-drop (`_ctx` bound, `format` discarded) is fixed: the
resolved `(Detail, GraphSerialization)` now threads into every arm.

## Shared types & signatures (single source; used across component files)

Defined in `mcp/response/verbosity.rs` (component 1), imported everywhere else — never
re-declared, never re-literalled (SR-03 / C-9):

```rust
pub const CONTENT_PREVIEW_BYTES: usize = 256;                 // the ONLY 256 in the graph path
pub enum Detail { Summary, Full }
pub fn parse_detail(detail: &Option<String>) -> Result<Detail, ServerError>;
pub fn content_preview(content: &str) -> (String, bool);      // (preview, truncated)
```

Defined in `mcp/graph_read_projection.rs` (component 2), graph-local:

```rust
#[derive(Serialize)]
struct NodeSummary {                                          // field order == wire order
    id: u64, title: String, category: String, tags: Vec<String>,
    status: &'static str,          // status_str(entry.status) — LIFECYCLE, not delivery status
    confidence: f64, content_preview: String, content_truncated: bool,
}
fn node_summary(entry: &EntryRecord) -> NodeSummary;
fn edge_summary(edge: &EdgeRecord) -> serde_json::Value;      // {source_id,target_id,relation_type,depth}
trait GraphSummaryProjection { fn to_summary_json(&self) -> serde_json::Value; }
// impl for SubgraphResponse, ChainResult, CurrentResponse, InverseResponse, FilterResponse
```

Defined in `mcp/graph_read.rs` (component 3):

```rust
enum GraphSerialization { Json }   // markdown rejected before this value is produced
fn resolve_graph_output(params: &GraphParams) -> Result<(Detail, GraphSerialization), ErrorData>;
struct GraphParams { /* existing fields unmoved */ detail: Option<String> }  // additive
```

## Reused, unchanged surface

| Symbol | Path | Use |
|--------|------|-----|
| `EntryRecord` | `unimatrix_core::EntryRecord` | source of `node_summary`; has `content: String`, `confidence: f64`, `tags: Vec<String>`, `status: Status`, `title`, `category`, `id: u64` |
| `EdgeRecord` | `graph_read.rs:143` | `{source_id,target_id,relation_type,direction,depth,metadata}` — projection reads 4, drops `direction`/`metadata` |
| `status_str` | `crate::mcp::response::status_str` (pub(crate)) | `Status -> &'static str` lifecycle string |
| `ServerError::InvalidInput{field,reason}` | `crate::error` | `→ ErrorData(ERROR_INVALID_PARAMS)` via existing `From` impl (error.rs:251) |
| `ERROR_INVALID_PARAMS`, `ERROR_INTERNAL` | `crate::error` | error codes |
| envelopes | `graph_read.rs` | `SubgraphResponse`, `ChainResult`, `CurrentResponse`, `InverseResponse`, `FilterResponse`, `NeighborsResponse`, `PathResponse` — unchanged; 5 gain the trait |

## Sequencing constraints

- Component 1 has zero internal deps — build & unit-test the boundary table first (R-01/R-02).
- Component 2's trait impls reference envelope types via `super::` (it is a `#[path]` child
  module of `graph_read`), so its module declaration lives in component 3's file.
- Component 3 wires 1 + 2 and is where line-budget is watched (389 → ~460; if it crosses 500,
  relocate `resolve_graph_output` to `graph_read_validation.rs` per C-7 / ADR-002 OQ-1).
- Component 4 (twin `&str` literals) is edited last so its prose matches shipped behavior; a
  byte-equality guard test (#869) links the two literals.

## Standing limitation carried into copy (SR-09 / R-11)

Projected `status` is **lifecycle** (`active/deprecated/proposed/quarantined`), **not**
capability **delivery** status (`missing/partial/proven/claimed`, which lives in the entry
`content` blob). A capability subgraph returns `status:"active"` for every node. This feature
delivers the payload-size + structure win only; the #913 delivery-status tally is NOT
delivered (named follow-up #3). Component 4's description states this plainly; testers must
NOT treat delivery-status absence as a defect.
