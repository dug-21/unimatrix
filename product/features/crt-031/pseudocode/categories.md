# Component Pseudocode: infra/categories (module split + lifecycle policy)

## Purpose

Split `categories.rs` (currently 454 lines, at risk of breaching 500-line ceiling) into a module
directory `infra/categories/`, add a second `RwLock<HashSet<String>>` field for lifecycle policy,
and expose three new public items: `from_categories_with_policy`, `is_adaptive`, `list_adaptive`.

The public import path `crate::infra::categories::CategoryAllowlist` is unchanged. No file outside
`infra/categories/` requires modification as a result of the module split.

---

## File Layout After Split

```
crates/unimatrix-server/src/infra/
  categories/
    mod.rs       -- all existing content + new field + new methods + all tests
    lifecycle.rs -- reserved stub; initially minimal (one doc comment block)
```

`categories.rs` is deleted. `infra/mod.rs` re-export of `categories` already resolves via directory.

---

## New / Modified: `infra/categories/mod.rs`

### Struct Definition (modified)

```
// All existing imports preserved.
// Add: use super or pub use lifecycle is not required -- lifecycle.rs is a stub.

pub(crate) const INITIAL_CATEGORIES: [&str; 5] = [
    "lesson-learned", "decision", "convention", "pattern", "procedure",
];

/// Runtime-extensible category validation with per-category lifecycle policy.
///
/// Two independent RwLock fields:
/// - `categories`: the presence set; consulted by `validate()` on every context_store.
/// - `adaptive`:   the lifecycle policy set; consulted by `is_adaptive()` in the tick
///                 and by `compute_report()` for status output.
///
/// Categories added at runtime via `add_category` are always pinned.
/// Only categories in `adaptive_categories` of the operator config are adaptive,
/// and this set is frozen after construction (no runtime mutation of `adaptive`).
pub struct CategoryAllowlist {
    categories: RwLock<HashSet<String>>,   // existing
    adaptive:   RwLock<HashSet<String>>,   // NEW: lifecycle policy set
}
```

### Constructor Hierarchy

```
CONSTRUCTOR: from_categories_with_policy(cats: Vec<String>, adaptive: Vec<String>) -> Self
  // Canonical constructor; all initialization logic lives here.
  // Called by: from_categories, and the two main.rs call sites directly.
  //
  PROCEDURE:
    1. Build categories_set: HashSet<String> by consuming cats
    2. Build adaptive_set: HashSet<String> by consuming adaptive
    3. Return CategoryAllowlist {
           categories: RwLock::new(categories_set),
           adaptive:   RwLock::new(adaptive_set),
       }
  // No validation — caller is responsible (validate_config runs before construction).
  // Poison recovery not applicable at construction time (fresh lock).
```

```
CONSTRUCTOR: from_categories(cats: Vec<String>) -> Self
  // Unchanged signature. Delegates to the canonical constructor with the standard
  // adaptive default. All existing call sites remain valid.
  //
  PROCEDURE:
    return from_categories_with_policy(cats, vec!["lesson-learned".to_string()])
```

```
CONSTRUCTOR: new() -> Self
  // Unchanged signature and behavior. Delegates to from_categories.
  //
  PROCEDURE:
    return from_categories(INITIAL_CATEGORIES.iter().map(|s| s.to_string()).collect())
```

### Unchanged Methods

`validate`, `add_category`, `list_categories` — no changes to logic or signatures.

Doc comment addition to `add_category`:

```
// Add to existing doc comment:
/// Domain pack categories registered at runtime are always pinned.
/// Lifecycle policy (adaptive/pinned) is config-only and frozen after startup.
/// Adding a category here never adds it to the adaptive set.
```

### New Method: is_adaptive

```
METHOD: is_adaptive(&self, category: &str) -> bool
  // Returns true if category is in the adaptive set.
  // Reads only the `adaptive` lock — no contention on the `categories` lock.
  // Returns false for unknown categories (not in allowlist AND not adaptive).
  //
  PROCEDURE:
    1. Acquire adaptive read lock:
         guard = self.adaptive.read().unwrap_or_else(|e| e.into_inner())
    2. Return guard.contains(category)
  //
  // Note: no .sort() needed — HashSet.contains is O(1).
```

### New Method: list_adaptive

```
METHOD: list_adaptive(&self) -> Vec<String>
  // Returns sorted list of all categories in the adaptive set.
  // Used by:
  //   (a) maintenance_tick Step 10b — acquired once per tick, not per-category
  //   (b) compute_report — as part of building category_lifecycle labels
  //
  PROCEDURE:
    1. Acquire adaptive read lock:
         guard = self.adaptive.read().unwrap_or_else(|e| e.into_inner())
    2. Collect into Vec<String>:
         list = guard.iter().cloned().collect()
    3. Sort alphabetically: list.sort()
    4. Return list
  //
  // R-06: Single lock acquisition per call. Callers must not call list_adaptive
  //       inside a loop and then call is_adaptive per-item — use list_adaptive once.
```

---

## New File: `infra/categories/lifecycle.rs`

```
//! Lifecycle policy extensions for CategoryAllowlist.
//!
//! Reserved for future lifecycle-specific extensions (crt-031, #409 insertion point).
//! Initially minimal — this file is committed as a placeholder for the module split.
//!
//! When #409 implements auto-deprecation, per-entry lifecycle logic belongs here.
```

The file has no `use` statements and no `pub` items. It is not referenced by `mod.rs` initially
(no `pub mod lifecycle;` needed until it exports something). Committed as part of the module split
to establish the directory boundary.

Wait — the architecture says `lifecycle.rs` is in the same directory, so `mod.rs` must declare it.
If it exports nothing, the declaration is:

```
mod lifecycle;  // reserved stub for #409
```

This ensures the file is compiled (catches compile errors in the stub itself) without exporting
anything. If the stub has zero items, `#[allow(dead_code)]` is not needed — an empty module is valid.

---

## Tests (in `mod.rs` #[cfg(test)] block)

All existing tests are preserved without modification (AC-12). New tests added:

### AC-05: is_adaptive returns true for default policy
```
test_is_adaptive_default_lesson_learned:
  al = CategoryAllowlist::new()
  assert al.is_adaptive("lesson-learned") == true
```

### AC-06: is_adaptive returns false for non-adaptive category
```
test_is_adaptive_default_decision_is_pinned:
  al = CategoryAllowlist::new()
  assert al.is_adaptive("decision") == false
```

### AC-07: is_adaptive returns false for unknown category
```
test_is_adaptive_unknown_category:
  al = CategoryAllowlist::new()
  assert al.is_adaptive("nonexistent") == false
  assert al.is_adaptive("") == false
```

### AC-13: new() delegation chain
```
test_new_delegates_adaptive_policy:
  al = CategoryAllowlist::new()
  assert al.is_adaptive("lesson-learned") == true
  // decision is in the allowlist but not adaptive
  assert al.is_adaptive("decision") == false
  // validate still works (categories lock unaffected)
  assert al.validate("decision").is_ok()
```

### from_categories_with_policy: custom adaptive list
```
test_from_categories_with_policy_custom_adaptive:
  al = CategoryAllowlist::from_categories_with_policy(
      vec!["lesson-learned", "decision", "convention", "pattern", "procedure"],
      vec!["lesson-learned", "pattern"],  // two adaptive
  )
  assert al.is_adaptive("lesson-learned") == true
  assert al.is_adaptive("pattern") == true
  assert al.is_adaptive("decision") == false
  assert al.is_adaptive("convention") == false
  assert al.list_adaptive() == vec!["lesson-learned", "pattern"]  // sorted
```

### from_categories_with_policy: empty adaptive list
```
test_from_categories_with_policy_empty_adaptive:
  al = CategoryAllowlist::from_categories_with_policy(
      vec!["lesson-learned"],
      vec![],
  )
  assert al.is_adaptive("lesson-learned") == false
  assert al.list_adaptive().is_empty()
```

### E-04: duplicate in adaptive list is silently deduplicated
```
test_adaptive_deduplication:
  al = CategoryAllowlist::from_categories_with_policy(
      vec!["lesson-learned"],
      vec!["lesson-learned", "lesson-learned"],
  )
  assert al.list_adaptive() == vec!["lesson-learned"]  // exactly once
```

### list_adaptive sorted order
```
test_list_adaptive_sorted:
  al = CategoryAllowlist::from_categories_with_policy(
      INITIAL_CATEGORIES.iter().map(String::from).collect(),
      vec!["procedure", "decision", "lesson-learned"],  // unsorted input
  )
  result = al.list_adaptive()
  assert result == vec!["decision", "lesson-learned", "procedure"]  // alphabetically sorted
```

### R-03: add_category is always pinned (AC from risk R-03)
```
test_add_category_is_always_pinned:
  al = CategoryAllowlist::new()
  al.add_category("new-cat".to_string())
  assert al.validate("new-cat").is_ok()    // in allowlist
  assert al.is_adaptive("new-cat") == false  // NOT adaptive
```

### AC-08: Poison recovery for adaptive lock
```
test_poison_recovery_is_adaptive:
  // Poison the adaptive lock using the same helper pattern as existing tests.
  al = Arc::new(CategoryAllowlist::new())
  poison_adaptive_lock(&al)  // new helper: spawns thread, acquires adaptive write lock, panics
  // is_adaptive must not panic
  assert al.is_adaptive("lesson-learned") == true  // recovers from poisoned guard
  assert al.is_adaptive("decision") == false

test_poison_recovery_list_adaptive:
  al = Arc::new(CategoryAllowlist::new())
  poison_adaptive_lock(&al)
  result = al.list_adaptive()
  // Should return valid data (recovered from poison)
  assert result.contains("lesson-learned")
```

Helper `poison_adaptive_lock` mirrors the existing `poison_allowlist` pattern but accesses
the `adaptive` field (same-crate test access):

```
fn poison_adaptive_lock(al: &Arc<CategoryAllowlist>) {
  let al_clone = Arc::clone(al)
  handle = thread::spawn(move || {
    guard = al_clone.adaptive.write().unwrap()
    guard.insert("pre-panic-insert")
    panic!("intentional poison for testing")
  })
  let _ = handle.join()
}
```

---

## Error Handling

No new error types in this component. All methods are infallible at the return level:
- `is_adaptive` and `list_adaptive` use poison recovery and never return `Result`.
- `from_categories_with_policy` cannot fail (takes owned data, no I/O).

---

## Key Test Scenarios Summary

| Scenario | Test Name | AC/Risk |
|----------|-----------|---------|
| Default policy has lesson-learned adaptive | `test_is_adaptive_default_lesson_learned` | AC-05 |
| Default policy has decision as pinned | `test_is_adaptive_default_decision_is_pinned` | AC-06 |
| Unknown category is not adaptive | `test_is_adaptive_unknown_category` | AC-07 |
| Poison recovery on adaptive lock | `test_poison_recovery_is_adaptive` | AC-08 |
| new() delegates to lesson-learned default | `test_new_delegates_adaptive_policy` | AC-13 |
| Custom two-adaptive policy | `test_from_categories_with_policy_custom_adaptive` | FR-05 |
| Empty adaptive list | `test_from_categories_with_policy_empty_adaptive` | AC-14 |
| Duplicates deduplicated silently | `test_adaptive_deduplication` | E-04 |
| list_adaptive returns sorted output | `test_list_adaptive_sorted` | R-08 |
| add_category always pinned | `test_add_category_is_always_pinned` | R-03 |
| Existing tests unchanged | (all pre-existing tests) | AC-12 |
