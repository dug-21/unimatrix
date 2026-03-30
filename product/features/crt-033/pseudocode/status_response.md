# Pseudocode: status.rs Response Modifications

## Purpose

Modify `crates/unimatrix-server/src/mcp/response/status.rs` to add
`pending_cycle_reviews: Vec<String>` to `StatusReport`, `StatusReport::default()`,
`StatusReportJson`, and the `From<&StatusReport>` → `StatusReportJson` conversion.
Update both summary and JSON formatters (FR-09, FR-11).

Four code sites must be updated (Integration Risk I-04):
1. `StatusReport` struct field
2. `StatusReport::default()` initializer
3. `StatusReportJson` struct field
4. `From<&StatusReport> for StatusReportJson` mapping (or the inline construction)
Plus two formatter sites:
5. Summary formatter — renders list when non-empty
6. JSON formatter — includes field as array

---

## Modified: StatusReport Struct

Add the new field after `category_lifecycle` (the current last field):

```
pub struct StatusReport {
    // ... all existing fields unchanged ...

    /// Per-category lifecycle label (crt-031).
    pub category_lifecycle: Vec<(String, String)>,

    /// Cycle IDs within the K-window that have cycle_events rows
    /// (event_type='cycle_start') but no cycle_review_index row.
    /// Empty vec when all K-window cycles have been reviewed.
    /// Populated unconditionally by Phase 7b of compute_report() (C-07).
    /// (crt-033, FR-09)
    pub pending_cycle_reviews: Vec<String>,
}
```

---

## Modified: StatusReport::default()

Add initialization after `category_lifecycle: Vec::new()`:

```
impl Default for StatusReport {
    fn default() -> Self {
        StatusReport {
            // ... all existing fields ...
            category_lifecycle: Vec::new(),
            pending_cycle_reviews: Vec::new(),  // NEW
        }
    }
}
```

---

## Modified: StatusReportJson Struct

Add field after `category_lifecycle`:

```
#[derive(Serialize)]
struct StatusReportJson {
    // ... all existing fields ...

    /// Per-category lifecycle label (crt-031).
    category_lifecycle: std::collections::BTreeMap<String, String>,

    /// Cycles with cycle_start events but no stored review (crt-033).
    /// Empty array when no cycles are pending review.
    pending_cycle_reviews: Vec<String>,  // NEW
}
```

No `#[serde(skip_serializing_if)]` — always include the field in JSON output,
even as an empty array (FR-11: "JSON formatter MUST include pending_cycle_reviews
as an array field"). An empty array signals "no backlog" to readers.

---

## Modified: From<&StatusReport> for StatusReportJson

The `StatusReportJson` is constructed from a `&StatusReport` in the `Json`
format branch of `format_status_report`. Add the mapping for the new field.

```
// Locate the StatusReportJson construction block (ResponseFormat::Json branch).
// Add after the category_lifecycle mapping:

let json_report = StatusReportJson {
    // ... all existing field mappings ...
    category_lifecycle: report
        .category_lifecycle
        .iter()
        .map(|(cat, label)| (cat.clone(), label.clone()))
        .collect::<std::collections::BTreeMap<_, _>>(),
    pending_cycle_reviews: report.pending_cycle_reviews.clone(),  // NEW
};
```

Note: Verify whether `StatusReportJson` is constructed via a `From<&StatusReport>`
impl or inline in the `Json` match arm. The existing code at line ~789 shows:
`let json_report = StatusReportJson::from(report)` — check if a `From` impl
exists or if it is inline construction. If a `From` impl exists, add the field
there. If inline, add it in the match arm.

---

## Modified: Summary Formatter

In the `ResponseFormat::Summary` branch of `format_status_report`, after the
`category_lifecycle` (adaptive categories) block and before `CallToolResult::success`:

```
// crt-033: Pending cycle reviews — show when backlog is non-empty.
// Silent when empty (no line added — consistent with other "nothing to report"
// fields like graph cohesion when all discriminating metrics are zero).
if !report.pending_cycle_reviews.is_empty() {
    text.push_str(&format!(
        "\nPending cycle reviews: {}",
        report.pending_cycle_reviews.join(", ")
    ))
}
```

The label "Pending cycle reviews" is authoritative (FR-11 specifies this label).

---

## Modified: Markdown Formatter

In the `ResponseFormat::Markdown` branch, add a section after the Observation
Pipeline or after the Category Lifecycle section (implementation agent's
discretion on exact placement — logically near the observation/retrospective
section). Add before the `CallToolResult::success` at end of markdown branch:

```
// crt-033: Pending cycle reviews section.
if !report.pending_cycle_reviews.is_empty() {
    text.push_str("\n### Pending Cycle Reviews\n\n")
    text.push_str("Cycles with `cycle_start` events but no stored review:\n\n")
    for cycle_id in &report.pending_cycle_reviews {
        text.push_str(&format!("- {}\n", cycle_id))
    }
}
```

---

## JSON Formatter

No changes needed beyond the `StatusReportJson` struct field addition and the
mapping in the construction block. The `serde_json::to_string_pretty(&json_report)`
call will automatically include `pending_cycle_reviews` as an array. An empty
`Vec<String>` serializes as `[]` (FR-11 empty-vec case).

---

## Error Handling

| Scenario | Response |
|----------|----------|
| `pending_cycle_reviews` is empty | Summary: no line added; JSON: `"pending_cycle_reviews": []` |
| `pending_cycle_reviews` has entries | Summary: comma-joined list; JSON: array of strings |
| `StatusReport::default()` used (thin-shell maintenance_tick path) | `pending_cycle_reviews` = `Vec::new()` (existing default pattern) |

---

## Key Test Scenarios

1. `StatusReport::default()` initializes `pending_cycle_reviews` as empty vec (R-01).
2. Summary formatter: non-empty list → "Pending cycle reviews: cycle-a, cycle-b" in output.
3. Summary formatter: empty list → no "Pending cycle reviews" line (FR-11 empty case).
4. Markdown formatter: non-empty list → "### Pending Cycle Reviews" section present.
5. JSON formatter: non-empty list → `"pending_cycle_reviews": ["cycle-a", "cycle-b"]` in JSON.
6. JSON formatter: empty list → `"pending_cycle_reviews": []` in JSON (always-present field).
7. `StatusReportJson` serialization: field is always serialized (no `skip_serializing_if`).
8. Confirm three `StatusReport` struct sites are all updated: struct, Default, and formatter
   usage — if any is missed, the report will panic on access or show incorrect values
   (I-04 integration risk).
