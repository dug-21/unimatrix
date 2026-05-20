# Test Plan: graph_read.rs — Wire Types, Dispatch, Centralized Validation

Component: `crates/unimatrix-server/src/mcp/graph_read.rs`
Responsibility: `GraphParams` wire struct, response envelopes, `handle_graph` dispatch,
`validate_no_unsupported_params` (single cross-mode rejection point).

---

## Unit Test Expectations

### AC-26 — Unrecognized Mode Lists All Seven Modes

**Test**: `test_graph_unrecognized_mode_error_lists_all_seven_modes`
**Arrange**: Construct a `GraphParams` with `mode = "unknown_mode_xyz"`.
**Act**: Call `validate_no_unsupported_params(&params)` (or invoke `handle_graph` via the
dispatch path).
**Assert**: Returns `Err(ErrorData)` with message containing all seven mode names:
`"chain"`, `"current"`, `"neighbors"`, `"subgraph"`, `"inverse"`, `"filter"`, `"path"`.
The exact fragment `"chain, current, neighbors, subgraph, inverse, filter, path"` must appear.

**Risk**: R-04 (AC-26)

---

### AC-25 — depth Rejected on Five Newly-Rejecting Modes

Five separate tests, one per mode. Each follows the same pattern:

**Test**: `test_depth_rejected_on_{chain|current|subgraph|inverse|filter}_mode`
**Arrange**: `GraphParams { mode: "{mode}", depth: Some(3), ... }` (all other fields None).
**Act**: Call `validate_no_unsupported_params(&params)`.
**Assert**: Returns `Err(ErrorData)` with message containing "depth is not supported in
{mode} mode" AND "neighbors or path mode".

**Regression** (no regression test needed here; covered in graph_read_path.md):
- `neighbors` still accepts `depth=3` — no error.
- `path` still accepts `depth=3` — no error.

**Risk**: R-07 (AC-25, SR-04)

---

### AC-22 — from_id / to_id Rejected on Non-path Modes

Five tests (one per affected mode: chain, current, neighbors, subgraph, filter).

**Test**: `test_from_id_rejected_on_{chain|current|neighbors|subgraph|filter}_mode`
**Arrange**: `GraphParams { mode: "{mode}", from_id: Some(1), ... }`.
**Act**: Call `validate_no_unsupported_params(&params)`.
**Assert**: Returns `Err(ErrorData)` with message naming `"path"` as the correct mode.

**Risk**: R-04 (AC-22)

---

### AC-23 — missing_edge_types Rejected on Non-inverse Modes

Six tests (chain, current, neighbors, subgraph, filter, path).

**Test**: `test_missing_edge_types_rejected_on_{chain|current|neighbors|subgraph|filter|path}`
**Arrange**: `GraphParams { mode: "{mode}", missing_edge_types: Some(vec!["Cites".to_string()]), ... }`.
**Act**: Call `validate_no_unsupported_params(&params)`.
**Assert**: Returns `Err(ErrorData)` naming `"inverse"` as the correct mode.

**Risk**: R-04 (AC-23, SR-08)

---

### AC-24 / R-04 — Filter-Only Params Rejected on Non-filter Modes

Priority set from R-04 rejection matrix (one test per new field × one wrong mode):

| Test Name | Field | Wrong Mode | Expected Error Fragment |
|-----------|-------|------------|------------------------|
| `test_category_rejected_on_path_mode` | `category` | path | "inverse/filter" or "inverse or filter" |
| `test_missing_edge_types_rejected_on_filter_mode` | `missing_edge_types` | filter | "inverse" |
| `test_limit_rejected_on_chain_mode` | `limit` | chain | "inverse/filter" |
| `test_min_age_days_rejected_on_path_mode` | `min_age_days` | path | "filter" |
| `test_min_confidence_rejected_on_subgraph_mode` | `min_confidence` | subgraph | "filter" |
| `test_max_confidence_rejected_on_current_mode` | `max_confidence` | current | "filter" |
| `test_min_edge_count_rejected_on_inverse_mode` | `min_edge_count` | inverse | "filter" |
| `test_max_edge_count_rejected_on_neighbors_mode` | `max_edge_count` | neighbors | "filter" |

Each test:
**Arrange**: `GraphParams { mode: "{wrong_mode}", {field}: Some({value}), ... }`.
**Act**: `validate_no_unsupported_params(&params)`.
**Assert**: `Err(ErrorData)` with the expected fragment in the error message.

**Also test**: `test_from_id_rejected_on_filter_mode` — `from_id` was a forward-compat stub,
now actively rejected on filter mode; assert error names "path".

**Risk**: R-04 (SR-08 — 8-field minimum)

---

### AC-03a — edge_types Rejected on inverse Mode

**Test**: `test_edge_types_rejected_on_inverse_mode`
**Arrange**: `GraphParams { mode: "inverse", edge_types: Some(vec!["Cites".to_string()]), ... }`.
**Act**: `validate_no_unsupported_params(&params)`.
**Assert**: `Err(ErrorData)` with message mentioning `missing_edge_types` as the correct
parameter (inverse uses `missing_edge_types` exclusively).

**Risk**: R-04

---

## Integration Test Expectations

The dispatch logic in `handle_graph` is exercised end-to-end by the AC-27 through AC-32
integration tests in `test_tools.py`. No additional integration tests are required for the
`graph_read.rs` component specifically — the sibling-module tests exercise dispatch indirectly.

---

## Edge Cases

- `depth=0` passed to `path` mode — validation error (range [1,10]), not a mode-rejection error.
  This is a path-mode range error, NOT a mode-rejection. Tested in graph_read_path.md.
- `depth=11` passed to `path` mode — same as above.
- `from_id` and `to_id` both present with mode="subgraph" — `validate_no_unsupported_params`
  fires on the first found rejected param; both should produce errors independently if only one
  is passed.
