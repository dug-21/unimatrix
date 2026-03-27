# Test Plan: format-output

Component: `format_status_report()` Summary and Markdown additions in
`crates/unimatrix-server/src/mcp/response/status.rs`

---

## Scope

Two unit tests for the Summary format conditional line and the Markdown `#### Graph Cohesion`
sub-section. Both tests call `format_status_report()` directly with a constructed `StatusReport`,
bypassing the MCP server and store layer entirely.

These tests cover AC-09, AC-10, and R-10.

---

## Test Infrastructure

```rust
// Both tests are in the existing #[cfg(test)] block in status.rs (or a new mod tests block
// if one does not already exist).
// No store, no tokio, no async — format_status_report is a pure synchronous function.
use crate::format_status_report;
use crate::StatusReport;
use unimatrix_core::ResponseFormat;
```

Helper: construct a `StatusReport` with all six cohesion fields populated:
```rust
fn make_report_with_cohesion() -> StatusReport {
    StatusReport {
        // Six cohesion fields set to non-zero values:
        graph_connectivity_rate: 0.75,
        isolated_entry_count: 2,
        cross_category_edge_count: 5,
        supports_edge_count: 3,
        mean_entry_degree: 1.5,
        inferred_edge_count: 4,
        // All other fields at default:
        ..StatusReport::default()
    }
}
```

---

## Test Functions

### test_format_summary_graph_cohesion_present

**Covers:** AC-09, R-10

**Purpose:** Verify the Summary format includes the graph cohesion line when
`isolated_entry_count + cross_category_edge_count + inferred_edge_count > 0`.

**Arrangement:** `make_report_with_cohesion()` — non-zero cohesion fields.

**Action:** `format_status_report(&report, ResponseFormat::Summary)`

**Assertions:**
- The returned text contains `"Graph cohesion:"` substring.
- The returned text contains `"75.0%"` (or `"75.0% connected"`) — connectivity rate formatted
  as percentage with one decimal place.
- The returned text contains `"2"` for `isolated_entry_count`.
- The returned text contains `"5"` for `cross_category_edge_count`.
- The returned text contains `"4"` for `inferred_edge_count` (optional per spec; included if
  the Summary format line ends with `"{} inferred"`).

**Example assertion:**
```rust
let text = get_text_content(&result);  // helper to extract text from CallToolResult
assert!(text.contains("Graph cohesion:"), "Summary must include graph cohesion line");
assert!(text.contains("75.0%"), "Connectivity rate must appear as percentage");
assert!(text.contains(" 2 ") || text.contains(" 2,") || text.ends_with(" 2"),
        "isolated_entry_count must appear");
```

**Conditional suppression sub-test:** Construct a `StatusReport::default()` (all six fields
at zero) and call `format_status_report` with Summary format. Assert that the text does NOT
contain `"Graph cohesion:"`. This confirms the conditional guard
(`isolated + cross_category + inferred > 0`) suppresses the line on empty store.

```rust
let empty_report = StatusReport::default();
let empty_result = format_status_report(&empty_report, ResponseFormat::Summary);
let empty_text = get_text_content(&empty_result);
assert!(!empty_text.contains("Graph cohesion:"),
        "Summary must omit graph cohesion line when all metrics are zero");
```

---

### test_format_markdown_graph_cohesion_section

**Covers:** AC-10

**Purpose:** Verify the Markdown format includes a `#### Graph Cohesion` sub-section with
all six metric labels.

**Arrangement:** `make_report_with_cohesion()`.

**Action:** `format_status_report(&report, ResponseFormat::Markdown)`

**Assertions:**
```rust
let text = get_text_content(&result);

// Sub-section header must be present inside the Coherence block
assert!(text.contains("#### Graph Cohesion"), "Markdown must include #### Graph Cohesion");

// All six metric labels must appear per AC-10
assert!(text.contains("Connectivity:"), "Missing Connectivity label");
assert!(text.contains("Isolated entries:"), "Missing Isolated entries label");
assert!(text.contains("Cross-category edges:"), "Missing Cross-category edges label");
assert!(text.contains("Supports edges:"), "Missing Supports edges label");
assert!(text.contains("Mean entry degree:"), "Missing Mean entry degree label");
assert!(text.contains("Inferred (NLI) edges:"), "Missing Inferred (NLI) edges label");

// Verify numeric values appear in the output
assert!(text.contains("75.0%"), "Connectivity percentage must appear in Markdown");
assert!(text.contains("2"), "isolated_entry_count must appear");
assert!(text.contains("5"), "cross_category_edge_count must appear");
assert!(text.contains("3"), "supports_edge_count must appear");
assert!(text.contains("1.50"), "mean_entry_degree must appear with 2 decimal places");
assert!(text.contains("4"), "inferred_edge_count must appear");

// Verify placement: #### Graph Cohesion must appear after ### Coherence
let coherence_pos = text.find("### Coherence").expect("### Coherence must exist");
let graph_cohesion_pos = text.find("#### Graph Cohesion").expect("#### Graph Cohesion must exist");
assert!(graph_cohesion_pos > coherence_pos,
        "#### Graph Cohesion must appear after ### Coherence block");
```

**Note on `#### Graph Cohesion` vs `### Graph Cohesion`:** The IMPLEMENTATION-BRIEF specifies
`#### Graph Cohesion` (four `#` — sub-subsection). The SPECIFICATION FR-13 states
`### Graph Cohesion` (three `#`). The ARCHITECTURE specifies `#### Graph Cohesion`.
The Architecture and Implementation Brief are the authoritative documents for implementation;
this test asserts `#### Graph Cohesion`. If the implementer uses `###`, the test must be
updated to match and the discrepancy documented.

---

## Edge Cases

### Summary with Only supports_edge_count > 0

Per the RISK-TEST-STRATEGY.md Integration Risks section: a store with only `supports_edge_count > 0`
and all other five fields at zero would suppress the Summary line (since the conditional is
`isolated + cross_category + inferred > 0`, and `supports` is not in that sum). This is an
accepted quirk — operators who only see Supports edges (no NLI, no isolation) get no Summary
annotation but do get the full Markdown sub-section.

This edge case does NOT require a unit test — it is a documented trade-off, not a bug. The
Markdown test covers it by always rendering the sub-section regardless of values.

### Connectivity Percentage Formatting

The format spec is `{:.1}%` — one decimal place. The test uses `graph_connectivity_rate = 0.75`
which should render as `"75.0%"`. Assert this exact string fragment to catch an implementation
that formats as `"75%"` (no decimal) or `"0.75"` (raw fraction without percent conversion).

---

## Coverage Summary

| AC-ID | Test | Coverage |
|-------|------|----------|
| AC-09 | `test_format_summary_graph_cohesion_present` | Summary line present with values |
| AC-09 | sub-test: empty report | Summary line absent when all zero |
| AC-10 | `test_format_markdown_graph_cohesion_section` | All six labels present, correct placement |
| R-10 | `test_format_summary_graph_cohesion_present` (zero sub-test) | Suppression confirmed |
