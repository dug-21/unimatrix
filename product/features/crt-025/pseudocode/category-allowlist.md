# Component 10: CategoryAllowlist
## File: `crates/unimatrix-server/src/infra/categories.rs`

---

## Purpose

Remove `"outcome"` from `INITIAL_CATEGORIES`. After this change, new `context_store` calls with `category = "outcome"` are rejected with `ServerError::InvalidCategory`. Existing database entries with `category = "outcome"` are not touched — only new ingest is blocked (ADR-005, C-11, FR-08).

---

## Modified Constant

```
// BEFORE:
const INITIAL_CATEGORIES: [&str; 8] = [
    "outcome",
    "lesson-learned",
    "decision",
    "convention",
    "pattern",
    "procedure",
    "duties",
    "reference",
];

// AFTER:
const INITIAL_CATEGORIES: [&str; 7] = [
    "lesson-learned",
    "decision",
    "convention",
    "pattern",
    "procedure",
    "duties",
    "reference",
];
```

Array size changes from 8 to 7. The `"outcome"` entry is removed. No other code changes in this file.

---

## Cascading Changes: Test Updates Required

The following tests in `categories.rs` assert `outcome` is valid. All must be updated to assert it is invalid:

### `test_validate_outcome`

```
// BEFORE:
assert!(al.validate("outcome").is_ok());

// AFTER:
assert!(al.validate("outcome").is_err());
```

### `test_validate_unknown_rejected`

```
// BEFORE: asserts valid_categories.len() == 8
// AFTER:  asserts valid_categories.len() == 7
```

### `test_list_categories_sorted`

```
// BEFORE: asserts list.len() == 8
// AFTER:  asserts list.len() == 7
```

### `test_error_lists_all_valid_categories`

```
// BEFORE: includes assert!(valid_categories.contains(&"outcome".to_string()))
// AFTER:  remove that assert (outcome is no longer valid)
```

### `test_new_allows_outcome_and_decision`

```
// BEFORE: assert!(al.validate("outcome").is_ok(), "outcome must be in default allowlist")
// AFTER:  assert!(al.validate("outcome").is_err(), "outcome must not be in default allowlist")
//         (keep existing assertions for "decision", "pattern", "lesson-learned" — those still pass)
```

### `test_poison_recovery_validate`

```
// BEFORE: assert!(al.validate("outcome").is_ok());
// AFTER:  assert!(al.validate("outcome").is_err());
```

### `test_poison_recovery_list_categories`

```
// BEFORE: assert!(list.contains(&"outcome".to_string()));
// AFTER:  remove or invert that assert
// AFTER:  assert!(list.len() >= 7);   (was >= 8)
```

### `test_poison_recovery_data_integrity`

```
// BEFORE: assert!(list.contains(&"outcome".to_string()));
// AFTER:  assert!(!list.contains(&"outcome".to_string()));  // outcome no longer default
```

### `test_new_delegates_to_from_categories_initial`

The loop iterates `INITIAL_CATEGORIES` and checks both instances agree. Since `INITIAL_CATEGORIES` no longer includes `"outcome"`, the loop will not check `"outcome"` at all. Test passes without change, but add a comment noting outcome is no longer in defaults.

---

## No Changes Required

- `validate`, `add_category`, `list_categories`, `from_categories` functions: **unchanged**.
- `outcome_tags.rs`: **retained** (its removal is tracked in GH #338, not in scope here).
- Existing `outcome`-category entries in the database: **not touched**.
- `context_search`, `context_lookup`, `context_get` on existing `outcome` entries: **unaffected** (allowlist only gates ingest, not retrieval).

---

## Behavior After Change

```
CategoryAllowlist::new():
    → valid categories: ["convention", "decision", "duties", "lesson-learned",
                         "pattern", "procedure", "reference"]  (7 total, sorted)

al.validate("outcome")   → Err(InvalidCategory { category: "outcome", valid_categories: [...7...] })
al.validate("decision")  → Ok(())
context_store(category="outcome")  → ServerError::InvalidCategory returned to caller
context_search(category="outcome") → unchanged (search filters, not allowlist)
```

---

## Error Handling

No new error paths. The existing `validate` function already returns `ServerError::InvalidCategory` for unknown categories. After this change, `"outcome"` simply falls into the unknown category branch.

---

## Key Test Scenarios

1. `CategoryAllowlist::new()` → `list_categories().len() == 7`
2. `al.validate("outcome")` → `Err(InvalidCategory { ... })`
3. All other 7 categories validate successfully (regression guard)
4. `al.validate("outcome")` after poison recovery → `Err` (not `Ok`)
5. `context_store` MCP call with `category = "outcome"` → `ServerError::InvalidCategory` response
6. `context_search` with `category = "outcome"` filter → still works (retrieval unaffected)
7. Existing DB entries with `category = "outcome"` remain queryable via `context_lookup` with explicit category filter
