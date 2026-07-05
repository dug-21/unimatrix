# Component 2 — Graph projection (`mcp/graph_read_projection.rs`, NEW)

## Purpose

The graph-local lean shape (ADR-002 §3/§4). Defines `NodeSummary` (the `context_graph`
instance of ADR-001 §5's field set), the record → summary builders, and the
`GraphSummaryProjection` trait implemented for the five node-bearing envelopes. Keeps lean
serialization out of shared `EntryRecord`/`EdgeRecord` (C-2/C-3/SR-07) and out of the
already-over-limit `graph_read_subgraph.rs` (C-7/SR-08).

## Module wiring

- New file `crates/unimatrix-server/src/mcp/graph_read_projection.rs`.
- Declared as a `#[path]` child module of `graph_read` (mirrors `graph_read_subgraph` etc.).
  The `mod` line lives in `graph_read.rs` (component 3):
  ```
  #[path = "graph_read_projection.rs"]
  mod graph_read_projection;
  ```
  Being a child of `graph_read` lets the trait impls see the envelope types via `super::`.
- Imports:
  ```
  use serde::Serialize;
  use serde_json::{Value, json};
  use unimatrix_core::EntryRecord;
  use crate::mcp::response::status_str;                     // pub(crate), lifecycle string
  use crate::mcp::response::verbosity::content_preview;     // component 1
  use super::{ChainResult, CurrentResponse, EdgeRecord, FilterResponse,
              InverseResponse, SubgraphResponse};            // envelopes + EdgeRecord
  ```

## Types

```
#[derive(Serialize)]
struct NodeSummary {
    id: u64,
    title: String,
    category: String,
    tags: Vec<String>,
    status: &'static str,        // status_str(entry.status) — LIFECYCLE, NOT delivery status (SR-09)
    confidence: f64,
    content_preview: String,
    content_truncated: bool,
}
```

- `derive(Serialize)` ⇒ JSON key order follows declaration order (the exact 8-field set,
  AC-03). No `serde(skip)` / no extra fields — the type IS the field-set contract.
- The projection type is **distinct** from `EntryRecord`; `EntryRecord` gains no
  `skip_serializing_if` (C-3/SR-07).

## Functions

### `node_summary`

```
fn node_summary(entry: &EntryRecord) -> NodeSummary {
    let (preview, truncated) = content_preview(&entry.content);
    NodeSummary {
        id:               entry.id,
        title:            entry.title.clone(),
        category:         entry.category.clone(),
        tags:             entry.tags.clone(),          // tags already hydrated by fetch_nodes_batch (R-10)
        status:           status_str(entry.status),    // lifecycle enum → &'static str
        confidence:       entry.confidence,            // f64, unchanged (R-10)
        content_preview:  preview,
        content_truncated: truncated,
    }
}
```

### `edge_summary`

```
fn edge_summary(edge: &EdgeRecord) -> Value {
    json!({
        "source_id":     edge.source_id,
        "target_id":     edge.target_id,
        "relation_type": edge.relation_type,
        "depth":         edge.depth,
    })
    // direction + metadata DROPPED at projection time (FR-5/AC-03/R-07).
    // EdgeRecord itself is NOT mutated and keeps no skip_serializing_if (C-2/SR-07).
}
```

### Trait

```
trait GraphSummaryProjection {
    fn to_summary_json(&self) -> Value;
}
```

Implemented for exactly the five node-bearing envelopes. Each impl maps node bodies through
`node_summary` / edges through `edge_summary` and **preserves every envelope metadata field**
(R-03 — silent metadata loss is the primary bug surface). `NeighborsResponse` and
`PathResponse` do NOT implement it (no node bodies).

```
impl GraphSummaryProjection for SubgraphResponse {
    fn to_summary_json(&self) -> Value {
        json!({
            "nodes":         self.nodes.iter().map(node_summary).collect::<Vec<_>>(),
            "edges":         self.edges.iter().map(edge_summary).collect::<Vec<_>>(),
            "truncated":     self.truncated,        // bool — PRESERVE
            "seed_ids":      self.seed_ids,         // Vec<u64> — PRESERVE
            "depth_reached": self.depth_reached,    // u8 — PRESERVE
        })
    }
}

impl GraphSummaryProjection for ChainResult {
    fn to_summary_json(&self) -> Value {
        json!({
            "entries":   self.entries.iter().map(node_summary).collect::<Vec<_>>(),
            "truncated": self.truncated,            // Truncated {forward,backward} — PRESERVE whole struct
        })
    }
}

impl GraphSummaryProjection for CurrentResponse {
    fn to_summary_json(&self) -> Value {
        json!({
            "entry": node_summary(&self.entry),     // SINGLE node object, NOT an array (R-03)
        })
    }
}

impl GraphSummaryProjection for InverseResponse {
    fn to_summary_json(&self) -> Value {
        json!({
            "entries":        self.entries.iter().map(node_summary).collect::<Vec<_>>(),
            "total_returned": self.total_returned,  // usize — PRESERVE
        })
    }
}

impl GraphSummaryProjection for FilterResponse {
    fn to_summary_json(&self) -> Value {
        json!({
            "entries":        self.entries.iter().map(node_summary).collect::<Vec<_>>(),
            "total_returned": self.total_returned,  // usize — PRESERVE
        })
    }
}
```

Note on key order: the summary output is a `serde_json::Value` built via `json!`; its top-level
key order is not contractual (AC-03 asserts the *key set* — present AND absent — not byte
order). Only `detail=full` requires byte-for-byte stability, and that path never touches this
module (it serializes the original struct directly — see graph_read.md). Each `NodeSummary`
object's internal key order is fixed by the struct (declaration order).

## Data flow

- Input: a typed envelope (`&self`) already produced by the unchanged mode handler.
- Output: a `serde_json::Value` with node bodies replaced by the 8-field summary, edges by the
  4-field summary, and all envelope metadata carried through verbatim.
- `content` is read (for the preview) but not emitted; hashes, timestamps, `embedding_dim`,
  `created_by`/`modified_by`, counts, version are all absent from the projection (R-07).

## Error handling

- Pure, total transforms — no fallible calls. `content_preview` never panics (component 1).
  Serialization failure is handled by the caller at the `serde_json::to_string` seam
  (`ERROR_INTERNAL`, existing path).

## Key test scenarios (hints; full plan in test-plan/graph_read_projection.md)

- Node summary key set == exactly `{id,title,category,tags,status,confidence,content_preview,
  content_truncated}` — assert present AND absent keys (`content`, `content_hash`,
  `previous_hash`, `embedding_dim`, timestamps, `created_by`/`modified_by`, counts all absent)
  (R-07/AC-03).
- Edge summary key set == exactly `{source_id,target_id,relation_type,depth}` — `direction` and
  `metadata` absent (R-07/AC-03).
- `status` value is `status_str(entry.status)` (`active|deprecated|proposed|quarantined`), not
  delivery status (R-07/R-11).
- Per-envelope metadata preservation under summary (R-03): subgraph keeps
  `truncated`/`seed_ids`/`depth_reached`; chain keeps `truncated:{forward,backward}`;
  inverse/filter keep `total_returned`; current returns a single `entry` object (not an array).
- Empty content → `content_preview:""`, `content_truncated:false` in the projected node (R-10).
- Multi-tag node → `tags` array fully preserved; `confidence` serialized as a JSON number
  (R-10).

## Constraints honored

- C-2/C-3/SR-07: distinct projection type + `serde_json::Value` edge builder; no
  `skip_serializing_if` on `EntryRecord`/`EdgeRecord`.
- C-7/SR-08: projection isolated in this new module, not in `graph_read_subgraph.rs`.
- C-8/Pattern #4500: all five node-bearing arms projected consistently through one trait.
- SR-09: `status` documented in-type as lifecycle, not delivery status.
