# Pseudocode: migration

## Purpose
Implement scan-and-rewrite migration triggered on Store::open() when schema_version is behind.

## New File: crates/unimatrix-store/src/migration.rs

```
pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 1;

/// Run migration if schema_version is behind CURRENT_SCHEMA_VERSION.
/// Called from Store::open() after table creation.
pub(crate) fn migrate_if_needed(db: &redb::Database) -> Result<()> {
    // Step 1: Read current schema version
    let current_version = read_schema_version(db)?;

    // Step 2: If up to date, return
    if current_version >= CURRENT_SCHEMA_VERSION {
        return Ok(());
    }

    // Step 3: Run migration in a single write transaction
    let txn = db.begin_write()?;

    if current_version < 1 {
        migrate_v0_to_v1(&txn)?;
    }

    // Step 4: Update schema version
    {
        let mut counters = txn.open_table(COUNTERS)?;
        counters.insert("schema_version", CURRENT_SCHEMA_VERSION)?;
    }

    txn.commit()?;
    Ok(())
}

fn read_schema_version(db: &redb::Database) -> Result<u64> {
    let txn = db.begin_read()?;
    let table = txn.open_table(COUNTERS)?;
    match table.get("schema_version")? {
        Some(guard) => Ok(guard.value()),
        None => Ok(0),
    }
}

/// Migrate from schema v0 (pre-nxs-004, 17 fields) to v1 (24 fields).
fn migrate_v0_to_v1(txn: &WriteTransaction) -> Result<()> {
    let entry_ids: Vec<u64>;

    // Collect all entry IDs first (avoid borrow conflict)
    {
        let table = txn.open_table(ENTRIES)?;
        entry_ids = table.iter()?.map(|r| {
            let (key, _) = r?;
            Ok(key.value())
        }).collect::<Result<Vec<u64>>>()?;
    }

    // If no entries, nothing to migrate
    if entry_ids.is_empty() {
        return Ok(());
    }

    // For each entry: read, deserialize with legacy struct, convert, rewrite
    for id in entry_ids {
        let old_bytes: Vec<u8>;
        {
            let table = txn.open_table(ENTRIES)?;
            match table.get(id)? {
                Some(guard) => old_bytes = guard.value().to_vec(),
                None => continue,  // deleted between scan and process
            }
        }

        // Deserialize as LegacyEntryRecord (17 fields)
        let legacy = deserialize_legacy_entry(&old_bytes)?;

        // Convert to current EntryRecord (24 fields)
        let record = EntryRecord {
            // Copy all existing fields from legacy
            id: legacy.id,
            title: legacy.title,
            content: legacy.content,
            // ... all 17 legacy fields ...
            // New fields:
            created_by: String::new(),
            modified_by: String::new(),
            content_hash: compute_content_hash(&legacy_title, &legacy_content),
            previous_hash: String::new(),
            version: 1,
            feature_cycle: String::new(),
            trust_source: "system".to_string(),
        };

        // Serialize with current schema and write back
        let new_bytes = serialize_entry(&record)?;
        {
            let mut table = txn.open_table(ENTRIES)?;
            table.insert(id, new_bytes.as_slice())?;
        }
    }

    Ok(())
}
```

### LegacyEntryRecord

A private struct matching the pre-nxs-004 schema (17 fields):
```
#[derive(Deserialize)]
struct LegacyEntryRecord {
    id: u64,
    title: String,
    content: String,
    topic: String,
    category: String,
    tags: Vec<String>,
    source: String,
    status: Status,
    confidence: f32,
    created_at: u64,
    updated_at: u64,
    last_accessed_at: u64,
    access_count: u32,
    supersedes: Option<u64>,
    superseded_by: Option<u64>,
    correction_count: u32,
    embedding_dim: u16,
}
```

Uses `bincode::serde::decode_from_slice` for deserialization.

## Modified File: crates/unimatrix-store/src/db.rs

In `Store::open()` and `Store::open_with_config()`, after table creation and commit, call:
```
migration::migrate_if_needed(&db)?;
```

## Modified File: crates/unimatrix-store/src/lib.rs

Add `mod migration;` to module declarations.

## Error Handling
- Migration read/write errors propagated as StoreError
- Legacy deserialization errors propagated as StoreError::Deserialization
- Transaction failure rolls back atomically (redb guarantee)

## Key Test Scenarios
- Empty database: schema_version set to 1, no entry scan
- Database with entries: all entries rewritten, content_hash computed, version=1
- Double open: migration runs once, second open is no-op
- Entry count preserved after migration
- Existing counters (next_entry_id, total_active) preserved
- Unicode content migrated correctly
