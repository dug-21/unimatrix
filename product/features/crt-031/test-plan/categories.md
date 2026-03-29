# Test Plan: infra/categories/mod.rs (+ lifecycle.rs stub)

Component from IMPLEMENTATION-BRIEF.md §Component Map row 1.

---

## Pre-Implementation Step: Module Split Compile Check

Before adding any new code, `categories.rs` must be split to
`infra/categories/mod.rs + lifecycle.rs`. After the split and before any new code:

```bash
cargo check -p unimatrix-server
```

This must exit 0 with no errors. Any import failures indicate an incomplete re-export in
`mod.rs`. This is the R-04 compile gate. Failure here blocks all downstream component work.

---

## Risks Addressed

- **R-03** (Medium): Two independent RwLock fields — `add_category` always defaults to pinned.
- **R-04** (High): Module split must not change import paths.
- **R-06** (Low): `list_adaptive()` lock hygiene (used in background.rs stub).
- **R-09** (Low): `CategoryAllowlist::new()` delegation chain.
- **R-10** (High): Tests for new methods must be present before gate 3b.

---

## Unit Test Expectations

### Pre-existing Tests — Must Pass Without Modification (AC-12)

All tests in the existing `categories.rs` test module must be preserved exactly. Verify each
of these still passes after the module split:

- `test_validate_outcome`
- `test_validate_lesson_learned`
- `test_validate_decision`
- `test_validate_convention`
- `test_validate_pattern`
- `test_validate_procedure`
- `test_validate_duties`
- `test_validate_reference`
- `test_validate_unknown_rejected`
- `test_validate_case_sensitive`
- `test_validate_empty_string_rejected`
- `test_add_category_then_validate`
- `test_list_categories_sorted`
- `test_error_lists_all_valid_categories`
- `test_poison_recovery_validate`
- `test_poison_recovery_add_category`
- `test_poison_recovery_list_categories`
- `test_poison_recovery_data_integrity`
- `test_new_delegates_to_from_categories_initial`
- `test_new_allows_outcome_and_decision`

Verification: `cargo test -p unimatrix-server -- categories` passes showing all these names.

---

### New Tests: from_categories_with_policy (R-04, FR-05)

**`test_from_categories_with_policy_categories_accessible`**
- Arrange: `CategoryAllowlist::from_categories_with_policy(vec!["lesson-learned".to_string(), "decision".to_string()], vec!["lesson-learned".to_string()])`
- Assert: `validate("lesson-learned").is_ok()`, `validate("decision").is_ok()`, `validate("pattern").is_err()`

**`test_from_categories_with_policy_empty_adaptive`**
- Arrange: construct with categories `["lesson-learned"]`, adaptive `[]`
- Assert: `validate("lesson-learned").is_ok()`, `is_adaptive("lesson-learned") == false`
- Covers: E-01 (empty adaptive_categories behavior)

**`test_from_categories_with_policy_all_adaptive`**
- Arrange: all 5 INITIAL_CATEGORIES in both `cats` and `adaptive`
- Assert: `is_adaptive(cat) == true` for all 5 categories
- Covers: E-02 (all 5 categories marked adaptive)

**`test_from_categories_with_policy_duplicate_adaptive_deduplicates`**
- Arrange: `adaptive = ["lesson-learned", "lesson-learned"]`
- Assert: `list_adaptive()` returns `["lesson-learned"]` (one entry, not two)
- Covers: E-04 (duplicates silently deduplicated)

---

### New Tests: is_adaptive (AC-05, AC-06, AC-07, FR-07)

**`test_is_adaptive_lesson_learned_default_true`** (AC-05)
- Arrange: `from_categories_with_policy(INITIAL_CATEGORIES.to_vec(), vec!["lesson-learned".to_string()])`
- Assert: `al.is_adaptive("lesson-learned") == true`

**`test_is_adaptive_decision_default_false`** (AC-06)
- Arrange: same construction as AC-05
- Assert: `al.is_adaptive("decision") == false`

**`test_is_adaptive_unknown_category_false`** (AC-07)
- Arrange: same construction
- Assert: `al.is_adaptive("nonexistent-category") == false`, `al.is_adaptive("") == false`
- Note: unknown categories return `false` — not in adaptive set.

**`test_is_adaptive_case_sensitive`**
- Arrange: adaptive list `["lesson-learned"]`
- Assert: `al.is_adaptive("Lesson-Learned") == false`
- Covers: E-06 (case-sensitive matching)

**`test_is_adaptive_single_char_category`**
- Arrange: `from_categories_with_policy(vec!["x".to_string()], vec!["x".to_string()])`
- Assert: `al.is_adaptive("x") == true`
- Covers: E-03 (single-character category)

---

### New Tests: list_adaptive (R-06, FR-08 via background.rs)

**`test_list_adaptive_returns_sorted`**
- Arrange: `from_categories_with_policy(all_5, vec!["pattern".to_string(), "lesson-learned".to_string()])`
- Assert: `list_adaptive()` returns `["lesson-learned", "pattern"]` (alphabetically sorted)
- Covers: lock acquired once, result sorted for determinism

**`test_list_adaptive_empty_when_no_adaptive`**
- Arrange: adaptive `vec![]`
- Assert: `list_adaptive()` returns `vec![]`

**`test_list_adaptive_returns_all_adaptive`**
- Arrange: 3 adaptive categories from a 5-category allowlist
- Assert: `list_adaptive().len() == 3`, all 3 are present

---

### New Tests: Delegation Chain (AC-13, R-09)

**`test_new_is_adaptive_lesson_learned_true`** (AC-13)
- Arrange: `CategoryAllowlist::new()`
- Assert: `al.is_adaptive("lesson-learned") == true`
- Assert: `al.is_adaptive("decision") == false`
- Covers: `new()` → `from_categories()` → `from_categories_with_policy()` chain

**`test_from_categories_delegates_with_lesson_learned_adaptive`**
- Arrange: `CategoryAllowlist::from_categories(INITIAL_CATEGORIES.to_vec())`
- Assert: `al.is_adaptive("lesson-learned") == true`
- Covers: `from_categories` passes `["lesson-learned"]` as default adaptive

---

### New Tests: Pinned-by-Default for add_category (R-03)

**`test_add_category_defaults_to_pinned`** (R-03 scenario 1)
- Arrange: `CategoryAllowlist::new()`, then `al.add_category("custom".to_string())`
- Assert: `al.validate("custom").is_ok()`, `al.is_adaptive("custom") == false`
- Covers: runtime-added categories are pinned by design (ADR-001 decision)

**`test_validate_passes_is_adaptive_false_simultaneously`** (R-03 scenario 2)
- Arrange: `add_category("new-cat")` on a fresh allowlist
- Assert: `validate("new-cat").is_ok()` AND `is_adaptive("new-cat") == false` in same test
- Covers: the two behaviors are simultaneously correct (not contradictory)

---

### New Tests: Poison Recovery on adaptive Lock (AC-08, NFR-03)

**`test_poison_recovery_is_adaptive`** (AC-08)
- Arrange: `Arc::new(CategoryAllowlist::new())`, poison the `adaptive` RwLock by spawning a
  thread that acquires `al.adaptive.write().unwrap()` and then panics
- Assert: `al.is_adaptive("lesson-learned")` returns a value without panicking
- Pattern: mirrors `test_poison_recovery_validate` from the existing test suite

**`test_poison_recovery_list_adaptive`**
- Arrange: same poison setup as AC-08
- Assert: `al.list_adaptive()` returns a vec without panicking

---

## Edge Cases to Cover

| Edge Case | Test | Notes |
|-----------|------|-------|
| E-01: empty adaptive | `test_from_categories_with_policy_empty_adaptive` | is_adaptive always false |
| E-02: all 5 adaptive | `test_from_categories_with_policy_all_adaptive` | all labeled adaptive |
| E-03: single-char category | `test_is_adaptive_single_char_category` | string equality check |
| E-04: duplicate adaptive | `test_from_categories_with_policy_duplicate_adaptive_deduplicates` | HashSet deduplicates |

---

## Assertions Summary

```rust
// Constructor
assert!(al.validate("lesson-learned").is_ok());
assert_eq!(al.is_adaptive("lesson-learned"), true);
assert_eq!(al.is_adaptive("decision"), false);
assert_eq!(al.is_adaptive("nonexistent"), false);

// list_adaptive: sorted
let adaptive = al.list_adaptive();
for i in 1..adaptive.len() { assert!(adaptive[i] >= adaptive[i-1]); }

// add_category: always pinned
al.add_category("custom".to_string());
assert!(al.validate("custom").is_ok());
assert_eq!(al.is_adaptive("custom"), false);

// Poison recovery: no panic
// (thread-based setup identical to existing poison tests)
let result = al.is_adaptive("lesson-learned");  // must not panic
```

---

## Integration Test Expectations

No direct integration test needed for `CategoryAllowlist` alone — its behavior is observable
through `context_status` output (see status.md). The integration test in `test_tools.py`
validates end-to-end behavior.
