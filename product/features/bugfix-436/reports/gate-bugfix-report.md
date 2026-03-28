# Gate Bugfix Report: bugfix-436

> Gate: Bugfix Validation
> Date: 2026-03-28
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | Duplicate constant eliminated; stale categories removed from both sites |
| No placeholders | PASS | No todo!(), unimplemented!(), TODO, FIXME in changed files |
| All tests pass | PASS | 3846 passed, 0 failed (full workspace) |
| No new clippy warnings | PASS | Changed files are clippy-clean; pre-existing warnings are in unrelated files (confirmed pre-existed before fix) |
| No unsafe code introduced | PASS | No unsafe blocks in any changed file |
| Fix is minimal | PASS | 7 files changed, all directly required by the category removal |
| New tests would catch original bug | PASS | test_validate_duties and test_validate_reference now assert .is_err(); test_category_allowlist_has_five_categories pins the count |
| Integration smoke tests passed | PASS | 20/20 smoke; 84/84 integration (plus pre-existing xfails) |
| xfail markers have GH Issues | PASS | One XPASS noted (test_search_multihop_injects_terminal_active / GH#406 — open, pre-existing, out of scope) |
| Knowledge stewardship — investigator | PASS | ## Knowledge Stewardship block present; Queried entries documented; Stored: nothing novel with reason |
| Knowledge stewardship — rust-dev | PASS | ## Knowledge Stewardship block in 436-agent-1-fix-report.md; Queried entry #3715; Stored: entry #3721 pattern |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

**Evidence**: The root cause was a duplicated `INITIAL_CATEGORIES` constant containing two stale categories (`duties`, `reference`). The fix:

1. Removed `duties` and `reference` from `categories.rs::INITIAL_CATEGORIES`, shrinking from `[&str; 7]` to `[&str; 5]` and making it `pub(crate)`.
2. Deleted the entire duplicate constant in `config.rs` and replaced it with an import: `use crate::infra::categories::INITIAL_CATEGORIES;`.
3. Removed both strings from `builtin_claude_code_pack()` in `unimatrix-observe/src/domain/mod.rs`.

The structural debt (two independent constants requiring lockstep updates) is resolved — `config.rs` now imports from the single source of truth. Confirmed via `grep` that `duties` and `reference` appear only in tests asserting their rejection.

### No Placeholders

**Status**: PASS

**Evidence**: Grep across all 7 changed files found zero occurrences of `todo!()`, `unimplemented!()`, `TODO`, or `FIXME`.

### All Tests Pass

**Status**: PASS

**Evidence**: `cargo test --workspace` output — all test suites returned `ok`, totaling 3846 passed, 0 failed. Confirmed by independent run during this validation (consistent with agent-2-verify report).

### No New Clippy Warnings

**Status**: PASS

**Evidence**: Clippy errors exist in the workspace but are pre-existing. Confirmed by stashing the fix commit and counting 55 pre-existing clippy errors. Agent-2-verify report attributes each error to commits predating this fix (`c5f4b54`, `f4d7fa9`, `8d4a791`, `f02a43b`). The three changed Rust files (`categories.rs`, `config.rs`, `domain/mod.rs`) produce no clippy output.

### No Unsafe Code

**Status**: PASS

**Evidence**: Grep for `unsafe` across `categories.rs`, `config.rs`, and `domain/mod.rs` returned no matches.

### Fix is Minimal

**Status**: PASS

**Evidence**: 7 files changed. Every change is directly attributable to the category removal:
- `categories.rs` / `config.rs` — primary constants and their tests
- `domain/mod.rs` / `domain_pack_tests.rs` — observe pack's hardcoded category list
- `generators.py` — test harness category pool
- `test_tools.py` / `test_lifecycle.py` — two test calls that used `"duties"` as a valid category

No unrelated changes are bundled. The architectural debt elimination (duplicate constant → import) is on the required fix path, not a scope addition.

### New Tests Would Catch Original Bug

**Status**: PASS

**Evidence**:
- `test_validate_duties` and `test_validate_reference` now assert `.is_err()` — had these existed before the bug was introduced, they would have failed immediately when `duties` and `reference` were added to `INITIAL_CATEGORIES`.
- `test_category_allowlist_has_five_categories` pins the exact count at 5 — any future unreviewed addition would fail this test.
- `test_error_lists_all_valid_categories` now has explicit negative assertions for both removed categories.

### Integration Smoke Tests Passed

**Status**: PASS

**Evidence**: Agent-2-verify report documents 20/20 smoke, 9/9 adaptation, 35/35 tools (relevant subset), 40/40 lifecycle (plus pre-existing xfails). Full workspace unit test run confirms 3846 passed.

### xfail Markers Have GH Issues

**Status**: PASS

**Evidence**: One XPASS detected (`test_search_multihop_injects_terminal_active`) against xfail marker referencing GH#406, which is confirmed open. This is unrelated to the fix (correction chain traversal, not category allowlist). The verify agent correctly flagged it for separate cleanup. The existing pre-existing xfails all reference open issues per the verify report.

### Knowledge Stewardship — Investigator

**Status**: PASS

**Evidence**: Investigator report (GH#436 comment by dug-21, first comment) contains:
```
## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — entries #2312, #3523 consulted
- Stored: nothing novel — pattern already documented by outcome retirement (ADR-005/crt-025)
```
Queried entries and reason for no-store are both documented.

### Knowledge Stewardship — Rust-Dev

**Status**: PASS

**Evidence**: `436-agent-1-fix-report.md` contains:
```
## Knowledge Stewardship
- Queried: entry #3715 (lockstep update requirement)
- Stored: entry #3721 "Retiring INITIAL_CATEGORIES requires updates across 5 locations" via /uni-store-pattern
```

Verify agent (`436-agent-2-verify-report.md`) also contains a stewardship block: Queried entries #3715/#3721; Stored: nothing novel with reason (fix-agent already stored the pattern).

## Rework Required

None.

## Notes

- The `outcome` category remains in `builtin_claude_code_pack()` in `domain/mod.rs` (and its corresponding test). This is a pre-existing inconsistency from ADR-005/crt-025, explicitly called out as out of scope by both the investigator and rust-dev agents. It does not affect the correctness of this fix — the domain pack's `categories` field serves a different purpose than `INITIAL_CATEGORIES`.
- `cargo audit` is not installed in this environment; CVE check could not be run. This is a pre-existing gap in the environment, not introduced by this fix. The changes are additive-removals with no new dependencies.

## Knowledge Stewardship

- Stored: nothing novel to store — no recurring gate failure pattern observed here; the fix was clean and complete on first pass.
