# 580-investigator — Bug Investigation Report

## Bug Summary

`context_quarantine` enforces `Capability::Admin` at `tools.rs:1456` and records `"admin"` in
both `AuditEvent.capability_used` fields (lines 1500 and 1538). The vnc-014
IMPLEMENTATION-BRIEF.md capability table (line 173) listed `Capability::Write` for
`context_quarantine`. Two integration tests written against that spec entry were xfailed because
the live code contradicted it.

## Root Cause

The vnc-014 IMPLEMENTATION-BRIEF capability table contained an error: `context_quarantine` was
listed under Write. The server always correctly enforced Admin. The tests were written against the
incorrect spec entry, not against the actual server behavior.

Product decision: `context_quarantine` remains `Capability::Admin` by design. Bearer tokens in
the personal cloud / OSS path will carry Admin scope, resolving end-user friction without weakening
the privilege model. See Unimatrix ADR #4413.

## Affected Files

| File | Issue |
|------|-------|
| `product/features/vnc-014/IMPLEMENTATION-BRIEF.md:173` | `context_quarantine` listed under Write — must move to Admin |
| `product/test/infra-001/suites/test_tools.py:777` | T-78 xfailed, asserted Write agent succeeds — wrong |
| `product/test/infra-001/suites/test_security.py:161` | S-24 xfailed, asserted Write agent succeeds — wrong |

## Fix Approach

- Correct IMPLEMENTATION-BRIEF capability table: move `context_quarantine` from Write row to Admin row
- Rewrite T-78: rename, remove xfail, flip to `assert_tool_error` (Write agent rejected)
- Rewrite S-24: rename, remove xfail, flip to `assert_tool_error`; add S-24b success companion for Admin agent
- No server code changes — `tools.rs:1456` is already correct

## Risk Assessment

Minimal. No Rust touched. Blast radius limited to two test files and one spec document. Admin agents
continue to succeed (Admin is superset of Write). The change correctly narrows what auto-enrolled
Write agents can do.

## Missing Test (retroactive)

The unit test at `tools.rs:8094` only checks the string value of `Capability::Admin.as_audit_str()`,
not which capability the handler actually enforces. A server-level integration test verifying
`audit_log.capability_used = 'admin'` after a quarantine call would have provided stronger coverage.
That gap is now closed by `test_quarantine_requires_admin_rejects_write_agent` (T-78 replacement).

## Knowledge Stewardship

**Queried:**
- `context_briefing` — entry #4375 (prior vnc-014 delivery lesson), #4360 (ADR-006: as_audit_str),
  #4365 (Capability exhaustive match) — all directly relevant
- `context_get(4375)`, `context_get(4360)` — full content retrieved

**Stored:**
- Entry #4411 — "Capability gate revert during delivery must propagate to all 4 dependent locations"
  (via `context_correct`, supersedes #4375; broadened to revert-propagation checklist)
