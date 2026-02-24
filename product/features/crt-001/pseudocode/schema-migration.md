# Pseudocode: C2 Schema Migration

## File: crates/unimatrix-store/src/migration.rs

### CURRENT_SCHEMA_VERSION

Change from 1 to 2:

```
pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 2;
```

### V1EntryRecord (New Struct)

24-field struct matching schema v1 (post-nxs-004). Used only for migration deserialization.

```
#[derive(Deserialize, Serialize)]
struct V1EntryRecord {
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
    created_by: String,
    modified_by: String,
    content_hash: String,
    previous_hash: String,
    version: u32,
    feature_cycle: String,
    trust_source: String,
}
```

### deserialize_v1_entry

```
fn deserialize_v1_entry(bytes: &[u8]) -> Result<V1EntryRecord> {
    bincode::serde::decode_from_slice::<V1EntryRecord, _>(bytes, bincode::config::standard())
        .map(|(record, _)| record)
        .map_err(StoreError::from)
}
```

### migrate_v1_to_v2

Same pattern as migrate_v0_to_v1:

```
fn migrate_v1_to_v2(txn: &WriteTransaction) -> Result<()> {
    // Step 1: Collect all entry IDs
    let entry_ids = {
        let table = txn.open_table(ENTRIES)?;
        table.iter()?.map(|r| {
            let (key, _) = r?;
            Ok(key.value())
        }).collect::<Result<Vec<u64>>>()?
    };

    if entry_ids.is_empty() { return Ok(()); }

    // Step 2: For each entry, read -> deserialize as V1 -> construct v2 -> rewrite
    for id in entry_ids {
        let old_bytes = {
            let table = txn.open_table(ENTRIES)?;
            match table.get(id)? {
                Some(guard) => guard.value().to_vec(),
                None => continue,
            }
        };

        let v1 = deserialize_v1_entry(&old_bytes)?;

        let record = EntryRecord {
            id: v1.id,
            title: v1.title,
            content: v1.content,
            topic: v1.topic,
            category: v1.category,
            tags: v1.tags,
            source: v1.source,
            status: v1.status,
            confidence: v1.confidence,
            created_at: v1.created_at,
            updated_at: v1.updated_at,
            last_accessed_at: v1.last_accessed_at,
            access_count: v1.access_count,
            supersedes: v1.supersedes,
            superseded_by: v1.superseded_by,
            correction_count: v1.correction_count,
            embedding_dim: v1.embedding_dim,
            created_by: v1.created_by,
            modified_by: v1.modified_by,
            content_hash: v1.content_hash,
            previous_hash: v1.previous_hash,
            version: v1.version,
            feature_cycle: v1.feature_cycle,
            trust_source: v1.trust_source,
            helpful_count: 0,      // NEW
            unhelpful_count: 0,    // NEW
        };

        let new_bytes = serialize_entry(&record)?;
        let mut table = txn.open_table(ENTRIES)?;
        table.insert(id, new_bytes.as_slice())?;
    }

    Ok(())
}
```

### migrate_if_needed Extension

Add v1->v2 step after v0->v1:

```
if current_version < 1 { migrate_v0_to_v1(&txn)?; }
if current_version < 2 { migrate_v1_to_v2(&txn)?; }
```

### Test Helper: create_v1_database

Create a function similar to create_legacy_database but writes 24-field V1EntryRecord format.
Must include AGENT_REGISTRY and AUDIT_LOG tables (added in vnc-001).
