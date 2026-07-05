# Component 3 — Output resolver + seam threading (`mcp/graph_read.rs`, MODIFY)

## Purpose

Add the `detail` axis to `GraphParams`, resolve `(format, detail)` into
`(Detail, GraphSerialization)` once at the top of `handle_graph`, and thread the resolved pair
into every mode arm — fixing the `graph_read.rs:251` parse-and-drop (`_ctx` bound, `format`
discarded). This is the only file whose line budget must be watched (389 → ~460).

## Edits (five, additive)

### E-1 — `GraphParams` gains `detail` (additive `Option<T>`, C-1/ADR-003)

Append a field at the END of the struct (no existing field removed/retyped/reordered — AC-09).
Also re-scope the `format` field's schemars doc (serialization only). These doc comments are
single-source (schemars derive; NOT covered by the #869 twin-literal byte-equality guard —
entry #5457 gotcha 3), but they are the schema surface agents read, so update them:

```
/// Serialization axis: "json" (default) or "markdown".
/// NOTE: context_graph currently rejects "markdown" (no graph-markdown renderer);
/// legacy "summary" is a deprecated alias for detail=summary. Does NOT select verbosity.
pub format: Option<String>,          // existing field — doc updated, type/position unchanged

/// ...existing fields unmoved...

/// Verbosity axis: "summary" (default, lean projection) or "full" (complete records).
/// Universal across all seven modes; accepted-and-ignored on neighbors/path (no node bodies).
#[serde(default)]
pub detail: Option<String>,          // NEW — additive, ADR-003-safe
```

### E-2 — declare the projection submodule + import the trait

Alongside the existing `#[path = "graph_read_*.rs"] mod …` block:

```
#[path = "graph_read_projection.rs"]
mod graph_read_projection;

use graph_read_projection::GraphSummaryProjection;   // brings to_summary_json into scope
```

Also import the verbosity primitives:
```
use crate::mcp::response::verbosity::{Detail, parse_detail};
```

### E-3 — `GraphSerialization` enum

```
/// Graph serialization axis. Single variant today — `markdown` is rejected in
/// `resolve_graph_output` before this value is ever produced (C-5, ADR-002 §2).
enum GraphSerialization { Json }
```

### E-4 — `resolve_graph_output`

```
fn resolve_graph_output(params: &GraphParams)
    -> Result<(Detail, GraphSerialization), ErrorData>
```

Resolution order (ADR-002 §2 — legacy alias FIRST, then serialization, then verbosity):

```
// 1. Legacy alias: format == "summary" (case-insensitive)
if let Some(f) = &params.format {
    if f.to_lowercase() == "summary" {
        if params.detail.is_some() {
            // conflict: do not combine the deprecated alias with an explicit detail (FR-9, R-08)
            return Err(ErrorData::new(
                ERROR_INVALID_PARAMS,
                "format=summary is a deprecated alias for detail=summary; \
                 do not combine it with an explicit detail",
                None,
            ));
        }
        return Ok((Detail::Summary, GraphSerialization::Json));   // alias → summary + json
    }
}

// 2. Serialization from the remaining format value
let serialization = match params.format.as_deref().map(|s| s.to_lowercase()).as_deref() {
    None | Some("json") => GraphSerialization::Json,
    Some("markdown") => {
        return Err(ErrorData::new(
            ERROR_INVALID_PARAMS,
            "format=markdown is not supported for context_graph — \
             no graph-markdown renderer exists yet; use format=json",   // reason + fix (SR-05, AC-08)
            None,
        ));
    }
    Some(_) => {
        return Err(ErrorData::new(
            ERROR_INVALID_PARAMS,
            "format must be json (markdown not yet supported for graph)",
            None,
        ));
    }
};

// 3. Verbosity via the shared parser (None → Summary default; bad value → ERROR_INVALID_PARAMS)
let detail = parse_detail(&params.detail).map_err(ErrorData::from)?;   // ServerError → ErrorData

Ok((detail, serialization))
```

Notes:
- `format=summary` short-circuits in step 1, so it never reaches the step-2 match (no double
  handling). `format=summary` + `detail=summary` still errors (conflict pinned — R-08 case 3).
- `.to_lowercase()` matches the established `parse_format` case-insensitivity.
- Returns `ErrorData` directly (not `ServerError`) because `handle_graph` returns
  `Result<_, ErrorData>`; `parse_detail`'s `ServerError` is adapted via the existing `From` impl.
- Line-budget escape hatch (C-7 / ADR-002 OQ-1): if adding this pushes `graph_read.rs` past
  500 lines, move `resolve_graph_output` + `GraphSerialization` into `graph_read_validation.rs`
  (it already holds cross-mode param logic). Measure after E-1..E-5; recommendation only.

### E-5 — call the resolver + thread the seam in `handle_graph`

At the top of `handle_graph`, BEFORE mode dispatch, so rejections are uniform across all seven
modes (ADR-002 §2, R-05). Per the architecture flow, resolve runs first, then validate:

```
pub(crate) async fn handle_graph(store, typed_graph_state, params, _ctx) -> Result<…, ErrorData> {
    // Step 0 (NEW): resolve output axes — rejects markdown / legacy-conflict / bad values
    //               for ALL SEVEN modes before any dispatch.
    let (detail, _serialization) = resolve_graph_output(&params)?;
    // GraphSerialization has one variant (Json); bound as _serialization until a renderer ships.

    // Step 1 (unchanged): centralized param validation. detail is UNIVERSAL — no new arm.
    if let Err(msg) = validate_no_unsupported_params(&params) {
        return Err(ErrorData::new(ERROR_INVALID_PARAMS, msg, None));
    }

    // Step 2: mode dispatch (unchanged wiring) — each arm now serializes via the seam below.
    ...
}
```

Serialization seam — **node-bearing** arms (`subgraph`, `chain`, `current`, `inverse`,
`filter`). Replace each arm's `let json = serde_json::to_string(&result)…?` with:

```
let json = match detail {
    Detail::Full    => serde_json::to_string(&result),                    // TODAY's output, byte-identical (AC-04/FR-10)
    Detail::Summary => serde_json::to_string(&result.to_summary_json()),  // lean projection
}
.map_err(|e| ErrorData::new(ERROR_INTERNAL, format!("serialization error: {e}"), None))?;
Ok(CallToolResult::success(vec![rmcp::model::Content::text(json)]))
```

Applies to these existing arms:
- `"subgraph"` (result: `SubgraphResponse`) — graph_read.rs ~:325
- `"chain"` (result: `ChainResult`) — ~:279
- `"current"` (`resp: CurrentResponse`, wrapped in a `match … Ok/Err`) — ~:288; branch the
  `Ok(resp)` body only
- `"inverse"` (`InverseResponse`) — ~:335
- `"filter"` (`FilterResponse`) — ~:345

**Edge-only** arms (`neighbors`, `path`) — detail accepted, ignored (FR-8/AC-08). LEAVE
UNCHANGED: keep `serde_json::to_string(&result)?` (no `to_summary_json`, `NeighborsResponse`/
`PathResponse` do not implement the trait). markdown was already rejected in Step 0.
- `"neighbors"` — ~:309
- `"path"` — ~:354

`current` sits inside the `"chain" | "current" | "neighbors"` group: chain & current branch on
`detail`; neighbors does not.

## State / initialization

No new state; no lifecycle. `handle_graph` remains a stateless async dispatcher. The only
initialization change is computing `(detail, _serialization)` once per call before dispatch.

## Data flow

```
GraphParams{format, detail, …}
  → resolve_graph_output → (Detail, GraphSerialization::Json)   [reject on markdown/conflict/bad]
  → dispatch → typed envelope (unchanged)
  → seam: Full → to_string(&result) | Summary → to_string(&result.to_summary_json())
  → CallToolResult::text(json)
```

## Error handling

| Condition | Result |
|-----------|--------|
| `format=markdown` (any mode) | `ERROR_INVALID_PARAMS`, reason + `format=json` |
| `format=summary` + explicit `detail` | `ERROR_INVALID_PARAMS` (deprecated-alias conflict) |
| `format` not in {json, markdown, summary} | `ERROR_INVALID_PARAMS` |
| `detail` not in {summary, full} | `ERROR_INVALID_PARAMS` (from `parse_detail`, all modes) |
| `serde_json::to_string` failure | `ERROR_INTERNAL` (existing path, unchanged) |

Resolution runs pre-dispatch, so every failure above is mode-independent.

## Validation note (`graph_read_validation.rs` — comment only, no logic)

`detail` is a **universal** field (like `format`, `agent_id`, `mode`) — `validate_no_
unsupported_params` adds **NO** per-mode rejection arm for it (ADR-002 §1, FR-8, R-09). Add a
short doc comment in `graph_read_validation.rs` recording this so a future maintainer does not
"helpfully" add a rejection arm. Invalid `detail` values are still rejected — by `parse_detail`
inside `resolve_graph_output`, not by the per-mode validator.

## Key test scenarios (hints; full plan in test-plan/graph_read.md)

- AC-02 threading: same query, `detail=summary` vs `detail=full` → different payloads (proves
  the value reaches serialization, not re-dropped).
- AC-05 default: call with no `detail` → equals `detail=summary`, differs from `detail=full`,
  on each of the five node-bearing modes (R-03).
- AC-04/NFR-1 golden: `detail=full` byte-for-byte == pre-vnc-044 payload for `subgraph` + ≥1
  other node-bearing mode; key order + field presence asserted (R-04).
- AC-08 markdown reject on ALL SEVEN modes (pre-dispatch), reason substring `"markdown"` +
  `"format=json"` asserted, not verbatim (R-05/R-13).
- AC-07 legacy alias: `format=summary` (no detail) == `detail=summary` output, accepted;
  `format=summary` + `detail=full` and + `detail=summary` both `ERROR_INVALID_PARAMS` (R-08).
- neighbors/path: `detail` summary/full/absent → identical non-erroring output; `detail=bogus`
  → `ERROR_INVALID_PARAMS` (R-09).
- AC-09: existing `GraphParams` layout test still passes; `detail` additive at end (R-integration).
- End-to-end: preview non-empty for non-empty content (guards against a silent `content` drop
  from `fetch_nodes_batch`).

## Constraints honored

- C-1/ADR-003: `detail` additive `Option<String>` at struct end; existing layout intact.
- C-5: `markdown` rejected loudly, no silent JSON fallback.
- C-7: line budget watched; resolver relocatable to `graph_read_validation.rs` if >500.
- C-8: five node-bearing arms threaded consistently via the shared trait.
- SR-06/C-4: shared `ResponseFormat`/`parse_format` NOT called by graph; graph uses its own
  `resolve_graph_output`.
