# Test Plan: store-confidence (C2)

## Location: `crates/unimatrix-store/src/write.rs` (inline #[cfg(test)])

### T-12: update_confidence basic (AC-17, R-06)
```
// Setup: insert an entry with confidence=0.0
store = test_store()
id = store.insert(new_entry())

// Update confidence
store.update_confidence(id, 0.75)

// Verify
entry = store.get(id)
assert entry.confidence == 0.75

// Verify other fields unchanged
assert entry.title unchanged
assert entry.access_count unchanged
assert entry.tags unchanged
```

### T-13: update_confidence idempotent (R-06)
```
store.update_confidence(id, 0.5)
store.update_confidence(id, 0.5)
entry = store.get(id)
assert entry.confidence == 0.5
```

### T-14: update_confidence not found (R-06)
```
result = store.update_confidence(999999, 0.5)
assert result is Err(StoreError::NotFound)
```

### T-15: record_usage_with_confidence with None (R-09, backward compat)
```
// When confidence_fn is None, behaves exactly like record_usage
store = test_store()
id = store.insert(entry_with(confidence=0.0))

store.record_usage_with_confidence(
    &[id], &[id], &[], &[], &[], &[],
    None,
)

entry = store.get(id)
assert entry.access_count == 1
assert entry.last_accessed_at > 0
assert entry.confidence == 0.0  // unchanged, no confidence function
```

### T-16: record_usage_with_confidence with function (AC-09)
```
fn test_confidence_fn(entry: &EntryRecord, _now: u64) -> f32 {
    0.42  // deterministic test value
}

store = test_store()
id = store.insert(entry_with(confidence=0.0))

store.record_usage_with_confidence(
    &[id], &[id], &[], &[], &[], &[],
    Some(&test_confidence_fn),
)

entry = store.get(id)
assert entry.access_count == 1
assert entry.confidence == 0.42  // confidence function applied
```

### T-17: record_usage_with_confidence batch (R-03)
```
// Multiple entries in one transaction
ids = [insert 5 entries]

store.record_usage_with_confidence(
    &ids, &ids, &[], &[], &[], &[],
    Some(&test_confidence_fn),
)

for id in ids:
    entry = store.get(id)
    assert entry.access_count == 1
    assert entry.confidence == 0.42
```

### T-18: record_usage_with_confidence deleted entry (R-03)
```
id1 = store.insert(entry1)
id2 = store.insert(entry2)
store.delete(id2)

// id2 is deleted; should be silently skipped
store.record_usage_with_confidence(
    &[id1, id2], &[id1, id2], &[], &[], &[], &[],
    Some(&test_confidence_fn),
)

entry1 = store.get(id1)
assert entry1.confidence == 0.42  // updated
assert store.get(id2) is Err  // still deleted
```

### T-19: record_usage delegates to record_usage_with_confidence (backward compat)
```
// Ensure existing record_usage still works identically
store.record_usage(&[id], &[id], &[], &[], &[], &[])
entry = store.get(id)
assert entry.access_count == 1
assert entry.confidence == 0.0  // no confidence function via old method
```
