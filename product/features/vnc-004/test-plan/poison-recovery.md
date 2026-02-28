# Test Plan: poison-recovery

## Unit Tests

All tests in `crates/unimatrix-server/src/categories.rs` module `tests`.

### test_poison_recovery_validate

- Arrange: Create CategoryAllowlist. Spawn a thread that acquires write lock, then panics (poisoning the lock).
- Act: Call al.validate("outcome") from the main thread after the poisoning thread joins.
- Assert: Returns Ok(()) -- recovery works, valid category is still recognized
- Risks: R-06, AC-06

### test_poison_recovery_add_category

- Arrange: Create CategoryAllowlist. Poison the lock via panic in write closure.
- Act: Call al.add_category("custom".to_string()) after poisoning.
- Assert: No panic. Then call al.validate("custom") and assert Ok(()).
- Risks: R-06, AC-06

### test_poison_recovery_list_categories

- Arrange: Create CategoryAllowlist. Poison the lock via panic in write closure.
- Act: Call al.list_categories() after poisoning.
- Assert: Returns the 8 initial categories (data integrity preserved through poisoning).
- Risks: R-06, AC-06

### test_poison_recovery_data_integrity

- Arrange: Create CategoryAllowlist. Add "custom-before" category. Then poison the lock by panicking mid-write.
- Act: After poisoning, call list_categories().
- Assert: "custom-before" is in the list. Initial 8 categories are in the list.
- Risks: R-06 (verifies no data loss from poisoning)

## Helper: Poisoning a RwLock

```rust
fn poison_lock(al: &CategoryAllowlist) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Acquire write lock, then panic
        let mut cats = al.categories.write().unwrap();
        cats.insert("poison-trigger".to_string());
        panic!("intentional poison");
    }));
    assert!(result.is_err()); // confirm the panic happened
}
```

Note: This requires `categories` field to be `pub(crate)` for test access, OR the test uses the public `add_category` API path with a custom panic mechanism. The simpler approach is to use `std::thread::spawn` to panic in a separate thread, since the main thread catching the panic via `catch_unwind` would not actually poison the lock (the panic is caught before the lock guard drops). Using a separate thread ensures the lock is actually poisoned.

Corrected helper:
```rust
fn poison_lock(al: &Arc<CategoryAllowlist>) {
    let al_clone = Arc::clone(al);
    let handle = std::thread::spawn(move || {
        let _guard = al_clone.categories.write().unwrap();
        panic!("intentional poison");
    });
    let _ = handle.join(); // thread panicked, lock is now poisoned
}
```

This requires `categories` field to be accessible. Since CategoryAllowlist is in the same crate, `pub(crate)` or direct access in `#[cfg(test)]` module works. Alternatively, we can test through the public API by having the thread call `add_category` in a way that panics -- but `add_category` does not panic internally after the fix. The cleanest approach is to make the field `pub(crate)` for testing.

## Existing Tests

All existing categories.rs tests remain valid and unchanged. The .unwrap_or_else replacement is a drop-in for .expect -- normal (non-poisoned) behavior is identical.
