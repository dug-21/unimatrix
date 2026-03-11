# Component: handler-dispatch

## Purpose

Modify the `context_retrospective` handler to route between the markdown formatter (new default) and the JSON formatter (existing) based on `params.format`. The JSON path keeps the existing `evidence_limit` default of `unwrap_or(3)`. The markdown path ignores `evidence_limit` entirely.

## Location

`crates/unimatrix-server/src/mcp/tools.rs` -- modify the tail of the `context_retrospective` method (steps 11-12, after report is fully built).

## Modified Function: context_retrospective (tail section only)

Current code (lines ~1451-1461):
```rust
let evidence_limit = params.evidence_limit.unwrap_or(3);
if evidence_limit > 0 {
    let mut truncated = report.clone();
    for hotspot in &mut truncated.hotspots {
        hotspot.evidence.truncate(evidence_limit);
    }
    Ok(format_retrospective_report(&truncated))
} else {
    Ok(format_retrospective_report(&report))
}
```

New pseudocode:
```
// Determine output format (vnc-011)
let format = params.format.as_deref().unwrap_or("markdown")

match format {
    "markdown" => {
        // Markdown path: formatter controls its own evidence selection (k=3 by timestamp).
        // evidence_limit is irrelevant here.
        Ok(format_retrospective_markdown(&report))
    }
    "json" => {
        // JSON path: keep existing evidence_limit default of 3 (unchanged)
        let evidence_limit = params.evidence_limit.unwrap_or(3)
        if evidence_limit > 0 {
            let mut truncated = report.clone()
            for hotspot in &mut truncated.hotspots {
                hotspot.evidence.truncate(evidence_limit)
            }
            Ok(format_retrospective_report(&truncated))
        } else {
            Ok(format_retrospective_report(&report))
        }
    }
    _ => {
        // Unrecognized format: return error
        Err(rmcp::model::ErrorData::new(
            ERROR_INVALID_PARAMS,   // reuse existing error code for bad params
            format!("Unknown format '{}'. Valid values: \"markdown\", \"json\".", format),
            None,
        ))
    }
}
```

## Import Changes

Add to the use-block at the top of `context_retrospective`:
```
use crate::mcp::response::format_retrospective_markdown;
```

The existing `use crate::mcp::response::format_retrospective_report;` stays.

## Also: Cached report path

The cached report path (line ~1162) currently returns JSON unconditionally:
```rust
return Ok(format_retrospective_report(&report));
```

This must also respect the format parameter. Apply the same dispatch:
```
// Cached path also respects format (vnc-011)
let format = params.format.as_deref().unwrap_or("markdown")
match format {
    "markdown" => return Ok(format_retrospective_markdown(&report)),
    "json" => return Ok(format_retrospective_report(&report)),
    _ => return Err(/* same error as above */)
}
```

Note: The cached path does NOT apply evidence_limit truncation since the cached report has no observation data (hotspots is empty). So the simple dispatch without clone-and-truncate is correct.

## Error Handling

- Invalid format string: return `ErrorData` with the existing `ERROR_INVALID_PARAMS` code and a message listing valid values.
- All other errors in the handler are unchanged.

## Module Registration: response/mod.rs

Add after existing briefing lines:
```rust
#[cfg(feature = "mcp-briefing")]
mod retrospective;

#[cfg(feature = "mcp-briefing")]
pub use retrospective::format_retrospective_markdown;
```

## Key Test Scenarios

1. **Default format routes to markdown**: Call with no `format` param. Assert response text starts with `# Retrospective:`.
2. **Explicit "markdown" routes to markdown**: Call with `format: "markdown"`. Assert same as above.
3. **Explicit "json" routes to JSON**: Call with `format: "json"`. Assert response is valid JSON matching existing behavior.
4. **Unknown format returns error**: Call with `format: "xml"`. Assert error response with descriptive message.
5. **evidence_limit default 3 on JSON**: Call with `format: "json"` and no `evidence_limit`. Build report with 10 evidence records per hotspot. Assert evidence truncated to 3 in JSON output (unwrap_or(3) preserves existing behavior).
6. **evidence_limit explicit on JSON**: Call with `format: "json"` and `evidence_limit: 3`. Assert evidence truncated to 3.
7. **evidence_limit ignored on markdown**: Call with `format: "markdown"` and `evidence_limit: 1`. Assert markdown still shows k=3 examples per collapsed finding (formatter controls selection).
8. **Cached path respects format**: Trigger cached path (no observation data, existing MetricVector). Call with `format: "json"`. Assert JSON output. Call with no format. Assert markdown output.
