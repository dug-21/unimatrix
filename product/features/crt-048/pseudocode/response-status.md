# Component C — mcp/response/status.rs

## Purpose

Status report data types and formatting. Contains `StatusReport` struct, `StatusReportJson`
struct, `Default` impl, `From<&StatusReport>` impl, and three format branches (Summary,
Markdown, JSON). After crt-048 two fields are removed from both structs and all their
downstream format references.

---

## Struct: StatusReport — Remove Two Fields (FR-08, AC-06)

### Fields to delete from struct definition (lines ~54-63)

```
CURRENT:
    /// Confidence freshness dimension score.
    pub confidence_freshness_score: f64,         // line ~55 — DELETE
    ...
    /// Number of entries with stale confidence.
    pub stale_confidence_count: u64,             // line ~63 — DELETE
```

Action: Delete both field declarations (field name, type, and doc comment). The struct
retains all other coherence fields: `coherence: f64`, `graph_quality_score: f64`,
`embedding_consistency_score: f64`, `contradiction_density_score: f64`,
`confidence_refreshed_count: u64`, `graph_stale_ratio: f64`, `graph_compacted: bool`.

---

## Impl Default for StatusReport — Remove Two Fields (lines ~172, ~176)

### Current Default impl (relevant lines)
```rust
impl Default for StatusReport {
    fn default() -> Self {
        StatusReport {
            ...
            coherence: 1.0,
            confidence_freshness_score: 1.0,     // line ~172 — DELETE
            graph_quality_score: 1.0,
            embedding_consistency_score: 1.0,
            contradiction_density_score: 1.0,
            stale_confidence_count: 0,           // line ~176 — DELETE
            confidence_refreshed_count: 0,
            ...
        }
    }
}
```

Action: Remove both field initializer lines from the struct literal. The Default impl
spans to ~line 208. Remove only `confidence_freshness_score: 1.0` and
`stale_confidence_count: 0`.

---

## Struct: StatusReportJson — Remove Two Fields (lines ~848, ~852)

### Current struct definition (relevant lines)
```rust
struct StatusReportJson {
    ...
    coherence: f64,
    confidence_freshness_score: f64,     // line ~848 — DELETE
    graph_quality_score: f64,
    embedding_consistency_score: f64,
    contradiction_density_score: f64,
    stale_confidence_count: u64,         // line ~852 — DELETE
    confidence_refreshed_count: u64,
    ...
}
```

Action: Delete both field declarations from `StatusReportJson`. JSON serialization is
automatic via `#[derive(Serialize)]` — no other change needed for the JSON output path
beyond removing the struct fields.

---

## Impl From<&StatusReport> for StatusReportJson — Remove Two Assignments (lines ~1622, ~1626)

### Current From impl (relevant lines in the StatusReportJson { ... } literal)
```rust
StatusReportJson {
    ...
    coherence: r.coherence,
    confidence_freshness_score: r.confidence_freshness_score,   // line ~1622 — DELETE
    graph_quality_score: r.graph_quality_score,
    embedding_consistency_score: r.embedding_consistency_score,
    contradiction_density_score: r.contradiction_density_score,
    stale_confidence_count: r.stale_confidence_count,           // line ~1626 — DELETE
    confidence_refreshed_count: r.confidence_refreshed_count,
    ...
}
```

Action: Remove both field-assignment lines from the `StatusReportJson { ... }` literal
inside the `From` impl. The remaining assignments are unchanged.

Risk R-08: If the `From` impl is not updated after removing the struct fields, the build
fails. There is no subtle silent-failure path here — the field removal in the struct
definition makes any retained assignment a compile error.

---

## format_status_report() — Summary Branch Changes (lines ~243-407)

### Change 1: Coherence line — remove confidence_freshness component (lines ~258-265)

**Current code:**
```rust
text.push_str(&format!(
    "\nCoherence: {:.4} (confidence_freshness: {:.4}, graph_quality: {:.4}, embedding_consistency: {:.4}, contradiction_density: {:.4})",
    report.coherence,
    report.confidence_freshness_score,
    report.graph_quality_score,
    report.embedding_consistency_score,
    report.contradiction_density_score,
));
```

**Post-crt-048 code:**
```rust
text.push_str(&format!(
    "\nCoherence: {:.4} (graph_quality: {:.4}, embedding_consistency: {:.4}, contradiction_density: {:.4})",
    report.coherence,
    report.graph_quality_score,
    report.embedding_consistency_score,
    report.contradiction_density_score,
));
```

Action: Remove `confidence_freshness: {:.4}, ` from the format string and remove
`report.confidence_freshness_score,` from the arguments.

### Change 2: Stale confidence count conditional block — DELETE (lines ~266-271)

**Current code:**
```rust
if report.stale_confidence_count > 0 {
    text.push_str(&format!(
        "\nStale confidence: {} entries",
        report.stale_confidence_count
    ));
}
```

**Post-crt-048 action:** Delete the entire `if` block (5 lines). The `stale_confidence_count`
field no longer exists on `StatusReport`. The build would fail if this block is retained.

---

## format_status_report() — Markdown Branch Changes (lines ~408-821)

### Change 1: Confidence Freshness bullet — DELETE (lines ~508-511)

**Current code in the `### Coherence` section:**
```rust
text.push_str(&format!(
    "- **Confidence Freshness**: {:.4}\n",
    report.confidence_freshness_score
));
```

**Post-crt-048 action:** Delete these 3 lines entirely.

The `### Coherence` section retains:
- `- **Lambda**: {:.4}` (unchanged)
- `- **Graph Quality**: {:.4}` (unchanged)
- `- **Embedding Consistency**: {:.4}` (unchanged)
- `- **Contradiction Density**: {:.4}` (unchanged)

### Change 2: Stale confidence entries line — DELETE (lines ~524-527)

**Current code:**
```rust
text.push_str(&format!(
    "Stale confidence entries: {}\n",
    report.stale_confidence_count
));
```

**Post-crt-048 action:** Delete these 3 lines entirely. The field no longer exists.

The Markdown branch retains the `Confidence refreshed: {}\n` line immediately after
(uses `report.confidence_refreshed_count` which is a surviving field).

---

## format_status_report() — JSON Branch

No changes required beyond the struct field removal. The JSON branch delegates to
`StatusReportJson::from(report)` and `serde_json::to_string_pretty(...)`. Once the two
fields are removed from `StatusReportJson`, they are automatically absent from JSON output.
No manual JSON key removal is needed.

---

## Sequence Constraint

Component C's changes are compile-prerequisites for Component D. The `StatusReport` struct
definition must have the two fields removed before any attempt to compile `mcp/response/mod.rs`,
which constructs `StatusReport` literals that will reference those fields. In practice, C
and D must be updated in the same build attempt.

---

## Error Handling

No error handling changes. `format_status_report()` returns `CallToolResult` and does not
produce errors. The only defensive path removed is the `stale_confidence_count > 0` check
in the Summary branch, which was a conditional format — not an error path.

---

## Key Test Scenarios

1. JSON output for a synthetic `StatusReport` does NOT contain `confidence_freshness_score`
   or `stale_confidence_count` keys (R-05 scenario 1, AC-06).
2. Summary format for a healthy report contains a coherence line with exactly 3 dimension
   components (graph, embedding, contradiction) and no `confidence_freshness:` substring.
3. Summary format does not contain `Stale confidence:` line for any `StatusReport`
   (the conditional block no longer exists).
4. Markdown format `### Coherence` section does not contain `**Confidence Freshness**` bullet.
5. Markdown format does not contain `Stale confidence entries:` line.
6. `StatusReport::default()` constructs without `confidence_freshness_score` or
   `stale_confidence_count` fields (compilation is the gate).
