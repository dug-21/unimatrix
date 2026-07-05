## ADR-002: `context_graph` Adoption of the Two-Axis Contract — Projection Type, Seam Threading, and Loud Rejections

**Feature:** vnc-044 | **Status:** active | **Depends on:** ADR-001 (#5509 — the suite contract)

### Context

ADR-001 fixes the suite-wide contract; this ADR fixes the graph-specific decisions needed to
implement it for `context_graph` — the first adopter — without disturbing shared types or the
locked `GraphParams` layout. The constraints that shape these decisions:

- `EntryRecord` (`unimatrix-store/src/schema.rs:49`) carries **no** `skip_serializing_if`; it
  is the shared store type serialized by every context tool. Adding field-skipping to it would
  leak lean output into every other serializer suite-wide (SR-07).
- `EdgeRecord` (`graph_read.rs:143`) wire shape is locked (ADR-004 vnc-018) — `metadata`/all
  fields always serialize; the edge projection must be a *separate* shape (SR-07).
- `GraphParams` (`graph_read.rs:72`) layout is locked (ADR-003 vnc-018/019, #4490/#4491):
  Option<T> additions only, no removal/reorder.
- `ResponseFormat`/`parse_format` (`response/mod.rs:59-81`) are suite-shared; vnc-044 must not
  change their behavior for non-graph callers (SR-06, blast-radius pattern #4831).
- Graph output is **JSON-only** today — no graph-markdown renderer exists.
- `graph_read_subgraph.rs` is **already 742 lines** — over the 500-line rule (pre-existing
  debt). The projection must not be added to it (SR-08).
- `handle_graph` (`graph_read.rs:247`) currently binds `_ctx` and every mode arm calls
  `serde_json::to_string(&result)`, format-blind. This is the single parse-and-drop seam.

### Decision

**1. Additive `detail` field on `GraphParams`, universal (not mode-scoped).** Add
`detail: Option<String>` to `GraphParams` (Option<T> addition — ADR-003-compliant). Unlike
mode-specific fields (`seed_ids`, `max_depth`, `category`, …), `detail` is a **universal
field** — valid on every mode, exactly like `format`, `agent_id`, `mode`. Therefore
`validate_no_unsupported_params` (`graph_read_validation.rs`) adds **no per-mode rejection arm**
for `detail`; it is never listed as an unsupported param. On modes with no node bodies
(`neighbors`, `path`) it is **accept-and-ignore** (D-4): accepted, changes nothing.

**2. Graph output resolver — `resolve_graph_output`.** `context_graph` does **not** call the
shared `parse_format` (that would treat `summary` as a `ResponseFormat` and never reject
`markdown`). Instead a graph-local resolver reads `params.format` + `params.detail` and returns
`(Detail, GraphSerialization)`:

```
enum GraphSerialization { Json }        // markdown rejected before this type is produced

fn resolve_graph_output(params: &GraphParams) -> Result<(Detail, GraphSerialization), ErrorData>
```

Resolution order:
- **Legacy alias:** `format = Some("summary")` →
  - if `detail` is **absent** → `(Detail::Summary, Json)` (the deprecated alias, ADR-001 §4).
  - if `detail` is **present** → `ERROR_INVALID_PARAMS`: "format=summary is a deprecated alias
    for detail=summary; do not combine it with an explicit detail".
- **Serialization:** on the remaining `format` value —
  - `None` or `"json"` → `Json`.
  - `"markdown"` → **`ERROR_INVALID_PARAMS`** (loud), message naming the reason and the fix:
    "format=markdown is not supported for context_graph — no graph-markdown renderer exists
    yet; use format=json" (SR-05, AC-08).
  - anything else → `ERROR_INVALID_PARAMS` ("format must be json (markdown not yet supported
    for graph)").
- **Verbosity:** on `detail` (via shared `parse_detail`) —
  - `None` → `Detail::Summary` (default, D-2 / AC-05).
  - `"summary"` → `Summary`; `"full"` → `Full`; else → `ERROR_INVALID_PARAMS`.

`resolve_graph_output` runs at the top of `handle_graph`, **before** mode dispatch, so
`markdown`/invalid values are rejected uniformly for all seven modes (including
`neighbors`/`path`).

**3. Distinct lean projection type in a dedicated module** (SR-07, SR-08). A new module
`crates/unimatrix-server/src/mcp/graph_read_projection.rs` defines the graph summary type — the
`context_graph` instance of ADR-001 §5's field set:

```
#[derive(Serialize)]
struct NodeSummary {
    id: u64,
    title: String,
    category: String,
    tags: Vec<String>,
    status: &'static str,        // lifecycle via status_str(entry.status) — NOT delivery status
    confidence: f64,
    content_preview: String,
    content_truncated: bool,
}
fn node_summary(entry: &EntryRecord) -> NodeSummary
```

`EntryRecord` and `EdgeRecord` are **not touched** — no `skip_serializing_if` added. The edge
projection reuses the already-lean `EdgeRecord` fields `{source_id, target_id, relation_type,
depth}` by building a `serde_json::Value` (dropping `direction`/`metadata` at projection time,
not by mutating `EdgeRecord`).

**4. Envelope projection at the serialization seam.** Traversal and the typed mode envelopes
(`SubgraphResponse`, `ChainResult`, `CurrentResponse`, `InverseResponse`, `FilterResponse`)
are **unchanged** — they keep `Vec<EntryRecord>` / `EntryRecord`. The projection happens only at
serialization. Each node-bearing envelope gains a summary serializer via a trait in the
projection module:

```
trait GraphSummaryProjection { fn to_summary_json(&self) -> serde_json::Value; }
```

implemented for the five node-bearing envelopes. Each impl replaces node bodies with
`node_summary(...)` and **preserves every envelope metadata field** (`truncated`, `seed_ids`,
`depth_reached`, `total_returned`, …). `NeighborsResponse` and `PathResponse` carry no node
bodies and do **not** implement the trait.

Each mode arm in `handle_graph` becomes:

```
let json = match detail {
    Detail::Full    => serde_json::to_string(&result)?,                  // today's output, byte-identical (AC-04)
    Detail::Summary => serde_json::to_string(&result.to_summary_json())?, // node-bearing modes
};
```

`neighbors`/`path` always `serde_json::to_string(&result)?` (detail ignored — AC-08). This is
the fix for the `graph_read.rs:251` parse-and-drop: the resolved `(detail, serialization)`
threads into every arm.

**5. Shared primitives imported, not redefined** (ADR-001 SR-03). `Detail`, `parse_detail`,
`CONTENT_PREVIEW_BYTES`, and `content_preview` live in a new shared module
`crates/unimatrix-server/src/mcp/response/verbosity.rs` and are **imported** by the graph
path. `content_preview` uses the codebase char-boundary idiom:

```
pub const CONTENT_PREVIEW_BYTES: usize = 256;
pub fn content_preview(content: &str) -> (String, bool) {
    if content.len() <= CONTENT_PREVIEW_BYTES { return (content.to_string(), false); }
    let mut end = CONTENT_PREVIEW_BYTES;
    while end > 0 && !content.is_char_boundary(end) { end -= 1; }
    (content[..end].to_string(), true)   // truncated == true because content.len() > 256
}
```

`truncated` is `content.len() > CONTENT_PREVIEW_BYTES` (byte compare), independent of where the
boundary floored to. Exactly-256-byte content returns `(whole, false)`; empty returns
`("", false)`.

**6. `fetch_nodes_batch` unchanged — no SQL content-drop** (SR-01). It still selects
`ENTRY_COLUMNS` (incl. `content`) and hydrates tags; the preview is computed from the read
`content`. The win is wire/context size only.

**7. Lifecycle-status disclosure in the tool description** (SR-09). The `context_graph` tool
description MUST state: summary `status` is the **lifecycle** status
(`active/deprecated/proposed/quarantined`); it is **not** capability *delivery* status
(`missing/partial/proven/claimed`), which lives in the entry `content`. A capability subgraph
returns `active` for every node. It MUST also document both axes, the `summary` default, the
`format=markdown` rejection (SR-05), and the accepted `full`→`summary` default flip (SR-04).

### Consequences

**Easier:**
- `context_graph` honors both axes end-to-end; the #913 pull drops from ~135KB to a few KB by
  default and stays valid JSON.
- Shared `EntryRecord`/`EdgeRecord`/`ResponseFormat`/`parse_format` are untouched — zero
  blast radius on non-graph callers (SR-06/SR-07).
- Traversal and Full output are byte-identical to today (AC-04) — no regression path.
- The projection lives in its own module, keeping `graph_read_subgraph.rs` from growing past
  its already-over-limit size (SR-08).

**Harder:**
- Default `full`→`summary` is a visible behavior change for existing graph callers (accepted,
  disclosed — AC-05/SR-04).
- Five envelope trait impls plus the resolver add code; the trait keeps it centralized in the
  projection module rather than scattered across mode handlers (Pattern #4500).
- `graph_read.rs` gains the resolver + arm branching; monitor its line count (currently 389) —
  if the resolver + trait wiring push it toward 500, move the resolver into
  `graph_read_validation.rs` or the new projection module.
- The projection answers lifecycle, not delivery, status — the honestly-carried #913 gap
  (SR-09), mitigated only by disclosure + follow-up #3.

### Cross-references

- Implements: ADR-001 (#5509) — the suite contract. **Prerequisite:** ADR-002 cannot be
  implemented correctly without ADR-001's ratified axis name, 256 constant, field set, and
  default.
- Locks respected: ADR-003 vnc-018/019 (`GraphParams` #4490/#4491), ADR-004 vnc-018
  (`EdgeRecord`). Pattern #4500 (per-mode coordinated change), #4518 (extract graph module at
  line limit).
