# Pseudocode: category-allowlist

**File**: `crates/unimatrix-server/src/infra/categories.rs` (modified)

## Purpose

Adds `CategoryAllowlist::from_categories(Vec<String>) -> Self` so that `main.rs`
can initialize the allowlist from config-supplied categories. The existing `new()`
constructor is unchanged in signature and behavior — it delegates to
`from_categories(INITIAL_CATEGORIES.to_vec())`. All existing test call sites
(`CategoryAllowlist::new()`) continue to work without modification (SR-07).

---

## Existing State (Pre-dsn-001)

`CategoryAllowlist::new()` creates a `HashSet<String>` from the 8 hardcoded
`INITIAL_CATEGORIES` entries. There is no other constructor.

---

## New Constructor

```
// Seed the allowlist from a caller-supplied list of category strings.
//
// Called from main.rs startup wiring after config load:
//   CategoryAllowlist::from_categories(config.knowledge.categories)
//
// The supplied list has already been validated by validate_config:
//   - each entry matches [a-z0-9_-]
//   - each entry is <= 64 chars
//   - list has <= 64 entries
// No re-validation here — trust that config loading validated the input.
pub fn from_categories(cats: Vec<String>) -> Self

BODY:
    let mut set = HashSet::new();
    for cat in cats {
        set.insert(cat);
    }
    CategoryAllowlist {
        categories: RwLock::new(set),
    }
```

---

## Modified Constructor

```
// Create a new allowlist with the initial 8 categories.
//
// Unchanged signature — all existing call sites remain valid.
// Delegates to from_categories to keep a single implementation path.
pub fn new() -> Self

BODY:
    // Delegate to from_categories using the compiled INITIAL_CATEGORIES defaults.
    // This ensures new() and from_categories(INITIAL_CATEGORIES.to_vec())
    // produce identical results (IR-05 invariant).
    CategoryAllowlist::from_categories(
        INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect()
    )
```

---

## Unchanged Code (No Modifications Required)

The following functions are NOT modified:
- `validate(&self, category: &str) -> Result<(), ServerError>` — unchanged
- `add_category(&self, category: String)` — unchanged
- `list_categories(&self) -> Vec<String>` — unchanged
- All existing tests — unchanged (they all use `CategoryAllowlist::new()`)

---

## Key Test Scenarios

1. **Delegation invariant** (IR-05):
   ```
   let from_new  = CategoryAllowlist::new();
   let from_list = CategoryAllowlist::from_categories(
       INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect()
   );
   // Both should allow the same categories
   for cat in INITIAL_CATEGORIES {
       assert!(from_new.validate(cat).is_ok());
       assert!(from_list.validate(cat).is_ok());
   }
   ```

2. **Custom categories via from_categories** (AC-02):
   ```
   let al = CategoryAllowlist::from_categories(vec!["ruling".to_string(), "statute".to_string()]);
   assert!(al.validate("ruling").is_ok());
   assert!(al.validate("statute").is_ok());
   assert!(al.validate("outcome").is_err());   // not in custom list
   assert!(al.validate("lesson-learned").is_err()); // not in custom list
   ```

3. **Empty categories list** (EC-01):
   ```
   let al = CategoryAllowlist::from_categories(vec![]);
   assert!(al.validate("outcome").is_err()); // empty allowlist rejects everything
   // Document: empty list is valid per validation (count <= 64);
   // all context_store calls will fail post-restart with this config.
   ```

4. **Existing new() tests all pass** (SR-07):
   - All existing tests in `categories.rs` continue to compile and pass.
   - No test calls `from_categories` directly (the new constructor is tested by
     the delegation invariant test above).

---

## Error Handling

`from_categories` is infallible — it takes validated input (post-`validate_config`).
The `RwLock` poison recovery pattern (`unwrap_or_else(|e| e.into_inner())`) is
already present in all other methods and does not change.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — no specific allowlist extension patterns found. The delegation approach (new() calls from_categories) follows the architecture's SR-07 resolution.
- Deviations from established patterns: none.
