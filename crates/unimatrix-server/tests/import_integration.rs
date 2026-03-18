//! Integration tests for the knowledge import module (nan-002).
//!
//! These tests exercise `run_import` end-to-end: real database, real file I/O,
//! real export + import round-trips. They verify acceptance criteria from the
//! import-pipeline test plan.

use std::io::Write;
use std::path::Path;

use serde_json::Value;
use sqlx::Row as _;
use tempfile::TempDir;
use unimatrix_server::export::run_export;
use unimatrix_server::import::run_import;
use unimatrix_server::project;
use unimatrix_store::{SqlxStore, compute_content_hash};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Set up a project directory and return (project_dir, db_path).
fn setup_project() -> (TempDir, std::path::PathBuf) {
    let project_dir = TempDir::new().expect("create project temp dir");
    let paths = project::ensure_data_directory(Some(project_dir.path()), None).unwrap();
    (project_dir, paths.db_path)
}

/// Open a SqlxStore synchronously from a db_path.
///
/// Uses block_in_place when inside a tokio runtime (e.g. #[tokio::test(flavor = "multi_thread")]),
/// otherwise creates a temporary current-thread runtime.
fn open_store(db_path: &Path) -> SqlxStore {
    let fut = SqlxStore::open(db_path, unimatrix_store::pool_config::PoolConfig::default());
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(fut)),
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("runtime");
            rt.block_on(fut)
        }
    }
    .expect("open store")
}

/// Get the current schema version from a database path.
async fn get_schema_version(store: &SqlxStore) -> i64 {
    sqlx::query_scalar("SELECT value FROM counters WHERE name = 'schema_version'")
        .fetch_one(store.write_pool_server())
        .await
        .expect("schema_version")
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
async fn insert_full_entry(pool: &sqlx::SqlitePool, id: i64) {
    let title = format!("Entry {id}");
    let content = format!("Content for entry {id}");
    let hash = compute_content_hash(&title, &content);
    sqlx::query(
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
    )
    .bind(id)
    .bind(&title)
    .bind(&content)
    .bind(&hash)
    .execute(pool)
    .await
    .unwrap();
}

/// Populate a database with representative data across all 8 tables.
async fn populate_representative_data(pool: &sqlx::SqlitePool) {
    for id in [1i64, 2, 3] {
        insert_full_entry(pool, id).await;
    }

    // Entry tags
    for (entry_id, tag) in [(1i64, "rust"), (1, "export"), (2, "testing"), (3, "data")] {
        sqlx::query("INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)")
            .bind(entry_id)
            .bind(tag)
            .execute(pool)
            .await
            .unwrap();
    }

    // Co-access pairs
    sqlx::query("INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (1, 2, 5, 1700000000)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (2, 3, 3, 1700000001)")
        .execute(pool)
        .await
        .unwrap();

    // Feature entries
    sqlx::query("INSERT INTO feature_entries (feature_id, entry_id) VALUES ('nan-001', 1)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO feature_entries (feature_id, entry_id) VALUES ('nan-001', 2)")
        .execute(pool)
        .await
        .unwrap();

    // Outcome index
    sqlx::query("INSERT INTO outcome_index (feature_cycle, entry_id) VALUES ('nan-001', 1)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO outcome_index (feature_cycle, entry_id) VALUES ('crt-001', 3)")
        .execute(pool)
        .await
        .unwrap();

    // Agent registry
    sqlx::query(
        "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
         allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
         VALUES ('bot-1', 2, '[\"Admin\",\"Read\"]', '[\"security\"]', '[\"decision\"]', 1700000000, 1700000001, 1)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
         allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
         VALUES ('bot-2', 1, '[]', NULL, NULL, 1700000002, 1700000003, 1)",
    )
    .execute(pool)
    .await
    .unwrap();

    // Audit log
    for i in 1i64..=3 {
        sqlx::query(
            "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
             operation, target_ids, outcome, detail)
             VALUES (?1, 1700000000 + ?1, 'sess-1', 'bot-1', 'store', '[1,2]', 0, 'ok')",
        )
        .bind(i)
        .execute(pool)
        .await
        .unwrap();
    }

    // Update counters to reflect inserted data
    sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES ('next_entry_id', 4)")
        .execute(pool)
        .await
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

#[tokio::test(flavor = "multi_thread")]
async fn test_round_trip_export_import_reexport() {
    // Step 1: Create populated DB and export
    let (project_a, db_a) = setup_project();
    let store_a = open_store(&db_a);
    populate_representative_data(store_a.write_pool_server()).await;
    store_a.close().await.unwrap();

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

#[tokio::test(flavor = "multi_thread")]
async fn test_force_import_replaces_data() {
    let (project_dir, db_path) = setup_project();
    let store = open_store(&db_path);
    let sv = get_schema_version(&store).await;

    // Populate with 10 entries
    for id in 1i64..=10 {
        insert_full_entry(store.write_pool_server(), id).await;
    }
    sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES ('next_entry_id', 11)")
        .execute(store.write_pool_server())
        .await
        .unwrap();
    store.close().await.unwrap();

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
    let store = open_store(&db_path);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entries")
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
    assert_eq!(count, 5, "should have 5 entries after force import");

    // Verify content is from import, not original
    let title: String = sqlx::query_scalar("SELECT title FROM entries WHERE id = 1")
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
    assert_eq!(title, "New 1");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_import_rejected_without_force_on_nonempty() {
    let (project_dir, db_path) = setup_project();
    let store = open_store(&db_path);
    let sv = get_schema_version(&store).await;

    // Populate with entries
    insert_full_entry(store.write_pool_server(), 1).await;
    store.close().await.unwrap();

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
    let store = open_store(&db_path);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entries")
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
    assert_eq!(count, 1, "original entry should remain");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_force_on_empty_database() {
    let (project_dir, db_path) = setup_project();
    let store = open_store(&db_path);
    let sv = get_schema_version(&store).await;
    store.close().await.unwrap();

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

    let store = open_store(&db_path);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entries")
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

// ---------------------------------------------------------------------------
// Counter Restoration (AC-09)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_counter_restoration_prevents_id_collision() {
    let (project_dir, db_path) = setup_project();
    let store = open_store(&db_path);
    let sv = get_schema_version(&store).await;
    store.close().await.unwrap();

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
    let store = open_store(&db_path);
    let next_id: i64 =
        sqlx::query_scalar("SELECT value FROM counters WHERE name = 'next_entry_id'")
            .fetch_one(store.write_pool_server())
            .await
            .unwrap();
    assert!(
        next_id >= 101,
        "next_entry_id should be >= 101, got {next_id}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_counter_values_match_export() {
    let (project_a, db_a) = setup_project();
    let store_a = open_store(&db_a);
    populate_representative_data(store_a.write_pool_server()).await;
    store_a.close().await.unwrap();

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
    let store_b = open_store(&db_b);
    for (name, expected_value) in &exported_counters {
        let actual: i64 = sqlx::query_scalar("SELECT value FROM counters WHERE name = ?1")
            .bind(name)
            .fetch_one(store_b.write_pool_server())
            .await
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

#[tokio::test(flavor = "multi_thread")]
async fn test_atomicity_rollback_on_parse_failure() {
    let (project_dir, db_path) = setup_project();
    let store = open_store(&db_path);
    let sv = get_schema_version(&store).await;
    store.close().await.unwrap();

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
    let store = open_store(&db_path);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entries")
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
    assert_eq!(count, 0, "transaction should have been rolled back");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_atomicity_rollback_on_fk_violation() {
    let (project_dir, db_path) = setup_project();
    let store = open_store(&db_path);
    let sv = get_schema_version(&store).await;
    store.close().await.unwrap();

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

#[tokio::test(flavor = "multi_thread")]
async fn test_skip_hash_validation_bypass() {
    let (project_dir, db_path) = setup_project();
    let store = open_store(&db_path);
    let sv = get_schema_version(&store).await;
    store.close().await.unwrap();

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

#[tokio::test(flavor = "multi_thread")]
async fn test_hash_validation_failure_prevents_commit() {
    let (project_dir, db_path) = setup_project();
    let store = open_store(&db_path);
    let sv = get_schema_version(&store).await;
    store.close().await.unwrap();

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
    let store = open_store(&db_path);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entries")
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
    assert_eq!(count, 0, "should have rolled back on hash failure");
}

// ---------------------------------------------------------------------------
// Empty Import (AC-16)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_empty_export_imports_successfully() {
    let (project_dir, db_path) = setup_project();
    let store = open_store(&db_path);
    let sv = get_schema_version(&store).await;
    store.close().await.unwrap();

    let tmp = TempDir::new().unwrap();
    let lines = vec![
        make_header(sv, 1, 0),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 1),
    ];
    let input_path = write_jsonl(&tmp, &lines);

    run_import(Some(project_dir.path()), &input_path, false, false)
        .expect("empty import should succeed");

    let store = open_store(&db_path);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entries")
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
    assert_eq!(count, 0);

    // Counters should be set
    let sv_imported: i64 =
        sqlx::query_scalar("SELECT value FROM counters WHERE name = 'schema_version'")
            .fetch_one(store.write_pool_server())
            .await
            .unwrap();
    assert_eq!(sv_imported, sv);
}

// ---------------------------------------------------------------------------
// Audit Provenance (AC-26)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_audit_provenance_entry_written() {
    let (project_dir, db_path) = setup_project();
    let store = open_store(&db_path);
    let sv = get_schema_version(&store).await;
    store.close().await.unwrap();

    let tmp = TempDir::new().unwrap();
    let lines = vec![
        make_header(sv, 1, 1),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 2),
        make_entry_line(1, "Test", "Content", ""),
    ];
    let input_path = write_jsonl(&tmp, &lines);

    run_import(Some(project_dir.path()), &input_path, false, false).expect("import should succeed");

    let store = open_store(&db_path);
    let provenance: String =
        sqlx::query_scalar("SELECT detail FROM audit_log WHERE operation = 'import'")
            .fetch_one(store.write_pool_server())
            .await
            .unwrap();
    assert!(
        provenance.contains("1 entries"),
        "provenance should mention entry count: {provenance}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_audit_provenance_no_id_collision() {
    let (project_a, db_a) = setup_project();
    let store_a = open_store(&db_a);
    populate_representative_data(store_a.write_pool_server()).await;
    store_a.close().await.unwrap();

    // Export (includes audit_log entries with event_ids 1-3)
    let tmp = TempDir::new().unwrap();
    let export_path = tmp.path().join("export.jsonl");
    run_export(Some(project_a.path()), Some(&export_path)).unwrap();

    // Import into fresh DB
    let (project_b, db_b) = setup_project();
    run_import(Some(project_b.path()), &export_path, false, false).expect("import should succeed");

    // Provenance entry should have event_id > 3
    let store_b = open_store(&db_b);
    let provenance_id: i64 =
        sqlx::query_scalar("SELECT event_id FROM audit_log WHERE operation = 'import'")
            .fetch_one(store_b.write_pool_server())
            .await
            .unwrap();
    assert!(
        provenance_id > 3,
        "provenance event_id should be > 3 (max imported), got {provenance_id}"
    );
}

// ---------------------------------------------------------------------------
// All 8 Tables Restored (AC-07)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_all_eight_tables_restored() {
    let (project_a, db_a) = setup_project();
    let store_a = open_store(&db_a);
    populate_representative_data(store_a.write_pool_server()).await;
    store_a.close().await.unwrap();

    // Export
    let tmp = TempDir::new().unwrap();
    let export_path = tmp.path().join("export.jsonl");
    run_export(Some(project_a.path()), Some(&export_path)).unwrap();

    // Import into fresh DB
    let (project_b, db_b) = setup_project();
    run_import(Some(project_b.path()), &export_path, false, false).expect("import should succeed");

    // Verify row counts
    let store_b = open_store(&db_b);
    let pool = store_b.write_pool_server();

    let count_entries: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entries")
        .fetch_one(pool)
        .await
        .unwrap();
    let count_tags: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entry_tags")
        .fetch_one(pool)
        .await
        .unwrap();
    let count_co: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM co_access")
        .fetch_one(pool)
        .await
        .unwrap();
    let count_fe: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM feature_entries")
        .fetch_one(pool)
        .await
        .unwrap();
    let count_oi: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outcome_index")
        .fetch_one(pool)
        .await
        .unwrap();
    let count_ar: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_registry")
        .fetch_one(pool)
        .await
        .unwrap();
    let count_al: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
        .fetch_one(pool)
        .await
        .unwrap();
    let count_ct: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM counters")
        .fetch_one(pool)
        .await
        .unwrap();

    assert_eq!(count_entries, 3);
    assert_eq!(count_tags, 4);
    assert_eq!(count_co, 2);
    assert_eq!(count_fe, 2);
    assert_eq!(count_oi, 2);
    assert_eq!(count_ar, 2);
    assert_eq!(count_al, 3 + 1); // 3 imported + 1 provenance
    assert!(count_ct >= 2); // at least schema_version + next_entry_id
}

// ---------------------------------------------------------------------------
// Per-Column Verification (AC-08)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_entry_columns_preserved_exactly() {
    let (project_a, db_a) = setup_project();
    let store_a = open_store(&db_a);

    // Insert entry with edge values
    let title = "Unicode \u{4e16}\u{754c}";
    let content = "Content with \u{1f600} emoji";
    let hash = compute_content_hash(title, content);
    sqlx::query(
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
    )
    .bind(title)
    .bind(content)
    .bind(&hash)
    .execute(store_a.write_pool_server())
    .await
    .unwrap();
    sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES ('next_entry_id', 43)")
        .execute(store_a.write_pool_server())
        .await
        .unwrap();
    store_a.close().await.unwrap();

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
    let store_b = open_store(&db_b);
    let expected_hash = compute_content_hash(title, content);

    let row = sqlx::query(
        "SELECT id, title, content, topic, category, source, status, confidence,
         created_at, updated_at, last_accessed_at, access_count,
         supersedes, superseded_by, correction_count, embedding_dim,
         created_by, modified_by, content_hash, previous_hash,
         version, feature_cycle, trust_source,
         helpful_count, unhelpful_count, pre_quarantine_status
         FROM entries WHERE id = 42",
    )
    .fetch_one(store_b.write_pool_server())
    .await
    .unwrap();

    assert_eq!(row.get::<i64, _>(0), 42i64, "id");
    assert_eq!(row.get::<String, _>(1), title, "title");
    assert_eq!(row.get::<String, _>(2), content, "content");
    assert_eq!(row.get::<String, _>(3), "testing", "topic");
    assert_eq!(row.get::<String, _>(4), "decision", "category");
    assert_eq!(row.get::<String, _>(5), "integration", "source");
    assert_eq!(row.get::<i64, _>(6), 2i64, "status");
    assert_eq!(
        row.get::<f64, _>(7).to_bits(),
        0.87654321_f64.to_bits(),
        "confidence"
    );
    assert_eq!(row.get::<i64, _>(8), 1_700_000_000i64, "created_at");
    assert_eq!(row.get::<i64, _>(9), 1_700_000_001i64, "updated_at");
    assert_eq!(row.get::<i64, _>(10), 1_700_000_002i64, "last_accessed_at");
    assert_eq!(row.get::<i64, _>(11), 15i64, "access_count");
    assert_eq!(row.get::<Option<i64>, _>(12), Some(10i64), "supersedes");
    assert_eq!(row.get::<Option<i64>, _>(13), Some(50i64), "superseded_by");
    assert_eq!(row.get::<i64, _>(14), 3i64, "correction_count");
    assert_eq!(row.get::<i64, _>(15), 384i64, "embedding_dim");
    assert_eq!(row.get::<String, _>(16), "agent-x", "created_by");
    assert_eq!(row.get::<String, _>(17), "agent-y", "modified_by");
    assert_eq!(row.get::<String, _>(18), expected_hash, "content_hash");
    assert_eq!(row.get::<String, _>(19), "prev-hash", "previous_hash");
    assert_eq!(row.get::<i64, _>(20), 7i64, "version");
    assert_eq!(row.get::<String, _>(21), "crt-002", "feature_cycle");
    assert_eq!(row.get::<String, _>(22), "human", "trust_source");
    assert_eq!(row.get::<i64, _>(23), 12i64, "helpful_count");
    assert_eq!(row.get::<i64, _>(24), 2i64, "unhelpful_count");
    assert_eq!(
        row.get::<Option<i64>, _>(25),
        Some(0i64),
        "pre_quarantine_status"
    );
}

// ---------------------------------------------------------------------------
// Force Import Counter Restoration (R-03)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_force_import_counter_restoration() {
    let (project_dir, db_path) = setup_project();
    let store = open_store(&db_path);
    let sv = get_schema_version(&store).await;

    // Populate with entries 1-5
    for id in 1i64..=5 {
        insert_full_entry(store.write_pool_server(), id).await;
    }
    sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES ('next_entry_id', 51)")
        .execute(store.write_pool_server())
        .await
        .unwrap();
    store.close().await.unwrap();

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

    let store = open_store(&db_path);
    let next_id: i64 =
        sqlx::query_scalar("SELECT value FROM counters WHERE name = 'next_entry_id'")
            .fetch_one(store.write_pool_server())
            .await
            .unwrap();
    assert!(
        next_id >= 101,
        "next_entry_id should be >= 101 after force import, got {next_id}"
    );
}
