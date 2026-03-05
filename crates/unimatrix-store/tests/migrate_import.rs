//! Integration tests for the SQLite import path (nxs-006).
//!
//! These tests compile under the SQLite backend (default) and verify
//! that the import function correctly creates a SQLite database from
//! a JSON-lines intermediate file.
//!
//! Since both backends cannot be compiled simultaneously, these tests
//! generate intermediate files programmatically rather than relying
//! on a redb export.
//!
//! For direct SQL verification, we open the imported database using
//! rusqlite directly (not via Store) to avoid pub(crate) visibility issues.

#![cfg(feature = "backend-sqlite")]

use std::io::Write;

use unimatrix_store::migrate::import::import;
use unimatrix_store::{
    CoAccessRecord, EntryRecord, Status, Store,
    deserialize_co_access, serialize_co_access, serialize_entry,
};

/// Encode bytes as standard base64.
fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Open a raw rusqlite connection for verification queries.
fn verify_conn(path: &std::path::Path) -> rusqlite::Connection {
    rusqlite::Connection::open(path).unwrap()
}

/// Generate a complete intermediate file with data in all 17 tables.
fn generate_intermediate_file(path: &std::path::Path) {
    let mut f = std::fs::File::create(path).unwrap();

    let entry1 = make_entry(1, "First Entry", "Content one", "auth", "convention",
        vec!["rust", "error"], Status::Active, 0.85, 1700000000, 1700001000,
        1700002000, 5, None, None, 0, 384, "agent-1", "agent-1", "abc123", 1,
        "nxs-006", "agent", 3, 1);
    let entry2 = make_entry(2, "Second Entry", "Content two", "auth", "pattern",
        vec!["rust"], Status::Active, 0.5, 1700010000, 1700011000,
        0, 0, None, None, 0, 0, "", "", "def456", 1, "", "", 0, 0);
    let entry3 = make_entry(3, "Deprecated Entry", "Old content", "deploy", "decision",
        vec!["ci"], Status::Deprecated, 0.2, 1690000000, 1690001000,
        0, 0, None, Some(1), 1, 0, "", "", "ghi789", 2, "", "", 0, 0);

    let e1_blob = b64(&serialize_entry(&entry1).unwrap());
    let e2_blob = b64(&serialize_entry(&entry2).unwrap());
    let e3_blob = b64(&serialize_entry(&entry3).unwrap());

    writeln!(f, r#"{{"table":"entries","key_type":"u64","value_type":"blob","row_count":3}}"#).unwrap();
    writeln!(f, r#"{{"key":1,"value":"{e1_blob}"}}"#).unwrap();
    writeln!(f, r#"{{"key":2,"value":"{e2_blob}"}}"#).unwrap();
    writeln!(f, r#"{{"key":3,"value":"{e3_blob}"}}"#).unwrap();

    writeln!(f, r#"{{"table":"topic_index","key_type":"str_u64","value_type":"unit","row_count":3}}"#).unwrap();
    writeln!(f, r#"{{"key":["auth",1],"value":null}}"#).unwrap();
    writeln!(f, r#"{{"key":["auth",2],"value":null}}"#).unwrap();
    writeln!(f, r#"{{"key":["deploy",3],"value":null}}"#).unwrap();

    writeln!(f, r#"{{"table":"category_index","key_type":"str_u64","value_type":"unit","row_count":3}}"#).unwrap();
    writeln!(f, r#"{{"key":["convention",1],"value":null}}"#).unwrap();
    writeln!(f, r#"{{"key":["pattern",2],"value":null}}"#).unwrap();
    writeln!(f, r#"{{"key":["decision",3],"value":null}}"#).unwrap();

    writeln!(f, r#"{{"table":"tag_index","key_type":"str","value_type":"u64","multimap":true,"row_count":5}}"#).unwrap();
    writeln!(f, r#"{{"key":"ci","value":3}}"#).unwrap();
    writeln!(f, r#"{{"key":"error","value":1}}"#).unwrap();
    writeln!(f, r#"{{"key":"rust","value":1}}"#).unwrap();
    writeln!(f, r#"{{"key":"rust","value":2}}"#).unwrap();
    writeln!(f, r#"{{"key":"rust","value":3}}"#).unwrap();

    writeln!(f, r#"{{"table":"time_index","key_type":"u64_u64","value_type":"unit","row_count":3}}"#).unwrap();
    writeln!(f, r#"{{"key":[1690000000,3],"value":null}}"#).unwrap();
    writeln!(f, r#"{{"key":[1700000000,1],"value":null}}"#).unwrap();
    writeln!(f, r#"{{"key":[1700010000,2],"value":null}}"#).unwrap();

    writeln!(f, r#"{{"table":"status_index","key_type":"u8_u64","value_type":"unit","row_count":3}}"#).unwrap();
    writeln!(f, r#"{{"key":[0,1],"value":null}}"#).unwrap();
    writeln!(f, r#"{{"key":[0,2],"value":null}}"#).unwrap();
    writeln!(f, r#"{{"key":[1,3],"value":null}}"#).unwrap();

    writeln!(f, r#"{{"table":"vector_map","key_type":"u64","value_type":"u64","row_count":2}}"#).unwrap();
    writeln!(f, r#"{{"key":1,"value":100}}"#).unwrap();
    writeln!(f, r#"{{"key":2,"value":101}}"#).unwrap();

    writeln!(f, r#"{{"table":"counters","key_type":"str","value_type":"u64","row_count":7}}"#).unwrap();
    writeln!(f, r#"{{"key":"next_entry_id","value":4}}"#).unwrap();
    writeln!(f, r#"{{"key":"schema_version","value":5}}"#).unwrap();
    writeln!(f, r#"{{"key":"total_active","value":2}}"#).unwrap();
    writeln!(f, r#"{{"key":"total_deprecated","value":1}}"#).unwrap();
    writeln!(f, r#"{{"key":"total_proposed","value":0}}"#).unwrap();
    writeln!(f, r#"{{"key":"total_quarantined","value":0}}"#).unwrap();
    writeln!(f, r#"{{"key":"next_audit_event_id","value":0}}"#).unwrap();

    let agent_blob = b64(&[1, 2, 3, 4, 5]);
    writeln!(f, r#"{{"table":"agent_registry","key_type":"str","value_type":"blob","row_count":1}}"#).unwrap();
    writeln!(f, r#"{{"key":"system","value":"{agent_blob}"}}"#).unwrap();

    let audit_blob = b64(&[10, 20, 30]);
    writeln!(f, r#"{{"table":"audit_log","key_type":"u64","value_type":"blob","row_count":1}}"#).unwrap();
    writeln!(f, r#"{{"key":1,"value":"{audit_blob}"}}"#).unwrap();

    writeln!(f, r#"{{"table":"feature_entries","key_type":"str","value_type":"u64","multimap":true,"row_count":3}}"#).unwrap();
    writeln!(f, r#"{{"key":"nxs-006","value":1}}"#).unwrap();
    writeln!(f, r#"{{"key":"nxs-006","value":2}}"#).unwrap();
    writeln!(f, r#"{{"key":"nxs-005","value":3}}"#).unwrap();

    let co_access = CoAccessRecord { count: 5, last_updated: 1700000000 };
    let co_blob = b64(&serialize_co_access(&co_access).unwrap());
    writeln!(f, r#"{{"table":"co_access","key_type":"u64_u64","value_type":"blob","row_count":1}}"#).unwrap();
    writeln!(f, r#"{{"key":[1,2],"value":"{co_blob}"}}"#).unwrap();

    writeln!(f, r#"{{"table":"outcome_index","key_type":"str_u64","value_type":"unit","row_count":1}}"#).unwrap();
    writeln!(f, r#"{{"key":["nxs-006",1],"value":null}}"#).unwrap();

    let metrics_blob = b64(&[42, 43, 44]);
    writeln!(f, r#"{{"table":"observation_metrics","key_type":"str","value_type":"blob","row_count":1}}"#).unwrap();
    writeln!(f, r#"{{"key":"nxs-006","value":"{metrics_blob}"}}"#).unwrap();

    writeln!(f, r#"{{"table":"signal_queue","key_type":"u64","value_type":"blob","row_count":0}}"#).unwrap();
    writeln!(f, r#"{{"table":"sessions","key_type":"str","value_type":"blob","row_count":0}}"#).unwrap();
    writeln!(f, r#"{{"table":"injection_log","key_type":"u64","value_type":"blob","row_count":0}}"#).unwrap();
}

#[allow(clippy::too_many_arguments)]
fn make_entry(id: u64, title: &str, content: &str, topic: &str, category: &str,
    tags: Vec<&str>, status: Status, confidence: f64, created_at: u64, updated_at: u64,
    last_accessed_at: u64, access_count: u32, supersedes: Option<u64>, superseded_by: Option<u64>,
    correction_count: u32, embedding_dim: u16, created_by: &str, modified_by: &str,
    content_hash: &str, version: u32, feature_cycle: &str, trust_source: &str,
    helpful_count: u32, unhelpful_count: u32) -> EntryRecord {
    EntryRecord {
        id, title: title.to_string(), content: content.to_string(),
        topic: topic.to_string(), category: category.to_string(),
        tags: tags.into_iter().map(|s| s.to_string()).collect(),
        source: "test".to_string(), status, confidence, created_at, updated_at,
        last_accessed_at, access_count, supersedes, superseded_by, correction_count,
        embedding_dim, created_by: created_by.to_string(), modified_by: modified_by.to_string(),
        content_hash: content_hash.to_string(), previous_hash: String::new(),
        version, feature_cycle: feature_cycle.to_string(), trust_source: trust_source.to_string(),
        helpful_count, unhelpful_count,
    }
}

fn write_empty_tables(f: &mut std::fs::File) {
    for (table, key_type, value_type, multimap) in [
        ("topic_index", "str_u64", "unit", false),
        ("category_index", "str_u64", "unit", false),
        ("tag_index", "str", "u64", true),
        ("time_index", "u64_u64", "unit", false),
        ("status_index", "u8_u64", "unit", false),
        ("vector_map", "u64", "u64", false),
        ("agent_registry", "str", "blob", false),
        ("audit_log", "u64", "blob", false),
        ("feature_entries", "str", "u64", true),
        ("co_access", "u64_u64", "blob", false),
        ("outcome_index", "str_u64", "unit", false),
        ("observation_metrics", "str", "blob", false),
        ("signal_queue", "u64", "blob", false),
        ("sessions", "str", "blob", false),
        ("injection_log", "u64", "blob", false),
    ] {
        if multimap {
            writeln!(f, r#"{{"table":"{table}","key_type":"{key_type}","value_type":"{value_type}","multimap":true,"row_count":0}}"#).unwrap();
        } else {
            writeln!(f, r#"{{"table":"{table}","key_type":"{key_type}","value_type":"{value_type}","row_count":0}}"#).unwrap();
        }
    }
}

// -- T-01: Full 17-table round-trip (import side) --

#[test]
fn test_import_all_17_tables() {
    let dir = tempfile::TempDir::new().unwrap();
    let input = dir.path().join("export.jsonl");
    let output = dir.path().join("imported.db");

    generate_intermediate_file(&input);
    let summary = import(&input, &output).unwrap();

    assert_eq!(summary.tables.len(), 17);

    let counts: std::collections::HashMap<&str, u64> = summary
        .tables.iter().map(|(n, c)| (n.as_str(), *c)).collect();

    assert_eq!(counts["entries"], 3);
    assert_eq!(counts["topic_index"], 3);
    assert_eq!(counts["category_index"], 3);
    assert_eq!(counts["tag_index"], 5);
    assert_eq!(counts["time_index"], 3);
    assert_eq!(counts["status_index"], 3);
    assert_eq!(counts["vector_map"], 2);
    assert_eq!(counts["counters"], 7);
    assert_eq!(counts["agent_registry"], 1);
    assert_eq!(counts["audit_log"], 1);
    assert_eq!(counts["feature_entries"], 3);
    assert_eq!(counts["co_access"], 1);
    assert_eq!(counts["outcome_index"], 1);
    assert_eq!(counts["observation_metrics"], 1);
    assert_eq!(counts["signal_queue"], 0);
    assert_eq!(counts["sessions"], 0);
    assert_eq!(counts["injection_log"], 0);

    // Verify via SQL
    let conn = verify_conn(&output);
    let entry_count: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0)).unwrap();
    assert_eq!(entry_count, 3);
}

// -- T-02: Blob fidelity --

#[test]
fn test_import_blob_fidelity() {
    let dir = tempfile::TempDir::new().unwrap();
    let input = dir.path().join("export.jsonl");
    let output = dir.path().join("imported.db");

    generate_intermediate_file(&input);
    import(&input, &output).unwrap();

    let store = Store::open(&output).unwrap();
    let entry = store.get(1).unwrap();
    assert_eq!(entry.title, "First Entry");
    assert_eq!(entry.content, "Content one");
    assert_eq!(entry.topic, "auth");
    assert_eq!(entry.category, "convention");
    assert_eq!(entry.tags, vec!["rust", "error"]);
    assert_eq!(entry.status, Status::Active);
    assert_eq!(entry.confidence, 0.85);
    assert_eq!(entry.created_at, 1700000000);
    assert_eq!(entry.helpful_count, 3);
    assert_eq!(entry.unhelpful_count, 1);
    assert_eq!(entry.feature_cycle, "nxs-006");
    assert_eq!(entry.embedding_dim, 384);

    let entry2 = store.get(2).unwrap();
    assert_eq!(entry2.title, "Second Entry");
    assert_eq!(entry2.status, Status::Active);

    let entry3 = store.get(3).unwrap();
    assert_eq!(entry3.title, "Deprecated Entry");
    assert_eq!(entry3.status, Status::Deprecated);
    assert_eq!(entry3.superseded_by, Some(1));
}

// -- T-08: Multimap round-trip --

#[test]
fn test_import_multimap_associations() {
    let dir = tempfile::TempDir::new().unwrap();
    let input = dir.path().join("export.jsonl");
    let output = dir.path().join("imported.db");

    generate_intermediate_file(&input);
    import(&input, &output).unwrap();

    let conn = verify_conn(&output);

    // tag_index: "rust" maps to entries 1, 2, 3
    let mut stmt = conn.prepare("SELECT entry_id FROM tag_index WHERE tag = 'rust' ORDER BY entry_id").unwrap();
    let ids: Vec<i64> = stmt.query_map([], |row| row.get::<_, i64>(0)).unwrap().map(|r| r.unwrap()).collect();
    assert_eq!(ids, vec![1, 2, 3]);

    // feature_entries: "nxs-006" maps to entries 1, 2
    let mut stmt = conn.prepare("SELECT entry_id FROM feature_entries WHERE feature_id = 'nxs-006' ORDER BY entry_id").unwrap();
    let ids: Vec<i64> = stmt.query_map([], |row| row.get::<_, i64>(0)).unwrap().map(|r| r.unwrap()).collect();
    assert_eq!(ids, vec![1, 2]);

    // Total tag_index pairs = 5
    let tag_count: i64 = conn.query_row("SELECT COUNT(*) FROM tag_index", [], |r| r.get(0)).unwrap();
    assert_eq!(tag_count, 5);
}

// -- T-10: Counter verification --

#[test]
fn test_import_counter_state() {
    let dir = tempfile::TempDir::new().unwrap();
    let input = dir.path().join("export.jsonl");
    let output = dir.path().join("imported.db");

    generate_intermediate_file(&input);
    import(&input, &output).unwrap();

    let conn = verify_conn(&output);

    let next_id: i64 = conn.query_row("SELECT value FROM counters WHERE name = 'next_entry_id'", [], |r| r.get(0)).unwrap();
    assert_eq!(next_id, 4);

    let schema_ver: i64 = conn.query_row("SELECT value FROM counters WHERE name = 'schema_version'", [], |r| r.get(0)).unwrap();
    assert_eq!(schema_ver, 5);

    let max_id: i64 = conn.query_row("SELECT MAX(id) FROM entries", [], |r| r.get(0)).unwrap();
    assert!(next_id > max_id, "next_entry_id ({next_id}) must be > MAX(entries.id) ({max_id})");
}

// -- T-11: Counter overwrite --

#[test]
fn test_import_counter_overwrite() {
    let dir = tempfile::TempDir::new().unwrap();
    let input = dir.path().join("export.jsonl");
    let output = dir.path().join("imported.db");

    generate_intermediate_file(&input);
    import(&input, &output).unwrap();

    // The imported next_entry_id should be 4, not 1 (Store::open default)
    let conn = verify_conn(&output);
    let next_id: i64 = conn.query_row("SELECT value FROM counters WHERE name = 'next_entry_id'", [], |r| r.get(0)).unwrap();
    assert_eq!(next_id, 4, "counter should be overwritten by import, not Store::open default");
}

// -- T-12: i64::MAX boundary --

#[test]
fn test_import_i64_max_boundary() {
    let dir = tempfile::TempDir::new().unwrap();
    let input = dir.path().join("export.jsonl");
    let output = dir.path().join("imported.db");

    let max_id = i64::MAX as u64;
    let entry = make_entry(max_id, "Boundary", "Test", "test", "test",
        vec![], Status::Active, 0.0, 1000, 1000, 0, 0, None, None, 0, 0,
        "", "", "", 0, "", "", 0, 0);
    let blob = b64(&serialize_entry(&entry).unwrap());

    let mut f = std::fs::File::create(&input).unwrap();
    writeln!(f, r#"{{"table":"entries","key_type":"u64","value_type":"blob","row_count":1}}"#).unwrap();
    writeln!(f, r#"{{"key":{max_id},"value":"{blob}"}}"#).unwrap();
    write_empty_tables(&mut f);
    writeln!(f, r#"{{"table":"counters","key_type":"str","value_type":"u64","row_count":2}}"#).unwrap();
    writeln!(f, r#"{{"key":"schema_version","value":5}}"#).unwrap();
    writeln!(f, r#"{{"key":"next_entry_id","value":{max_id}}}"#).unwrap();
    drop(f);

    // Should fail: next_entry_id (i64::MAX) is not > MAX(entries.id) (i64::MAX)
    let result = import(&input, &output);
    assert!(result.is_err(), "should fail verification: next_entry_id not > MAX(entries.id)");
}

// -- T-13: u64 overflow detection --

#[test]
fn test_validate_i64_range_overflow() {
    use unimatrix_store::migrate::format::validate_i64_range;

    assert!(validate_i64_range(0).is_ok());
    assert!(validate_i64_range(i64::MAX as u64).is_ok());
    assert!(validate_i64_range(i64::MAX as u64 + 1).is_err());
    assert!(validate_i64_range(u64::MAX).is_err());
}

// -- T-14: Empty database round-trip --

#[test]
fn test_import_empty_database() {
    let dir = tempfile::TempDir::new().unwrap();
    let input = dir.path().join("export.jsonl");
    let output = dir.path().join("imported.db");

    let mut f = std::fs::File::create(&input).unwrap();
    writeln!(f, r#"{{"table":"entries","key_type":"u64","value_type":"blob","row_count":0}}"#).unwrap();
    write_empty_tables(&mut f);
    writeln!(f, r#"{{"table":"counters","key_type":"str","value_type":"u64","row_count":5}}"#).unwrap();
    writeln!(f, r#"{{"key":"schema_version","value":5}}"#).unwrap();
    writeln!(f, r#"{{"key":"next_entry_id","value":1}}"#).unwrap();
    writeln!(f, r#"{{"key":"next_signal_id","value":0}}"#).unwrap();
    writeln!(f, r#"{{"key":"next_log_id","value":0}}"#).unwrap();
    writeln!(f, r#"{{"key":"next_audit_event_id","value":0}}"#).unwrap();
    drop(f);

    let summary = import(&input, &output).unwrap();
    assert_eq!(summary.tables.len(), 17);

    let counts: std::collections::HashMap<&str, u64> = summary.tables.iter().map(|(n, c)| (n.as_str(), *c)).collect();
    for (table, expected) in [
        ("entries", 0), ("topic_index", 0), ("category_index", 0),
        ("tag_index", 0), ("time_index", 0), ("status_index", 0),
        ("vector_map", 0), ("counters", 5), ("agent_registry", 0),
        ("audit_log", 0), ("feature_entries", 0), ("co_access", 0),
        ("outcome_index", 0), ("observation_metrics", 0),
        ("signal_queue", 0), ("sessions", 0), ("injection_log", 0),
    ] {
        assert_eq!(counts[table], expected, "table {table} count mismatch");
    }
}

// -- AC-09: Import refuses overwrite --

#[test]
fn test_import_refuses_overwrite() {
    let dir = tempfile::TempDir::new().unwrap();
    let input = dir.path().join("export.jsonl");
    let output = dir.path().join("existing.db");

    generate_intermediate_file(&input);
    std::fs::write(&output, "existing content").unwrap();

    let result = import(&input, &output);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("already exists"), "error should mention file exists: {err}");
}

// -- Co-access ordering invariant (AC-06) --

#[test]
fn test_import_co_access_ordering() {
    let dir = tempfile::TempDir::new().unwrap();
    let input = dir.path().join("export.jsonl");
    let output = dir.path().join("imported.db");

    generate_intermediate_file(&input);
    import(&input, &output).unwrap();

    let conn = verify_conn(&output);

    let mut stmt = conn.prepare("SELECT entry_id_a, entry_id_b FROM co_access").unwrap();
    let rows: Vec<(i64, i64)> = stmt
        .query_map([], |row| Ok((row.get::<_, i64>(0).unwrap(), row.get::<_, i64>(1).unwrap())))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    for (a, b) in &rows {
        assert!(*a < *b, "co_access ordering violated: {a} >= {b}");
    }
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0], (1, 2));

    // Verify blob fidelity
    let co_blob: Vec<u8> = conn.query_row(
        "SELECT data FROM co_access WHERE entry_id_a = 1 AND entry_id_b = 2",
        [], |row| row.get(0),
    ).unwrap();
    let record = deserialize_co_access(&co_blob).unwrap();
    assert_eq!(record.count, 5);
    assert_eq!(record.last_updated, 1700000000);
}
