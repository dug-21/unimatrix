# format-output — Pseudocode

Component: `format_status_report` Summary + Markdown additions
File: `crates/unimatrix-server/src/mcp/response/status.rs`

---

## Purpose

Add two output blocks to `format_status_report`:

1. **Summary path** — a conditional one-line `"Graph cohesion: ..."` string appended
   after the existing `graph_compacted` / `graph_stale_ratio` block.
2. **Markdown path** — a `#### Graph Cohesion` sub-section inserted inside the
   existing `### Coherence` block, after the `graph_compacted` line (~line 435).

Both blocks read the six new `StatusReport` fields added by the `status-report-fields`
component. No computation happens here — display only.

---

## Summary Path Addition

### Insertion point

Immediately after the existing `graph_compacted` conditional block in
`ResponseFormat::Summary`:

```
// Existing code (unchanged, shown for orientation):
if report.graph_compacted {
    text.push_str("\nGraph compacted: yes");
}

// <<<< INSERT HERE >>>>

for rec in &report.maintenance_recommendations {
    // ... existing code continues
```

### New block

```
// Graph cohesion summary (col-029)
// Condition: suppress when all six metrics are zero (empty/bootstrap-only store)
// to avoid a misleading "0% connected, 0 isolated" line on a fresh deployment.
// The condition mirrors the existing graph_stale_ratio > 0.0 guard pattern.
//
// Note: a store with only supports_edge_count > 0 and others at zero will suppress
// the summary line (R-10 edge case, accepted trade-off — Markdown always shows the
// sub-section regardless).
if report.isolated_entry_count > 0
    || report.cross_category_edge_count > 0
    || report.inferred_edge_count > 0
{
    text.push_str(&format!(
        "\nGraph cohesion: {:.1}% connected, {} isolated, {} cross-category, {} inferred",
        report.graph_connectivity_rate * 100.0,
        report.isolated_entry_count,
        report.cross_category_edge_count,
        report.inferred_edge_count,
    ));
}
```

---

## Markdown Path Addition

### Insertion point

Inside `ResponseFormat::Markdown`, within the `### Coherence` block.
Specifically, after the `"Graph compacted: {}\n"` push (~line 435) and before the
`#### Maintenance Recommendations` conditional block (~line 437).

```
// Existing code (unchanged, shown for orientation):
text.push_str(&format!(
    "Graph compacted: {}\n",
    if report.graph_compacted { "yes" } else { "no" }
));

// <<<< INSERT HERE >>>>

if !report.maintenance_recommendations.is_empty() {
    text.push_str("\n#### Maintenance Recommendations\n\n");
    // ... existing code continues
```

### New block

```
// Graph Cohesion sub-section (col-029)
// Always present inside ### Coherence. Shows all six metrics regardless of
// whether they are zero — operators on a fresh store see the sub-section header
// and know the feature is active (mitigates R-10 for the Markdown format).
text.push_str("\n#### Graph Cohesion\n");
text.push_str(&format!(
    "- Connectivity: {:.1}% ({}/{} active entries connected)\n",
    report.graph_connectivity_rate * 100.0,
    // connected_entry_count is derived: total_active - isolated
    report.total_active.saturating_sub(report.isolated_entry_count),
    report.total_active,
));
text.push_str(&format!(
    "- Isolated entries: {}\n",
    report.isolated_entry_count,
));
text.push_str(&format!(
    "- Cross-category edges: {}\n",
    report.cross_category_edge_count,
));
text.push_str(&format!(
    "- Supports edges: {}\n",
    report.supports_edge_count,
));
text.push_str(&format!(
    "- Mean entry degree: {:.2}\n",
    report.mean_entry_degree,
));
text.push_str(&format!(
    "- Inferred (NLI) edges: {}\n",
    report.inferred_edge_count,
));
```

---

## Connectivity Count Display Note

The Markdown sub-section shows `{connected}/{total_active} active entries connected`
for the Connectivity line. `connected_entry_count` is not stored in `StatusReport`
directly — only `isolated_entry_count` and `total_active` are available. The connected
count is re-derived as `total_active.saturating_sub(isolated_entry_count)`.

This is correct because:
- `isolated_entry_count = active - connected` (computed in `compute_graph_cohesion_metrics`)
- Therefore `connected = active - isolated`
- `total_active` in `StatusReport` is populated from Phase 1 (`compute_status_aggregates`)
  which counts all entries with any status, but the connectivity metrics use
  `entries.status = 0` only.

Caveat: if `total_active` (Phase 1) and the `active_entry_count` from Query 2 differ
(e.g., due to WAL snapshot lag between the two queries), the display arithmetic may
show a slightly inconsistent fraction. This is a cosmetic inconsistency, not a data
corruption, and is an accepted consequence of reading from two separate SQL snapshots.
The displayed fraction is informational.

---

## Spec Alignment Note

The SPECIFICATION FR-13 uses `### Graph Cohesion` as the heading. The IMPLEMENTATION
BRIEF and ARCHITECTURE both use `#### Graph Cohesion` (sub-section within `### Coherence`).
The four-hash heading is correct — it is a sub-section of Coherence, not a new
top-level section. Use `####`.

---

## Data Flow

```
StatusReport fields read by format-output:
  - report.graph_connectivity_rate    (f64)
  - report.isolated_entry_count       (u64)
  - report.cross_category_edge_count  (u64)
  - report.supports_edge_count        (u64)
  - report.mean_entry_degree          (f64)
  - report.inferred_edge_count        (u64)
  - report.total_active               (u64)  [existing, for connected count display]

No fields are written — this component is read-only over StatusReport.
```

---

## Error Handling

No error paths. All six fields are `f64` or `u64` — they format without error.
If the service call site left them at default (`0` / `0.0`), the Markdown sub-section
displays zero values cleanly. The Summary line is suppressed when all discriminating
fields are zero (see condition above).

---

## Key Test Scenarios

### Summary line present on non-empty store (AC-09, R-10)

```
Setup: StatusReport with inferred_edge_count=5, isolated_entry_count=2,
       cross_category_edge_count=3, graph_connectivity_rate=0.7

Call: format_status_report(&report, ResponseFormat::Summary)

Assert:
  - Output contains "Graph cohesion:"
  - Output contains "70.0% connected"
  - Output contains "2 isolated"
  - Output contains "3 cross-category"
  - Output contains "5 inferred"
```

### Summary line absent on all-zero store

```
Setup: StatusReport::default() (all six fields zero)

Call: format_status_report(&report, ResponseFormat::Summary)

Assert:
  - Output does NOT contain "Graph cohesion:"
```

### Markdown sub-section always present (AC-10)

```
Setup: StatusReport::default() (all six fields zero)

Call: format_status_report(&report, ResponseFormat::Markdown)

Assert:
  - Output contains "#### Graph Cohesion"
  - Output contains "- Connectivity:"
  - Output contains "- Isolated entries:"
  - Output contains "- Cross-category edges:"
  - Output contains "- Supports edges:"
  - Output contains "- Mean entry degree:"
  - Output contains "- Inferred (NLI) edges:"
```

### Markdown sub-section is inside ### Coherence block

```
Setup: any StatusReport

Call: format_status_report(&report, ResponseFormat::Markdown)

Assert:
  - "### Coherence" appears before "#### Graph Cohesion" in the output string
  - "#### Graph Cohesion" appears before "### Co-Access Patterns" in the output string
  (the sub-section is nested, not a peer-level section)
```

### Connectivity display arithmetic

```
Setup: StatusReport with total_active=10, isolated_entry_count=3

Expected connected display: 10 - 3 = 7
Assert output contains "7/10 active entries connected"
```

---

## Knowledge Stewardship

- Queried: /uni-query-patterns — found #298 (Generic Formatter Pattern), #307 (Response Formatting Convention). Both confirm the push_str + format! pattern used throughout format_status_report.
- Deviations from established patterns: none. Follows existing conditional push_str pattern for Summary path and unconditional sub-section pattern (e.g., "#### Top Co-Access Clusters") for Markdown path.
