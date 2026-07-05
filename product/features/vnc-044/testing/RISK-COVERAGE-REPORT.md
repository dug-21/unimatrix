# Risk Coverage Report: vnc-044

> `context_graph` two-axis split — `format` (serialization `json|markdown`) + `detail`
> (verbosity `summary|full`) with a lean node/edge projection. Stage 3c execution.
> Binary: `target/release/unimatrix` (built from `feature/vnc-044`). Harness: infra-001.
> Method note (R-04/AC-04): a true pre-vnc-044 byte-for-byte golden capture is impractical
> in-harness, so the plan's documented fallback was used — **complete `EntryRecord` key set
> present + byte-stability across two identical runs**.

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `content_preview` UTF-8 char-boundary flooring (DoS) | UNIT `verbosity::test_content_preview_*` (10-case boundary table #1–#10 + `test_content_preview_never_panics_on_arbitrary_unicode`) | PASS | Full |
| R-02 | `content_truncated` byte-compare (257-floors-to-256 trap) | UNIT `verbosity::test_content_truncated_257_ascii_true`, `_equals_byte_compare_invariant`, empty/255/256/multibyte | PASS | Full |
| R-03 | Default-summary + projection across all 5 node-bearing modes, metadata preserved | UNIT `graph_read_projection::test_{subgraph,chain,current,inverse,filter}_summary_*`; INTEG `test_graph_{subgraph,chain,current,inverse,filter}_default_is_summary_*` (lifecycle) + `test_graph_detail_axis_threaded`/`test_graph_default_is_summary` (tools) | PASS | Full |
| R-04 | `detail=full` byte-for-byte no-regression | UNIT `graph_read_tests_vnc044::test_full_arm_serializes_raw_result`, `_byte_identical_to_direct_to_string`; INTEG `test_graph_full_golden_chain_complete_and_stable`, `test_graph_full_golden_subgraph_complete_and_stable` | PASS | Full (fallback method) |
| R-05 | `format=markdown` rejected uniformly on all 7 modes (pre-dispatch) | UNIT `test_resolve_markdown_rejected_substring`; INTEG `test_graph_markdown_rejected_all_modes[7 modes]` | PASS | Full |
| R-06 | Shared `ResponseFormat`/`parse_format`/`EntryRecord`/`EdgeRecord` unchanged for non-graph callers | STATIC `cargo test --workspace --no-run` (link smoke, no new exhaustive-match arms); full-workspace `cargo test` green; UNIT `test_graph_params_detail_additive_*`; INTEG non-graph tools + smoke suite green | PASS | Full |
| R-07 | Exact summary field set — present AND absent keys (node + edge) | UNIT `graph_read_projection::test_node_summary_exact_key_set`, `_omits_content_and_hashes`, `test_edge_summary_exact_key_set`, `_omits_direction_and_metadata`; INTEG `test_graph_summary_node_field_set`, `test_graph_summary_edge_field_set` | PASS | Full |
| R-08 | Legacy `format=summary` alias + explicit-`detail` conflict (order pinned) | UNIT `test_resolve_legacy_summary_alias`, `_summary_plus_explicit_full_conflict`, `_summary_plus_explicit_summary_conflict`; INTEG `test_graph_legacy_summary_alias_equivalent`, `test_graph_legacy_summary_conflict_rejected` | PASS | Full |
| R-09 | `detail` accept-and-ignore on `neighbors`/`path`; bogus still rejected | UNIT `test_detail_not_rejected_on_neighbors`/`_on_path`; INTEG `test_graph_neighbors_detail_ignored`, `test_graph_path_detail_ignored`, `test_graph_detail_bogus_rejected_on_edge_modes`, `_on_node_modes` | PASS | Full |
| R-10 | Empty/boundary content, tags, confidence fidelity in lean shape | UNIT `graph_read_projection::test_node_summary_empty_content`, `_preserves_all_tags`, `_zero_tags_empty_array`, `_confidence_is_number`; INTEG node field-set carries `content_truncated`/`tags` | PASS | Full |
| R-11 | Lifecycle-vs-delivery status gap (doc/expectation, NOT a code defect) | REVIEW tool-description guard `tools::test_graph_description_states_lifecycle_status_caveat`; INTEG illustration `test_graph_summary_shrinks_payload_913` asserts `status=="active"` for every capability node (demonstrates the gap, does NOT treat absence as a defect); `test_graph_tool_description_advertises_detail` (live attr carries "lifecycle") | PASS (doc gate) | Full |
| R-12 | `256` single-sourced as `CONTENT_PREVIEW_BYTES` | STATIC no bare `256` in graph path (dev-agent verified, clippy clean); UNIT length asserts reference the constant symbolically | PASS | Full |
| R-13 | Assertion strings vs running strings — substring discipline | All error-copy assertions use `ERROR_INVALID_PARAMS` + substring (`"markdown"`, `"format=json"`, `"detail"`) only; no verbatim-sentence assertions | PASS | Full |

R-14 (file placement / line budget) is a Gate-3b static concern (`graph_read_projection.rs`
new = 430 lines; `graph_read.rs` = 469 lines; both under 500) — verified by the implementing
agent, out of Stage-3c test scope.

## Test Results

### Unit Tests
- `cargo test -p unimatrix-server --lib`: **4482 passed, 0 failed, 1 ignored**.
- Hardened full-workspace `cargo test --workspace` (setsid -w + timeout): **rc=0, all crates pass**.
- `#878` full-workspace LINK smoke (`check-workspace-link-smoke.sh`): **PASS (rc=0)** — profile/jobs
  invariant holds; also satisfies R-06 `--workspace --no-run` compile guard (no new
  `ResponseFormat` exhaustive-match arms).
- vnc-044-specific unit coverage (all within the 4482):
  - `response/verbosity.rs` — 24 tests (R-01/R-02/R-10/R-12 + `parse_detail`).
  - `graph_read_projection.rs` — 17 tests (R-07 present/absent key sets, R-03 per-envelope
    metadata incl. `current` single-node trap, R-10).
  - `graph_read_tests_vnc044.rs` — 21 tests (resolver decision table R-05/R-08, seam threading
    R-03/R-04, additive `GraphParams.detail` R-06/AC-09, R-09 validation).
  - `tools.rs` — 4 description-guard tests (R-11/R-13 + `test_graph_tool_attr_description_matches_const`
    twin-literal byte-equality guard stays green).

### Integration Tests (infra-001, compiled binary via MCP JSON-RPC)
- **Smoke gate (`pytest -m smoke`): 30 passed, 0 failed** — MANDATORY gate MET. Includes the 2
  new vnc-044 smoke tests (`test_graph_detail_axis_threaded`, `test_graph_default_is_summary`).
- **New vnc-044 integration tests: 26 test node-ids, all PASS** (18 in `test_tools.py`
  — axis threading, node/edge field-set, markdown-reject ×7 modes, legacy alias + conflict,
  accept-and-ignore ×2 + bogus, description-advertises; 8 in `test_lifecycle.py` — 5-mode
  default-summary + envelope-metadata, full-golden chain + subgraph, #913 size win).
- **Modified existing graph tests (5): all PASS** — `detail="full"` added to tests that assert
  the pre-vnc-044 full `EntryRecord`/`EdgeRecord` shape (broken by the accepted default→summary
  flip; the addition preserves their exact semantic intent). 3 in `test_tools.py`
  (`test_graph_subgraph_node_shape_matches_entry_record`, `_edge_record_fields`,
  `_direction_outgoing_on_all_edge_records`); 2 in `test_lifecycle.py`
  (`test_graph_subgraph_topology_traversal`, `_depth1_write_then_read_visible`).
- **Regression suites green:** `test_protocol.py` (14-tool discovery, handshake, graceful
  shutdown), `test_get_edges.py` (unaffected — exercises `context_get_edges`, a different tool).
- **Harness extension (cumulative):** `harness/client.py::context_graph(...)` gained
  `detail: str | None = None` + arg-marshalling (mirrors the existing `format` handling). Single
  change; all new tests depend on it. No parallel client scaffolded.

Totals across executed integration runs: smoke 30/30; `test_tools.py` graph set 50/50;
`test_lifecycle.py` + `test_protocol.py` + `test_get_edges.py` selected set 51 passed + **1
pre-existing xfail** (unrelated — an ONNX/tick-model lifecycle test that matched the `-k`
filter; not introduced by vnc-044).

### xfail / GH Issues
- **No new `xfail` markers created.** The 2 initial failures
  (`test_graph_summary_edge_field_set`, `test_graph_full_golden_subgraph_complete_and_stable`)
  were **defects in the newly-authored test fixtures** (near-duplicate node content tripped the
  server's >0.9 semantic dedup, collapsing the two endpoints into one so no edge existed) — a
  bad-test-assertion (triage category 3), fixed in-place with cross-domain distinct content, not
  a production issue. Both now pass.
- **No GH Issues filed** — no genuine pre-existing, feature-unrelated integration failure was
  surfaced by this work.

## Gaps

None. Every risk R-01..R-13 has PASS coverage; R-14 is covered by the Gate-3b file-size review.
R-11 is satisfied as a documentation/expectation gate (not tested as a defect, per its mandate).

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | N/A (companion ADR) | ADR-001 (#5509) / ADR-002 (#5510) — architect deliverable, verified by artifact review, not by this test run. |
| AC-02 | PASS | `test_graph_detail_axis_threaded` — same chain query at `detail=summary` vs `detail=full` yields structurally different payloads (summary lacks `content`, full has it) → resolved axis threaded past `graph_read.rs:251`. |
| AC-03 | PASS | `test_graph_summary_node_field_set` (exactly 8-field node, absent-key asserts), `test_graph_summary_edge_field_set` (exactly 4-field edge, `direction`/`metadata` absent); UNIT `graph_read_projection` key-set tests; `status` is a lifecycle string. |
| AC-03b | PASS | UNIT `verbosity` boundary table (empty/<256/=256/257-ASCII/2-,3-,4-byte straddle/boundary-exact) + `content_truncated == len>256` incl. the 257-floors-to-256 trap; no ellipsis; never panics. |
| AC-04 | PASS | `test_graph_full_golden_chain_complete_and_stable`, `test_graph_full_golden_subgraph_complete_and_stable` — `detail=full` carries the complete `EntryRecord` (content + hashes + timestamps) and full `EdgeRecord` (direction + metadata), byte-stable across identical runs (fallback method; documented above). UNIT full-arm-not-projected guard. |
| AC-05 | PASS | Per node-bearing mode `test_graph_{subgraph,chain,current,inverse,filter}_default_is_summary_*` — a call with NO `detail` equals `detail=summary` and differs from `detail=full`; each preserves its own envelope metadata (`truncated`/`seed_ids`/`depth_reached`; `Truncated`; `total_returned`; `current` single node). |
| AC-06 | PASS | `test_graph_summary_shrinks_payload_913` — capability-style subgraph (goal + 12 incoming Advances, substantial bodies): summary payload < 60% of `detail=full` for the same query, valid parseable JSON. Documented caveat: every node's `status` is lifecycle `"active"` — the #913 delivery-status tally is NOT delivered (named follow-up #3), asserted as an illustration, not a defect (R-11). |
| AC-07 | PASS | `test_graph_legacy_summary_alias_equivalent` (`format=summary` == `detail=summary`, byte-identical), `test_graph_legacy_summary_conflict_rejected` (`format=summary` + explicit `detail` → `ERROR_INVALID_PARAMS`, both `full` and `summary`). |
| AC-08 | PASS | `test_graph_markdown_rejected_all_modes[subgraph,chain,current,neighbors,inverse,filter,path]` → `ERROR_INVALID_PARAMS` with `"markdown"` + `"format=json"` substrings on all 7 modes; `neighbors`/`path` `detail` accept-and-ignore identical output; `detail=bogus` rejected. |
| AC-09 | PASS | UNIT `test_graph_params_detail_additive_*` (layout invariant, additive `Option<String>`); `test_graph_tool_description_advertises_detail` — live `tools/list` description carries `detail`/`summary`/`full` + `"lifecycle"` caveat. |
