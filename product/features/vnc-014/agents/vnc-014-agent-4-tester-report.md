# Agent Report: vnc-014-agent-4-tester

## Phase: Stage 3c — Test Execution

## Summary

All unit tests pass (4,806/4,806). Integration smoke gate passes (23/23). All required integration suites pass after test fixes. 2 pre-existing failures triaged with GH Issues and xfail markers. 2 integration test assertions corrected for vnc-014's intentional capability change (quarantine Admin→Write). 4 new integration tests added per test plan. Full risk coverage achieved.

## Unit Tests

- **4,806 passed, 0 failed** across all workspace crates.
- vnc-014 specific tests: 37 tests covering migration, AuditEvent defaults, Capability::as_audit_str, server initialize, tool attribution, and JSON metadata safety.

## Integration Tests (infra-001)

### Smoke Gate: PASS (23/23)
### Protocol Suite: PASS (13/13)
### Tools Suite: PASS (119 passed, 1 xfail GH#575, 4 new tests added)
### Lifecycle Suite: PASS (49 passed, 5 pre-existing xfails)
### Security Suite: PASS (19/19, 1 test assertion corrected)
### Edge Cases Suite: PASS (23 passed, 1 xfail GH#576, 1 pre-existing xfail)

## Test Fixes

### Assertion corrections (vnc-014 changed behavior):
1. `test_security.py::test_restricted_agent_quarantine_rejected` → renamed `test_restricted_agent_quarantine_allowed_write`, assertion changed to success. Cause: vnc-014 moved quarantine from Admin→Write per IMPLEMENTATION-BRIEF capability table.
2. `test_tools.py::test_quarantine_requires_admin` → renamed `test_quarantine_requires_write`, assertion changed to success. Same cause.

### Pre-existing failures (xfail):
- GH#575: `test_retrospective_format_invalid` — centralized `parse_format` error string vs old inline "Unknown format" message. Pre-dates vnc-014.
- GH#576: `test_very_long_content` — 8KB content cap (fix #561) rejects 50KB test. Pre-dates vnc-014.

## New Integration Tests Added

4 new tests in `suites/test_tools.py`:
- `test_initialize_client_info_name_stored` — AC-01/AC-08
- `test_single_session_attribution_roundtrip` — R-03/AC-07
- `test_long_client_name_no_crash` — AC-10/EC-01/EC-02
- `test_special_chars_client_name_no_crash` — SEC-02/EC-06

Harness infrastructure: `client.py::initialize()` gained `client_name` parameter.

## Known Deviations Verified

- `context_lookup` uses `Capability::Read` → "read" (not "search" per IMPLEMENTATION-BRIEF). Confirmed in code at tools.rs line 551.
- `context_quarantine` uses `Capability::Write` → "write" (matches IMPLEMENTATION-BRIEF; changed from Admin in prior code).
- `Capability` enum has 5 variants including `SessionWrite` → "session_write". `as_audit_str()` exhaustive match covers all 5.

## Files Modified

- `/workspaces/unimatrix/product/test/infra-001/suites/test_tools.py` — renamed quarantine test, marked xfail, added 4 new tests
- `/workspaces/unimatrix/product/test/infra-001/suites/test_security.py` — renamed quarantine test
- `/workspaces/unimatrix/product/test/infra-001/suites/test_edge_cases.py` — marked xfail
- `/workspaces/unimatrix/product/test/infra-001/harness/client.py` — added `client_name` param to `initialize()`
- `/workspaces/unimatrix/product/features/vnc-014/testing/RISK-COVERAGE-REPORT.md` — created

## GH Issues Filed

- GH#575: `[infra-001] test_retrospective_format_invalid: error message mismatch`
- GH#576: `[infra-001] test_very_long_content: 50KB content rejected by 8000-byte content size cap`

## Risk Coverage

All 18 risks covered. No gaps. AC-01 through AC-12 all PASS.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned vnc-014 ADRs and testing patterns. Applied to test design.
- Stored: nothing novel to store — all findings are feature-specific test corrections; no cross-feature harness pattern extracted.
