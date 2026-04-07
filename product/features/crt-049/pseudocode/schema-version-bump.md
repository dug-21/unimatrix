# Component 6: SUMMARY_SCHEMA_VERSION — `unimatrix-store/src/cycle_review_index.rs`

## Purpose

Bump `SUMMARY_SCHEMA_VERSION` from `2` to `3` to signal that stored `cycle_review_index.summary_json`
rows computed before crt-049 are stale. Update the forced-value assertion test and the
advisory message text. No other changes to this file.

---

## Scope of Changes

Three sub-changes, all in `crates/unimatrix-store/src/cycle_review_index.rs`:

1. The constant value: `2` → `3`
2. The advisory message text in `check_stored_review` (or wherever the stale-record check is)
3. The forced-value assertion test `CRS-V24-U-01`

---

## Change 1: Constant Bump

```
// CURRENT (line ~33):
/// ...
/// - crt-047: bumped 1 → 2 to trigger stale-record advisory for all rows
///     written before curation health columns were added.
pub const SUMMARY_SCHEMA_VERSION: u32 = 2;

// NEW:
/// ...
/// - crt-047: bumped 1 → 2 to trigger stale-record advisory for all rows
///     written before curation health columns were added.
/// - crt-049: bumped 2 → 3; adding explicit_read_count, explicit_read_by_category,
///     and redefining total_served (search exposures no longer contribute).
pub const SUMMARY_SCHEMA_VERSION: u32 = 3;
```

Bump policy (referenced in SPECIFICATION domain models): bump when any field is added,
removed, or renamed on `RetrospectiveReport` or any nested type affecting JSON round-trip
fidelity. Three qualifying changes in crt-049:
- `explicit_read_count` added
- `explicit_read_by_category` added
- `total_served` semantics changed (different value for same stored rows on re-review)

---

## Change 2: Advisory Message Text

Locate the stale-record advisory path in `context_cycle_review` handler in `tools.rs`
(or wherever `check_stored_review` compares `stored.schema_version` against
`SUMMARY_SCHEMA_VERSION`). The advisory message returned to callers when
`stored.schema_version < SUMMARY_SCHEMA_VERSION` must be updated.

```
// CURRENT advisory message (approximate):
"Stored review has schema_version {stored_version}, current is {SUMMARY_SCHEMA_VERSION}. \
 Use force=true to recompute."

// NEW advisory message (FR-10, R-03):
"Stored review has schema_version {stored_version} (current: {SUMMARY_SCHEMA_VERSION}). \
 schema_version 2 predates the explicit read signal and total_served redefinition \
 (search exposures no longer contribute to total_served); use force=true to recompute."
```

The message must specifically name the `total_served` semantic change (not merely
"schema version mismatch") — callers need to understand why the stored value differs
from a freshly computed value (SR-05, SPECIFICATION FR-10).

Note: If the advisory message is in `cycle_review_index.rs` rather than `tools.rs`,
update it there. Locate the exact path by grepping for the existing advisory text
before implementing.

---

## Change 3: Forced-Value Assertion Test (CRS-V24-U-01)

The existing test at the bottom of `cycle_review_index.rs` asserts the version constant:

```
// CURRENT test (line ~438):
#[test]
fn test_summary_schema_version_is_two() {
    assert_eq!(
        SUMMARY_SCHEMA_VERSION, 2u32,
        "SUMMARY_SCHEMA_VERSION must be 2 (bumped in crt-047)"
    );
}

// NEW test (CRS-V24-U-01):
#[test]
fn test_summary_schema_version_is_three() {
    assert_eq!(
        SUMMARY_SCHEMA_VERSION, 3u32,
        "SUMMARY_SCHEMA_VERSION must be 3 (bumped in crt-049: \
         added explicit_read_count, explicit_read_by_category, \
         redefined total_served)"
    );
}
```

Both the function name and the assertion message must be updated. The old test name
`test_summary_schema_version_is_two` must be renamed to `test_summary_schema_version_is_three`
(or similar) — leaving the old name active while the assertion is updated is acceptable
if the project has no test name convention against it.

---

## Why This Bump is Mandatory (AC-08)

If `SUMMARY_SCHEMA_VERSION` remains `2` after crt-049 ships:
- Any existing stored cycle review row is served from cache without recomputation.
- The served row lacks `explicit_read_count` (defaults to 0 via `#[serde(default)]`).
- The served row has `total_served` computed under old semantics (was aliased to `delivery_count`).
- No advisory is emitted — the caller receives a silently degraded response.

The bump is the only mechanism that surfaces the stale-record behavioral delta to callers.

---

## Error Handling

This component has no runtime error paths. The constant, its test, and the advisory
message are all static values. The advisory message path (`check_stored_review`) does
not change its logic — only its text changes.

---

## Key Test Scenarios

### AC-08 — Version constant assertion (updated CRS-V24-U-01)

```
Test: test_summary_schema_version_is_three
    assert_eq!(SUMMARY_SCHEMA_VERSION, 3u32, "<message>")
```

This test is required and non-negotiable per AC-08. It is the only test for this component.

### Advisory message text verification (from R-03)

```
// Not a unit test for this file, but required coverage in tools.rs integration tests:
// Simulate stored record with schema_version = 2.
// Assert context_cycle_review returns an advisory containing:
//   "explicit read signal" or "total_served redefinition" (semantic description)
// Assert the advisory does NOT merely say "schema version mismatch" (R-03 failure mode).
```

---

## Integration Surface

| Name | Type | Notes |
|------|------|-------|
| `SUMMARY_SCHEMA_VERSION` | `pub const u32 = 3` | Read by `check_stored_review` in tools.rs |
| `test_summary_schema_version_is_three` | test fn | Replaces `test_summary_schema_version_is_two` |
