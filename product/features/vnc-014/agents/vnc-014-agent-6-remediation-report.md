# Agent Report: vnc-014-agent-6-remediation

## Task
Remove DELETE paths on `audit_log` that conflict with the append-only triggers introduced in schema v25 (ADR-005).

## Files Modified

- `crates/unimatrix-store/src/retention.rs`
- `crates/unimatrix-server/src/import/mod.rs`

## Changes Made

### retention.rs — `gc_audit_log()` made a no-op

Replaced the `DELETE FROM audit_log WHERE timestamp < ...` implementation with a no-op that:
- Logs a WARN explaining the deferral (retention_days appears in the structured field)
- Returns `Ok(0)` unconditionally
- Retains the function signature so callers in `services/status.rs` continue to compile

### retention.rs — Tests rewritten

Two tests that asserted positive deletion counts were rewritten:

| Old test name | New test name | Change |
|---|---|---|
| `test_gc_audit_log_retention_boundary` | `test_gc_audit_log_noop` | Asserts `Ok(0)` returned and rows are unchanged |
| `test_gc_audit_log_epoch_row_deleted` | `test_gc_audit_log_epoch_row_preserved` | Asserts row survives the no-op call |

The two tests `test_gc_protected_tables_regression` and `test_gc_protected_tables_row_level` both call `gc_audit_log(180)` — these continue to pass because `Ok(0)` is a valid return, and the tests only assert on other (non-audit_log) protected tables.

### import/mod.rs — `DELETE FROM audit_log` removed from `drop_all_data()`

Removed the single `DELETE FROM audit_log;` line from the multi-statement batch delete. Added an explanatory comment referencing vnc-014 / ASS-050 / ADR-005 per spec requirement.

## Test Results

All 14 retention tests pass (0 failures):
- `test_gc_audit_log_noop` — ok
- `test_gc_audit_log_epoch_row_preserved` — ok
- `test_gc_protected_tables_regression` — ok
- `test_gc_protected_tables_row_level` — ok
- 10 other retention tests — ok

## Build Status

`unimatrix-store` compiles cleanly. `unimatrix-server` has 22 pre-existing errors from parallel swarm agents' work on `AuditEvent` struct fields and `ToolContext` — these are not caused by this agent's changes. My changes (retention.rs, import/mod.rs) contribute zero new errors.

## Blockers

None.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- not invoked (focused remediation task with complete spec in pseudocode/remediation.md; no ambiguity requiring Unimatrix lookup)
- Stored: nothing novel to store -- the no-op pattern and the test rewrite approach are both specified explicitly in pseudocode/remediation.md; no runtime gotchas discovered beyond what the spec described.
