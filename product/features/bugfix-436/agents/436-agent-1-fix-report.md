# Agent Report: 436-agent-1-fix

## Summary

Implemented the approved fix for GH #436: removed `duties` and `reference` from `INITIAL_CATEGORIES`, made the constant `pub(crate)`, and eliminated the duplicate constant in `config.rs` by importing from `categories` instead.

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/infra/categories.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/infra/config.rs`
- `/workspaces/unimatrix/crates/unimatrix-observe/src/domain/mod.rs`
- `/workspaces/unimatrix/crates/unimatrix-observe/tests/domain_pack_tests.rs`
- `/workspaces/unimatrix/product/test/infra-001/harness/generators.py`
- `/workspaces/unimatrix/product/test/infra-001/suites/test_tools.py`
- `/workspaces/unimatrix/product/test/infra-001/suites/test_lifecycle.py`

## Changes Made

### categories.rs
- `const INITIAL_CATEGORIES: [&str; 7]` → `pub(crate) const INITIAL_CATEGORIES: [&str; 5]`
- Removed `"duties"` and `"reference"` entries and their inline comments
- Updated doc comment: "initial 7" → "initial 5"
- `test_category_allowlist_has_seven_categories` → `test_category_allowlist_has_five_categories` (assert == 5)
- `test_all_remaining_seven_categories_valid` → `test_all_remaining_five_categories_valid` (removed duties/reference from list)
- `test_validate_duties`: `.is_ok()` → `.is_err()`
- `test_validate_reference`: `.is_ok()` → `.is_err()`
- `test_error_lists_all_valid_categories`: removed duties/reference positive asserts; added negative asserts
- Fixed all 4 hardcoded `7` count assertions → `5`: test_validate_unknown_rejected, test_list_categories_sorted, test_poison_recovery_list_categories, test_outcome_category_validate_err

### config.rs
- Added `use crate::infra::categories::INITIAL_CATEGORIES;` import
- Deleted the entire `pub const INITIAL_CATEGORIES: [&str; 7]` block (the duplicate)
- Updated doc comment: "7 INITIAL_CATEGORIES" → "5 INITIAL_CATEGORIES"
- `test_observation_config_absent_section_is_default`: removed `"duties"` and `"reference"` from hardcoded TOML categories string
- `test_default_config_toml_empty_parses_defaults`: already references `INITIAL_CATEGORIES` by name; now resolves via the new import

### unimatrix-observe/src/domain/mod.rs
- Removed `"duties".to_string()` and `"reference".to_string()` from `builtin_claude_code_pack()` categories vec
- Updated comment: "All 8 INITIAL_CATEGORIES" → "All 5 active INITIAL_CATEGORIES"

### domain_pack_tests.rs
- `test_builtin_pack_has_all_initial_categories`: removed duties/reference from expected array; added negative assertions confirming they are absent; updated comment

### generators.py
- Removed `"duties"` and `"reference"` from `CATEGORIES` list

### test_tools.py / test_lifecycle.py
- Replaced `context_store(..., "duties", ...)` with `"convention"` in both briefing-related tests

## Test Results

- `cargo test -p unimatrix-server`: 2270 + 46 + 16 + 16 + 7 = **2355 passed, 0 failed**
- `cargo test -p unimatrix-observe`: 423 + 22 + 44 + 6 = **495 passed, 0 failed**
- `cargo clippy` on modified packages: no warnings in modified files (pre-existing warnings in unimatrix-engine and unmodified observe files are out of scope)

## Tests Renamed

- `test_category_allowlist_has_seven_categories` → `test_category_allowlist_has_five_categories`
- `test_all_remaining_seven_categories_valid` → `test_all_remaining_five_categories_valid`

## Issues

None. All changes were straightforward per the approved fix approach.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entry #3715 surfaced confirming the two constants must be updated in lockstep (lesson-learned from a prior encounter with this exact scenario)
- Stored: entry #3721 "Retiring INITIAL_CATEGORIES requires updates across 5 locations — grep for the count before assuming only 2" via `/uni-store-pattern` — captures the full set of touch points and the hardcoded-count trap (4 sites, not 2)
