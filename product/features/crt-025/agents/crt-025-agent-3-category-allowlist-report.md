# Agent Report: crt-025-agent-3-category-allowlist

## Task

Remove `"outcome"` from `INITIAL_CATEGORIES` in `crates/unimatrix-server/src/infra/categories.rs` per ADR-005 (block new ingest only — no data deletion). `CategoryAllowlist::new()` must have exactly 7 categories after this change.

## Files Modified

- `crates/unimatrix-server/src/infra/categories.rs`

## Changes Made

### Production code

- `INITIAL_CATEGORIES` array size changed from `[&str; 8]` to `[&str; 7]`; `"outcome"` entry removed
- Doc comment on `new()` updated: "initial 8 categories" → "initial 7 categories"
- No changes to `validate`, `add_category`, `list_categories`, or `from_categories` functions (per pseudocode spec)

### Existing tests updated (inversions per pseudocode Component 10)

| Test | Before | After |
|------|--------|-------|
| `test_validate_outcome` | `is_ok()` | `is_err()` |
| `test_validate_unknown_rejected` | `valid_categories.len() == 8` | `== 7` |
| `test_list_categories_sorted` | `list.len() == 8` | `== 7` |
| `test_error_lists_all_valid_categories` | `contains("outcome")` asserted present | asserted absent |
| `test_poison_recovery_validate` | `validate("outcome").is_ok()` | `is_err()`; added `validate("decision").is_ok()` |
| `test_poison_recovery_list_categories` | `contains("outcome")`, `len >= 8` | `!contains("outcome")`, `len >= 7` |
| `test_poison_recovery_data_integrity` | `contains("outcome")` | `!contains("outcome")` |
| `test_new_allows_outcome_and_decision` | `validate("outcome").is_ok()` | `is_err()` |

### New tests added (from test plan)

- `test_category_allowlist_has_seven_categories` — AC-15, FR-08.2
- `test_outcome_category_is_not_in_allowlist` — AC-15
- `test_outcome_category_validate_err` — AC-15; verifies error carries `valid_categories.len() == 7` and does not contain `"outcome"`
- `test_all_remaining_seven_categories_valid` — R-03 regression guard; iterates all 7 remaining categories
- `test_only_outcome_removed_not_others` — R-03 surgical-removal check
- `test_category_allowlist_poison_recovery` — poison recovery path also uses updated INITIAL_CATEGORIES

## Tests

**31 passed / 0 failed** (`cargo test --package unimatrix-server --lib infra::categories`)

- 25 pre-existing tests (all passing, 8 with inverted assertions)
- 6 new tests from the test plan

## Constraints Verified

- C-11: No data deleted — only `INITIAL_CATEGORIES` modified; `validate`, `from_categories`, etc. unchanged
- ADR-005: Existing entries with `category = "outcome"` are not touched; only new ingest is blocked
- `outcome_tags.rs` retained (removal tracked in GH #338, not in scope)

## Issues Encountered

During development, `unimatrix-observe` had a pre-existing compile error (`RetrospectiveReport` initializer in `src/report.rs` missing `phase_narrative: None` — added by the phase-narrative agent's concurrent work). That initializer was already fixed by the time I attempted to restore it. The `unimatrix-server` build was clean at commit time.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — found entry #2312 re: `boosted_categories` validation gotcha; not relevant to this change. No category-allowlist-specific gotchas found.
- Stored: nothing novel to store — the allowlist inversion pattern is a straightforward constant change; behaviour is fully visible in source and test diffs.
