# C7: Store Extension -- Test Plan

## Tests (added to `crates/unimatrix-store/src/read.rs::tests`)

Per W1 alignment approval, these tests MUST be included in the unimatrix-store crate.

```
test_iter_vector_mappings_empty:
    db = TestDb::new()
    mappings = db.store().iter_vector_mappings().unwrap()
    ASSERT mappings.is_empty()

test_iter_vector_mappings_populated:
    db = TestDb::new()
    db.store().put_vector_mapping(1, 100).unwrap()
    db.store().put_vector_mapping(2, 200).unwrap()
    db.store().put_vector_mapping(3, 300).unwrap()

    mappings = db.store().iter_vector_mappings().unwrap()
    ASSERT mappings.len() == 3
    ASSERT mappings.contains(&(1, 100))
    ASSERT mappings.contains(&(2, 200))
    ASSERT mappings.contains(&(3, 300))

test_iter_vector_mappings_after_overwrite:
    db = TestDb::new()
    db.store().put_vector_mapping(1, 100).unwrap()
    db.store().put_vector_mapping(1, 999).unwrap()  // overwrite
    db.store().put_vector_mapping(2, 200).unwrap()

    mappings = db.store().iter_vector_mappings().unwrap()
    ASSERT mappings.len() == 2          // only 2 unique keys
    ASSERT mappings.contains(&(1, 999)) // latest value
    ASSERT mappings.contains(&(2, 200))

test_iter_vector_mappings_consistency_with_get:
    db = TestDb::new()
    for i in 1..=50:
        db.store().put_vector_mapping(i, i * 10).unwrap()

    mappings = db.store().iter_vector_mappings().unwrap()
    ASSERT mappings.len() == 50

    // Every mapping matches individual get
    for (entry_id, data_id) in mappings:
        got = db.store().get_vector_mapping(entry_id).unwrap()
        ASSERT got == Some(data_id)

test_iter_vector_mappings_after_delete:
    db = TestDb::new()
    // Insert a store entry and vector mapping
    entry = TestEntry::new("t", "c").build()
    id = db.store().insert(entry).unwrap()
    db.store().put_vector_mapping(id, 100).unwrap()

    // Delete the entry (which removes VECTOR_MAP entry)
    db.store().delete(id).unwrap()

    mappings = db.store().iter_vector_mappings().unwrap()
    ASSERT mappings.is_empty()
```

## Risks Covered
- IR-02 (VECTOR_MAP iteration): 0, 1, 50 entries tested.
- W1 alignment: All required test scenarios (empty, populated, consistency) covered.
