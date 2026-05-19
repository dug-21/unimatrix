# Component: response_format

## Purpose

FR-10 conditional text append. After `format_correct_success` returns a
`CallToolResult`, the redirect summary line is optionally appended to the first
`Content` text item in the result. This approach requires no change to
`format_correct_success`'s signature (preferred minimal-impact approach from the
architecture).

## File Location

`crates/unimatrix-server/src/mcp/tools.rs`

Replaces the current step 10 (`Ok(format_correct_success(...))`) with a two-step
pattern: build the result, conditionally append, then return.

## Authoritative Format Table (SPEC FR-10 + AC-17)

| Condition | Text appended |
|-----------|---------------|
| `redirect_summary == None` (found=0 OR query error) | No append — response unchanged |
| `found > 0`, `truncated == false`, `skipped == 0` | `"Redirected {redirected} incoming edges ({failed} failed, see logs)"` |
| `found > 0`, `truncated == false`, `skipped > 0` | `"Redirected {redirected} incoming edges ({skipped} skipped — invalid source, {failed} failed, see logs)"` |
| `truncated == true` | `"Redirected {redirected} incoming edges (truncated from {total_raw}, see logs)"` |

Where:
- `redirected` = count of `Ok(())` returns from `redirect_graph_edge`
- `failed` = count of `Err(_)` returns
- `skipped` = count of edges skipped due to Quarantined/Deprecated source
- `total_raw` = raw incoming count before ceiling truncation (truncation variant only)

Note: the `found == 0` gating is already enforced by the redirect loop returning
`None` when `incoming.is_empty()`. The response_format step only runs the append
path when `redirect_summary` is `Some(_)` with `found > 0`.

Note on AC-17: When all sources are Quarantined/Deprecated, `redirected == 0` and
`skipped == N`. The skipped-variant applies because `skipped > 0`. The text will
read `"Redirected 0 incoming edges (N skipped — invalid source, 0 failed, see logs)"`.
This is intentional — it signals that edges were found but all were invalid sources.

## Step 10 Pseudocode

```
// 10. Format response and optionally append redirect summary (vnc-017 FR-10).
let mut result = format_correct_success(
    &correct_result.deprecated_original,
    &correct_result.corrected_entry,
    ctx.format,
);

if let Some(summary) = redirect_summary {
    // Build redirect summary text per FR-10 authoritative format table.
    let summary_text = if summary.truncated {
        // Truncation variant: total_raw is the raw count before ceiling cap.
        format!(
            "Redirected {} incoming edges (truncated from {}, see logs)",
            summary.redirected,
            summary.total_raw
        )
    } else if summary.skipped > 0 {
        // Skipped-count variant: some sources were Quarantined or Deprecated.
        format!(
            "Redirected {} incoming edges ({} skipped — invalid source, {} failed, see logs)",
            summary.redirected,
            summary.skipped,
            summary.failed
        )
    } else {
        // Normal variant: no truncation, no skipped sources.
        format!(
            "Redirected {} incoming edges ({} failed, see logs)",
            summary.redirected,
            summary.failed
        )
    };

    // Append to the first Content text item (all format_correct_success variants
    // produce exactly one Content::text item).
    if let Some(content) = result.content.first_mut() {
        // Append on a new line to separate from the existing response body.
        content.raw.text.push('\n');
        content.raw.text.push_str(&summary_text);
    }
}

Ok(result)
```

## Notes on Content Mutation

`format_correct_success` always returns `CallToolResult::success(vec![Content::text(...)])`.
The result has exactly one `Content` item. `content.first_mut()` is guaranteed to find it.

`Content::text` in rmcp wraps a `RawContent` struct. The exact field path to the
string depends on the rmcp version (`content.raw.text` or similar). The implementer
must inspect the rmcp model to confirm the mutation path. If direct field mutation is
not possible (e.g., `Content` is not pub-mutable), the alternative is to reconstruct
the `CallToolResult`:

```
// Alternative if Content fields are not directly mutable:
let existing_text = result.content
    .into_iter()
    .next()
    .and_then(|c| c.as_text().map(|s| s.to_string()))
    .unwrap_or_default();
let combined = format!("{}\n{}", existing_text, summary_text);
result = CallToolResult::success(vec![Content::text(combined)]);
```

The implementer should choose the approach that compiles cleanly against the actual
rmcp API. Both produce identical output.

## Data Flow

- Input: `redirect_summary: Option<RedirectSummary>` from the redirect loop (step 8c)
- Input: `CallToolResult` from `format_correct_success` (existing step 10)
- Output: `Ok(CallToolResult)` — same structure, with optional text appended
- Side effects: none

## Error Handling

This component has no error paths. String formatting is infallible. The `first_mut()`
guard protects against an empty `content` vec (which cannot occur from
`format_correct_success` in practice, but is defensive).

## Key Test Scenarios

**AC-11 — Zero-edge path produces unchanged response (unit test)**
- Call `context_correct` on an entry with no incoming edges
- Assert: response text does not contain "Redirected"
- Assert: no `tracing::info!` summary log emitted

**AC-12 — Normal variant format (integration test)**
- Seed 2 incoming Prerequisite edges, both with Active sources
- Call `context_correct`
- Assert: response text contains exactly
  `"Redirected 2 incoming edges (0 failed, see logs)"`

**AC-13 — Partial failure format (unit test with stub)**
- Stub 1 of 2 edges to return `Err(EdgeRedirectError::TransactionError(...))`
- Assert: response text contains `"Redirected 1 incoming edges (1 failed, see logs)"`

**AC-17 — All-skipped variant (unit test)**
- Seed 3 incoming edges, all sources Quarantined
- Call `context_correct`
- Assert: response text contains
  `"Redirected 0 incoming edges (3 skipped — invalid source, 0 failed, see logs)"`
- Assert: `failed == 0`

**R-05 — Truncation variant format (unit test)**
- Seed 55 incoming edges (all Active sources)
- Call `context_correct`
- Assert: response text contains
  `"Redirected 50 incoming edges (truncated from 55, see logs)"`

**R-11 — Format assertion in integration test (integration test)**
- After AC-06 flow (2 Prerequisite edges redirected)
- Assert: actual MCP `CallToolResult` response body contains
  `"Redirected 2 incoming edges"` as a substring (not just unit-stub assertion)

**AC-11 / query_error path — No append when query fails**
- Inject pool error on `query_incoming_edges`
- Assert: response text is identical to current `context_correct` output
  (no "Redirected" substring)
