# Gate Bugfix Report: bugfix-340

> Gate: Bugfix Validation
> Date: 2026-03-22
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | All three `s.len()` sites replaced with `s.chars().count()` |
| No stubs / placeholders | PASS | No `todo!()`, `unimplemented!()`, TODO, FIXME in changed file |
| Tests pass | PASS | 1915 tests, 0 failed (unimatrix-server full suite) |
| No new clippy warnings | PASS | 0 errors in unimatrix-server; pre-existing warnings in unimatrix-store unchanged |
| No unsafe code | PASS | No `unsafe` blocks in validation.rs |
| Fix is minimal | PASS | Diff: exactly 3 one-line changes + 90 lines of new tests, nothing else |
| New tests catch original bug | PASS | 8 tests exercise the exact multibyte boundary at and past the limit |
| Integration smoke tests | PASS | 20/20 passed (per tester report) |
| xfail markers | PASS | No xfail markers added; none needed |
| Knowledge stewardship | PASS | Both investigator (#3103) and rust-dev (#3105) entries confirmed in Unimatrix |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

**Evidence**: `git diff HEAD~1` shows exactly three substitutions in `validation.rs`:

- Line 40 (`check_length`): `value.len() > max` → `value.chars().count() > max`
- Line 431 (`validate_cycle_params`, outcome field): `s.len() > MAX_OUTCOME_LEN` → `s.chars().count() > MAX_OUTCOME_LEN`
- Line 459 (`validate_phase_field`): `normalized.len() > MAX_PHASE_LEN` → `normalized.chars().count() > MAX_PHASE_LEN`

Line 369 (`is_valid_feature_id`) was correctly left unchanged — it enforces `is_ascii_alphanumeric()` on every character, so only ASCII bytes are ever present and `len()` == char count there.

### No Stubs / Placeholders

**Status**: PASS

Grep over `validation.rs` found no `todo!()`, `unimplemented!()`, TODO, FIXME, or placeholder markers outside the test module. All `unwrap()` calls are inside `#[cfg(test)]`, which is acceptable per project rules.

### Tests Pass

**Status**: PASS

Full `unimatrix-server` test suite: 1915 passed, 0 failed (across 6 test binaries). The 8 new tests are registered and present:

- `infra::validation::tests::test_check_length_multibyte_at_max_passes`
- `infra::validation::tests::test_check_length_multibyte_over_max_rejected`
- `infra::validation::tests::test_validate_outcome_multibyte_at_max_passes`
- `infra::validation::tests::test_validate_outcome_multibyte_over_max_rejected`
- `infra::validation::tests::test_validate_phase_multibyte_at_max_passes`
- `infra::validation::tests::test_validate_phase_multibyte_over_max_rejected`
- `infra::validation::tests::test_validate_next_phase_multibyte_at_max_passes`
- `infra::validation::tests::test_validate_next_phase_multibyte_over_max_rejected`

### No New Clippy Warnings

**Status**: PASS

`cargo clippy --package unimatrix-server` produced 0 errors. The 19 pre-existing warnings in `unimatrix-store` are tracked under GH#342 and pre-date this fix.

### No Unsafe Code

**Status**: PASS

Grep confirms zero `unsafe` blocks in `validation.rs`.

### Fix is Minimal

**Status**: PASS

The diff contains only the targeted changes: 3 one-line substitutions and 90 lines of new test code. No refactoring, formatting, or unrelated changes are bundled.

### New Tests Would Have Caught the Original Bug

**Status**: PASS

Each "at_max_passes" test constructs a string of `MAX_X` emoji characters (4 bytes each). Under the old `s.len()` code, `MAX_TITLE_LEN * 4 = 800 bytes > 200` would have caused a false rejection — the test would have failed. Each "over_max_rejected" test confirms the boundary is enforced correctly after the fix. These tests directly encode the regression condition.

### Integration Smoke Tests

**Status**: PASS (per tester report — 20/20 passed, cycle/validation/unicode suites 35/35)

### xfail Markers

**Status**: PASS

No xfail markers were added. No new test failures were encountered that would require them.

### Knowledge Stewardship

**Status**: PASS

- Entry #3103: "str::len() Returns Bytes, Not Chars — Use chars().count() for Character Limits" (lesson-learned) — confirmed active in Unimatrix.
- Entry #3105: "Use chars().count() not len() for character limits in validation.rs" (pattern) — confirmed active in Unimatrix.

Both investigator and rust-dev agents queried existing entries (#604, #308, #3103) before implementing, fulfilling the `Queried:` obligation. Both stored novel findings.

## File Size Note

`validation.rs` is 1775 lines, which exceeds the 500-line guideline. This is pre-existing (the file was already over 500 lines before this fix). This fix adds 90 test lines which is justified and traceable. The pre-existing oversize condition is not introduced by this fix and should be tracked separately.

## Rework Required

None.

## Knowledge Stewardship

- Stored: nothing novel to store -- single-feature gate result; no recurring cross-feature pattern identified beyond what investigator/rust-dev already stored in #3103 and #3105.
