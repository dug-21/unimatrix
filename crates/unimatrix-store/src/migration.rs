use redb::{ReadableDatabase, ReadableTable};
use serde::Deserialize;

use crate::error::{Result, StoreError};
use crate::hash::compute_content_hash;
use crate::schema::{COUNTERS, ENTRIES, EntryRecord, Status, serialize_entry};

/// Current schema version. Bumped when EntryRecord fields change.
pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 2;

/// Run migration if schema_version is behind CURRENT_SCHEMA_VERSION.
/// Called from Store::open() after table creation.
pub(crate) fn migrate_if_needed(db: &redb::Database) -> Result<()> {
    let current_version = read_schema_version(db)?;

    if current_version >= CURRENT_SCHEMA_VERSION {
        return Ok(());
    }

    // Run migration in a single write transaction
    let txn = db.begin_write()?;

    if current_version < 1 {
        migrate_v0_to_v1(&txn)?;
    }
    if current_version < 2 {
        migrate_v1_to_v2(&txn)?;
    }

    // Update schema version
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

/// Pre-nxs-004 schema (17 fields). Used only for migration deserialization.
#[derive(Deserialize, serde::Serialize)]
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

fn deserialize_legacy_entry(bytes: &[u8]) -> Result<LegacyEntryRecord> {
    let (record, _) = bincode::serde::decode_from_slice::<LegacyEntryRecord, _>(
        bytes,
        bincode::config::standard(),
    )?;
    Ok(record)
}

/// Migrate from schema v0 (pre-nxs-004, 17 fields) to v1 (24 fields).
fn migrate_v0_to_v1(txn: &redb::WriteTransaction) -> Result<()> {
    // Collect all entry IDs first (avoid borrow conflict with table)
    let entry_ids: Vec<u64> = {
        let table = txn.open_table(ENTRIES)?;
        table
            .iter()?
            .map(|r| {
                let (key, _) = r.map_err(StoreError::Storage)?;
                Ok(key.value())
            })
            .collect::<Result<Vec<u64>>>()?
    };

    if entry_ids.is_empty() {
        return Ok(());
    }

    // For each entry: read, deserialize with legacy struct, convert, rewrite
    for id in entry_ids {
        let old_bytes: Vec<u8> = {
            let table = txn.open_table(ENTRIES)?;
            match table.get(id)? {
                Some(guard) => guard.value().to_vec(),
                None => continue,
            }
        };

        let legacy = deserialize_legacy_entry(&old_bytes)?;

        let content_hash = compute_content_hash(&legacy.title, &legacy.content);

        let record = EntryRecord {
            id: legacy.id,
            title: legacy.title,
            content: legacy.content,
            topic: legacy.topic,
            category: legacy.category,
            tags: legacy.tags,
            source: legacy.source,
            status: legacy.status,
            confidence: legacy.confidence,
            created_at: legacy.created_at,
            updated_at: legacy.updated_at,
            last_accessed_at: legacy.last_accessed_at,
            access_count: legacy.access_count,
            supersedes: legacy.supersedes,
            superseded_by: legacy.superseded_by,
            correction_count: legacy.correction_count,
            embedding_dim: legacy.embedding_dim,
            created_by: String::new(),
            modified_by: String::new(),
            content_hash,
            previous_hash: String::new(),
            version: 1,
            feature_cycle: String::new(),
            trust_source: "system".to_string(),
            helpful_count: 0,
            unhelpful_count: 0,
        };

        let new_bytes = serialize_entry(&record)?;
        {
            let mut table = txn.open_table(ENTRIES)?;
            table.insert(id, new_bytes.as_slice())?;
        }
    }

    Ok(())
}

/// Post-nxs-004 schema (24 fields). Used only for v1->v2 migration deserialization.
#[derive(Deserialize, serde::Serialize)]
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

fn deserialize_v1_entry(bytes: &[u8]) -> Result<V1EntryRecord> {
    let (record, _) = bincode::serde::decode_from_slice::<V1EntryRecord, _>(
        bytes,
        bincode::config::standard(),
    )?;
    Ok(record)
}

/// Migrate from schema v1 (24 fields) to v2 (26 fields: +helpful_count, +unhelpful_count).
fn migrate_v1_to_v2(txn: &redb::WriteTransaction) -> Result<()> {
    let entry_ids: Vec<u64> = {
        let table = txn.open_table(ENTRIES)?;
        table
            .iter()?
            .map(|r| {
                let (key, _) = r.map_err(StoreError::Storage)?;
                Ok(key.value())
            })
            .collect::<Result<Vec<u64>>>()?
    };

    if entry_ids.is_empty() {
        return Ok(());
    }

    for id in entry_ids {
        let old_bytes: Vec<u8> = {
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
            helpful_count: 0,
            unhelpful_count: 0,
        };

        let new_bytes = serialize_entry(&record)?;
        {
            let mut table = txn.open_table(ENTRIES)?;
            table.insert(id, new_bytes.as_slice())?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_entry_record_roundtrip() {
        let legacy = LegacyEntryRecord {
            id: 1,
            title: "Test".to_string(),
            content: "Content".to_string(),
            topic: "auth".to_string(),
            category: "convention".to_string(),
            tags: vec!["rust".to_string()],
            source: "test".to_string(),
            status: Status::Active,
            confidence: 0.5,
            created_at: 1000,
            updated_at: 2000,
            last_accessed_at: 0,
            access_count: 0,
            supersedes: None,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 384,
        };

        let bytes =
            bincode::serde::encode_to_vec(&legacy, bincode::config::standard()).unwrap();
        let deserialized = deserialize_legacy_entry(&bytes).unwrap();

        assert_eq!(deserialized.id, 1);
        assert_eq!(deserialized.title, "Test");
        assert_eq!(deserialized.content, "Content");
        assert_eq!(deserialized.topic, "auth");
        assert_eq!(deserialized.category, "convention");
        assert_eq!(deserialized.tags, vec!["rust".to_string()]);
        assert_eq!(deserialized.status, Status::Active);
        assert_eq!(deserialized.confidence, 0.5);
        assert_eq!(deserialized.embedding_dim, 384);
    }

    #[test]
    fn test_legacy_deserialization_all_status_variants() {
        for status in [Status::Active, Status::Deprecated, Status::Proposed] {
            let legacy = LegacyEntryRecord {
                id: 1,
                title: String::new(),
                content: String::new(),
                topic: String::new(),
                category: String::new(),
                tags: vec![],
                source: String::new(),
                status,
                confidence: 0.0,
                created_at: 0,
                updated_at: 0,
                last_accessed_at: 0,
                access_count: 0,
                supersedes: None,
                superseded_by: None,
                correction_count: 0,
                embedding_dim: 0,
            };
            let bytes =
                bincode::serde::encode_to_vec(&legacy, bincode::config::standard()).unwrap();
            let deserialized = deserialize_legacy_entry(&bytes).unwrap();
            assert_eq!(deserialized.status, status);
        }
    }

    /// Helper: create a database with legacy (17-field) entries, bypassing migration.
    /// Returns (tempdir, db_path, entry_count).
    fn create_legacy_database(
        entries: &[LegacyEntryRecord],
    ) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("legacy.redb");

        // Create database and tables WITHOUT running migration
        let db = redb::Builder::new()
            .create(&path)
            .unwrap();
        let txn = db.begin_write().unwrap();
        {
            txn.open_table(ENTRIES).unwrap();
            txn.open_table(crate::schema::TOPIC_INDEX).unwrap();
            txn.open_table(crate::schema::CATEGORY_INDEX).unwrap();
            txn.open_multimap_table(crate::schema::TAG_INDEX).unwrap();
            txn.open_table(crate::schema::TIME_INDEX).unwrap();
            txn.open_table(crate::schema::STATUS_INDEX).unwrap();
            txn.open_table(crate::schema::VECTOR_MAP).unwrap();
            txn.open_table(COUNTERS).unwrap();
        }
        txn.commit().unwrap();

        // Write legacy-format entries directly (no schema_version set = v0)
        if !entries.is_empty() {
            let txn = db.begin_write().unwrap();
            {
                let mut table = txn.open_table(ENTRIES).unwrap();
                for entry in entries {
                    let bytes = bincode::serde::encode_to_vec(entry, bincode::config::standard())
                        .unwrap();
                    table.insert(entry.id, bytes.as_slice()).unwrap();
                }
            }
            // Set counters for entries
            {
                let mut counters = txn.open_table(COUNTERS).unwrap();
                let next_id = entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
                counters.insert("next_entry_id", next_id).unwrap();
                let active = entries.iter().filter(|e| e.status == Status::Active).count() as u64;
                let deprecated = entries.iter().filter(|e| e.status == Status::Deprecated).count() as u64;
                let proposed = entries.iter().filter(|e| e.status == Status::Proposed).count() as u64;
                counters.insert("total_active", active).unwrap();
                counters.insert("total_deprecated", deprecated).unwrap();
                counters.insert("total_proposed", proposed).unwrap();
            }
            txn.commit().unwrap();
        }

        drop(db);
        (dir, path)
    }

    fn make_legacy_entry(id: u64, title: &str, content: &str, status: Status) -> LegacyEntryRecord {
        LegacyEntryRecord {
            id,
            title: title.to_string(),
            content: content.to_string(),
            topic: "topic".to_string(),
            category: "category".to_string(),
            tags: vec!["tag1".to_string()],
            source: "test".to_string(),
            status,
            confidence: 0.8,
            created_at: 1000,
            updated_at: 2000,
            last_accessed_at: 0,
            access_count: 0,
            supersedes: None,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 384,
        }
    }

    // -- R-01: Migration Preserves Entries --

    #[test]
    fn test_migration_preserves_entries() {
        let entries: Vec<LegacyEntryRecord> = (1..=10)
            .map(|i| make_legacy_entry(i, &format!("Title {i}"), &format!("Content {i}"), Status::Active))
            .collect();

        let (_dir, path) = create_legacy_database(&entries);

        // Open with Store (triggers migration)
        let store = crate::Store::open(&path).unwrap();

        // All 10 entries should be readable with original fields
        for i in 1..=10u64 {
            let record = store.get(i).unwrap();
            assert_eq!(record.title, format!("Title {i}"));
            assert_eq!(record.content, format!("Content {i}"));
            assert_eq!(record.topic, "topic");
            assert_eq!(record.category, "category");
            assert_eq!(record.tags, vec!["tag1".to_string()]);
            assert_eq!(record.status, Status::Active);
            assert_eq!(record.confidence, 0.8);
            assert_eq!(record.embedding_dim, 384);
        }
    }

    // -- R-01: Migration on Empty Database --

    #[test]
    fn test_migration_empty_database() {
        let (_dir, path) = create_legacy_database(&[]);

        let store = crate::Store::open(&path).unwrap();
        let version = store.read_counter("schema_version").unwrap();
        assert_eq!(version, 2);
    }

    // -- R-01/R-04: Migration Populates Security Fields --

    #[test]
    fn test_migration_populates_security_fields() {
        let entries = vec![
            make_legacy_entry(1, "A", "B", Status::Active),
            make_legacy_entry(2, "C", "D", Status::Deprecated),
            make_legacy_entry(3, "E", "F", Status::Proposed),
        ];

        let (_dir, path) = create_legacy_database(&entries);
        let store = crate::Store::open(&path).unwrap();

        for id in 1..=3u64 {
            let record = store.get(id).unwrap();
            assert_eq!(record.version, 1);
            assert_eq!(record.created_by, "");
            assert_eq!(record.modified_by, "");
            assert_eq!(record.previous_hash, "");
            assert_eq!(record.trust_source, "system");
            assert_eq!(record.feature_cycle, "");
            assert_eq!(record.content_hash.len(), 64);
            assert!(record.content_hash.chars().all(|c| c.is_ascii_hexdigit()));
        }
    }

    // -- R-02 (partial): Migration Content Hash Correct --

    #[test]
    fn test_migration_content_hash_computed_correctly() {
        let entries = vec![make_legacy_entry(1, "Known", "Value", Status::Active)];
        let (_dir, path) = create_legacy_database(&entries);
        let store = crate::Store::open(&path).unwrap();

        let record = store.get(1).unwrap();
        let expected = compute_content_hash("Known", "Value");
        assert_eq!(record.content_hash, expected);
    }

    // -- R-09: Migration Idempotent --

    #[test]
    fn test_migration_idempotent() {
        let entries = vec![
            make_legacy_entry(1, "Title1", "Content1", Status::Active),
            make_legacy_entry(2, "Title2", "Content2", Status::Active),
        ];
        let (_dir, path) = create_legacy_database(&entries);

        // First open: migration runs (v0->v1->v2)
        {
            let store = crate::Store::open(&path).unwrap();
            let r1 = store.get(1).unwrap();
            assert_eq!(r1.version, 1);
            assert_eq!(r1.helpful_count, 0);
            assert_eq!(r1.unhelpful_count, 0);
            assert_eq!(store.read_counter("schema_version").unwrap(), 2);
        }

        // Second open: migration should be a no-op
        {
            let store = crate::Store::open(&path).unwrap();
            let r1 = store.get(1).unwrap();
            assert_eq!(r1.version, 1); // Still 1, not re-migrated
            assert_eq!(r1.helpful_count, 0);
            assert_eq!(store.read_counter("schema_version").unwrap(), 2);
        }
    }

    // -- IR-03: Migration Preserves Counters --

    #[test]
    fn test_migration_preserves_counters() {
        let entries = vec![
            make_legacy_entry(1, "A", "A", Status::Active),
            make_legacy_entry(2, "B", "B", Status::Active),
            make_legacy_entry(3, "C", "C", Status::Active),
            make_legacy_entry(4, "D", "D", Status::Deprecated),
            make_legacy_entry(5, "E", "E", Status::Proposed),
        ];
        let (_dir, path) = create_legacy_database(&entries);

        let store = crate::Store::open(&path).unwrap();
        assert_eq!(store.read_counter("next_entry_id").unwrap(), 6);
        assert_eq!(store.read_counter("total_active").unwrap(), 3);
        assert_eq!(store.read_counter("total_deprecated").unwrap(), 1);
        assert_eq!(store.read_counter("total_proposed").unwrap(), 1);
        assert_eq!(store.read_counter("schema_version").unwrap(), 2);
    }

    // -- EC-06: Migration Unicode Content --

    #[test]
    fn test_migration_unicode_content() {
        let entries = vec![LegacyEntryRecord {
            id: 1,
            title: "\u{4e16}\u{754c}".to_string(),   // CJK characters
            content: "\u{1f510} secure".to_string(),   // Emoji
            topic: "intl".to_string(),
            category: "test".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            confidence: 0.0,
            created_at: 0,
            updated_at: 0,
            last_accessed_at: 0,
            access_count: 0,
            supersedes: None,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 0,
        }];
        let (_dir, path) = create_legacy_database(&entries);
        let store = crate::Store::open(&path).unwrap();

        let record = store.get(1).unwrap();
        assert_eq!(record.title, "\u{4e16}\u{754c}");
        assert_eq!(record.content, "\u{1f510} secure");
        assert_eq!(record.content_hash.len(), 64);
    }

    // -- EC-05: Migration Empty String Fields --

    #[test]
    fn test_migration_empty_string_fields() {
        let entries = vec![LegacyEntryRecord {
            id: 1,
            title: String::new(),
            content: String::new(),
            topic: "t".to_string(),
            category: "c".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            confidence: 0.0,
            created_at: 0,
            updated_at: 0,
            last_accessed_at: 0,
            access_count: 0,
            supersedes: None,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 0,
        }];
        let (_dir, path) = create_legacy_database(&entries);
        let store = crate::Store::open(&path).unwrap();

        let record = store.get(1).unwrap();
        assert_eq!(
            record.content_hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(record.version, 1);
    }

    // -- crt-001: V1->V2 Migration Tests --

    /// Create a v1 database (24-field entries) with schema_version=1.
    fn create_v1_database(
        entries: &[V1EntryRecord],
    ) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("v1.redb");

        let db = redb::Builder::new().create(&path).unwrap();
        let txn = db.begin_write().unwrap();
        {
            txn.open_table(ENTRIES).unwrap();
            txn.open_table(crate::schema::TOPIC_INDEX).unwrap();
            txn.open_table(crate::schema::CATEGORY_INDEX).unwrap();
            txn.open_multimap_table(crate::schema::TAG_INDEX).unwrap();
            txn.open_table(crate::schema::TIME_INDEX).unwrap();
            txn.open_table(crate::schema::STATUS_INDEX).unwrap();
            txn.open_table(crate::schema::VECTOR_MAP).unwrap();
            txn.open_table(COUNTERS).unwrap();
            txn.open_table(crate::schema::AGENT_REGISTRY).unwrap();
            txn.open_table(crate::schema::AUDIT_LOG).unwrap();
        }
        txn.commit().unwrap();

        if !entries.is_empty() {
            let txn = db.begin_write().unwrap();
            {
                let mut table = txn.open_table(ENTRIES).unwrap();
                for entry in entries {
                    let bytes = bincode::serde::encode_to_vec(entry, bincode::config::standard())
                        .unwrap();
                    table.insert(entry.id, bytes.as_slice()).unwrap();
                }
            }
            {
                let mut counters = txn.open_table(COUNTERS).unwrap();
                let next_id = entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
                counters.insert("next_entry_id", next_id).unwrap();
                let active = entries.iter().filter(|e| e.status == Status::Active).count() as u64;
                let deprecated = entries.iter().filter(|e| e.status == Status::Deprecated).count() as u64;
                let proposed = entries.iter().filter(|e| e.status == Status::Proposed).count() as u64;
                counters.insert("total_active", active).unwrap();
                counters.insert("total_deprecated", deprecated).unwrap();
                counters.insert("total_proposed", proposed).unwrap();
                counters.insert("schema_version", 1u64).unwrap();
            }
            txn.commit().unwrap();
        } else {
            let txn = db.begin_write().unwrap();
            {
                let mut counters = txn.open_table(COUNTERS).unwrap();
                counters.insert("schema_version", 1u64).unwrap();
            }
            txn.commit().unwrap();
        }

        drop(db);
        (dir, path)
    }

    fn make_v1_entry(id: u64, title: &str, content: &str, status: Status) -> V1EntryRecord {
        let content_hash = compute_content_hash(title, content);
        V1EntryRecord {
            id,
            title: title.to_string(),
            content: content.to_string(),
            topic: "topic".to_string(),
            category: "category".to_string(),
            tags: vec!["tag1".to_string()],
            source: "test".to_string(),
            status,
            confidence: 0.8,
            created_at: 1000,
            updated_at: 2000,
            last_accessed_at: 500,
            access_count: 5,
            supersedes: None,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 384,
            created_by: "agent-1".to_string(),
            modified_by: "agent-2".to_string(),
            content_hash,
            previous_hash: String::new(),
            version: 1,
            feature_cycle: "nxs-004".to_string(),
            trust_source: "agent".to_string(),
        }
    }

    #[test]
    fn test_v1_to_v2_migration_preserves_entries() {
        let entries: Vec<V1EntryRecord> = (1..=10)
            .map(|i| make_v1_entry(i, &format!("Title {i}"), &format!("Content {i}"), Status::Active))
            .collect();
        let (_dir, path) = create_v1_database(&entries);

        let store = crate::Store::open(&path).unwrap();
        for i in 1..=10u64 {
            let record = store.get(i).unwrap();
            assert_eq!(record.title, format!("Title {i}"));
            assert_eq!(record.content, format!("Content {i}"));
            assert_eq!(record.helpful_count, 0);
            assert_eq!(record.unhelpful_count, 0);
            // Verify existing fields preserved
            assert_eq!(record.access_count, 5);
            assert_eq!(record.last_accessed_at, 500);
            assert_eq!(record.created_by, "agent-1");
            assert_eq!(record.version, 1);
        }
        assert_eq!(store.read_counter("schema_version").unwrap(), 2);
    }

    #[test]
    fn test_v1_to_v2_migration_preserves_non_zero_fields() {
        let mut entry = make_v1_entry(1, "Test", "Content", Status::Active);
        entry.access_count = 42;
        entry.correction_count = 3;
        entry.supersedes = Some(10);
        entry.confidence = 0.95;

        let (_dir, path) = create_v1_database(&[entry]);
        let store = crate::Store::open(&path).unwrap();
        let record = store.get(1).unwrap();

        assert_eq!(record.access_count, 42);
        assert_eq!(record.correction_count, 3);
        assert_eq!(record.supersedes, Some(10));
        assert_eq!(record.confidence, 0.95);
        assert_eq!(record.helpful_count, 0);
        assert_eq!(record.unhelpful_count, 0);
    }

    #[test]
    fn test_v1_to_v2_idempotent() {
        let entries = vec![make_v1_entry(1, "A", "B", Status::Active)];
        let (_dir, path) = create_v1_database(&entries);

        // First open: v1->v2 migration runs
        {
            let store = crate::Store::open(&path).unwrap();
            assert_eq!(store.read_counter("schema_version").unwrap(), 2);
            let r = store.get(1).unwrap();
            assert_eq!(r.helpful_count, 0);
        }

        // Second open: migration should be a no-op
        {
            let store = crate::Store::open(&path).unwrap();
            assert_eq!(store.read_counter("schema_version").unwrap(), 2);
            let r = store.get(1).unwrap();
            assert_eq!(r.helpful_count, 0);
        }
    }

    #[test]
    fn test_v0_to_v2_chain_migration() {
        // Create a v0 database (17-field entries, no schema_version)
        let entries = vec![
            make_legacy_entry(1, "Legacy", "Content", Status::Active),
        ];
        let (_dir, path) = create_legacy_database(&entries);

        let store = crate::Store::open(&path).unwrap();
        let record = store.get(1).unwrap();

        // v0->v1 added security fields
        assert_eq!(record.version, 1);
        assert_eq!(record.trust_source, "system");
        assert!(!record.content_hash.is_empty());

        // v1->v2 added usage fields
        assert_eq!(record.helpful_count, 0);
        assert_eq!(record.unhelpful_count, 0);

        assert_eq!(store.read_counter("schema_version").unwrap(), 2);
    }

    #[test]
    fn test_v1_entry_record_roundtrip() {
        let entry = make_v1_entry(1, "Test", "Content", Status::Active);
        let bytes = bincode::serde::encode_to_vec(&entry, bincode::config::standard()).unwrap();
        let deserialized = deserialize_v1_entry(&bytes).unwrap();
        assert_eq!(deserialized.id, 1);
        assert_eq!(deserialized.title, "Test");
        assert_eq!(deserialized.created_by, "agent-1");
        assert_eq!(deserialized.trust_source, "agent");
    }
}
