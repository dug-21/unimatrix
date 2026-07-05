# vnc-044 — Implementation Brief

> Split the `context_graph` `format` overload into two orthogonal axes — `format` (serialization: `markdown|json`) and `detail` (verbosity: `summary|full`) — and add a lean summary node projection. First adopter of the suite-wide two-axis contract (ADR-001). Design SETTLED 2026-07-05; ADRs ratified; alignment clean (5 PASS, 1 WARN doc-sync, no open variances). Tracks GH #913.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-044/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-044/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-044/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-044/architecture/ARCHITECTURE.md |
| ADR-001 (suite contract) | product/features/vnc-044/architecture/ADR-001-two-axis-format-verbosity-contract.md |
| ADR-002 (graph adoption) | product/features/vnc-044/architecture/ADR-002-context-graph-adoption.md |
| Risk / Test Strategy | product/features/vnc-044/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-044/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-044/ACCEPTANCE-MAP.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| Shared verbosity primitives (`response/verbosity.rs`) | pseudocode/verbosity.md ✓ | test-plan/verbosity.md ✓ |
| Graph projection (`graph_read_projection.rs`) | pseudocode/graph_read_projection.md ✓ | test-plan/graph_read_projection.md ✓ |
| Output resolver + seam threading (`graph_read.rs`) | pseudocode/graph_read.md ✓ | test-plan/graph_read.md ✓ |
| Tool description (`tools.rs`) | pseudocode/tools.md ✓ | test-plan/tools.md ✓ |

Stage 3a complete (✓ = file written). All four component pseudocode + test-plan files exist; OVERVIEW files below.

### Cross-Cutting Artifacts (Stage 3a — written)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Fix `context_graph`'s `format` category error: `format` fuses serialization (`markdown|json`) with verbosity (`summary` vs full), is parsed and then discarded (`graph_read.rs:251`), and every mode emits identical full-content JSON. Split into two orthogonal axes — `format` (serialization only) and a new additive `detail` (verbosity) — wire both end-to-end, and add a lean summary node projection so a large traversal (the #913 vision-root pull: 75 nodes / 82 edges / ~135KB) drops to a few KB, consumable in one call. Default verbosity flips to `summary` (accepted behavior change). Full output stays reachable byte-for-byte via `detail=full`.

## Resolved Decisions

| Decision | Resolution | Source | ADR |
|----------|-----------|--------|-----|
| Axis spelling / values | `detail: summary\|full`, default `summary`; `format` = serialization only (`markdown\|json`) | SCOPE D-1/D-4, OQ-4 | ADR-001 §2/§3 (#5509) |
| Default verbosity | `summary` (lean) suite-wide norm; accepted behavior change for graph (returns full today) | SCOPE D-2, OQ-2 | ADR-001 §3 |
| Legacy `format=summary` | Deprecated alias → `detail=summary` + serialization `json`; `format=summary` **with** explicit `detail` → `ERROR_INVALID_PARAMS` (conflict) | SCOPE D-2, FR-9 | ADR-001 §4, ADR-002 §2 |
| Projected `status` | Lifecycle `EntryRecord.status` (`active/deprecated/proposed/quarantined`) only — **NOT** capability delivery status | SCOPE D-3, OQ-3 | ADR-001 §7, ADR-002 §7 |
| Summary field set | Node `{id,title,category,tags,status,confidence,content_preview,content_truncated}`; edge `{source_id,target_id,relation_type,depth}` | SCOPE D-6 | ADR-001 §5, ADR-002 §3 |
| Preview cap | `CONTENT_PREVIEW_BYTES = 256`, single-sourced; UTF-8 char-boundary floor; no ellipsis | SCOPE D-6 | ADR-001 §6, ADR-002 §5 |
| `content_truncated` | `content.len() > 256` (byte compare), independent of the flooring index | SCOPE D-6 | ADR-002 §5 |
| `format=markdown` on graph | Rejected loudly (`ERROR_INVALID_PARAMS`) — no graph-markdown renderer; no silent JSON fallback | SCOPE D-4 | ADR-002 §2 |
| `detail` on neighbors/path | Universal field, accept-and-ignore (no per-mode rejection arm) | SCOPE D-4 | ADR-002 §1 |
| crt-057 reconciliation | ADR generalizes crt-057's render-axis and adds the missing verbosity axis; #5434 stays active, not superseded | SCOPE D-5 | ADR-001 §Reconciliation |
| Resolver, not shared `parse_format` | Graph uses its own `resolve_graph_output`; shared `ResponseFormat`/`parse_format` untouched | SCOPE Constraints | ADR-002 §2 |

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/mcp/response/verbosity.rs` | **create** | Shared verbosity primitives: `Detail` enum, `parse_detail`, `CONTENT_PREVIEW_BYTES=256`, `content_preview()`. Single source for ADR-001's shared constants. |
| `crates/unimatrix-server/src/mcp/graph_read_projection.rs` | **create** | Graph-local `NodeSummary`, `node_summary()`, `GraphSummaryProjection` trait + impls for the 5 node-bearing envelopes. |
| `crates/unimatrix-server/src/mcp/graph_read.rs` | modify | Add `detail: Option<String>` to `GraphParams`; add `resolve_graph_output`; thread `(Detail, GraphSerialization)` into each mode arm (fix the `:251` parse-and-drop). |
| `crates/unimatrix-server/src/mcp/graph_read_validation.rs` | modify (comment only) | Document `detail` as a universal field — **no** per-mode rejection arm added. |
| `crates/unimatrix-server/src/mcp/tools.rs` | modify | `context_graph` tool description: document both axes, `summary` default + per-tool divergence, `format=markdown` rejection, and the lifecycle-vs-delivery status caveat. |
| `crates/unimatrix-server/src/mcp/graph_read_subgraph.rs` | **UNTOUCHED** | Already 742 lines (pre-existing over-limit debt — flag, do not fix here). `fetch_nodes_batch` unchanged; still reads `content`. |
| `crates/unimatrix-store/src/schema.rs` | **UNTOUCHED** | Shared `EntryRecord`/`EdgeRecord`/`Status` — no `skip_serializing_if` added. |
| `crates/unimatrix-server/src/mcp/response/mod.rs` | **UNTOUCHED** | Shared `ResponseFormat`/`parse_format` unchanged for non-graph callers; `status_str()` reused read-only. |

## Data Structures

```rust
// response/verbosity.rs (new, shared)
pub const CONTENT_PREVIEW_BYTES: usize = 256;
pub enum Detail { Summary, Full }

// graph_read_projection.rs (new, graph-local)
#[derive(Serialize)]
struct NodeSummary {
    id: u64,
    title: String,
    category: String,
    tags: Vec<String>,
    status: &'static str,        // status_str(entry.status) — LIFECYCLE, not delivery status
    confidence: f64,
    content_preview: String,
    content_truncated: bool,
}

// graph_read.rs (new)
enum GraphSerialization { Json }  // markdown rejected before this value is produced

// graph_read.rs (edit) — additive Option<T>, ADR-003-safe
struct GraphParams { /* ...existing fields unmoved... */ detail: Option<String> }
```

Edge summary is built as a `serde_json::Value` projecting `{source_id, target_id, relation_type, depth}` from `EdgeRecord` — `direction`/`metadata` dropped at projection time, `EdgeRecord` not mutated.

## Function Signatures

```rust
// response/verbosity.rs
pub fn parse_detail(detail: &Option<String>) -> Result<Detail, ServerError>;
pub fn content_preview(content: &str) -> (String, bool);   // (preview, truncated)

// graph_read_projection.rs
fn node_summary(entry: &EntryRecord) -> NodeSummary;
trait GraphSummaryProjection { fn to_summary_json(&self) -> serde_json::Value; }
// impl for SubgraphResponse, ChainResult, CurrentResponse, InverseResponse, FilterResponse

// graph_read.rs
fn resolve_graph_output(params: &GraphParams)
    -> Result<(Detail, GraphSerialization), ErrorData>;
```

`content_preview` uses the codebase char-boundary idiom (NOT nightly `floor_char_boundary`, NOT `&s[..256]`, NOT `.chars().take()`):

```rust
pub fn content_preview(content: &str) -> (String, bool) {
    if content.len() <= CONTENT_PREVIEW_BYTES { return (content.to_string(), false); }
    let mut end = CONTENT_PREVIEW_BYTES;
    while end > 0 && !content.is_char_boundary(end) { end -= 1; }
    (content[..end].to_string(), true)   // truncated == content.len() > 256
}
```

Per-arm serialization seam (node-bearing modes):

```rust
let json = match detail {
    Detail::Full    => serde_json::to_string(&result)?,                   // today's output, byte-identical
    Detail::Summary => serde_json::to_string(&result.to_summary_json())?, // lean projection
};
```

`neighbors`/`path` always `serde_json::to_string(&result)?` (detail accepted, ignored — no node bodies).

## Modes: project vs accept-and-ignore

| Mode | Node payload | `detail` behavior |
|------|--------------|-------------------|
| subgraph | `SubgraphResponse.nodes: Vec<EntryRecord>` | **projects** — preserves `truncated`/`seed_ids`/`depth_reached` |
| chain | `ChainResult.entries: Vec<EntryRecord>` | **projects** — preserves `Truncated` |
| current | `CurrentResponse.entry: EntryRecord` | **projects** — single node, not an array |
| inverse | `InverseResponse.entries: Vec<EntryRecord>` | **projects** — preserves `total_returned` |
| filter | `FilterResponse.entries: Vec<EntryRecord>` | **projects** — preserves `total_returned` |
| neighbors | edges only (`EdgeRecord`) | **accept-and-ignore** |
| path | hops only (`PathHop`) | **accept-and-ignore** |

`format=markdown` → rejected for **all seven** modes (resolver runs before dispatch).

## Constraints

- **C-1 `GraphParams` layout locked** (ADR-003, #4490/#4491): `detail` additive `Option<T>` only; no field removed/retyped/reordered.
- **C-2 `EdgeRecord` wire locked** (ADR-004 vnc-018): no `skip_serializing_if`; edge projection is a separate `serde_json::Value`.
- **C-3 Do NOT add `skip_serializing_if` to `EntryRecord`** (shared `unimatrix-store`): projection is a distinct type.
- **C-4 `ResponseFormat`/`parse_format` suite-shared** (`response/mod.rs`, ~45-site blast radius, pattern #4831): graph code change stays graph-local; shared enum behavior unchanged for non-graph callers. If the shared enum is touched, enumerate all sites via `cargo test --workspace --no-run` first.
- **C-5 Graph output JSON-only today**: `format=markdown` rejected until a renderer ships.
- **C-6 Capability delivery status is not a first-class field**: projection carries lifecycle status only.
- **C-7 Max 500 lines/file**: projection lives in the new `graph_read_projection.rs`, never in the already-742-line `graph_read_subgraph.rs`. Watch `graph_read.rs` (389→); if it nears 500, relocate `resolve_graph_output` to `graph_read_validation.rs` or the projection module.
- **C-8 Per-mode change coordinated** (Pattern #4500): the 5 node-bearing arms projected consistently via the trait.
- **C-9 `256` single-sourced** as `CONTENT_PREVIEW_BYTES`; no bare `256` literal in the graph path.

## Dependencies

- **Crates / modules:** `unimatrix-server` (`mcp/graph_read.rs`, `mcp/graph_read_projection.rs` new, `mcp/response/verbosity.rs` new, `mcp/response/mod.rs`, `mcp/graph_read_validation.rs`, `mcp/tools.rs`); `unimatrix-store` (`schema.rs`: `EntryRecord`, `EdgeRecord`, `Status` — read-only). `serde`/`serde_json`.
- **Existing components reused:** `handle_graph` dispatch + seven mode handlers; `GraphParams`; `fetch_nodes_batch` (full `ENTRY_COLUMNS` fetch + tag hydration, unchanged); `status_str` (`response/mod.rs:110`).
- **Companion deliverable:** ADR-001 suite contract (#5509) — the ratified axis name/values, 256 constant, field set, default. ADR-002 (#5510) — graph adoption.
- **Prior art:** vnc-018/019/020 (graph modes, `GraphParams`/`EdgeRecord` locks); crt-057/vnc-011 = GH #894 (render-axis precedent on `context_cycle_review`, entry #5434).

## NOT in Scope

1. Migrating the other context tools (`context_get`, `context_search`, `context_lookup`, `context_status`, mutations, `context_briefing`) to the two-axis model — deferred follow-ups. Model *consistency* is delivered by the ADR; the *code migration* is not. Do not change shared `ResponseFormat` behavior for those callers.
2. A markdown rendering of graph structure — `format=markdown` rejected loudly until a renderer ships.
3. A schema change to `EntryRecord` — including promoting capability delivery status to a first-class column. Named follow-up #3.
4. Changing traversal semantics — BFS, `max_depth`, `max_nodes`, `resolve_supersessions`, edge filtering, truncation unchanged (vnc-018/019/020).
5. New graph modes or new edge fields — `EdgeRecord` wire shape locked.
6. Removing or renaming `format` — additive change; `format` retained and re-scoped.
7. Folding `context_cycle_review` (crt-057) onto the `detail` axis — named follow-up #2.
8. Fixing the pre-existing over-limit `graph_read_subgraph.rs` (742 lines) — flagged for a future cleanup feature, not this one.

## Standing Risk (carry honestly — do not let the projection imply it answers #913 orientation status)

The summary projection carries **lifecycle** `EntryRecord.status` (`active/deprecated/proposed/quarantined`), **not** capability **delivery** status (`missing/partial/proven/claimed`, which lives inside the entry `content` blob). A subgraph of capability entries returns `status:"active"` for **every** node. `content_preview` only *partially and unreliably* softens this. So #913's one-call orientation *delivery-status tally* is **NOT delivered** — this feature delivers the payload-size + structure win only. Promoting delivery status to a first-class projected field is named follow-up #3. The tool description, AC-06 criterion text, ADR-001 §7, and ADR-002 §7 all state this plainly; R-11 is a documentation/expectation gate that instructs testers **not** to treat delivery-status absence as a defect.

## Critical Test Gates (from RISK-TEST-STRATEGY — non-negotiable)

- **R-01 (Critical, DoS):** `content_preview` UTF-8 char-boundary flooring — multibyte codepoint straddling byte 256 must floor below 256 to a char boundary, valid UTF-8, **never panic**. `content` is attacker-influenceable → naive `&content[..256]` is a request-triggered DoS. Boundary table: empty / <256 / exactly-256 / 257-ASCII / multibyte-straddle-256; assert no ellipsis.
- **R-02 (Critical):** `content_truncated == content.len() > 256` (byte compare), decoupled from the flooring index. The 257B-ASCII-floors-to-256 false-negative is the non-negotiable trap case.
- **R-03 (Critical):** default-summary + explicit-summary tested on **each** of the five node-bearing modes, each asserting the projected node shape AND preserved envelope metadata. No mode covered by `subgraph` alone.
- **R-04 (Critical):** `detail=full` golden byte-for-byte equality vs pre-vnc-044 payload (key order + field presence) for `subgraph` + ≥1 other node-bearing mode.
- **R-05 (High):** `format=markdown` rejected on **all seven** modes (resolution before dispatch); reason substring asserted, not verbatim.
- **R-06 (High):** non-graph serialization regression suite green; `cargo test --workspace --no-run` compiles with no new `ResponseFormat` exhaustive-match arms; shared structs untouched (code-review gate).
- **R-07 (High):** exact summary field set — present AND absent keys for node and edge (edge `direction`/`metadata` absent; node `content`/hashes/timestamps/counts absent).

## Alignment Status

ALIGNMENT-REPORT.md: **6 checks — 5 PASS, 1 WARN, no VARIANCE, no FAIL.** No open variances; no human approval required.

- **WARN (doc-sync, non-blocking):** SPECIFICATION.md front-matter/OQ-A described `detail`/`summary`/`full` as "placeholder pending ADR ratification," but ADR-001 §2 has ratified exactly that spelling. Semantics already match; the hedge is stale (pattern #3337). Reconcile OQ-A to reference ADR-001 §2 during delivery — no logic rework implied. (Note: the SPECIFICATION front-matter has since been updated to state RATIFIED; verify at Gate 3a.)
- **WARN (awareness only):** ADR-001 binds the whole suite while only `context_graph` exercises it. Mitigated in-artifact (per-tool field-set override in §5; single-sourced constants). Treat the first non-graph adopter as ratification-under-load; expect a possible ADR-001 amendment then. Accepted, not a variance.
- Default `full`→`summary` flip and the SR-09 lifecycle-vs-delivery gap were both scrutinized and found aligned (human-settled, disclosed, backward-compat preserved, follow-ups named).
