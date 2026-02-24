# Pseudocode: C1 Schema Extension

## File: crates/unimatrix-store/src/schema.rs

### EntryRecord Extension

Append two fields after `trust_source`:

```
struct EntryRecord {
    // ... existing 24 fields ending with trust_source ...
    #[serde(default)]
    pub helpful_count: u32,    // NEW -- field index 24 (zero-based 23)
    #[serde(default)]
    pub unhelpful_count: u32,  // NEW -- field index 25 (zero-based 24)
}
```

CRITICAL: Fields MUST be appended at end. bincode v2 positional encoding.

### FEATURE_ENTRIES Table Definition

Add after AUDIT_LOG definition:

```
/// Feature-entry multimap: feature_id -> set of entry_ids.
pub const FEATURE_ENTRIES: MultimapTableDefinition<&str, u64> =
    MultimapTableDefinition::new("feature_entries");
```

Comment: "// -- Table Definitions (11 total) --"

### make_test_record Helper Update

Add `helpful_count: 0` and `unhelpful_count: 0` to the test helper.

### Existing Test Updates

All tests that construct EntryRecord literals need the two new fields added.

## File: crates/unimatrix-store/src/lib.rs

Re-export FEATURE_ENTRIES:

```
pub use schema::FEATURE_ENTRIES;
```

## File: crates/unimatrix-store/src/write.rs

Update EntryRecord literal in `insert()`:

```
let record = EntryRecord {
    // ... existing fields ...
    helpful_count: 0,
    unhelpful_count: 0,
};
```

## File: crates/unimatrix-store/src/db.rs

Update `open_with_config()` to create FEATURE_ENTRIES table:

```
// Inside the table creation write transaction:
txn.open_multimap_table(FEATURE_ENTRIES).map_err(StoreError::Table)?;
```

Update comment: "Ensure all 11 tables exist"

## File: crates/unimatrix-server/src/server.rs

Update EntryRecord literals in `insert_with_audit()` and `correct_with_audit()`:

```
helpful_count: 0,
unhelpful_count: 0,
```
