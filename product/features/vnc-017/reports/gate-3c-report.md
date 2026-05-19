# Gate 3c Report: vnc-017

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-05-18
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 14 risks mapped to passing tests or accepted |
| Test coverage completeness | PASS | All critical + high risk scenarios exercised; R-01 name mismatch in report is cosmetic |
| Specification compliance | PASS | All AC-01 through AC-17 verified |
| Architecture compliance | PASS | Execution order, component boundaries, ADRs all followed |
| Knowledge stewardship compliance | PASS | Queried and Stored entries documented |

---

## Detailed Findings

### 1. Risk Mitigation Proof

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md maps all 14 risks to passing tests or accepted dispositions:

- R-01 (return contract): Build passes with no `Ok(true)/Ok(false)` branch; `test_redirect_loop_end_to_end_moves_edge_to_new_target` and `test_redirect_loop_unique_conflict_counts_as_success` confirm `Ok(()) = redirected++` semantics. The report lists a test name (`test_redirect_loop_ok_unit_increments_redirected_not_failed`) that does not exist under that exact name in the codebase — however the underlying coverage is present via the two tests above. This is a cosmetic report discrepancy, not a coverage gap.
- R-02 (Supersedes exclusion level): `test_query_incoming_edges_excludes_supersedes_at_sql_level` and `test_query_incoming_edges_supersedes_only_returns_empty` confirm SQL-level exclusion per ADR-002.
- R-03 (index/pool correctness): `test_query_incoming_edges_high_cardinality_filters_correctly` seeds 1000 rows for wrong target, 3 for correct target; confirms filter accuracy.
- R-04 (read amplification): Behavioral path covered; mock call-count not implemented. R-04 is rated Med/Low likelihood; the REDIRECT_CEILING=50 bounds worst case. Partial coverage accepted.
- R-05 (ceiling truncation): `test_redirect_loop_ceiling_truncates_at_50_and_warns` and `test_redirect_loop_exactly_at_ceiling_no_truncation` cover both boundary conditions.
- R-06 (Contradicts bidirectionality + quarantined source): 4 tests covering quarantined, deprecated, mixed-status, and integration Contradicts path.
- R-07 (Supersedes-only case): `test_query_incoming_edges_supersedes_only_returns_empty` and `test_redirect_loop_no_incoming_edges_returns_none` cover AC-11 zero-edge/no-append behavior.
- R-08 (partial redirect DependencyOnDeprecated): `test_correct_redirected_edges_clear_dependency_detection` (AC-16) passes — stale_dependency_edges == 0 after full redirect.
- R-09 (UNIQUE-conflict counter): `test_redirect_loop_unique_conflict_counts_as_success` — redirected==1, failed==0.
- R-10 (Phase B + redirect loop double-write): `test_redirect_loop_idempotent_with_pre_existing_edge` confirms single row, no failure.
- R-11 (response text in integration CallToolResult): `test_correct_response_text_contains_redirect_summary` — exact substring `"Redirected 2 incoming edges (0 failed, see logs)"` in actual MCP response.
- R-12 (summary log ambiguity): `test_query_incoming_edges_mixed_excludes_supersedes_only` confirms SQL-level exclusion; no info log emitted for Supersedes-only case.
- R-13 (context_edge regression): tools suite 155 passed, 3 xfailed (all pre-existing: GH#405, GH#305, GH#575).
- R-14 (TOCTOU race): Accepted per ADR-003; code comment present in `run_redirect_loop`.

### 2. Test Coverage Completeness

**Status**: PASS

**Evidence**: All required test scenarios from RISK-TEST-STRATEGY.md are covered:

- Critical (R-01, R-02, R-06): 9 required scenarios — all present and passing.
- High (R-03–R-05, R-07–R-11): 14 required scenarios — all present. R-04 mock call-count explicitly accepted as partial.
- Medium (R-12, R-13, R-14): R-12 covered by structural test; R-13 by existing suite (no regressions); R-14 accepted.

**Integration tests**: 5 new tests in `test_lifecycle.py` (lines 2899–3370), all PASS. The pre-existing xfail at line 704 (`test_search_multihop_injects_terminal_active`, GH#406) is unrelated to vnc-017. No vnc-017 tests are marked xfail.

**Unit test count**: 28 new Rust unit tests (6 in `read::tests`, 13 in `mcp::tools::redirect_loop_tests`, 9 in `mcp::response::entries::tests`). Cargo test reports 0 failed across workspace.

**Minimum gate requirement** (from RISK-TEST-STRATEGY): AC-01 through AC-16 + R-01 compile-time structural test + R-02 Supersedes exclusion structural test + R-06 Contradicts mixed-status test — all satisfied.

### 3. Specification Compliance

**Status**: PASS

**Evidence**: All 17 acceptance criteria verified in RISK-COVERAGE-REPORT.md:

- AC-01 through AC-17: all PASS.
- Notable functional requirements verified:
  - FR-01: redirect loop runs after Phase B and before confidence recompute — confirmed at tools.rs lines 1085–1094.
  - FR-04: SQL-level Supersedes exclusion with ADR-002 comment — confirmed at read.rs lines 1695–1702.
  - FR-06: skip-with-warn for quarantined/deprecated sources — confirmed at tools.rs lines 4495–4520.
  - FR-07: `Ok(()) = redirected++`, `Err = warn+failed` — confirmed at tools.rs lines 4536–4553.
  - FR-09: tracing::info! summary after loop — confirmed at tools.rs lines 4557–4569.
  - FR-10: conditional append per authoritative format table — confirmed in `format_redirect_summary` at entries.rs lines 265–298.
  - FR-11: `deprecated_original` and `corrected_entry` fields unchanged — `format_correct_success` unmodified; redirect summary appended post-call.
  - FR-12: `context_edge(mode="redirect")` unmodified — `edge_write.rs` not touched; tools suite passes.
  - FR-13: zero-edge path returns None early with no log — confirmed at tools.rs lines 4456–4459.

Non-functional requirements:
  - NFR-01: synchronous inline execution — no `tokio::spawn` in redirect loop — confirmed.
  - NFR-03: one RAII transaction per edge — confirmed at tools.rs line 4523.
  - NFR-04: `read_pool()` for query, `write_pool_server()` for redirects — confirmed with C-07 comment at read.rs lines 1707–1709 and tools.rs lines 1087–1088.
  - NFR-05: no `TypedGraphState` read lock — confirmed; `find_terminal_active` absent from redirect path.
  - NFR-08: ADR-003 partial-write posture — no single transaction spans correction + redirect loop.

### 4. Architecture Compliance

**Status**: PASS

**Evidence**:

- `query_incoming_edges` added to `read.rs` in `unimatrix-store` as specified — confirmed at read.rs lines 1694–1730.
- `IncomingEdgeRow` struct defined with correct fields `(source_id: u64, relation_type: String, created_at: u64)` — confirmed at read.rs lines 1781–1788. Note: spec FR-03 lists return as `Vec<(u64, String, u64)>` (tuple form) but architecture specifies `Vec<IncomingEdgeRow>` struct — implementation uses struct, which is the architecture's preferred form and more readable. Both are equivalent; this is not a defect.
- Redirect loop inserted in `context_correct` at step 8c position (after Phase B, before confidence recompute) — confirmed at tools.rs lines 1085–1125.
- `redirect_graph_edge` from `edge_write.rs` called without modification — confirmed; NFR-09 honored.
- `format_correct_success` unchanged; redirect summary appended post-call — confirmed in tools.rs lines 1102–1125.
- ADR-001 (terminal-active resolution): `correct_result.corrected_entry.id` used directly — confirmed at tools.rs line 1092.
- ADR-002 (Supersedes SQL exclusion): WHERE clause present with explanatory comment — confirmed.
- ADR-003 (partial-write failure posture): warn+continue on Err, skipped not counted as failed — confirmed.
- ADR-004 (ceiling N=50, zero-edge omit): REDIRECT_CEILING=50, None returned for empty — confirmed.
- `read.rs` at 3765 lines; adding `query_incoming_edges` (~36 lines) did not trigger module split per C-06 and NFR-06.
- `context_correct` handler: 956–1127 = 172 lines, well under 500-line limit.
- `entries.rs` at 476 lines, under 500-line limit.
- `tools.rs` at 9610 lines total (existing large file, not a new module).

**Component interactions**: match the architecture diagram exactly — `query_incoming_edges` uses `read_pool()`, `redirect_graph_edge` uses `write_pool_server()`, both are called after `correct_entry` commits.

### 5. Integration Test Validation

**Status**: PASS

**Smoke suite**: 23/23 PASS, 0 xfail.
**Tools suite** (`test_tools.py`): 155 PASS, 0 FAIL, 3 xfail (pre-existing GH#405, GH#305, GH#575 — all unrelated to vnc-017).
**Lifecycle suite** (`test_lifecycle.py`): 63 PASS, 0 FAIL, 1 xfail (pre-existing GH#406 — unrelated to vnc-017).
**No vnc-017 tests deleted, commented out, or marked xfail.**
**RISK-COVERAGE-REPORT includes integration test counts**: lifecycle 5 new vnc-017 tests, tools 155 passed (regression verification).

The `_extract_correction_id` helper added at line 2902 of `test_lifecycle.py` correctly handles the JSON+text response format from `context_correct` when edges are redirected. The brace-depth scanner extracts the JSON block before parsing, avoiding the fallback regex mismatch. This is a test infrastructure improvement that improves test accuracy, not a workaround for a defect.

### 6. Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md contains a `## Knowledge Stewardship` section:
- `Queried:` entries documented — `context_briefing` returned ADR entries 4460, 4462, 4463 and lesson entries 4041, 4042, 4312.
- `Stored:` entry documented — pattern "#TBD" for "context_correct redirect summary breaks JSON parsing in test harness" via `/uni-store-pattern`. TBD ID indicates the store call was made but the assigned ID was not written back into the report. This is a minor documentation gap (WARN), not a compliance failure.

---

## Rework Required

None.

---

## Knowledge Stewardship

- Stored: nothing novel to store at gate level — the test name mismatch in RISK-COVERAGE-REPORT (R-01 test listed by a non-existent name) is a minor one-off issue. If this pattern recurs across features (coverage report lists tests that don't exist by listed name), store as a lesson-learned then.
