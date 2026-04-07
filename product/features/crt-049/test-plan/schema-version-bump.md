# Test Plan: SUMMARY_SCHEMA_VERSION (cycle_review_index.rs)

**File**: `crates/unimatrix-store/src/cycle_review_index.rs`
**Test module**: existing `#[cfg(test)] mod tests` block
**Existing test name**: `test_summary_schema_version_is_two` (CRS-V24-U-01)

---

## Risks Covered

| Risk | AC | Priority |
|------|----|----------|
| R-08: SUMMARY_SCHEMA_VERSION not bumped | AC-08 | High |
| R-03: Stale-record advisory message wording | AC-08 (partial) | High |
| F-04: Advisory not triggered | AC-08 (partial) | Medium |

---

## AC-08: CRS-V24-U-01 Assertion Update (Non-Negotiable)

The existing test `test_summary_schema_version_is_two` is a forced-value assertion test.
It must be updated to assert version `3`.

**Updated Test: `test_summary_schema_version_is_three` (rename of `test_summary_schema_version_is_two`)**

```
// CRS-V24-U-01 (updated by crt-049): SUMMARY_SCHEMA_VERSION is 3
#[test]
fn test_summary_schema_version_is_three() {
    assert_eq!(
        SUMMARY_SCHEMA_VERSION, 3u32,
        "SUMMARY_SCHEMA_VERSION must be 3 (bumped in crt-049 for explicit_read_count \
         addition and total_served redefinition)"
    );
}
```

Both the constant value (`= 3`) and the test assertion (`3u32`) must be updated together.
Updating one without the other is incomplete (R-08 coverage requirement).

The test name change (from `_is_two` to `_is_three`) is required to keep the test
self-documenting. If the name cannot change due to gate grep patterns referencing it,
add the version number to the assertion message and keep the grep-target name as-is.
Prefer renaming.

---

## Advisory Message Verification (R-03)

The advisory message emitted when `stored.schema_version < SUMMARY_SCHEMA_VERSION` must
communicate the semantic change, not merely the version number. The specification requires:

> "schema_version 2 predates the explicit read signal and total_served redefinition
> (search exposures no longer contribute to total_served); use force=true to recompute"

**Structural check (code review, not a test assertion)**:
- Search for the advisory message string in `cycle_review_index.rs` or `tools.rs`
  (wherever the stale-record advisory is generated).
- Assert the message text names "explicit read signal" and "total_served" specifically
  (not just "schema version mismatch" or a generic version number comparison message).

This is a code-review check because the advisory text is not returned through a stable
public API that a unit test can intercept cleanly. If feasible, a unit test that calls
`check_stored_review` with a `schema_version = 2` record and asserts the returned advisory
string contains "total_served" is preferred.

---

## R-08 Completeness: Both Must Be Updated Together

| Item | Must Change | Verification |
|------|------------|--------------|
| `pub const SUMMARY_SCHEMA_VERSION: u32 = 3` | Yes | `grep SUMMARY_SCHEMA_VERSION cycle_review_index.rs` shows `3` |
| `test_summary_schema_version_is_three` assertion | Yes | `cargo test CRS-V24-U-01` (or new test name) passes |
| Advisory message text | Yes | Code review: message text names the semantic change |

Updating the constant without the test is caught by the existing test (it was asserting 2,
now fails). Updating the test without the constant is caught by the assertion itself.

---

## Other Existing Tests: No Change Required

The following existing tests use `SUMMARY_SCHEMA_VERSION` as a field value (not as an
assertion target):

- `test_cycle_review_record_round_trip` — sets `schema_version: SUMMARY_SCHEMA_VERSION`
- `test_store_cycle_review_4mb_ceiling_exceeded` — same pattern
- `test_store_cycle_review_4mb_ceiling_boundary` — same pattern
- `test_store_and_get_cycle_review_round_trip` — asserts `schema_version == SUMMARY_SCHEMA_VERSION`
- Various other store round-trip tests

These tests use the constant symbolically. After the constant is bumped to `3`, they
automatically reflect the new value. No modifications needed to these tests.

---

## Expected Test Count Delta

- 1 test renamed/updated: `test_summary_schema_version_is_two` → `test_summary_schema_version_is_three`
- 0 new tests (the existing forced-value assertion pattern is sufficient)
- Net: 0 new tests, 1 updated test in `crates/unimatrix-store/src/cycle_review_index.rs`
