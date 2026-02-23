# Pseudocode: C4 Category Extension

## File: `crates/unimatrix-server/src/categories.rs`

### Change: Expand INITIAL_CATEGORIES from 6 to 8

```
BEFORE:
  const INITIAL_CATEGORIES: [&str; 6] = [
      "outcome", "lesson-learned", "decision",
      "convention", "pattern", "procedure",
  ];

AFTER:
  const INITIAL_CATEGORIES: [&str; 8] = [
      "outcome", "lesson-learned", "decision",
      "convention", "pattern", "procedure",
      "duties",       // NEW: role duties for context_briefing
      "reference",    // NEW: general reference material
  ];
```

### Change: Update `new()` doc comment

```
BEFORE: "Create a new allowlist with the initial 6 categories."
AFTER:  "Create a new allowlist with the initial 8 categories."
```

### Test Updates Required

- `test_validate_unknown_rejected`: Change `assert_eq!(valid_categories.len(), 6)` to `8`
- `test_list_categories_sorted`: Change `assert_eq!(list.len(), 6)` to `8`
- Add `test_validate_duties` -- validates "duties" is accepted
- Add `test_validate_reference` -- validates "reference" is accepted
- `test_error_lists_all_valid_categories`: Add assertions for "duties" and "reference"
