# vnc-011 Pseudocode Overview

## Components

| Component | File | Crate Location |
|-----------|------|----------------|
| retrospective-formatter | retrospective-formatter.md | `crates/unimatrix-server/src/mcp/response/retrospective.rs` (new) |
| params-extension | params-extension.md | `crates/unimatrix-server/src/mcp/tools.rs` (modify `RetrospectiveParams`) |
| handler-dispatch | handler-dispatch.md | `crates/unimatrix-server/src/mcp/tools.rs` (modify `context_retrospective` handler) |

## Data Flow

```
MCP request { feature_cycle, format?, evidence_limit?, agent_id? }
    |
    v
[params-extension] RetrospectiveParams { format: Option<String> }  -- NEW field
    |
    v
[handler-dispatch] context_retrospective handler
    |
    +-- build_report pipeline (UNCHANGED) --> RetrospectiveReport
    |
    +-- match format:
    |     "json" or Some("json") --> clone-and-truncate(evidence_limit.unwrap_or(3)) --> format_retrospective_report() --> JSON CallToolResult
    |     "markdown" / None / default --> format_retrospective_markdown(&report) --> Markdown CallToolResult
    |     unrecognized --> error CallToolResult
```

## Shared Types

### CollapsedFinding (formatter-internal, NOT exported)

```
struct CollapsedFinding {
    rule_name: String,
    severity: Severity,               // highest in group (Critical > Warning > Info)
    claims: Vec<String>,              // from grouped findings
    total_events: f64,                // sum of measured across group
    tool_breakdown: Vec<(String, usize)>,  // tool -> count from evidence
    examples: Vec<EvidenceRecord>,    // k=3 earliest by timestamp
    narrative_summary: Option<String>, // from matched narrative.summary (FR-09)
    cluster_count: Option<usize>,     // from matched narrative
    sequence_pattern: Option<String>, // from matched narrative
}
```

No new shared/exported types are introduced. `CollapsedFinding` is private to `retrospective.rs`.

## Sequencing Constraints

1. **params-extension** first -- adds `format` field to `RetrospectiveParams`. Other components depend on this field existing.
2. **retrospective-formatter** second -- the new module with `format_retrospective_markdown`. Must exist before handler-dispatch can call it.
3. **handler-dispatch** third -- wires the format parameter to the correct formatter. Also requires `mod retrospective` registration in `response/mod.rs`.

## Module Registration (response/mod.rs)

Add after existing `#[cfg(feature = "mcp-briefing")] mod briefing;`:

```
#[cfg(feature = "mcp-briefing")]
mod retrospective;

#[cfg(feature = "mcp-briefing")]
pub use retrospective::format_retrospective_markdown;
```

## File Size Budget

- `retrospective.rs`: ~300-400 lines (header + 8 render helpers + collapse_findings + format_duration + CollapsedFinding struct + format_retrospective_markdown orchestrator). Under 500-line limit.
- `tools.rs` changes: ~15 lines net (1 field addition + ~12 lines dispatch logic change).
- `response/mod.rs` changes: 4 lines (module declaration + re-export).

## Open Questions

1. **Baseline sample count unavailable**: FR-06 says the Outliers heading should show `vs {N}-feature baseline`. However, `BaselineComparison` does not carry `sample_count` -- that field lives on `BaselineEntry` in `BaselineSet`, which is not passed to the formatter. The formatter should either omit the count from the heading or derive it from `history.len()` passed as an argument. Recommendation: omit the count (render `## Outliers` without the N) since adding a parameter would change the public signature beyond what the architecture specifies.
