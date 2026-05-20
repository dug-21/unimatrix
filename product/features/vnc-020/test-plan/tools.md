# Test Plan: tools.rs — Tool Description Update

Component: `crates/unimatrix-server/src/mcp/tools.rs`
Responsibility: Extend `context_graph` tool description to cover inverse, filter, and path
modes. Mandatory staleness disclosure for path mode. AND semantics example for inverse mode.
No logic changes.

---

## AC-19 — Manual Inspection Checklist

**Verification method**: Manual code review inspection of the `context_graph` tool
description string in `tools.rs`. This is a non-automated check — it is a required gate
item per the Risk Strategy (R-01: Critical).

### Required Phrases to Confirm Present in tools.rs

Inspect the string assigned to the `context_graph` tool description. Confirm ALL of
the following phrases are present (exact substring match):

1. `"cache is rebuilt each tick (typically 30-60 seconds)"`
   — Required verbatim. The "30-60 seconds" range and "tick" terminology are mandatory.

2. `"{ found: false }"`
   — Must appear in the path mode section explaining that not-in-snapshot is not an error.
   Acceptable variants: `"{ found: false, hops: [], length: 0 }"` or shorter form.

3. `"not an error"`
   — Must appear near the `found: false` disclosure (either "not an error" or "not
   an error — not an error code").

4. `"in-memory graph cache"` or `"in-memory"`
   — Must describe BFS traversal as operating on an in-memory structure.

5. `"resolve_supersessions=true"`
   — Must appear in the path mode section describing endpoint resolution.

6. `"inverse"` and `"filter"` mode descriptions present
   — Both modes must have documentation text in the tool description.

7. AND semantics example for inverse mode
   — Must state that `missing_edge_types` uses AND semantics (entries missing ALL listed
   types). Example phrase: `"entries missing ALL"` or `"missing all specified"`.
   This satisfies R-05 (ADR-003 requirement).

### Required Phrases to Confirm ABSENT From inverse/filter Descriptions

Confirm the following phrases do NOT appear in the description sections for `inverse` or
`filter` modes (these modes are live-DB and must not be described with staleness language):

- `"tick"` — must not appear in the inverse mode or filter mode description block.
- `"cache"` — must not appear in the inverse mode or filter mode description block.
- `"staleness"` — must not appear for these modes.

Exception: it is acceptable for the overall `context_graph` description to mention that
some modes use the live database and are not subject to tick-window lag, provided this
distinction is clear.

---

## Note on Automated Testing

AC-19 is designated as manual inspection only. There is no automated test for the tool
description string. The integration test suite does NOT verify description content.

However, if the implementation team desires an automated regression guard, a unit test
may be added:

```rust
#[test]
fn test_context_graph_tool_description_contains_staleness_disclosure() {
    let desc = get_context_graph_tool_description(); // extract the string
    assert!(desc.contains("cache is rebuilt each tick (typically 30-60 seconds)"),
        "Staleness disclosure missing required text");
    assert!(desc.contains("found: false"),
        "Not-found-is-not-error disclosure missing");
}
```

This is optional but recommended for regression prevention. If added, it must live in
`tools.rs` test module.

---

## Not in Scope for tools.rs Tests

- Logic testing — tools.rs contains no handler logic for vnc-020.
- Capability gate — already tested by the existing infra-001 security suite.
- Tool registration count — already tested by `test_list_tools_returns_14`.
