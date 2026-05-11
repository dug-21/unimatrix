# 580-agent-1-fix — Fix Execution Report

## Changes Made

No Rust changes. Fix was entirely spec and test correction.

| File | Change |
|------|--------|
| `product/features/vnc-014/IMPLEMENTATION-BRIEF.md:173-174` | Moved `context_quarantine` from Write row to Admin row in capability table |
| `product/test/infra-001/suites/test_tools.py:777` | Replaced `test_quarantine_requires_write` (xfail, wrong assertion) with `test_quarantine_requires_admin_rejects_write_agent` (no xfail, `assert_tool_error`) |
| `product/test/infra-001/suites/test_security.py:161` | Replaced `test_restricted_agent_quarantine_allowed_write` (xfail, wrong assertion) with `test_restricted_agent_quarantine_rejected_requires_admin` (`assert_tool_error`) + companion `test_admin_agent_quarantine_allowed` (`assert_tool_success`) |

## New Tests

- `test_quarantine_requires_admin_rejects_write_agent` (test_tools.py) — T-78 replacement
- `test_restricted_agent_quarantine_rejected_requires_admin` (test_security.py) — S-24 replacement
- `test_admin_agent_quarantine_allowed` (test_security.py) — S-24b, confirms Admin gate is enforcement not lockout

## Test Results

All tests pass. No issues encountered.

## Knowledge Stewardship

**Queried:**
- `context_briefing` — surfaced ADR #4413 (quarantine gate is Admin by design), #4411 (lesson:
  vnc-014 delivery wrote tests against incorrect spec), #1435 (capability gate placement pattern)
  — all confirmed fix direction is correct

**Stored:** Nothing novel — the root cause, design decision, and lesson are fully captured in
entries #4413 and #4411. This task was a mechanical correction of downstream artifacts.
