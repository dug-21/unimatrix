# status-report-fields — Pseudocode

Component: `StatusReport` six new fields
File: `crates/unimatrix-server/src/mcp/response/status.rs`

---

## Purpose

Append six cohesion metric fields to the `StatusReport` struct and add matching
entries to the hand-written `StatusReport::default()` impl. This is a purely
additive struct change — no existing fields are removed or reordered.

`StatusReport` has no `#[derive(Default)]` — the `Default` impl is hand-written.
Omitting any of the six new fields from `default()` is a compile error (R-04).

---

## Struct Modification

```
// File: crates/unimatrix-server/src/mcp/response/status.rs
// Location: inside `pub struct StatusReport { ... }`
// Insertion point: after `pub graph_compacted: bool` (the last graph-adjacent field)

    // --- Graph Cohesion Metrics (col-029) ---

    /// Fraction of active entries with at least one non-bootstrap edge. Range [0.0, 1.0].
    /// 0.0 when no active entries exist or when compute_graph_cohesion_metrics() fails.
    pub graph_connectivity_rate: f64,

    /// Active entries with zero non-bootstrap edges on either endpoint.
    /// Complement of connected_entry_count: total_active - connected_active.
    pub isolated_entry_count: u64,

    /// Non-bootstrap edges where both active endpoints have different category values.
    /// Excludes edges where either endpoint is deprecated or quarantined.
    pub cross_category_edge_count: u64,

    /// Non-bootstrap edges with relation_type = 'Supports'.
    pub supports_edge_count: u64,

    /// Average in+out degree across active entries: (2 * non_bootstrap_edges) / active_entries.
    /// 0.0 when no active entries exist or when compute_graph_cohesion_metrics() fails.
    pub mean_entry_degree: f64,

    /// Non-bootstrap edges with source = 'nli' (NLI-inferred edges from GH #412).
    pub inferred_edge_count: u64,
```

---

## Default Impl Modification

```
// File: crates/unimatrix-server/src/mcp/response/status.rs
// Location: inside `impl Default for StatusReport { fn default() -> Self { StatusReport { ... } } }`
// Insertion point: after `graph_compacted: false,`

            // --- Graph Cohesion Metrics (col-029) ---
            graph_connectivity_rate: 0.0,
            isolated_entry_count: 0,
            cross_category_edge_count: 0,
            supports_edge_count: 0,
            mean_entry_degree: 0.0,
            inferred_edge_count: 0,
```

---

## Ordering Note

The six fields are appended after `graph_compacted: bool` in both the struct body
and the `default()` block. The struct ordering in `default()` must exactly mirror
the struct body ordering — Rust requires all fields to be present but does not
require them in declaration order; however, keeping them aligned prevents reviewer
confusion and makes auditing against R-04 straightforward.

Existing fields are unchanged. No field is removed or renamed.

---

## Error Handling

This component introduces no runtime error paths. All fields default to zero / 0.0.
The service call site handles the error case — on `Err`, it leaves the fields at
their default values. No additional logic is required here.

---

## Key Test Scenarios

### Compile check (R-04)

```
Verification: cargo check -p unimatrix-server

If any of the six fields is missing from StatusReport::default(), the compiler
emits "missing field `<name>` in initializer of `StatusReport`". This is the
primary verification for this component.
```

### Default value assertions (R-04, AC-12)

```
Construct: let report = StatusReport::default()

Assert all six fields at zero:
  - report.graph_connectivity_rate  == 0.0
  - report.isolated_entry_count     == 0
  - report.cross_category_edge_count == 0
  - report.supports_edge_count      == 0
  - report.mean_entry_degree        == 0.0
  - report.inferred_edge_count      == 0
```

These assertions can live in a unit test in the same file, or as part of a
`#[test]` block that constructs a default report and verifies no field is
accidentally set to a non-zero sentinel.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns — found #3544 (col-028 cascading struct field compile cycles) and #704 (StatusAggregates precedent). R-04 risk rating elevated from medium to medium-high on basis of #3544.
- Deviations from established patterns: none. Follows the established field-append pattern used by col-028 and crt-013.
