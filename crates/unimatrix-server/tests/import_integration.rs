//! Integration tests for the knowledge import module (nan-002).
//!
//! These tests exercise `run_import` end-to-end: real database, real file I/O,
//! real export + import round-trips. They verify acceptance criteria from the
//! import-pipeline test plan.

use std::io::Write;
use std::path::Path;

use serde_json::Value;
use tempfile::TempDir;
use unimatrix_server::export::run_export;
use unimatrix_server::import::run_import;
use unimatrix_server::project;
use unimatrix_store::rusqlite;
use unimatrix_store::{Store, compute_content_hash};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Set up a project directory and return (project_dir, db_path).
fn setup_project() -> (TempDir, std::path::PathBuf) {
    let project_dir = TempDir::new().expect("create project temp dir");
    let paths = project::ensure_data_directory(Some(project_dir.path()), None).unwrap();
    (project_dir, paths.db_path)
}

/// Get the current schema version from a Store.
fn get_schema_version(db_path: &Path) -> i64 {
    let store = Store::open(db_path).unwrap();
    let conn = store.lock_conn();
    conn.query_row(
        "SELECT value FROM counters WHERE name = 'schema_version'",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

/// Build an ExportHeader JSON line.
fn make_header(schema_version: i64, format_version: i64, entry_count: i64) -> String {
    serde_json::json!({
        "_header": true,
        "schema_version": schema_version,
        "exported_at": 1700000000i64,
        "entry_count": entry_count,
        "format_version": format_version
    })
    .to_string()
}

/// Build a minimal valid entry JSON line with correct content_hash.
fn make_entry_line(id: i64, title: &str, content: &str, previous_hash: &str) -> String {
    let hash = compute_content_hash(title, content);
    serde_json::json!({
        "_table": "entries",
        "id": id,
        "title": title,
        "content": content,
        "topic": "testing",
        "category": "pattern",
        "source": "test",
        "status": 0,
        "confidence": 0.5,
        "created_at": 1700000000i64,
        "updated_at": 1700000001i64,
        "last_accessed_at": 0,
        "access_count": 0,
        "supersedes": null,
        "superseded_by": null,
        "correction_count": 0,
        "embedding_dim": 384,
        "created_by": "agent",
        "modified_by": "agent",
        "content_hash": hash,
        "previous_hash": previous_hash,
        "version": 1,
        "feature_cycle": "",
        "trust_source": "direct",
        "helpful_count": 0,
        "unhelpful_count": 0,
        "pre_quarantine_status": null
    })
    .to_string()
}

/// Build a counter JSON line.
fn make_counter_line(name: &str, value: i64) -> String {
    serde_json::json!({
        "_table": "counters",
        "name": name,
        "value": value
    })
    .to_string()
}

/// Write lines to a temporary file and return the path.
fn write_jsonl(dir: &TempDir, lines: &[String]) -> std::path::PathBuf {
    let path = dir.path().join("import.jsonl");
    let mut f = std::fs::File::create(&path).unwrap();
    for line in lines {
        writeln!(f, "{line}").unwrap();
    }
    path
}

/// Insert a representative entry with all 26 columns filled.
fn insert_full_entry(conn: &rusqlite::Connection, id: i64) {
    let title = format!("Entry {id}");
    let content = format!("Content for entry {id}");
    let hash = compute_content_hash(&title, &content);
    conn.execute(
        "INSERT INTO entries (
            id, title, content, topic, category, source, status, confidence,
            created_at, updated_at, last_accessed_at, access_count,
            supersedes, superseded_by, correction_count, embedding_dim,
            created_by, modified_by, content_hash, previous_hash,
            version, feature_cycle, trust_source,
            helpful_count, unhelpful_count, pre_quarantine_status
        ) VALUES (
            ?1, ?2, ?3, 'testing', 'pattern', 'integration-test',
            1, 0.87654321,
            1700000000, 1700000001, 1700000002, 15,
            NULL, NULL, 3, 384,
            'agent-x', 'agent-y', ?4, '',
            7, 'nan-001', 'human',
            12, 2, NULL
        )",
        rusqlite::params![id, title, content, hash],
    )
    .unwrap();
}

/// Populate a database with representative data across all 8 tables.
fn populate_representative_data(conn: &rusqlite::Connection) {
    for id in [1, 2, 3] {
        insert_full_entry(conn, id);
    }

    // Entry tags
    for (entry_id, tag) in [(1, "rust"), (1, "export"), (2, "testing"), (3, "data")] {
        conn.execute(
            "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
            rusqlite::params![entry_id, tag],
        )
        .unwrap();
    }

    // Co-access pairs
    conn.execute(
        "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (1, 2, 5, 1700000000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (2, 3, 3, 1700000001)",
        [],
    )
    .unwrap();

    // Feature entries
    conn.execute(
        "INSERT INTO feature_entries (feature_id, entry_id) VALUES ('nan-001', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO feature_entries (feature_id, entry_id) VALUES ('nan-001', 2)",
        [],
    )
    .unwrap();

    // Outcome index
    conn.execute(
        "INSERT INTO outcome_index (feature_cycle, entry_id) VALUES ('nan-001', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO outcome_index (feature_cycle, entry_id) VALUES ('crt-001', 3)",
        [],
    )
    .unwrap();

    // Agent registry
    conn.execute(
        "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
         allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
         VALUES ('bot-1', 2, '[\"Admin\",\"Read\"]', '[\"security\"]', '[\"decision\"]', 1700000000, 1700000001, 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
         allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
         VALUES ('bot-2', 1, '[]', NULL, NULL, 1700000002, 1700000003, 1)",
        [],
    )
    .unwrap();

    // Audit log
    for i in 1..=3 {
        conn.execute(
            "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
             operation, target_ids, outcome, detail)
             VALUES (?1, 1700000000 + ?1, 'sess-1', 'bot-1', 'store', '[1,2]', 0, 'ok')",
            rusqlite::params![i],
        )
        .unwrap();
    }

    // Update counters to reflect inserted data
    conn.execute(
        "INSERT OR REPLACE INTO counters (name, value) VALUES ('next_entry_id', 4)",
        [],
    )
    .unwrap();
}

/// Parse all lines from export output string.
fn parse_lines(output: &str) -> Vec<Value> {
    output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("invalid JSON: {e}: {l}")))
        .collect()
}

/// Run export to string by writing to a file then reading it back.
fn run_export_to_string(project_dir: &Path, output_file: &Path) -> String {
    run_export(Some(project_dir), Some(output_file)).expect("run_export should succeed");
    std::fs::read_to_string(output_file).expect("read output file")
}

// ---------------------------------------------------------------------------
// Round-Trip (AC-15, AC-24)
// ---------------------------------------------------------------------------

#[test]
fn test_round_trip_export_import_reexport() {
    // Step 1: Create populated DB and export
    let (project_a, db_a) = setup_project();
    let store_a = Store::open(&db_a).unwrap();
    {
        let conn = store_a.lock_conn();
        populate_representative_data(&conn);
    }
    drop(store_a);

    let tmp = TempDir::new().unwrap();
    let export1_path = tmp.path().join("export1.jsonl");
    let export1 = run_export_to_string(project_a.path(), &export1_path);

    // Step 2: Import into fresh DB
    let (project_b, _db_b) = setup_project();
    run_import(
        Some(project_b.path()),
        &export1_path,
        false, // validate hashes
        false, // not force (empty DB)
    )
    .expect("import should succeed");

    // Step 3: Re-export
    let export2_path = tmp.path().join("export2.jsonl");
    let export2 = run_export_to_string(project_b.path(), &export2_path);

    // Step 4: Compare (normalize exported_at and ignore provenance audit entry)
    let normalize = |s: &str| -> Vec<String> {
        let mut result: Vec<String> = Vec::new();
        for line in s.lines() {
            if line.is_empty() {
                continue;
            }
            let mut val: Value = serde_json::from_str(line).unwrap();
            // Normalize exported_at in header
            if let Some(obj) = val.as_object_mut() {
                if obj.contains_key("_header") {
                    obj.insert("exported_at".into(), Value::Number(0.into()));
                }
            }
            result.push(serde_json::to_string(&val).unwrap());
        }
        result
    };

    let lines1 = normalize(&export1);
    let lines2_all = normalize(&export2);

    // The re-export will have an extra audit_log entry (the import provenance).
    // Filter it out for comparison.
    let lines2: Vec<String> = lines2_all
        .iter()
        .filter(|l| {
            if let Ok(v) = serde_json::from_str::<Value>(l) {
                if v.get("_table").and_then(|t| t.as_str()) == Some("audit_log") {
                    if v.get("operation").and_then(|o| o.as_str()) == Some("import") {
                        return false;
                    }
                }
            }
            true
        })
        .cloned()
        .collect();

    assert_eq!(
        lines1.len(),
        lines2.len(),
        "line count mismatch: export1={} export2={}",
        lines1.len(),
        lines2.len()
    );

    for (i, (a, b)) in lines1.iter().zip(lines2.iter()).enumerate() {
        assert_eq!(a, b, "line {i} differs:\n  export1: {a}\n  export2: {b}");
    }
}

// ---------------------------------------------------------------------------
// Force Import (AC-02, AC-06, AC-27)
// ---------------------------------------------------------------------------

#[test]
fn test_force_import_replaces_data() {
    let (project_dir, db_path) = setup_project();
    let sv = get_schema_version(&db_path);

    // Populate with 10 entries
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        for id in 1..=10 {
            insert_full_entry(&conn, id);
        }
        conn.execute(
            "INSERT OR REPLACE INTO counters (name, value) VALUES ('next_entry_id', 11)",
            [],
        )
        .unwrap();
    }
    drop(store);

    // Create import file with 5 different entries
    let tmp = TempDir::new().unwrap();
    let mut lines = vec![
        make_header(sv, 1, 5),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 6),
    ];
    for id in 1..=5 {
        lines.push(make_entry_line(
            id,
            &format!("New {id}"),
            &format!("New content {id}"),
            "",
        ));
    }
    let input_path = write_jsonl(&tmp, &lines);

    // Import with --force
    run_import(Some(project_dir.path()), &input_path, true, true)
        .expect("force import should succeed");

    // Verify only 5 entries remain
    let store = Store::open(&db_path).unwrap();
    let conn = store.lock_conn();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 5, "should have 5 entries after force import");

    // Verify content is from import, not original
    let title: String = conn
        .query_row("SELECT title FROM entries WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(title, "New 1");
}

#[test]
fn test_import_rejected_without_force_on_nonempty() {
    let (project_dir, db_path) = setup_project();
    let sv = get_schema_version(&db_path);

    // Populate with entries
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        insert_full_entry(&conn, 1);
    }
    drop(store);

    let tmp = TempDir::new().unwrap();
    let lines = vec![
        make_header(sv, 1, 1),
        make_counter_line("schema_version", sv),
        make_entry_line(2, "New", "Content", ""),
    ];
    let input_path = write_jsonl(&tmp, &lines);

    let result = run_import(Some(project_dir.path()), &input_path, false, false);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("--force"), "should suggest --force: {err}");

    // DB unchanged
    let store = Store::open(&db_path).unwrap();
    let conn = store.lock_conn();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1, "original entry should remain");
}

#[test]
fn test_force_on_empty_database() {
    let (project_dir, db_path) = setup_project();
    let sv = get_schema_version(&db_path);

    let tmp = TempDir::new().unwrap();
    let lines = vec![
        make_header(sv, 1, 1),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 2),
        make_entry_line(1, "Test", "Content", ""),
    ];
    let input_path = write_jsonl(&tmp, &lines);

    let result = run_import(Some(project_dir.path()), &input_path, false, true);
    assert!(result.is_ok(), "force on empty should succeed: {result:?}");

    let store = Store::open(&db_path).unwrap();
    let conn = store.lock_conn();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

// ---------------------------------------------------------------------------
// Counter Restoration (AC-09)
// ---------------------------------------------------------------------------

#[test]
fn test_counter_restoration_prevents_id_collision() {
    let (project_dir, db_path) = setup_project();
    let sv = get_schema_version(&db_path);

    let tmp = TempDir::new().unwrap();
    let mut lines = vec![
        make_header(sv, 1, 5),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 101),
    ];
    for id in 1..=5 {
        lines.push(make_entry_line(
            id,
            &format!("Entry {id}"),
            &format!("Content {id}"),
            "",
        ));
    }
    let input_path = write_jsonl(&tmp, &lines);

    run_import(Some(project_dir.path()), &input_path, false, false).expect("import should succeed");

    // Verify next_entry_id is 101
    let store = Store::open(&db_path).unwrap();
    let conn = store.lock_conn();
    let next_id: i64 = conn
        .query_row(
            "SELECT value FROM counters WHERE name = 'next_entry_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        next_id >= 101,
        "next_entry_id should be >= 101, got {next_id}"
    );
}

#[test]
fn test_counter_values_match_export() {
    let (project_a, db_a) = setup_project();
    let store_a = Store::open(&db_a).unwrap();
    {
        let conn = store_a.lock_conn();
        populate_representative_data(&conn);
    }
    drop(store_a);

    // Export
    let tmp = TempDir::new().unwrap();
    let export_path = tmp.path().join("export.jsonl");
    run_export(Some(project_a.path()), Some(&export_path)).unwrap();

    // Read exported counters
    let export_content = std::fs::read_to_string(&export_path).unwrap();
    let exported_counters: std::collections::HashMap<String, i64> = parse_lines(&export_content)
        .iter()
        .filter(|v| v.get("_table").and_then(|t| t.as_str()) == Some("counters"))
        .map(|v| {
            (
                v["name"].as_str().unwrap().to_string(),
                v["value"].as_i64().unwrap(),
            )
        })
        .collect();

    // Import into fresh DB
    let (project_b, db_b) = setup_project();
    run_import(Some(project_b.path()), &export_path, false, false).expect("import should succeed");

    // Compare counters
    let store_b = Store::open(&db_b).unwrap();
    let conn = store_b.lock_conn();
    for (name, expected_value) in &exported_counters {
        let actual: i64 = conn
            .query_row(
                "SELECT value FROM counters WHERE name = ?1",
                rusqlite::params![name],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            actual, *expected_value,
            "counter '{name}' mismatch: expected {expected_value}, got {actual}"
        );
    }
}

// ---------------------------------------------------------------------------
// Atomicity (AC-22)
// ---------------------------------------------------------------------------

#[test]
fn test_atomicity_rollback_on_parse_failure() {
    let (project_dir, db_path) = setup_project();
    let sv = get_schema_version(&db_path);

    let tmp = TempDir::new().unwrap();
    let lines = vec![
        make_header(sv, 1, 5),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 6),
        make_entry_line(1, "A", "A", ""),
        make_entry_line(2, "B", "B", ""),
        "THIS IS NOT VALID JSON".to_string(), // corrupt line
        make_entry_line(4, "D", "D", ""),
        make_entry_line(5, "E", "E", ""),
    ];
    let input_path = write_jsonl(&tmp, &lines);

    let result = run_import(Some(project_dir.path()), &input_path, true, false);
    assert!(result.is_err());

    // Database should have zero entries (rolled back)
    let store = Store::open(&db_path).unwrap();
    let conn = store.lock_conn();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0, "transaction should have been rolled back");
}

#[test]
fn test_atomicity_rollback_on_fk_violation() {
    let (project_dir, db_path) = setup_project();
    let sv = get_schema_version(&db_path);

    let tmp = TempDir::new().unwrap();
    let lines = vec![
        make_header(sv, 1, 0),
        make_counter_line("schema_version", sv),
        // entry_tag referencing non-existent entry
        serde_json::json!({
            "_table": "entry_tags",
            "entry_id": 999,
            "tag": "orphan"
        })
        .to_string(),
    ];
    let input_path = write_jsonl(&tmp, &lines);

    let result = run_import(Some(project_dir.path()), &input_path, true, false);
    assert!(result.is_err(), "FK violation should fail");
}

// ---------------------------------------------------------------------------
// Hash Validation Integration
// ---------------------------------------------------------------------------

#[test]
fn test_skip_hash_validation_bypass() {
    let (project_dir, db_path) = setup_project();
    let sv = get_schema_version(&db_path);

    // Create entry with tampered content but original hash
    let mut entry: Value =
        serde_json::from_str(&make_entry_line(1, "Original", "Original content", "")).unwrap();
    entry["content"] = Value::String("TAMPERED content".to_string());
    // content_hash still from "Original" + "Original content"

    let tmp = TempDir::new().unwrap();
    let lines = vec![
        make_header(sv, 1, 1),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 2),
        entry.to_string(),
    ];
    let input_path = write_jsonl(&tmp, &lines);

    // With --skip-hash-validation: should succeed
    let result = run_import(
        Some(project_dir.path()),
        &input_path,
        true, // skip
        false,
    );
    assert!(
        result.is_ok(),
        "skip-hash should allow tampered content: {result:?}"
    );
}

#[test]
fn test_hash_validation_failure_prevents_commit() {
    let (project_dir, db_path) = setup_project();
    let sv = get_schema_version(&db_path);

    let mut entry: Value =
        serde_json::from_str(&make_entry_line(1, "Title", "Content", "")).unwrap();
    entry["content_hash"] = Value::String("wrong_hash_value".to_string());

    let tmp = TempDir::new().unwrap();
    let lines = vec![
        make_header(sv, 1, 1),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 2),
        entry.to_string(),
    ];
    let input_path = write_jsonl(&tmp, &lines);

    let result = run_import(Some(project_dir.path()), &input_path, false, false);
    assert!(result.is_err());

    // Database should be empty (rolled back)
    let store = Store::open(&db_path).unwrap();
    let conn = store.lock_conn();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0, "should have rolled back on hash failure");
}

// ---------------------------------------------------------------------------
// Empty Import (AC-16)
// ---------------------------------------------------------------------------

#[test]
fn test_empty_export_imports_successfully() {
    let (project_dir, db_path) = setup_project();
    let sv = get_schema_version(&db_path);

    let tmp = TempDir::new().unwrap();
    let lines = vec![
        make_header(sv, 1, 0),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 1),
    ];
    let input_path = write_jsonl(&tmp, &lines);

    run_import(Some(project_dir.path()), &input_path, false, false)
        .expect("empty import should succeed");

    let store = Store::open(&db_path).unwrap();
    let conn = store.lock_conn();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);

    // Counters should be set
    let sv_imported: i64 = conn
        .query_row(
            "SELECT value FROM counters WHERE name = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(sv_imported, sv);
}

// ---------------------------------------------------------------------------
// Audit Provenance (AC-26)
// ---------------------------------------------------------------------------

#[test]
fn test_audit_provenance_entry_written() {
    let (project_dir, db_path) = setup_project();
    let sv = get_schema_version(&db_path);

    let tmp = TempDir::new().unwrap();
    let lines = vec![
        make_header(sv, 1, 1),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 2),
        make_entry_line(1, "Test", "Content", ""),
    ];
    let input_path = write_jsonl(&tmp, &lines);

    run_import(Some(project_dir.path()), &input_path, false, false).expect("import should succeed");

    let store = Store::open(&db_path).unwrap();
    let conn = store.lock_conn();
    let provenance: String = conn
        .query_row(
            "SELECT detail FROM audit_log WHERE operation = 'import'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        provenance.contains("1 entries"),
        "provenance should mention entry count: {provenance}"
    );
}

#[test]
fn test_audit_provenance_no_id_collision() {
    let (project_a, db_a) = setup_project();
    let store_a = Store::open(&db_a).unwrap();
    {
        let conn = store_a.lock_conn();
        populate_representative_data(&conn);
    }
    drop(store_a);

    // Export (includes audit_log entries with event_ids 1-3)
    let tmp = TempDir::new().unwrap();
    let export_path = tmp.path().join("export.jsonl");
    run_export(Some(project_a.path()), Some(&export_path)).unwrap();

    // Import into fresh DB
    let (project_b, db_b) = setup_project();
    run_import(Some(project_b.path()), &export_path, false, false).expect("import should succeed");

    // Provenance entry should have event_id > 3
    let store_b = Store::open(&db_b).unwrap();
    let conn = store_b.lock_conn();
    let provenance_id: i64 = conn
        .query_row(
            "SELECT event_id FROM audit_log WHERE operation = 'import'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        provenance_id > 3,
        "provenance event_id should be > 3 (max imported), got {provenance_id}"
    );
}

// ---------------------------------------------------------------------------
// All 8 Tables Restored (AC-07)
// ---------------------------------------------------------------------------

#[test]
fn test_all_eight_tables_restored() {
    let (project_a, db_a) = setup_project();
    let store_a = Store::open(&db_a).unwrap();
    {
        let conn = store_a.lock_conn();
        populate_representative_data(&conn);
    }
    drop(store_a);

    // Export
    let tmp = TempDir::new().unwrap();
    let export_path = tmp.path().join("export.jsonl");
    run_export(Some(project_a.path()), Some(&export_path)).unwrap();

    // Import into fresh DB
    let (project_b, db_b) = setup_project();
    run_import(Some(project_b.path()), &export_path, false, false).expect("import should succeed");

    // Verify row counts
    let store_b = Store::open(&db_b).unwrap();
    let conn = store_b.lock_conn();

    let count = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
    };

    assert_eq!(count("entries"), 3);
    assert_eq!(count("entry_tags"), 4);
    assert_eq!(count("co_access"), 2);
    assert_eq!(count("feature_entries"), 2);
    assert_eq!(count("outcome_index"), 2);
    assert_eq!(count("agent_registry"), 2);
    assert_eq!(count("audit_log"), 3 + 1); // 3 imported + 1 provenance
    assert!(count("counters") >= 2); // at least schema_version + next_entry_id
}

// ---------------------------------------------------------------------------
// Per-Column Verification (AC-08)
// ---------------------------------------------------------------------------

#[test]
fn test_entry_columns_preserved_exactly() {
    let (project_a, db_a) = setup_project();
    let store_a = Store::open(&db_a).unwrap();
    {
        let conn = store_a.lock_conn();
        // Insert entry with edge values
        let title = "Unicode \u{4e16}\u{754c}";
        let content = "Content with \u{1f600} emoji";
        let hash = compute_content_hash(title, content);
        conn.execute(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at, last_accessed_at, access_count,
                supersedes, superseded_by, correction_count, embedding_dim,
                created_by, modified_by, content_hash, previous_hash,
                version, feature_cycle, trust_source,
                helpful_count, unhelpful_count, pre_quarantine_status
            ) VALUES (
                42, ?1, ?2, 'testing', 'decision', 'integration', 2, 0.87654321,
                1700000000, 1700000001, 1700000002, 15,
                10, 50, 3, 384,
                'agent-x', 'agent-y', ?3, 'prev-hash',
                7, 'crt-002', 'human',
                12, 2, 0
            )",
            rusqlite::params![title, content, hash],
        )
        .unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO counters (name, value) VALUES ('next_entry_id', 43)",
            [],
        )
        .unwrap();
    }
    drop(store_a);

    // Export
    let tmp = TempDir::new().unwrap();
    let export_path = tmp.path().join("export.jsonl");
    run_export(Some(project_a.path()), Some(&export_path)).unwrap();

    // Import into fresh DB
    let (project_b, db_b) = setup_project();
    // Use skip_hash_validation because previous_hash="prev-hash" is intentionally
    // a non-matching value to test that it gets preserved exactly.
    run_import(Some(project_b.path()), &export_path, true, false).expect("import should succeed");

    // Verify every column
    let store_b = Store::open(&db_b).unwrap();
    let conn = store_b.lock_conn();

    let title = "Unicode \u{4e16}\u{754c}";
    let content = "Content with \u{1f600} emoji";
    let expected_hash = compute_content_hash(title, content);

    let row = conn
        .query_row(
            "SELECT id, title, content, topic, category, source, status, confidence,
             created_at, updated_at, last_accessed_at, access_count,
             supersedes, superseded_by, correction_count, embedding_dim,
             created_by, modified_by, content_hash, previous_hash,
             version, feature_cycle, trust_source,
             helpful_count, unhelpful_count, pre_quarantine_status
             FROM entries WHERE id = 42",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, f64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, Option<i64>>(12)?,
                    row.get::<_, Option<i64>>(13)?,
                    row.get::<_, i64>(14)?,
                    row.get::<_, i64>(15)?,
                    row.get::<_, String>(16)?,
                    row.get::<_, String>(17)?,
                    row.get::<_, String>(18)?,
                    row.get::<_, String>(19)?,
                    row.get::<_, i64>(20)?,
                    row.get::<_, String>(21)?,
                    row.get::<_, String>(22)?,
                    row.get::<_, i64>(23)?,
                    row.get::<_, i64>(24)?,
                    row.get::<_, Option<i64>>(25)?,
                ))
            },
        )
        .unwrap();

    assert_eq!(row.0, 42, "id");
    assert_eq!(row.1, title, "title");
    assert_eq!(row.2, content, "content");
    assert_eq!(row.3, "testing", "topic");
    assert_eq!(row.4, "decision", "category");
    assert_eq!(row.5, "integration", "source");
    assert_eq!(row.6, 2, "status");
    assert_eq!(row.7.to_bits(), 0.87654321_f64.to_bits(), "confidence");
    assert_eq!(row.8, 1_700_000_000, "created_at");
    assert_eq!(row.9, 1_700_000_001, "updated_at");
    assert_eq!(row.10, 1_700_000_002, "last_accessed_at");
    assert_eq!(row.11, 15, "access_count");
    assert_eq!(row.12, Some(10), "supersedes");
    assert_eq!(row.13, Some(50), "superseded_by");
    assert_eq!(row.14, 3, "correction_count");
    assert_eq!(row.15, 384, "embedding_dim");
    assert_eq!(row.16, "agent-x", "created_by");
    assert_eq!(row.17, "agent-y", "modified_by");
    assert_eq!(row.18, expected_hash, "content_hash");
    assert_eq!(row.19, "prev-hash", "previous_hash");
    assert_eq!(row.20, 7, "version");
    assert_eq!(row.21, "crt-002", "feature_cycle");
    assert_eq!(row.22, "human", "trust_source");
    assert_eq!(row.23, 12, "helpful_count");
    assert_eq!(row.24, 2, "unhelpful_count");
    assert_eq!(row.25, Some(0), "pre_quarantine_status");
}

// ---------------------------------------------------------------------------
// Force Import Counter Restoration (R-03)
// ---------------------------------------------------------------------------

#[test]
fn test_force_import_counter_restoration() {
    let (project_dir, db_path) = setup_project();
    let sv = get_schema_version(&db_path);

    // Populate with entries 1-50
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        for id in 1..=5 {
            insert_full_entry(&conn, id);
        }
        conn.execute(
            "INSERT OR REPLACE INTO counters (name, value) VALUES ('next_entry_id', 51)",
            [],
        )
        .unwrap();
    }
    drop(store);

    // Force-import entries 1-3 with next_entry_id=101
    let tmp = TempDir::new().unwrap();
    let mut lines = vec![
        make_header(sv, 1, 3),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 101),
    ];
    for id in 1..=3 {
        lines.push(make_entry_line(
            id,
            &format!("Imported {id}"),
            &format!("Imported content {id}"),
            "",
        ));
    }
    let input_path = write_jsonl(&tmp, &lines);

    run_import(Some(project_dir.path()), &input_path, true, true)
        .expect("force import should succeed");

    let store = Store::open(&db_path).unwrap();
    let conn = store.lock_conn();
    let next_id: i64 = conn
        .query_row(
            "SELECT value FROM counters WHERE name = 'next_entry_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        next_id >= 101,
        "next_entry_id should be >= 101 after force import, got {next_id}"
    );
}
