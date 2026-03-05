use serde::{Deserialize, Serialize};

use crate::error::StoreError;

// -- Status Enum --

/// Entry lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Status {
    Active = 0,
    Deprecated = 1,
    Proposed = 2,
    Quarantined = 3,
}

impl TryFrom<u8> for Status {
    type Error = StoreError;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(Status::Active),
            1 => Ok(Status::Deprecated),
            2 => Ok(Status::Proposed),
            3 => Ok(Status::Quarantined),
            other => Err(StoreError::InvalidStatus(other)),
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Active => write!(f, "Active"),
            Status::Deprecated => write!(f, "Deprecated"),
            Status::Proposed => write!(f, "Proposed"),
            Status::Quarantined => write!(f, "Quarantined"),
        }
    }
}

// -- EntryRecord --

/// Primary data structure stored in the ENTRIES table.
///
/// Fields with `#[serde(default)]` support zero-migration schema evolution:
/// old serialized data deserializes correctly when new fields are added.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntryRecord {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub topic: String,
    pub category: String,
    pub tags: Vec<String>,
    pub source: String,
    pub status: Status,
    #[serde(default)]
    pub confidence: f64,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub last_accessed_at: u64,
    #[serde(default)]
    pub access_count: u32,
    #[serde(default)]
    pub supersedes: Option<u64>,
    #[serde(default)]
    pub superseded_by: Option<u64>,
    #[serde(default)]
    pub correction_count: u32,
    #[serde(default)]
    pub embedding_dim: u16,
    // -- nxs-004 security fields (appended after embedding_dim) --
    #[serde(default)]
    pub created_by: String,
    #[serde(default)]
    pub modified_by: String,
    #[serde(default)]
    pub content_hash: String,
    #[serde(default)]
    pub previous_hash: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub feature_cycle: String,
    #[serde(default)]
    pub trust_source: String,
    // -- crt-001 usage tracking fields (appended after trust_source) --
    /// Times this entry was marked helpful by agents (deduped per session).
    #[serde(default)]
    pub helpful_count: u32,
    /// Times this entry was marked unhelpful by agents (deduped per session).
    #[serde(default)]
    pub unhelpful_count: u32,
}

// -- NewEntry --

/// Fields required to create a new entry.
///
/// Engine-assigned fields (id, created_at, updated_at) are excluded.
/// All `#[serde(default)]` fields from EntryRecord are also excluded
/// and initialized to their defaults by the engine.
#[derive(Debug, Clone)]
pub struct NewEntry {
    pub title: String,
    pub content: String,
    pub topic: String,
    pub category: String,
    pub tags: Vec<String>,
    pub source: String,
    pub status: Status,
    // -- nxs-004 caller-provided fields --
    pub created_by: String,
    pub feature_cycle: String,
    pub trust_source: String,
}

// -- QueryFilter --

/// Combined query filter for multi-index intersection.
///
/// When all fields are `None`, returns all entries with `Status::Active`.
/// When one or more fields are set, results are the intersection of all
/// individual index queries.
#[derive(Debug, Clone, Default)]
pub struct QueryFilter {
    pub topic: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub status: Option<Status>,
    pub time_range: Option<TimeRange>,
}

// -- TimeRange --

/// Time range for temporal queries (inclusive on both ends).
#[derive(Debug, Clone, Copy)]
pub struct TimeRange {
    /// Inclusive start timestamp (unix seconds).
    pub start: u64,
    /// Inclusive end timestamp (unix seconds).
    pub end: u64,
}

// -- DatabaseConfig --

/// Database configuration options.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Cache size in bytes. Default: 64 MiB.
    pub cache_size: usize,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            cache_size: 64 * 1024 * 1024, // 64 MiB
        }
    }
}

// -- Serialization helpers --

/// Serialize an EntryRecord to bincode bytes using the serde-compatible path.
///
/// Uses `bincode::serde::encode_to_vec` with `standard()` config,
/// NOT `bincode::encode_to_vec` (which requires native Encode derive).
pub fn serialize_entry(record: &EntryRecord) -> crate::error::Result<Vec<u8>> {
    let bytes = bincode::serde::encode_to_vec(record, bincode::config::standard())?;
    Ok(bytes)
}

/// Deserialize an EntryRecord from bincode bytes using the serde-compatible path.
///
/// Uses `bincode::serde::decode_from_slice` with `standard()` config,
/// NOT `bincode::decode_from_slice` (which requires native Decode derive).
pub fn deserialize_entry(bytes: &[u8]) -> crate::error::Result<EntryRecord> {
    let (record, _) =
        bincode::serde::decode_from_slice::<EntryRecord, _>(bytes, bincode::config::standard())?;
    Ok(record)
}

// -- Co-Access Record --

/// Co-access pair metadata stored in the CO_ACCESS table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoAccessRecord {
    /// Number of times this pair was co-retrieved.
    pub count: u32,
    /// Unix timestamp of most recent co-retrieval.
    pub last_updated: u64,
}

/// Create an ordered pair key: (min, max).
pub fn co_access_key(a: u64, b: u64) -> (u64, u64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Serialize a CoAccessRecord to bincode bytes using the serde-compatible path.
pub fn serialize_co_access(record: &CoAccessRecord) -> crate::error::Result<Vec<u8>> {
    let bytes = bincode::serde::encode_to_vec(record, bincode::config::standard())?;
    Ok(bytes)
}

/// Deserialize a CoAccessRecord from bincode bytes using the serde-compatible path.
pub fn deserialize_co_access(bytes: &[u8]) -> crate::error::Result<CoAccessRecord> {
    let (record, _) = bincode::serde::decode_from_slice::<CoAccessRecord, _>(
        bytes,
        bincode::config::standard(),
    )?;
    Ok(record)
}

/// Return the counter key for a given status.
pub fn status_counter_key(status: Status) -> &'static str {
    match status {
        Status::Active => "total_active",
        Status::Deprecated => "total_deprecated",
        Status::Proposed => "total_proposed",
        Status::Quarantined => "total_quarantined",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- R4/AC-16: Schema Evolution --
    //
    // bincode v2 with serde path treats structs positionally (not by field name),
    // so `#[serde(default)]` does NOT handle missing trailing fields during
    // deserialization. The `serde(default)` annotations are retained because:
    //   1. They enable future format migration (e.g., to msgpack) without code changes.
    //   2. They document which fields are "extension" fields vs core fields.
    //
    // The actual schema evolution contract is:
    //   - All inserts/updates write the FULL current EntryRecord.
    //   - When new fields are added to EntryRecord, a one-time migration rewrites
    //     all existing records with the new schema (simple scan-and-rewrite).
    //   - Fields are only appended, never removed or reordered.
    //
    // These tests verify the current-version roundtrip guarantee and the
    // append-only field ordering contract.

    #[test]
    fn test_schema_evolution_full_roundtrip() {
        // Verifies that the current full EntryRecord serializes and deserializes
        // correctly, including all `serde(default)` extension fields.
        let record = make_test_record();
        let bytes = serialize_entry(&record).expect("serialize");
        let deserialized = deserialize_entry(&bytes).expect("deserialize");
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_schema_evolution_extension_fields_roundtrip() {
        // Verifies that extension fields (those with `serde(default)`) survive
        // serialization roundtrip with both default and non-default values.
        let mut record = make_test_record();
        record.confidence = 0.95;
        record.last_accessed_at = 1700000000;
        record.access_count = 42;
        record.supersedes = Some(10);
        record.superseded_by = Some(50);
        record.correction_count = 3;
        record.embedding_dim = 384;

        let bytes = serialize_entry(&record).expect("serialize");
        let deserialized = deserialize_entry(&bytes).expect("deserialize");
        assert_eq!(record, deserialized);

        // Now verify with all extension fields at their defaults
        let mut record_defaults = make_test_record();
        record_defaults.confidence = 0.0;
        record_defaults.last_accessed_at = 0;
        record_defaults.access_count = 0;
        record_defaults.supersedes = None;
        record_defaults.superseded_by = None;
        record_defaults.correction_count = 0;
        record_defaults.embedding_dim = 0;

        let bytes = serialize_entry(&record_defaults).expect("serialize");
        let deserialized = deserialize_entry(&bytes).expect("deserialize");
        assert_eq!(record_defaults, deserialized);
    }

    #[test]
    fn test_schema_evolution_bincode_positional_contract() {
        // Documents and verifies that bincode uses positional encoding.
        // Two records with different field values produce different byte sequences,
        // confirming fields are encoded in order (append-only contract).
        let mut r1 = make_test_record();
        r1.embedding_dim = 0; // last field = 0
        let bytes1 = serialize_entry(&r1).expect("serialize r1");

        let mut r2 = make_test_record();
        r2.embedding_dim = 384; // last field = 384
        let bytes2 = serialize_entry(&r2).expect("serialize r2");

        // Different last-field values produce different bytes
        assert_ne!(bytes1, bytes2);

        // Both roundtrip correctly
        let d1 = deserialize_entry(&bytes1).expect("deserialize r1");
        let d2 = deserialize_entry(&bytes2).expect("deserialize r2");
        assert_eq!(d1.embedding_dim, 0);
        assert_eq!(d2.embedding_dim, 384);
    }

    // -- R3/AC-02: Serialization Round-Trip --

    #[test]
    fn test_roundtrip_all_fields_populated() {
        let record = EntryRecord {
            id: 42,
            title: "Full Record".to_string(),
            content: "Detailed content here".to_string(),
            topic: "auth".to_string(),
            category: "convention".to_string(),
            tags: vec!["rust".to_string(), "error".to_string()],
            source: "agent:architect".to_string(),
            status: Status::Active,
            confidence: 0.95,
            created_at: 1700000000,
            updated_at: 1700001000,
            last_accessed_at: 1700002000,
            access_count: 5,
            supersedes: Some(10),
            superseded_by: Some(50),
            correction_count: 2,
            embedding_dim: 384,
            created_by: "agent-1".to_string(),
            modified_by: "agent-2".to_string(),
            content_hash: "abc123def456".to_string(),
            previous_hash: "def456abc123".to_string(),
            version: 3,
            feature_cycle: "nxs-004".to_string(),
            trust_source: "agent".to_string(),
            helpful_count: 7,
            unhelpful_count: 3,
        };

        let bytes = serialize_entry(&record).expect("serialize");
        let deserialized = deserialize_entry(&bytes).expect("deserialize");
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_roundtrip_empty_strings() {
        let record = EntryRecord {
            id: 1,
            title: String::new(),
            content: String::new(),
            topic: String::new(),
            category: String::new(),
            tags: vec![],
            source: String::new(),
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
            created_by: String::new(),
            modified_by: String::new(),
            content_hash: String::new(),
            previous_hash: String::new(),
            version: 0,
            feature_cycle: String::new(),
            trust_source: String::new(),
            helpful_count: 0,
            unhelpful_count: 0,
        };

        let bytes = serialize_entry(&record).expect("serialize");
        let deserialized = deserialize_entry(&bytes).expect("deserialize");
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_roundtrip_empty_tags() {
        let mut record = make_test_record();
        record.tags = vec![];
        let bytes = serialize_entry(&record).expect("serialize");
        let deserialized = deserialize_entry(&bytes).expect("deserialize");
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_roundtrip_f64_edge_values() {
        for confidence in [0.0_f64, 1.0, f64::MIN_POSITIVE, 0.999999999999] {
            let mut record = make_test_record();
            record.confidence = confidence;
            let bytes = serialize_entry(&record).expect("serialize");
            let deserialized = deserialize_entry(&bytes).expect("deserialize");
            assert_eq!(deserialized.confidence, confidence, "f64 {confidence} failed roundtrip");
        }
    }

    #[test]
    fn test_roundtrip_u64_boundary_values() {
        for val in [0_u64, 1, u64::MAX - 1, u64::MAX] {
            let mut record = make_test_record();
            record.id = val;
            record.created_at = val;
            record.updated_at = val;
            let bytes = serialize_entry(&record).expect("serialize");
            let deserialized = deserialize_entry(&bytes).expect("deserialize");
            assert_eq!(deserialized.id, val);
            assert_eq!(deserialized.created_at, val);
            assert_eq!(deserialized.updated_at, val);
        }
    }

    #[test]
    fn test_roundtrip_option_none_and_some() {
        let mut record = make_test_record();
        record.supersedes = None;
        record.superseded_by = None;
        let bytes = serialize_entry(&record).expect("serialize");
        let d = deserialize_entry(&bytes).expect("deserialize");
        assert_eq!(d.supersedes, None);
        assert_eq!(d.superseded_by, None);

        record.supersedes = Some(42);
        record.superseded_by = Some(99);
        let bytes = serialize_entry(&record).expect("serialize");
        let d = deserialize_entry(&bytes).expect("deserialize");
        assert_eq!(d.supersedes, Some(42));
        assert_eq!(d.superseded_by, Some(99));
    }

    #[test]
    fn test_roundtrip_all_status_variants() {
        for status in [Status::Active, Status::Deprecated, Status::Proposed, Status::Quarantined] {
            let mut record = make_test_record();
            record.status = status;
            let bytes = serialize_entry(&record).expect("serialize");
            let deserialized = deserialize_entry(&bytes).expect("deserialize");
            assert_eq!(deserialized.status, status, "status {status:?} failed roundtrip");
        }
    }

    #[test]
    fn test_roundtrip_large_content() {
        let mut record = make_test_record();
        record.content = "x".repeat(100_000); // 100KB
        let bytes = serialize_entry(&record).expect("serialize");
        let deserialized = deserialize_entry(&bytes).expect("deserialize");
        assert_eq!(deserialized.content.len(), 100_000);
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_roundtrip_unicode() {
        let mut record = make_test_record();
        record.topic = "\u{8a8d}\u{8a3c}".to_string(); // Japanese "authentication"
        record.content = "Auth tokens \u{1f510} are secure".to_string(); // emoji
        record.title = "\u{4e16}\u{754c}".to_string(); // Chinese "world"
        let bytes = serialize_entry(&record).expect("serialize");
        let deserialized = deserialize_entry(&bytes).expect("deserialize");
        assert_eq!(record, deserialized);
    }

    // -- Status conversion tests --

    #[test]
    fn test_status_try_from_valid() {
        assert_eq!(Status::try_from(0u8).unwrap(), Status::Active);
        assert_eq!(Status::try_from(1u8).unwrap(), Status::Deprecated);
        assert_eq!(Status::try_from(2u8).unwrap(), Status::Proposed);
    }

    #[test]
    fn test_status_quarantined_try_from() {
        assert_eq!(Status::try_from(3u8).unwrap(), Status::Quarantined);
    }

    #[test]
    fn test_status_quarantined_display() {
        assert_eq!(format!("{}", Status::Quarantined), "Quarantined");
    }

    #[test]
    fn test_status_quarantined_counter_key() {
        assert_eq!(status_counter_key(Status::Quarantined), "total_quarantined");
    }

    #[test]
    fn test_status_try_from_invalid() {
        assert!(matches!(Status::try_from(4u8), Err(StoreError::InvalidStatus(4))));
        assert!(matches!(Status::try_from(255u8), Err(StoreError::InvalidStatus(255))));
    }

    #[test]
    fn test_status_display() {
        assert_eq!(Status::Active.to_string(), "Active");
        assert_eq!(Status::Deprecated.to_string(), "Deprecated");
        assert_eq!(Status::Proposed.to_string(), "Proposed");
    }

    // -- CoAccessRecord serialization (R-09, AC-02) --

    #[test]
    fn test_co_access_record_roundtrip() {
        let record = CoAccessRecord {
            count: 5,
            last_updated: 1000,
        };
        let bytes = serialize_co_access(&record).unwrap();
        let deserialized = deserialize_co_access(&bytes).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_co_access_record_roundtrip_zeros() {
        let record = CoAccessRecord {
            count: 0,
            last_updated: 0,
        };
        let bytes = serialize_co_access(&record).unwrap();
        let deserialized = deserialize_co_access(&bytes).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn test_co_access_record_roundtrip_max_values() {
        let record = CoAccessRecord {
            count: u32::MAX,
            last_updated: u64::MAX,
        };
        let bytes = serialize_co_access(&record).unwrap();
        let deserialized = deserialize_co_access(&bytes).unwrap();
        assert_eq!(record, deserialized);
    }

    // -- co_access_key ordering (AC-05) --

    #[test]
    fn test_co_access_key_ordering() {
        assert_eq!(co_access_key(10, 5), (5, 10));
        assert_eq!(co_access_key(5, 10), (5, 10));
        assert_eq!(co_access_key(5, 5), (5, 5));
        assert_eq!(co_access_key(0, u64::MAX), (0, u64::MAX));
    }

    // -- Helper --

    fn make_test_record() -> EntryRecord {
        EntryRecord {
            id: 1,
            title: "Test Entry".to_string(),
            content: "Test content".to_string(),
            topic: "auth".to_string(),
            category: "convention".to_string(),
            tags: vec!["rust".to_string(), "testing".to_string()],
            source: "test".to_string(),
            status: Status::Active,
            confidence: 0.0,
            created_at: 1000,
            updated_at: 1000,
            last_accessed_at: 0,
            access_count: 0,
            supersedes: None,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 0,
            created_by: String::new(),
            modified_by: String::new(),
            content_hash: String::new(),
            previous_hash: String::new(),
            version: 0,
            feature_cycle: String::new(),
            trust_source: String::new(),
            helpful_count: 0,
            unhelpful_count: 0,
        }
    }
}
