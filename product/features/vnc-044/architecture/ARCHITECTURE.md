# vnc-044 Architecture — `context_graph` Two-Axis Output (serialization + verbosity)

**Feature:** vnc-044 | **Author:** vnc-044-agent-1-architect
**ADRs:** ADR-001 (suite contract, #5509) · ADR-002 (graph adoption, #5510)

This document is the `context_graph`-only implementation of the suite-wide contract in
ADR-001. Read ADR-001 first for the contract; ADR-002 for the graph-specific decisions. This
file wires them to concrete code seams.

---

## System Overview

`context_graph` is one of the shared context tools exposed by `unimatrix-server`'s MCP layer.
Its `format` parameter is a category error: it fuses **serialization** (`markdown|json`) with
**verbosity** (`summary` vs full). vnc-044 splits these into two axes and, per ADR-001, makes
lean (`detail=summary`) the default. The suite-wide contract is the load-bearing artifact
(ADR-001); vnc-044 is its **first and only** adopter — every other context tool keeps its
current `format` handling until a later migration feature.

The change is a **projection + parameter-resolution** change. Traversal (BFS, `max_depth`,
`max_nodes`, edge filtering, supersession resolution) and the DB read path are **untouched**.
The only new behavior is: resolve the two axes at the top of `handle_graph`, and at
serialization emit either the full record (today's output) or a lean projection.

```
MCP call
  → tools.rs (cap check, ctx build)                       [unchanged]
  → handle_graph(store, graph_state, params, _ctx)        [graph_read.rs:247]
      → resolve_graph_output(&params) -> (Detail, Json)   [NEW — rejects markdown/legacy conflict]
      → validate_no_unsupported_params(&params)           [unchanged logic; detail is universal]
      → mode dispatch → traversal → typed envelope        [unchanged]
      → serialize:
          Detail::Full    → serde_json::to_string(&result)              [byte-identical to today]
          Detail::Summary → serde_json::to_string(&result.to_summary_json())
```

---

## Component Breakdown

| Component | File | Change | Responsibility |
|-----------|------|--------|----------------|
| Shared verbosity primitives | `mcp/response/verbosity.rs` **(new)** | new | `Detail` enum, `parse_detail`, `CONTENT_PREVIEW_BYTES=256`, `content_preview()`. Single source for ADR-001's shared constants (SR-03). |
| Graph projection | `mcp/graph_read_projection.rs` **(new)** | new | `NodeSummary` type, `node_summary()`, `GraphSummaryProjection` trait + impls for the 5 node-bearing envelopes. Keeps the lean shape out of shared `EntryRecord` (SR-07) and out of the over-limit subgraph file (SR-08). |
| Output resolver | `mcp/graph_read.rs` | edit | `resolve_graph_output(&GraphParams) -> Result<(Detail, GraphSerialization), ErrorData>`; thread `(detail, serialization)` into each mode arm (fix the `:251` parse-and-drop). |
| Params | `mcp/graph_read.rs:72` | edit | add `detail: Option<String>` to `GraphParams` (additive Option<T>, ADR-003-safe). |
| Cross-mode validation | `mcp/graph_read_validation.rs` | edit (doc/comment only) | `detail` is a **universal** field — **no** per-mode rejection arm added. |
| Tool description | `mcp/tools.rs` (graph tool def) | edit | document both axes, `summary` default, `format=markdown` rejection, and the lifecycle-vs-delivery status caveat (SR-09). |
| Store types | `unimatrix-store/src/schema.rs`, `mcp/graph_read.rs` (`EntryRecord`, `EdgeRecord`) | **UNTOUCHED** | shared/wire-locked — no `skip_serializing_if` (SR-06/SR-07). |
| Shared response layer | `mcp/response/mod.rs` (`ResponseFormat`, `parse_format`) | **UNTOUCHED** | suite-shared; behavior unchanged for non-graph callers (SR-06). `status_str()` is reused read-only. |
| DB fetch | `mcp/graph_read_subgraph.rs:639` (`fetch_nodes_batch`) | **UNTOUCHED** | still selects `content`; preview computed from it (SR-01). |

---

## Component Interactions

### 1. Axis resolution (`resolve_graph_output`, new, in `graph_read.rs`)

Runs at the **top of `handle_graph`, before mode dispatch**, so rejections are uniform across
all seven modes. `context_graph` does **not** call the shared `parse_format` (it would accept
`summary` as a `ResponseFormat` and silently no-op `markdown`).

```
enum GraphSerialization { Json }   // markdown is rejected before this value is produced

fn resolve_graph_output(params: &GraphParams) -> Result<(Detail, GraphSerialization), ErrorData> {
    // 1. Legacy alias: format == "summary"
    //      detail absent  -> (Detail::Summary, Json)
    //      detail present -> ERROR_INVALID_PARAMS (conflict; do not combine)
    // 2. Serialization from remaining format:
    //      None | "json" -> Json
    //      "markdown"    -> ERROR_INVALID_PARAMS
    //                       "format=markdown is not supported for context_graph — no
    //                        graph-markdown renderer exists yet; use format=json"
    //      else          -> ERROR_INVALID_PARAMS
    // 3. Verbosity via parse_detail(params.detail):
    //      None -> Summary (default), "summary" -> Summary, "full" -> Full, else -> ERROR
}
```

### 2. Mode dispatch → serialization seam

Each **node-bearing** arm (subgraph, chain, current, inverse, filter):

```
let json = match detail {
    Detail::Full    => serde_json::to_string(&result)?,                   // today's output (AC-04)
    Detail::Summary => serde_json::to_string(&result.to_summary_json())?, // lean projection
};
```

Each **accept-and-ignore** arm (neighbors, path): `serde_json::to_string(&result)?` — `detail`
is accepted but changes nothing (no node bodies). `markdown` was already rejected in step 1
(AC-08).

### 3. Projection (`graph_read_projection.rs`, new)

```
#[derive(Serialize)]
struct NodeSummary {
    id: u64, title: String, category: String, tags: Vec<String>,
    status: &'static str,      // status_str(entry.status) — LIFECYCLE, not delivery status
    confidence: f64,
    content_preview: String, content_truncated: bool,
}
fn node_summary(entry: &EntryRecord) -> NodeSummary   // calls content_preview(&entry.content)

trait GraphSummaryProjection { fn to_summary_json(&self) -> serde_json::Value; }
// impl for SubgraphResponse, ChainResult, CurrentResponse, InverseResponse, FilterResponse.
// Each maps node bodies -> node_summary(...), projects edges to {source_id,target_id,
// relation_type,depth}, and PRESERVES envelope metadata (truncated, seed_ids, depth_reached,
// total_returned, ...).
```

### 4. Preview construction (`response/verbosity.rs`, new — shared)

```
pub const CONTENT_PREVIEW_BYTES: usize = 256;
pub fn content_preview(content: &str) -> (String, bool) {
    if content.len() <= CONTENT_PREVIEW_BYTES { return (content.to_string(), false); }
    let mut end = CONTENT_PREVIEW_BYTES;
    while end > 0 && !content.is_char_boundary(end) { end -= 1; }  // codebase idiom, not nightly
    (content[..end].to_string(), true)
}
```

`content_truncated == content.len() > 256` (byte compare) — independent of where the char
boundary floored. Exactly-256-byte content → `(whole, false)`; empty → `("", false)`. No
ellipsis appended.

---

## Integration Surface

| Integration Point | Type / Signature | Source | Change |
|-------------------|------------------|--------|--------|
| `GraphParams.format` | `Option<String>` | `graph_read.rs:78` | re-scoped to serialization-only |
| `GraphParams.detail` | `Option<String>` **(new)** | `graph_read.rs` (additive) | new verbosity axis |
| `handle_graph` | `async fn(&Store, &Arc<RwLock<TypedGraphState>>, GraphParams, &ToolContext) -> Result<CallToolResult, ErrorData>` | `graph_read.rs:247` | read `params.format`/`params.detail`; thread resolved pair into arms |
| `resolve_graph_output` | `fn(&GraphParams) -> Result<(Detail, GraphSerialization), ErrorData>` **(new)** | `graph_read.rs` | axis resolution + loud rejects |
| `Detail` | `enum { Summary, Full }` **(new)** | `response/verbosity.rs` | shared verbosity enum (SR-03 single source) |
| `parse_detail` | `fn(&Option<String>) -> Result<Detail, ServerError>` **(new)** | `response/verbosity.rs` | parse the verbosity axis |
| `CONTENT_PREVIEW_BYTES` | `const usize = 256` **(new)** | `response/verbosity.rs` | shared preview cap (single source) |
| `content_preview` | `fn(&str) -> (String, bool)` **(new)** | `response/verbosity.rs` | UTF-8-floored preview + truncated flag |
| `NodeSummary` | `struct { id:u64, title:String, category:String, tags:Vec<String>, status:&'static str, confidence:f64, content_preview:String, content_truncated:bool }` **(new)** | `graph_read_projection.rs` | graph lean node shape |
| `node_summary` | `fn(&EntryRecord) -> NodeSummary` **(new)** | `graph_read_projection.rs` | build projection from a record |
| `GraphSummaryProjection` | `trait { fn to_summary_json(&self) -> serde_json::Value }` **(new)** | `graph_read_projection.rs` | per-envelope summary serializer |
| `status_str` | `fn(Status) -> &'static str` | `response/mod.rs:110` | **reused** for lifecycle status |
| `EntryRecord` | `struct` (34 fields, no `skip_serializing_if`) | `store/schema.rs:49` | **UNTOUCHED** |
| `EdgeRecord` | `struct { source_id:u64, target_id:u64, relation_type:String, direction:String, depth:u8 }` | `graph_read.rs:143` | **UNTOUCHED**; projection reads `{source_id,target_id,relation_type,depth}` via a `Value` |
| `SubgraphResponse` | `struct { nodes:Vec<EntryRecord>, edges:Vec<EdgeRecord>, truncated:bool, seed_ids:Vec<u64>, depth_reached:u8 }` | `graph_read.rs:190` | envelope unchanged; gains `to_summary_json` impl |
| `ChainResult` | `struct { entries:Vec<EntryRecord>, truncated:Truncated }` | `graph_read.rs:165` | envelope unchanged; gains impl |
| `CurrentResponse` | `struct { entry:EntryRecord }` | `graph_read.rs:172` | envelope unchanged; gains impl |
| `InverseResponse` | `struct { entries:Vec<EntryRecord>, total_returned:usize }` | `graph_read.rs:200` | envelope unchanged; gains impl |
| `FilterResponse` | `struct { entries:Vec<EntryRecord>, total_returned:usize }` | `graph_read.rs:207` | envelope unchanged; gains impl |
| `NeighborsResponse` | edges only | `graph_read.rs:178` | **accept-and-ignore** `detail`; no impl |
| `PathResponse` | `struct { found:bool, from_id:u64, to_id:u64, hops:Vec<PathHop>, length:u8 }` | `graph_read.rs:229` | **accept-and-ignore** `detail`; no impl |
| `parse_format` / `ResponseFormat` | `fn(&Option<String>) -> Result<ResponseFormat, ServerError>` / `enum` | `response/mod.rs:59-81` | **UNTOUCHED** — not called by graph; unchanged for all other tools (SR-06) |
| `validate_no_unsupported_params` | `fn(&GraphParams) -> Result<(), String>` | `graph_read.rs:379` → `graph_read_validation.rs` | `detail` is universal — **no** rejection arm |
| `fetch_nodes_batch` | `async fn(&Store, &[u64]) -> Result<Vec<EntryRecord>, ErrorData>` | `graph_read_subgraph.rs:639` | **UNTOUCHED** — still reads `content` (SR-01) |

---

## Which modes project vs accept-and-ignore

| Mode | Node payload | `detail` behavior |
|------|--------------|-------------------|
| subgraph | `SubgraphResponse.nodes: Vec<EntryRecord>` | **projects** (summary/full) |
| chain | `ChainResult.entries: Vec<EntryRecord>` | **projects** |
| current | `CurrentResponse.entry: EntryRecord` | **projects** |
| inverse | `InverseResponse.entries: Vec<EntryRecord>` | **projects** |
| filter | `FilterResponse.entries: Vec<EntryRecord>` | **projects** |
| neighbors | edges only (`EdgeRecord`) | **accept-and-ignore** |
| path | hops only (`PathHop`) | **accept-and-ignore** |

`format=markdown` → **rejected** for **all seven** modes (no graph-markdown renderer). `json`
is the only accepted serialization.

---

## Error Boundaries

| Origin | Condition | Result |
|--------|-----------|--------|
| `resolve_graph_output` | `format=markdown` (any mode) | `ERROR_INVALID_PARAMS`, names reason + `format=json` (AC-08, SR-05) |
| `resolve_graph_output` | `format=summary` **and** explicit `detail` | `ERROR_INVALID_PARAMS` (deprecated-alias conflict) |
| `resolve_graph_output` | `format` not in {json, markdown, summary} | `ERROR_INVALID_PARAMS` |
| `parse_detail` | `detail` not in {summary, full} | `ERROR_INVALID_PARAMS` |
| serialization | `serde_json::to_string` failure | `ERROR_INTERNAL` (existing path) |

---

## Constraints Honored

- **`GraphParams` layout lock** (ADR-003 #4490/#4491): `detail` is an additive `Option<T>`;
  no field removed or reordered.
- **`EdgeRecord`/`EntryRecord` wire locks** (ADR-004, SR-07): projection is a distinct type /
  `serde_json::Value`; **no `skip_serializing_if`** added to either shared type.
- **Shared `ResponseFormat`/`parse_format` untouched** (SR-06): graph uses its own resolver;
  the shared enum's behavior for non-graph callers is unchanged.
- **JSON-only graph output** (D-4): `format=markdown` rejected loudly, no silent fallback.
- **500-line/file rule** (SR-08): `graph_read_subgraph.rs` is **already 742 lines (pre-existing
  over-limit debt)** — the projection goes in the **new** `graph_read_projection.rs`, never
  into the subgraph file. `graph_read.rs` (389) gains the resolver + arm branching; if it nears
  500, relocate `resolve_graph_output` to `graph_read_validation.rs` or the projection module.
- **No SQL content-drop** (SR-01): `fetch_nodes_batch` still reads `content`; the win is
  wire/context size.
- **Lifecycle ≠ delivery status** (SR-09): projected `status` is lifecycle only; the tool
  description states this and points delivery-status needs at `context_get` / follow-up #3.

---

## Acceptance Criteria Traceability

| AC | Where satisfied |
|----|-----------------|
| AC-01 (suite ADR) | ADR-001 (#5509) |
| AC-02 (both axes end-to-end) | `resolve_graph_output` + arm threading |
| AC-03 / AC-03b (summary field set, UTF-8 preview) | `NodeSummary` + `content_preview` |
| AC-04 (full byte-identical) | `Detail::Full` arm = `serde_json::to_string(&result)` unchanged |
| AC-05 (default summary) | `parse_detail(None) = Summary` |
| AC-06 (#913 repro shrinks) | subgraph `to_summary_json` |
| AC-07 (legacy `format=summary`) | resolver legacy-alias branch |
| AC-08 (`markdown` reject; neighbors/path ignore) | resolver + accept-and-ignore arms |
| AC-09 (layout invariant + tool description) | additive `detail`, description edit |

---

## Open Questions

1. **`graph_read.rs` line budget.** Adding the resolver + five-way arm branching to a
   389-line file is comfortable, but the pseudocode/impl agent should measure. If it crosses
   500, move `resolve_graph_output` into `graph_read_validation.rs`. **Non-blocking** —
   recommendation stated (ADR-002 §Consequences).
2. **Pre-existing over-limit file.** `graph_read_subgraph.rs` is already 742 lines. vnc-044
   must **not** add to it, but this ADR does not mandate splitting it (out of scope). Flag for
   a future cleanup feature. **For the human / delivery lead to note.**
3. **`current`/`inverse`/`filter` summary tests.** #913 focuses on subgraph; the projection
   applies to all five node-bearing modes. The risk-strategist/tester should confirm coverage
   spans more than subgraph (each envelope's `to_summary_json` preserves its own metadata
   fields). **For the tester.**
