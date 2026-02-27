# Pseudocode: C1 Schema Migration v2 -> v3

## Purpose

Migrate `EntryRecord.confidence` from f32 (4 bytes bincode) to f64 (8 bytes bincode). This is a type-only change with no new fields. Follows the established migration pattern from v0->v1 (nxs-004) and v1->v2 (crt-001).

## Files Modified

- `crates/unimatrix-store/src/schema.rs` -- `EntryRecord.confidence` type change
- `crates/unimatrix-store/src/migration.rs` -- V2EntryRecord, migrate_v2_to_v3, version bump

## Schema Change (schema.rs)

```
EntryRecord {
    ...
    #[serde(default)]
    pub confidence: f64,   // WAS: f32
    ...
}
```

All 26 fields remain. Only the type of `confidence` changes. The `#[serde(default)]` attribute stays.

## V2EntryRecord Intermediate Struct (migration.rs)

Define a 26-field struct matching the CURRENT v2 schema exactly, with `confidence: f32`.

```
#[derive(Deserialize, Serialize)]
struct V2EntryRecord {
    id: u64,
    title: String,
    content: String,
    topic: String,
    category: String,
    tags: Vec<String>,
    source: String,
    status: Status,
    confidence: f32,          // <-- This is the key difference
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
    helpful_count: u32,
    unhelpful_count: u32,
}
```

CRITICAL: Field order MUST match the current EntryRecord exactly. Bincode uses positional encoding. A field order mismatch causes every entry to fail deserialization during migration (R-13).

## deserialize_v2_entry helper

```
fn deserialize_v2_entry(bytes: &[u8]) -> Result<V2EntryRecord>:
    bincode::serde::decode_from_slice::<V2EntryRecord>(bytes, bincode::config::standard())
    return record
```

## migrate_v2_to_v3 function

```
fn migrate_v2_to_v3(txn: &WriteTransaction) -> Result<()>:
    // Step 1: Collect all entry IDs (avoids borrow conflict)
    entry_ids = open ENTRIES table, iterate, collect all keys as Vec<u64>

    if entry_ids is empty:
        return Ok(())

    // Step 2: For each entry, read v2 bytes, deserialize, convert, rewrite
    for id in entry_ids:
        old_bytes = read ENTRIES[id]
        if not found: continue

        v2 = deserialize_v2_entry(old_bytes)

        // Construct v3 EntryRecord with f64 confidence
        record = EntryRecord {
            id: v2.id,
            title: v2.title,
            content: v2.content,
            topic: v2.topic,
            category: v2.category,
            tags: v2.tags,
            source: v2.source,
            status: v2.status,
            confidence: v2.confidence as f64,   // IEEE 754 lossless promotion
            created_at: v2.created_at,
            updated_at: v2.updated_at,
            last_accessed_at: v2.last_accessed_at,
            access_count: v2.access_count,
            supersedes: v2.supersedes,
            superseded_by: v2.superseded_by,
            correction_count: v2.correction_count,
            embedding_dim: v2.embedding_dim,
            created_by: v2.created_by,
            modified_by: v2.modified_by,
            content_hash: v2.content_hash,
            previous_hash: v2.previous_hash,
            version: v2.version,
            feature_cycle: v2.feature_cycle,
            trust_source: v2.trust_source,
            helpful_count: v2.helpful_count,
            unhelpful_count: v2.unhelpful_count,
        }

        new_bytes = serialize_entry(&record)
        write ENTRIES[id] = new_bytes

    return Ok(())
```

## Version Bump

```
CURRENT_SCHEMA_VERSION = 3   // WAS: 2
```

## migrate_if_needed Changes

Add the v2->v3 step after the existing v1->v2 step:

```
fn migrate_if_needed(db: &Database) -> Result<()>:
    current_version = read_schema_version(db)

    if current_version >= CURRENT_SCHEMA_VERSION:
        return Ok(())

    txn = db.begin_write()

    if current_version < 1:
        migrate_v0_to_v1(&txn)
    if current_version < 2:
        migrate_v1_to_v2(&txn)
    if current_version < 3:
        migrate_v2_to_v3(&txn)

    // Update schema version in the SAME transaction
    counters = txn.open_table(COUNTERS)
    counters.insert("schema_version", CURRENT_SCHEMA_VERSION)

    txn.commit()
    Ok(())
```

NOTE: The schema_version counter update is MOVED OUT of individual migration functions and handled once at the end. This matches the existing pattern where migrate_if_needed sets the version after all migrations.

## Error Handling

- If any entry fails deserialization: the error propagates, the write transaction is NOT committed, database stays at v2. Next Store::open retries.
- If disk full during rewrite: same behavior -- transaction rolls back.
- Empty database: no-op (entry_ids is empty), version still bumps to 3.

## Key Test Scenarios

1. V2EntryRecord roundtrip: serialize with current schema, deserialize with V2EntryRecord
2. Migration with known f32 values: verify exact f64 match after `as f64`
3. Migration with edge f32 values: 0.0, f32::MIN_POSITIVE, near-1.0 values
4. Migration chain: v0 -> v1 -> v2 -> v3
5. Empty database migration: schema version bumps, no errors
6. Idempotent: second Store::open is a no-op
