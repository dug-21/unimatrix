# Gate Bugfix Report v2: bugfix-383

> Gate: Bug Fix Validation (Rework iteration 2 of 2)
> Date: 2026-03-25
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | All user-visible identifiers renamed; struct, rule_name, claim, recommendation, remediation text updated |
| No todo!/unimplemented!/FIXME placeholders | PASS | One intentional `// TODO(col-028):` comment — deferred SQLite migration, not a code placeholder |
| All tests pass | PASS | 0 failures across workspace; 422 observe tests, 2068 server tests all green |
| No new clippy warnings | PASS | Pre-existing clippy errors in friction.rs (lines 191, 231, 337, 385) confirmed present on main before fix via stash test |
| No unsafe code | PASS | No unsafe blocks in any changed file |
| Fix is minimal | WARN | 15 additional remediation arms for other rules — acceptable; required by new contract test and fixes latent defect |
| New tests would catch original bug | PASS | `test_all_default_rules_have_non_fallback_recommendation_and_remediation` in report.rs:647; `test_orphaned_calls_*` variants confirm rename |
| Skill documentation updated | PASS | Both skill files now show `orphaned_calls` at line 39 with correct description |
| Smoke tests pass | PASS | 10/10 smoke tests pass (protocol 3, lifecycle/adaptation 5, security 1, volume 1) |
| Knowledge stewardship — rust-dev (v2) | PASS | `383-agent-1-fix-v2-report.md` has `## Knowledge Stewardship` with `Queried:` and `Stored:` entries |
| Knowledge stewardship — investigator | PASS | `383-investigator-report.md` has `## Knowledge Stewardship` with `Queried:` and `Stored:` entries |
| Zero stale permission_retries in non-comment code | PASS | Only occurrence is in comment on friction.rs:18-19 documenting rename history |

## Detailed Findings

### Root Cause Addressed
**Status**: PASS
**Evidence**: `OrphanedCallsRule` struct (friction.rs:22), `fn name() -> "orphaned_calls"` (friction.rs:26), claim text updated, `recommendation_for()` match arm `"orphaned_calls"` (report.rs:65), `remediation_for_rule()` match arm `"orphaned_calls"` (recurring_friction.rs:109). Rename is complete across all five string-keyed sites.

### No Placeholders
**Status**: PASS
**Evidence**: `// TODO(col-028): rename field to orphaned_call_events once metric migration is complete` at metrics.rs:85 is an intentional deferred-work comment referencing a separate tracked issue. No `todo!()`, `unimplemented!()`, or `FIXME` markers present anywhere in observe or server crates.

### All Tests Pass
**Status**: PASS
**Evidence**:
- Workspace test run: all `test result: ok` across all crates, 0 failures
- observe crate: 422 passed, 0 failed
- server crate: 2068 passed, 0 failed
- detection_isolation integration tests: pass

### No New Clippy Warnings
**Status**: PASS
**Evidence**: `cargo clippy --workspace -- -D warnings` shows errors in `friction.rs` at lines 191, 231, 337, 385. Stash verification confirms these same lines appear on main before the fix (stash test showed identical error locations from main). The fix introduced zero new clippy errors.

### No Unsafe Code
**Status**: PASS
**Evidence**: `grep -n "unsafe"` on friction.rs returns no matches. No unsafe blocks in any of the 12 changed files.

### Fix Minimality — 15 Remediation Arms
**Status**: WARN (same as v1 gate)
**Evidence**: 15 new match arms in `remediation_for_rule()` for rules unrelated to the rename. These were required to satisfy the new contract test `test_all_default_rules_have_non_fallback_recommendation_and_remediation`. They fix a latent defect (all 22 rules fell through to generic fallback) and are low-risk string literals with no logic.
**Assessment**: WARN (not FAIL) — changes are semantically coupled to the same module and required by the new test.

### Skill Documentation Updated
**Status**: PASS
**Evidence**:
- `/workspaces/unimatrix/.claude/skills/uni-retro/SKILL.md` line 39: `` - `orphaned_calls` → tool invocations with no terminal event — check context overflow or parallel call management ``
- `/workspaces/unimatrix/packages/unimatrix/skills/retro/SKILL.md` line 39: identical correct text
Both files updated from stale `permission_retries` text.

### Smoke Tests Pass
**Status**: PASS
**Evidence**: 10/10 smoke tests pass:
- `test_initialize_returns_capabilities`, `test_server_info`, `test_graceful_shutdown` (protocol, 3)
- `test_store_search_find_flow`, `test_correction_chain_integrity`, `test_isolation_no_state_leakage`, `test_concurrent_search_stability` (lifecycle, 4)
- `test_cold_start_search_equivalence` (adaptation, 1)
- `test_injection_patterns_detected` (security, 1)
- `test_store_1000_entries` (volume, 1)

### Knowledge Stewardship — rust-dev (v2)
**Status**: PASS
**Evidence**: `product/features/bugfix-383/agents/383-agent-1-fix-v2-report.md` contains:
```
## Knowledge Stewardship
- Queried: `/uni-query-patterns` for `unimatrix-observe` — no new patterns relevant to this rework
- Stored: nothing novel to store — this rework was pure documentation corrections with no new implementation insight
```

### Knowledge Stewardship — Investigator
**Status**: PASS
**Evidence**: `product/features/bugfix-383/agents/383-investigator-report.md` contains:
```
## Knowledge Stewardship
Queried:
- Unimatrix #3446 (PermissionRetriesRule lesson — correction chain, confirmed col-027 deferred rename)
- Unimatrix #3419 (permission_friction_events pattern)
- Unimatrix #3477 (ADR-005 ToolFailureRule)
- Unimatrix #3476 (ADR-004 atomic fix)

Stored: nothing novel — orphaned-call semantics follow directly from existing lessons #3446 and #3476.
```

### Zero Stale permission_retries / PermissionRetriesRule in Non-Comment Code
**Status**: PASS
**Evidence**: Only occurrences in `.rs` files are comments on friction.rs:18-19 (`// -- Rule 1: OrphanedCallsRule (renamed from PermissionRetriesRule in #383) --`). Research spike docs `ass-015` and `ass-016` contain the old name as historical record — these are not operational code or agent-facing guidance and do not require update.

### New Tests Would Catch Original Bug
**Status**: PASS
**Evidence**:
- `test_all_default_rules_have_non_fallback_recommendation_and_remediation` (report.rs:647) iterates all 22 rules and asserts each has a non-fallback recommendation and remediation — would have caught the original `permission_retries` rule name not matching any arm
- `test_orphaned_calls_*` variants (friction.rs:481, 499, 513, 658, 677, 694) assert `rule_name == "orphaned_calls"` — would have caught the wrong name

## Knowledge Stewardship

- Stored: entry via `/uni-store-lesson` — "Skill documentation files are often omitted from a bugfix's file list despite being agent-facing and operationally equivalent to source code; validators must explicitly check all `SKILL.md` files referencing renamed identifiers." Topic: `validation`. Category: `lesson-learned`. Note: this became a recurring pattern (two skill files missed in iteration 1), warranting storage.
