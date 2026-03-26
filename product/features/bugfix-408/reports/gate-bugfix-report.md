# Gate Bugfix Report: bugfix-408

> Gate: Bugfix Validation
> Date: 2026-03-26
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Fix addresses root cause | PASS | Constant updated 30→365 days; doc comment explains dormant-cycle rationale |
| No todo!/unimplemented!/TODO/FIXME/placeholders | PASS | None found in coaccess.rs or any changed file |
| All tests pass | PASS | 3671 unit + 20/20 smoke; 0 failures |
| No new clippy warnings | PASS | coaccess.rs clean; 2 pre-existing warnings in auth.rs/event_queue.rs not introduced by this fix |
| No unsafe code introduced | PASS | No `unsafe` blocks in coaccess.rs |
| Fix is minimal | PASS | Diff is exactly: constant value change + expanded doc comment + 1 regression test |
| New test catches original bug | PASS | `co_access_staleness_at_least_one_year` asserts `>= 365 * 24 * 3600` — would have failed on old value |
| Integration smoke tests passed | PASS | 20/20 smoke tests pass |
| xfail markers have GH Issues | PASS | All xfails pre-existing; GH#406 XPASS noted for follow-up (not introduced by this fix) |
| Knowledge stewardship — rust-dev | PASS | `408-agent-1-fix-report.md` contains `## Knowledge Stewardship` with Queried/Stored entries |
| Knowledge stewardship — tester | PASS | `408-agent-2-verify-report.md` and RISK-COVERAGE-REPORT.md both contain `## Knowledge Stewardship` with Queried/Stored entries |
| File size <= 500 lines | PASS | coaccess.rs is 315 lines |

## Detailed Findings

### Fix Addresses Root Cause
**Status**: PASS
**Evidence**: The diagnosed root cause is that `CO_ACCESS_STALENESS_SECONDS = 30 * 24 * 3600` prematurely discards co-access pairs accumulated during feature cycles that pause for weeks or months. The fix changes this to `365 * 24 * 3600` (31,536,000 seconds). The diff is exactly two logical changes: the constant value and an expanded doc comment with the rationale. This is a direct treatment of the identified constant — not a symptom workaround.

### No Placeholder Code
**Status**: PASS
**Evidence**: Grep for `unsafe|todo!|unimplemented!|TODO|FIXME|unwrap\(\)` in coaccess.rs returned zero matches.

### All Tests Pass
**Status**: PASS
**Evidence**: `cargo test --workspace` — all test suites return `ok` with 0 failures. Verified directly: 298 (unimatrix-engine), 2133 (unimatrix-server), 422, 144, 106, 73 and others — all pass. Aggregate consistent with the 3671 total reported by agent-2-verify. The 27 ignored tests are pre-existing.

### No New Clippy Warnings
**Status**: PASS
**Evidence**: `cargo clippy -p unimatrix-engine` produces 2 warnings — both in `auth.rs:113` (collapsible_if) and `event_queue.rs:164` (collapsible_if). These files are not touched by this fix. coaccess.rs produces zero warnings. The fix introduces no new lint issues.

### No Unsafe Code
**Status**: PASS
**Evidence**: coaccess.rs contains no `unsafe` keyword. Verified by direct grep.

### Fix Is Minimal
**Status**: PASS
**Evidence**: `git diff main` shows exactly one file changed (`crates/unimatrix-engine/src/coaccess.rs`). Within that file: the constant declaration and its doc comment are updated, and one regression test is added in the existing `#[cfg(test)]` block. No unrelated files, no dependency changes, no refactoring.

### New Test Would Have Caught the Original Bug
**Status**: PASS
**Evidence**: `co_access_staleness_at_least_one_year` asserts `CO_ACCESS_STALENESS_SECONDS >= 365 * 24 * 3600`. The original value was `30 * 24 * 3600 = 2_592_000`, which is less than `365 * 24 * 3600 = 31_536_000`. The assertion would have failed, catching the regression.

### Integration Smoke Tests
**Status**: PASS
**Evidence**: 20/20 smoke tests pass per agent-2-verify report and RISK-COVERAGE-REPORT.md.

### xfail Markers
**Status**: PASS
**Evidence**: All xfails are pre-existing (GH#405, GH#406, plus 2 unmarked pre-existing). No new xfail markers were added by this fix. The GH#406 XPASS (`test_search_multihop_injects_terminal_active` now passing) is flagged as a follow-up item — not caused by this fix and not masking a new failure.

### Knowledge Stewardship — rust-dev (408-agent-1-fix)
**Status**: PASS
**Evidence**: `product/features/bugfix-408/agents/408-agent-1-fix-report.md` contains `## Knowledge Stewardship` block with:
- `Queried:` — context_search for `co-access staleness maintenance background tick`, found entry #3553
- `Stored:` — "nothing novel to store — entry #3553 already documents this lesson"

### Knowledge Stewardship — tester (408-agent-2-verify)
**Status**: PASS
**Evidence**: `product/features/bugfix-408/agents/408-agent-2-verify-report.md` and `product/features/bugfix-408/testing/RISK-COVERAGE-REPORT.md` both contain `## Knowledge Stewardship` blocks with Queried/Stored entries referencing entries #2326, #3257, #3479.

### File Size
**Status**: PASS
**Evidence**: coaccess.rs is 315 lines — within the 500-line limit.

## Risk Coverage

| Risk | Test | Result |
|------|------|--------|
| R-01: Co-access signal lost after 30-day dormancy | `co_access_staleness_at_least_one_year` | PASS |
| R-02: Regression in other co-access behaviour | `test_co_access_training_improves_retrieval`, `test_status_report_with_adaptation_active`, `test_search_coac_signal_reaches_scorer` | PASS |
| R-03: Maintenance tick deletes valid pairs prematurely | `test_confidence_evolution_over_access`, `test_full_lifecycle_pipeline` | PASS |
| R-04: No guard against future threshold reduction | `co_access_staleness_at_least_one_year` (assert >= 365d) | PASS |

All 4 identified risks have full test coverage.

## Notes

- **GH#406 XPASS**: `test_search_multihop_injects_terminal_active` passes despite being marked xfail. This is unrelated to the staleness constant change. The Bugfix Leader should verify GH#406 and remove the xfail marker in a follow-up.
- **Pre-existing clippy errors**: 58 in `unimatrix-observe` + 2 in `unimatrix-engine` (`auth.rs`, `event_queue.rs`). None introduced by this fix. Pre-existing on `main`.

## Knowledge Stewardship

- Queried: context_search for recurring gate failure patterns in `co-access` and `validation` topics before writing report.
- Stored: nothing novel to store — this is a clean single-constant fix with complete coverage. No systemic gate failure pattern to record; the fix and its verification follow established procedures.
