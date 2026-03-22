# Test Plan: CategoryAllowlist (Component 10)

File: `crates/unimatrix-server/src/infra/categories.rs`
Risks: R-03, AC-15, FR-08, ADR-005

---

## Unit Test Expectations

All inline `#[cfg(test)]` functions. Focus: confirming `"outcome"` is removed from
`INITIAL_CATEGORIES` and no other categories were accidentally removed.

### Category Count (AC-15, FR-08.2)

**`test_category_allowlist_has_seven_categories`** (AC-15)
- Arrange: `let al = CategoryAllowlist::new();`
- Assert: count of categories == 7 (not 8)
- Note: previously 8 categories with "outcome"; after retirement this is 7

### Outcome Rejection (AC-15, FR-08.1, FR-08.3)

**`test_outcome_category_is_not_in_allowlist`** (AC-15)
- Assert: `al.validate("outcome").is_err()`

**`test_outcome_category_validate_err`** (AC-15 — replaces old `test_validate_outcome_ok`)
- Assert: `al.validate("outcome")` returns `Err(...)` with meaningful error message

### Remaining 7 Categories Are Valid (R-03 — regression guard)

**`test_all_remaining_seven_categories_valid`** (R-03)
- Assert each of the 7 remaining valid categories passes `al.validate(...)`:
  - `"decision"` → `Ok(())`
  - `"convention"` → `Ok(())`
  - `"pattern"` → `Ok(())`
  - `"procedure"` → `Ok(())`
  - `"lesson-learned"` → `Ok(())`
  - `"duties"` → `Ok(())`
  - `"issue"` → `Ok(())` (or whatever the 7th is — verify against implementation)

This test prevents a regression where removing `"outcome"` accidentally removed another
category.

**`test_only_outcome_removed_not_others`** (R-03)
- Assert: `al.validate("decision").is_ok()`
- Assert: `al.validate("convention").is_ok()`
- Assert: `al.validate("outcome").is_err()`
- Combined as a focused regression: the removal is surgical

### Poison Recovery Behavior (from Risk Strategy — CategoryAllowlist poison recovery test)

**`test_category_allowlist_poison_recovery`** (existing behavior preserved)
- If the `RwLock` is poisoned and `.unwrap_or_else(|e| e.into_inner())` recovery is exercised,
  the allowlist must still not include "outcome"
- This verifies that the poison recovery path also uses the updated INITIAL_CATEGORIES

### Existing Entries Not Deleted (FR-08.4, C-11)

This is not a unit test on the allowlist itself, but a constraint that MUST be documented:

**`test_existing_outcome_entries_queryable_after_retirement`** (integration, not unit)
- Arrange: store contains entries with `category = "outcome"` (seeded before retirement)
- Assert: `context_search` or `context_get` can still retrieve those entries
- Assert: `context_store(category="outcome")` fails with `InvalidCategory`
- The `context_search` succeeds because search does not filter by allowlist; it returns
  whatever is in the store

---

## Integration Test Expectations

### infra-001 `tools` suite

**`test_cycle_outcome_category_rejected`** (AC-15, FR-08.3)
- Send `context_store(content="x", topic="testing", category="outcome", agent_id="human")`
- Assert: error response with category-related error message (not success)

### infra-001 `adaptation` suite (update existing)

Scan `suites/test_adaptation.py` for any test that uses `category="outcome"`:
- If found: update to use a valid category (e.g., `"convention"`) or assert error if testing
  category validation
- Document any changes in the RISK-COVERAGE-REPORT.md

### infra-001 `lifecycle` suite (scan for outcome usage)

- Scan `suites/test_lifecycle.py` for `category="outcome"` usage
- Update as needed to avoid false failures from the retirement

---

## Tests That Must Be Updated

The ARCHITECTURE.md explicitly identifies these tests as requiring updates (FR-08.6, R-03):

| Old Test Name | What to Change | New Assertion |
|---------------|----------------|---------------|
| `test_validate_outcome` (or similar) | Was asserting `is_ok()` | Now assert `is_err()` |
| `test_new_allows_outcome_and_decision` (or similar) | Included `outcome` as valid | Remove `outcome` from valid set |
| `test_list_categories_sorted` (or similar) | Expected 8 categories | Now expect 7 |
| Any `test_poison_recovery_validate` | Validated `outcome` | Now validate a different category |

The exact test names depend on the current `categories.rs` test module. During Stage 3b
(implementation), the implementer must identify and update all such tests.

---

## Assertions Summary

| Assertion | Evidence |
|-----------|----------|
| `CategoryAllowlist::new()` has 7 categories | `test_category_allowlist_has_seven_categories` |
| `"outcome"` is not in allowlist | `test_outcome_category_is_not_in_allowlist` |
| `al.validate("outcome")` returns `Err` | `test_outcome_category_validate_err` |
| All other 7 categories remain valid | `test_all_remaining_seven_categories_valid` |
| `context_store(category="outcome")` returns error | infra-001 `test_cycle_outcome_category_rejected` |
| Existing `outcome` entries remain queryable | `test_existing_outcome_entries_queryable_after_retirement` |
