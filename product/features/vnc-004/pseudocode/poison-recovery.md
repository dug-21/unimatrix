# Pseudocode: poison-recovery

## Purpose

Replace three `.expect("category lock poisoned")` calls in CategoryAllowlist with `.unwrap_or_else(|e| e.into_inner())` to recover from poisoned RwLock instead of panicking.

## File: crates/unimatrix-server/src/categories.rs

### Modified: validate()

```
// Current:
let cats = self.categories.read().expect("category lock poisoned");

// New:
let cats = self.categories.read().unwrap_or_else(|e| e.into_inner());
```

### Modified: add_category()

```
// Current:
let mut cats = self.categories.write().expect("category lock poisoned");

// New:
let mut cats = self.categories.write().unwrap_or_else(|e| e.into_inner());
```

### Modified: list_categories()

```
// Current:
let cats = self.categories.read().expect("category lock poisoned");

// New:
let cats = self.categories.read().unwrap_or_else(|e| e.into_inner());
```

## Rationale

A poisoned RwLock means a previous writer panicked mid-update. For a `HashSet<String>`, `insert` is atomic at the Rust level -- either the key is in the set or it is not. There is no partial state. Recovery via `into_inner()` returns the data that was inside the lock, which is structurally valid.

## Error Handling

- No new error paths -- the `.unwrap_or_else` converts a PoisonError into the inner data
- The recovered data is used normally (no degraded mode)
- Logging is not needed here -- the panic that caused poisoning will have its own log

## Key Test Scenarios

1. Poison the RwLock by panicking in a write closure, then verify validate() still works
2. Poison the RwLock, then verify add_category() still works
3. Poison the RwLock, then verify list_categories() still works
4. After poisoning and recovery, verify data integrity (categories from before the panic are still present)
