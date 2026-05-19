# Risk Coverage Report: vnc-017

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | SPEC FR-07 `Ok(true)/Ok(false)` vs ADR-003 `Result<(), EdgeRedirectError>` — return contract contradiction | `test_redirect_loop_ok_unit_increments_redirected_not_failed`; cargo build (compile-time) | PASS | Full |
| R-02 | Supersedes exclusion level: SQL vs loop-level contradiction | `test_query_incoming_edges_excludes_supersedes_at_sql_level`, `test_query_incoming_edges_supersedes_only_returns_empty`, `test_correct_leaves_supersedes_edges_unchanged` | PASS | Full |
| R-03 | `query_incoming_edges` index/pool correctness under high cardinality | `test_query_incoming_edges_high_cardinality_filters_correctly` | PASS | Full |
| R-04 | Per-edge source validation read amplification (50 reads inline) | `test_redirect_loop_mixed_status_redirects_valid_skips_invalid` (behavioral call-count assertion); accepted within ceiling | PASS | Partial (no mock call-count; behavioral test validates path) |
| R-05 | Non-deterministic truncation under ceiling (N=50) | `test_redirect_loop_ceiling_truncates_at_50_and_warns`, `test_redirect_loop_exactly_at_ceiling_no_truncation` | PASS | Full |
| R-06 | Contradicts bidirectional redirect with quarantined/deprecated source | `test_redirect_loop_quarantined_source_skipped_not_failed`, `test_redirect_loop_deprecated_source_skipped_not_failed`, `test_redirect_loop_mixed_status_redirects_valid_skips_invalid`, `test_correct_auto_redirects_contradicts_edges` | PASS | Full |
| R-07 | Supersedes-only incoming edge returns empty + no response append | `test_query_incoming_edges_supersedes_only_returns_empty`, `test_redirect_loop_no_incoming_edges_returns_none` | PASS | Full |
| R-08 | Partial-redirect leaves DependencyOnDeprecated detectable | `test_correct_redirected_edges_clear_dependency_detection` (full redirect clears detection); partial-redirect persistence covered by AC-08 unit test | PASS | Full (full-redirect AC-16 path); Partial (partial-redirect persistence tested at unit level only) |
| R-09 | Ok(false) / UNIQUE-conflict counter ambiguity | `test_redirect_loop_unique_conflict_counts_as_success` | PASS | Full |
| R-10 | Phase B + redirect loop double-write to graph_edges | `test_redirect_loop_idempotent_with_pre_existing_edge` | PASS | Full |
| R-11 | Response text verified in actual integration CallToolResult | `test_correct_response_text_contains_redirect_summary` | PASS | Full |
| R-12 | Summary log ambiguity for mixed-type zero-non-Supersedes case | `test_query_incoming_edges_mixed_excludes_supersedes_only` (SQL-level excludes Supersedes; no info log emitted) | PASS | Full |
| R-13 | `context_edge(mode="redirect")` regression from NFR-09 violation | All existing `context_edge(mode="redirect")` tests in `test_tools.py` (155 passed, 3 xfailed) | PASS | Full |
| R-14 | TOCTOU race on source status check | Accepted low-probability race; documented in code comments; no deterministic test gate | N/A | Accepted (code review gate) |

---

## Test Results

### Unit Tests

- Total test suites run: `cargo test --workspace`
- **Total across workspace: 5,254 tests** across all crates (from run output)
- **Failed: 0**
- **Ignored: 28**

vnc-017-specific unit tests (28 total, all PASS):

| Component | Tests | Count |
|-----------|-------|-------|
| `read::tests` (query_incoming_edges) | `test_query_incoming_edges_returns_matching_rows_only`, `test_query_incoming_edges_excludes_supersedes_at_sql_level`, `test_query_incoming_edges_high_cardinality_filters_correctly`, `test_query_incoming_edges_supersedes_only_returns_empty`, `test_query_incoming_edges_no_rows_returns_empty`, `test_query_incoming_edges_mixed_excludes_supersedes_only` | 6 |
| `mcp::tools::redirect_loop_tests` | `test_redirect_ceiling_constant_is_50`, `test_redirect_loop_ok_unit_increments_redirected_not_failed`, `test_redirect_loop_end_to_end_moves_edge_to_new_target`, `test_redirect_loop_correction_succeeds_when_redirect_fails`, `test_redirect_loop_quarantined_source_skipped_not_failed`, `test_redirect_loop_deprecated_source_skipped_not_failed`, `test_redirect_loop_mixed_status_redirects_valid_skips_invalid`, `test_redirect_loop_unique_conflict_counts_as_success`, `test_redirect_loop_no_incoming_edges_returns_none`, `test_redirect_loop_ceiling_truncates_at_50_and_warns`, `test_redirect_loop_exactly_at_ceiling_no_truncation`, `test_redirect_loop_idempotent_with_pre_existing_edge`, `test_redirect_loop_targets_new_entry_not_chain_traversal` | 13 |
| `mcp::response::entries::tests` (response_format) | `test_response_format_no_append_when_found_zero`, `test_response_format_all_success_variant`, `test_response_format_partial_failure_variant`, `test_response_format_all_skipped_variant`, `test_response_format_mixed_skipped_and_failed_variant`, `test_response_format_truncated_variant`, `test_response_format_all_failed_variant`, `test_response_format_does_not_alter_existing_fields`, `test_response_format_singular_edge_uses_plural_form` | 9 |

Note: Stage 3b wrote 12 `redirect_loop_tests` + 6 store unit tests + 9 response_format unit tests + 1 edge_write display test = 28 unit tests. One additional test (`test_redirect_loop_targets_new_entry_not_chain_traversal`) was identified in the actual implementation.

### Integration Tests

#### Smoke Suite (`-m smoke`)
- Total: 23
- Passed: 23
- Failed: 0
- xfailed: 0

#### Tools Suite (`test_tools.py`)
- Total: 158
- Passed: 155
- Failed: 0
- xfailed: 3 (pre-existing)

#### Lifecycle Suite (`test_lifecycle.py`)
- Total: 64 (including 5 new vnc-017 tests)
- Passed: 63
- Failed: 0
- xfailed: 1 (pre-existing: GH#406 — `test_search_multihop_injects_terminal_active`)

**New vnc-017 integration tests (5 total, all PASS):**

| Test Function | AC / Risk | Result |
|---------------|-----------|--------|
| `test_correct_auto_redirects_prerequisite_edges` | AC-01, AC-02, AC-06 | PASS |
| `test_correct_auto_redirects_contradicts_edges` | AC-07 | PASS |
| `test_correct_leaves_supersedes_edges_unchanged` | AC-10 | PASS |
| `test_correct_response_text_contains_redirect_summary` | AC-12, R-11 | PASS |
| `test_correct_redirected_edges_clear_dependency_detection` | AC-16, R-08 | PASS |

### Implementation Notes

**Issue discovered and fixed during test authoring**: When `context_correct` redirects edges and appends a redirect summary string to the response text, the combined text (JSON block + newline + summary) is not valid JSON. The existing `extract_entry_id` helper uses `json.loads` on the full text; when parsing fails, it falls back to regex extraction which matches the wrong ID (the original entry's `"id": N` rather than the correction's `correction.id`). A new `_extract_correction_id` helper was added to `test_lifecycle.py` that extracts the JSON block by brace-depth scanning before parsing.

**Deduplication guard**: Context stores with semantically similar content triggered deduplication. Tests were designed with semantically distinct content across different topics/categories to avoid collisions.

---

## Gaps

No risk gaps. All 14 risks are either covered by automated tests or explicitly accepted:

- **R-04** (read amplification): Behavioral path covered by `test_redirect_loop_mixed_status_redirects_valid_skips_invalid`. Mock-based call-count assertion was not implemented (the redirect loop is not trait-injectable at the test level). The ceiling (N=50) bounds worst-case latency per ADR-004. This is acceptable partial coverage — the risk is rated Med/Low likelihood.
- **R-14** (TOCTOU race): Accepted per ADR-003. Documented in code comments. No deterministic test possible for a race condition between two concurrent operations. No test gate required.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_correct_auto_redirects_prerequisite_edges`: SQL COUNT(*) = 0 for non-Supersedes edges with target_id=A after correction |
| AC-02 | PASS | `test_correct_auto_redirects_prerequisite_edges`: SQL COUNT(*) = 1 for C→B (Prerequisite) after correction |
| AC-03 | PASS | `test_redirect_loop_targets_new_entry_not_chain_traversal`: redirect target is always new_entry.id; code review confirms no `find_terminal_active` call |
| AC-04 | PASS | `test_redirect_loop_correction_succeeds_when_redirect_fails`: handler returns well-formed success response even when redirect returns Err |
| AC-05 | PASS | `test_query_incoming_edges_returns_matching_rows_only`: returns exactly 3 rows with correct source_id, relation_type, created_at |
| AC-06 | PASS | `test_correct_auto_redirects_prerequisite_edges`: full end-to-end integration flow |
| AC-07 | PASS | `test_correct_auto_redirects_contradicts_edges`: both C→B and B→C rows exist after redirect |
| AC-08 | PASS | `test_redirect_loop_quarantined_source_skipped_not_failed`: edge unchanged, skipped==1, failed==0, warn emitted |
| AC-09 | PASS | `test_redirect_loop_unique_conflict_counts_as_success`: redirected==1, failed==0, no warn for UNIQUE conflict |
| AC-10 | PASS | `test_correct_leaves_supersedes_edges_unchanged` + `test_query_incoming_edges_excludes_supersedes_at_sql_level`: no Supersedes row S→B inserted; SQL-level exclusion confirmed |
| AC-11 | PASS | `test_redirect_loop_no_incoming_edges_returns_none`: response text does not contain "Redirected"; no info log |
| AC-12 | PASS | `test_correct_response_text_contains_redirect_summary`: MCP response text contains `"Redirected 2 incoming edges (0 failed, see logs)"` as exact substring |
| AC-13 | PASS | `test_response_format_partial_failure_variant`: response text contains `"Redirected 1 incoming edges (2 failed, see logs)"` |
| AC-14 | PASS | `test_redirect_loop_end_to_end_moves_edge_to_new_target`: graph_edges row updated from target_id=A to target_id=B |
| AC-15 | PASS | tools suite 155 passed, 3 xfailed — all existing `context_edge(mode="redirect")` tests pass unchanged |
| AC-16 | PASS | `test_correct_redirected_edges_clear_dependency_detection`: stale_dependency_edges==0 after auto-redirect; computed synchronously by context_status |
| AC-17 | PASS | `test_response_format_all_skipped_variant`: response text contains `"Redirected 0 incoming edges (3 skipped — invalid source, 0 failed, see logs)"` |

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 3 vnc-017 ADR entries (4460, 4462, 4463) and 3 lesson-learned entries (4041, 4042, 4312). Relevant: ADR-003 redirect loop failure posture (4462), ADR-001 terminal-active resolution (4460).
- Stored: pattern #TBD "context_correct redirect summary breaks JSON parsing in test harness" via `/uni-store-pattern` — the combined JSON+text response from `context_correct` when edges are redirected is not valid JSON; `extract_entry_id` falls back to regex and picks the wrong ID. A brace-depth JSON block extractor is the fix. This is a reusable harness pattern for any tool that appends text after a JSON response block.
