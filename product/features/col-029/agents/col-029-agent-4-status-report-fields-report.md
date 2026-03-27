# Agent Report: col-029-agent-4-status-report-fields

Feature: col-029 — Graph Cohesion Metrics (GH #413)
Component: status-report-fields + format-output

---

## Work Completed

### Files Modified

- `crates/unimatrix-server/src/mcp/response/status.rs` — primary file
  - Added six graph cohesion fields to `StatusReport` struct after `graph_compacted: bool`
  - Added all six fields to hand-written `StatusReport::default()` (R-04 satisfied)
  - Added six fields to `StatusReportJson` struct
  - Added six field mappings in `From<&StatusReport> for StatusReportJson`
  - Summary format: conditional graph cohesion one-liner (suppressed when isolated + cross_category + inferred all zero)
  - Markdown format: `#### Graph Cohesion` sub-section always present inside `### Coherence` block
  - Added unit tests: `test_status_report_default_cohesion_fields`, `test_format_summary_graph_cohesion_present`, `test_format_summary_graph_cohesion_absent`, `test_format_markdown_graph_cohesion_section`

- `crates/unimatrix-server/src/services/status.rs` — compile fix
  - Added six new fields (all zero defaults) to the full `StatusReport` struct literal in `compute_report()` (Phase 5 call site handled by service-call-site agent separately)

- `crates/unimatrix-server/src/mcp/response/mod.rs` — compile fix (test code)
  - Added six fields to all 8 full `StatusReport` struct literals in the test module

---

## Tests

| Test | Result |
|------|--------|
| `test_status_report_default_cohesion_fields` | pass |
| `test_format_summary_graph_cohesion_present` | pass |
| `test_format_summary_graph_cohesion_absent` | pass |
| `test_format_markdown_graph_cohesion_section` | pass |
| `cargo test -p unimatrix-server` | 2163 passed, 0 failed |
| `cargo build --workspace` | 0 errors |
| `cargo test --workspace` (excluding pre-existing flaky) | 0 new failures |

Pre-existing failures (unrelated to this work):
- `download::tests::test_ensure_model_creates_directory` — network/model download
- `uds::listener::tests::col018_long_prompt_truncated` — flaky under parallel test suite, passes in isolation

---

## Issues / Deviations

None. Implementation follows pseudocode exactly. All six fields placed after `graph_compacted` as specified.

The `services/status.rs` struct literal required the six new fields to compile (not documented in the brief as a separate change, but necessary to fix compile errors caused by adding fields to `StatusReport`). This is within scope as a consequence of the struct modification.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for unimatrix-server status report format — found #952, #298, #276, #307, #94. Results confirmed push_str+format! pattern and generic formatter conventions. Applied: used identical push_str pattern throughout.
- Stored: entry #3603 "StatusReport struct literal locations — three files require updates when adding fields" via /uni-store-pattern. This is a gotcha: `mcp/response/mod.rs` contains 8 full StatusReport struct literals in test code that all produce compile errors when new fields are added. The pseudocode and brief did not mention this. Pattern documents the three files and suggests replace_all mitigation.
