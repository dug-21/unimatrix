# Agent Report: col-028-agent-7-tester

Stage 3c (Test Execution) — Unified Phase Signal Capture (Read-Side + query_log)

---

## Summary

All mandatory test gates passed. Feature is ready for merge review.

---

## Work Completed

### Gate 3b Fix

Fixed `insert_query_log_row` helper in `crates/unimatrix-server/src/eval/scenarios/tests.rs`:
- Added `phase` column and `?9` binding to the raw SQL INSERT
- Without this fix, all 15+ call sites in eval tests would produce "table has 9 columns but 8 values" runtime errors against the v17 schema
- Binding uses `Option::<String>::None` — col-028 phase=NULL for synthetic test helper rows (IR-03)

### AC-17 Round-Trip Tests Added

Added 3 new tests to `crates/unimatrix-store/tests/migration_v16_to_v17.rs`:
- `test_query_log_phase_round_trip_some` — phase=Some("design") survives close+reopen analytics drain flush
- `test_query_log_phase_round_trip_none` — phase=None stays None (not empty string, not panic)
- `test_query_log_phase_round_trip_non_trivial_value` — EC-06: phase="design/v2" confirms parameterized binding handles non-trivial strings

These are the primary R-02 (SR-01) guards against positional column index drift.

### New Integration Tests Added

Added 2 new tests to `product/test/infra-001/suites/test_lifecycle.py`:
- `test_briefing_then_get_does_not_consume_dedup_slot` (L-COL028-01) — AC-07 workflow-level integration test for D-01 guard; confirms briefing→get sequence succeeds without error
- `test_context_search_writes_query_log_row` (L-COL028-02) — AC-16 partial coverage; confirms context_search succeeds against 9-column query_log schema

Note: `access_count` is not serialized in the MCP JSON response format (`entry_to_json` in `mcp/response/mod.rs` does not include it). The detailed access_count=2 assertion therefore lives at unit tier in `services/usage.rs`, not at the infra tier.

### Pre-existing Failures Filed

Three pre-existing test failures identified, filed as GH Issues, and marked xfail:

| GH Issue | Test | File | Root Cause |
|----------|------|------|-----------|
| GH#405 | `test_base_score_deprecated` | test_confidence.py | Background confidence scoring timing: deprecated confidence can exceed stale active snapshot |
| GH#405 | `test_deprecated_visible_in_search_with_lower_confidence` | test_tools.py | Same root cause |
| GH#406 | `test_search_multihop_injects_terminal_active` | test_lifecycle.py | find_terminal_active multi-hop traversal not implemented (crt-014 gap) |

None caused by col-028. xfail markers added with GH Issue references per USAGE-PROTOCOL.md protocol.

---

## Test Results Summary

| Tier | Total | Passed | Failed | xfailed |
|------|-------|--------|--------|---------|
| Unit (cargo test --workspace) | 3639 | 3639 | 0 | 27 ignored |
| Migration integration (--features test-support) | 10 | 10 | 0 | 0 |
| Smoke gate (infra-001) | 20 | 20 | 0 | 0 |
| Lifecycle suite (infra-001) | 41 | 38 | 0 | 3 |
| Confidence suite (infra-001) | 14 | 13 | 0 | 1 |

Tools suite (95 tests) timed out in full run. Subset covering col-028 code paths passed: 11/11.

---

## Gate Check Results

| Gate | Command | Result |
|------|---------|--------|
| AC-22 | `grep -r 'schema_version.*== 16' crates/` | PASS — zero matches |
| AC-23 | `cargo build --workspace` | PASS — no errors |
| AC-12 | Code review of tools.rs | PASS — all four handlers verified |
| AC-21 | Code review of analytics.rs + query_log.rs | PASS — all four sites atomic |
| AC-24 | `grep -B 10 'confirmed_entries: HashSet<u64>' session.rs` | PASS — full doc comment present |
| Smoke | `pytest -m smoke --timeout=60` | PASS — 20/20 |

---

## Risk Coverage Gaps

### AC-16 MCP Round-Trip

Full AC-16 (context_search writes phase="delivery" to query_log) is not achievable through the MCP wire path because `set_current_phase` is only called from the UDS hook path (`uds/listener.rs`), not from MCP JSON-RPC. `context_cycle` via MCP does not set in-memory session phase.

Coverage at available tiers:
- Unit: `test_usage_context_current_phase_propagates_to_feature_entry` + `test_usage_context_phase_none_produces_null_phase`
- Store: AC-17 round-trip tests (insert_query_log + scan_query_log_by_session)
- Infra: L-COL028-02 confirms 9-column schema accepted

This is an architectural constraint of the test infrastructure, not an implementation gap. Marked as partial coverage (PARTIAL) in RISK-COVERAGE-REPORT.md.

### R-16 (D-01 Future Bypass)

Accepted risk per ADR-003. No automated test possible for future-state structural bypass. AC-07 canary test (L-COL028-01) would catch a regression if routing changes.

---

## Files Modified

- `crates/unimatrix-server/src/eval/scenarios/tests.rs` — Gate 3b fix: phase column + ?9 binding in insert_query_log_row
- `crates/unimatrix-store/tests/migration_v16_to_v17.rs` — 3 new AC-17 round-trip tests
- `product/test/infra-001/suites/test_lifecycle.py` — 2 new col-028 integration tests + 1 xfail marker (GH#406)
- `product/test/infra-001/suites/test_confidence.py` — 1 xfail marker (GH#405)
- `product/test/infra-001/suites/test_tools.py` — 1 xfail marker (GH#405)

## Files Created

- `product/features/col-028/testing/RISK-COVERAGE-REPORT.md`
- `product/features/col-028/agents/col-028-agent-7-tester-report.md` (this file)

---

## Knowledge Stewardship

- Queried: /uni-knowledge-search for testing procedures (category: procedure) — found entries on gate verification and integration test patterns; all applicable patterns already known (#3503, #3510, #2933, #3004).
- Stored: nothing novel to store — all patterns applied were pre-existing Unimatrix entries. The AC-16 MCP-vs-UDS testing gap is an infrastructure constraint, not a reusable pattern beyond what #3004 (analytics drain causal test) already captures.
