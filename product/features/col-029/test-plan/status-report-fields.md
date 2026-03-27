# Test Plan: status-report-fields

Component: `StatusReport` six new fields + `StatusReport::default()` in
`crates/unimatrix-server/src/mcp/response/status.rs`

---

## Scope

Compile-time verification of all six new fields being present in `StatusReport` and in the
hand-written `Default` impl. Plus one runtime unit test asserting the six default values.

This component primarily covers R-04 (Medium: missing field in `default()`) and R-09 (Medium:
`EDGE_SOURCE_NLI` re-export). Both risks are fundamentally compile-time concerns, with one
runtime unit test providing explicit assertion documentation.

---

## Compile-Time Verification (AC-12, R-04)

`StatusReport` uses a hand-written `Default` impl with no `#[derive(Default)]`. Omitting any
of the six new fields from either the struct definition or the `default()` block is a
**compile error** caught by `cargo check -p unimatrix-server`.

**Verification command (run at Gate 3c):**
```bash
cargo check -p unimatrix-server
```

Must pass with no errors. This confirms:
- All six fields are present in the struct definition.
- All six fields are present in `StatusReport::default()`.
- The `StatusReportJson` struct (also in `status.rs`) has the corresponding six fields.
- The `From<&StatusReport> for StatusReportJson` impl maps all six fields.

**Grep verification (AC-01, AC-12):**
```bash
grep -E "graph_connectivity_rate|isolated_entry_count|cross_category_edge_count|supports_edge_count|mean_entry_degree|inferred_edge_count" \
  crates/unimatrix-server/src/mcp/response/status.rs
```
All six names must appear in the struct definition, the `default()` block, and the format
output functions.

---

## Unit Test

### test_status_report_default_cohesion_fields

**Covers:** AC-12, R-04

**Location:** `crates/unimatrix-server/src/mcp/response/status.rs` `#[cfg(test)]` block

**Arrangement:** Construct `StatusReport::default()` — no setup required.

**Action:** Access each of the six new fields.

**Assertions:**
```rust
let r = StatusReport::default();
assert_eq!(r.graph_connectivity_rate, 0.0_f64);
assert_eq!(r.isolated_entry_count, 0_u64);
assert_eq!(r.cross_category_edge_count, 0_u64);
assert_eq!(r.supports_edge_count, 0_u64);
assert_eq!(r.mean_entry_degree, 0.0_f64);
assert_eq!(r.inferred_edge_count, 0_u64);
```

This test serves as living documentation of the expected default values and will fail if any
field is missing from the `default()` impl (compile error before runtime) or has an incorrect
initial value.

---

## EDGE_SOURCE_NLI Re-Export Verification (R-09, AC-01)

`EDGE_SOURCE_NLI` is defined in `crates/unimatrix-store/src/read.rs` and must be re-exported
from `crates/unimatrix-store/src/lib.rs`.

**Grep check (run at Gate 3c):**
```bash
grep "EDGE_SOURCE_NLI" crates/unimatrix-store/src/lib.rs
```
Must return a `pub use` line (e.g., `pub use crate::read::EDGE_SOURCE_NLI;` or equivalent).

**Compile check:**
If the server crate imports `unimatrix_store::EDGE_SOURCE_NLI`, `cargo check -p unimatrix-server`
failing would reveal a missing re-export. The constant is used in `compute_graph_cohesion_metrics()`
in `read.rs` — if the function body references `EDGE_SOURCE_NLI` and it is defined in the same
file, the re-export is the only remaining check.

---

## StatusReportJson Completeness (implicit compile coverage)

`StatusReportJson` in `status.rs` is an intermediate serializable struct used for JSON output.
It must also contain the six new fields and a mapping in `From<&StatusReport>`. This is not
a separate test — it is verified transitively by:
1. `cargo check -p unimatrix-server` (compile).
2. `test_status_all_formats` in infra-001 `test_tools.py` which calls `context_status` with
   `format="json"` and asserts success — a missing field in `StatusReportJson` or `From` impl
   would cause a serde serialization failure or compile error.

---

## Field Type and Ordering

Per the IMPLEMENTATION-BRIEF, the six fields are appended after `graph_compacted: bool`:

| Field | Type | Default |
|-------|------|---------|
| `graph_connectivity_rate` | `f64` | `0.0` |
| `isolated_entry_count` | `u64` | `0` |
| `cross_category_edge_count` | `u64` | `0` |
| `supports_edge_count` | `u64` | `0` |
| `mean_entry_degree` | `f64` | `0.0` |
| `inferred_edge_count` | `u64` | `0` |

The unit test must assert the exact default values above, not `Default::default()` shorthands,
to make the expected values explicit to future readers.
