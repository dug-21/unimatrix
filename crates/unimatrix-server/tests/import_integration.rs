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
use unimatrix_server::export::run_export_with_base;
use unimatrix_server::import::run_import_with_base;
use unimatrix_server::project;
use unimatrix_store::{SqlxStore, compute_content_hash};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Set up a project directory with an isolated base_dir and return
/// (project_dir, base_dir, db_path).
///
/// Uses an explicit `base_dir` TempDir so the database is resolved inside a
/// temporary directory rather than `~/.unimatrix/`. This prevents test runs
/// from leaking orphaned hash directories into the user's home directory.
fn setup_project() -> (TempDir, TempDir, std::path::PathBuf) {
    let project_dir = TempDir::new().expect("create project temp dir");
    let base_dir = TempDir::new().expect("create base temp dir");
    let paths =
        project::ensure_data_directory(Some(project_dir.path()), Some(base_dir.path())).unwrap();
    // Meta-assertion: data_dir must live inside base_dir (GH#640 guard).
    assert!(
        paths.data_dir.starts_with(base_dir.path()),
        "data_dir must be inside base_dir to prevent home directory leaks"
    );
    (project_dir, base_dir, paths.db_path)
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
    // Sync next_audit_id to match the 3 directly-inserted audit_log rows.
    // Without this, export captures next_audit_id=0 while audit_log has
    // event_ids 1-3, and the post-import provenance write (which now correctly
    // uses the counter via log_audit_event) would collide at event_id=1 (GH#633).
    sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES ('next_audit_id', 3)")
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
/// Uses `run_export_with_base` to keep all test data inside `base_dir`.
fn run_export_to_string(project_dir: &Path, base_dir: &Path, output_file: &Path) -> String {
    run_export_with_base(
        Some(project_dir),
        Some(output_file),
        base_dir,
        None,
        false,
        false,
    )
    .expect("run_export_with_base should succeed");
    std::fs::read_to_string(output_file).expect("read output file")
}

// ---------------------------------------------------------------------------
// Round-Trip (AC-15, AC-24)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_round_trip_export_import_reexport() {
    // Step 1: Create populated DB and export
    let (project_a, base_dir_a, db_a) = setup_project();
    let store_a = open_store(&db_a);
    populate_representative_data(store_a.write_pool_server()).await;
    store_a.close().await.unwrap();

    let tmp = TempDir::new().unwrap();
    let export1_path = tmp.path().join("export1.jsonl");
    let export1 = run_export_to_string(project_a.path(), base_dir_a.path(), &export1_path);

    // Step 2: Import into fresh DB
    let (project_b, base_dir_b, _db_b) = setup_project();
    run_import_with_base(
        Some(project_b.path()),
        &export1_path,
        None,
        false, // validate hashes
        false, // not force (empty DB)
        base_dir_b.path(),
    )
    .expect("import should succeed");

    // Step 3: Re-export
    let export2_path = tmp.path().join("export2.jsonl");
    let export2 = run_export_to_string(project_b.path(), base_dir_b.path(), &export2_path);

    // Step 4: Compare (normalize exported_at, next_audit_id counter,
    // and ignore provenance audit entry)
    let normalize = |s: &str| -> Vec<String> {
        let mut result: Vec<String> = Vec::new();
        for line in s.lines() {
            if line.is_empty() {
                continue;
            }
            let mut val: Value = serde_json::from_str(line).unwrap();
            if let Some(obj) = val.as_object_mut() {
                // Normalize exported_at in header
                if obj.contains_key("_header") {
                    obj.insert("exported_at".into(), Value::Number(0.into()));
                }
                // Normalize next_audit_id counter value: record_provenance
                // increments it by 1 during import (GH#633 fix), so the
                // re-export will have value+1. Zero it for comparison.
                if obj.get("_table").and_then(|t| t.as_str()) == Some("counters")
                    && obj.get("name").and_then(|n| n.as_str()) == Some("next_audit_id")
                {
                    obj.insert("value".into(), Value::Number(0.into()));
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
            if let Ok(v) = serde_json::from_str::<Value>(l)
                && v.get("_table").and_then(|t| t.as_str()) == Some("audit_log")
                && v.get("operation").and_then(|o| o.as_str()) == Some("import")
            {
                return false;
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
    let (project_dir, base_dir, db_path) = setup_project();
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
    run_import_with_base(
        Some(project_dir.path()),
        &input_path,
        None,
        true,
        true,
        base_dir.path(),
    )
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
    let (project_dir, base_dir, db_path) = setup_project();
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

    let result = run_import_with_base(
        Some(project_dir.path()),
        &input_path,
        None,
        false,
        false,
        base_dir.path(),
    );
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
    let (project_dir, base_dir, db_path) = setup_project();
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

    let result = run_import_with_base(
        Some(project_dir.path()),
        &input_path,
        None,
        false,
        true,
        base_dir.path(),
    );
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
    let (project_dir, base_dir, db_path) = setup_project();
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

    run_import_with_base(
        Some(project_dir.path()),
        &input_path,
        None,
        false,
        false,
        base_dir.path(),
    )
    .expect("import should succeed");

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
    let (project_a, base_dir_a, db_a) = setup_project();
    let store_a = open_store(&db_a);
    populate_representative_data(store_a.write_pool_server()).await;
    store_a.close().await.unwrap();

    // Export
    let tmp = TempDir::new().unwrap();
    let export_path = tmp.path().join("export.jsonl");
    run_export_with_base(
        Some(project_a.path()),
        Some(&export_path),
        base_dir_a.path(),
        None,
        false,
        false,
    )
    .unwrap();

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
    let (project_b, base_dir_b, db_b) = setup_project();
    run_import_with_base(
        Some(project_b.path()),
        &export_path,
        None,
        false,
        false,
        base_dir_b.path(),
    )
    .expect("import should succeed");

    // Compare counters
    let store_b = open_store(&db_b);
    for (name, expected_value) in &exported_counters {
        let actual: i64 = sqlx::query_scalar("SELECT value FROM counters WHERE name = ?1")
            .bind(name)
            .fetch_one(store_b.write_pool_server())
            .await
            .unwrap();
        // next_audit_id is incremented by 1 during import because
        // record_provenance writes a provenance audit entry via
        // log_audit_event (GH#633 fix).
        if name == "next_audit_id" {
            assert_eq!(
                actual,
                *expected_value + 1,
                "counter '{name}' mismatch: expected {expected_value}+1 (provenance), got {actual}"
            );
        } else {
            assert_eq!(
                actual, *expected_value,
                "counter '{name}' mismatch: expected {expected_value}, got {actual}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Atomicity (AC-22)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_atomicity_rollback_on_parse_failure() {
    let (project_dir, base_dir, db_path) = setup_project();
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

    let result = run_import_with_base(
        Some(project_dir.path()),
        &input_path,
        None,
        true,
        false,
        base_dir.path(),
    );
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
    let (project_dir, base_dir, db_path) = setup_project();
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

    let result = run_import_with_base(
        Some(project_dir.path()),
        &input_path,
        None,
        true,
        false,
        base_dir.path(),
    );
    assert!(result.is_err(), "FK violation should fail");
}

// ---------------------------------------------------------------------------
// Hash Validation Integration
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_skip_hash_validation_bypass() {
    let (project_dir, base_dir, db_path) = setup_project();
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
    let result = run_import_with_base(
        Some(project_dir.path()),
        &input_path,
        None,
        true, // skip
        false,
        base_dir.path(),
    );
    assert!(
        result.is_ok(),
        "skip-hash should allow tampered content: {result:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_hash_validation_failure_prevents_commit() {
    let (project_dir, base_dir, db_path) = setup_project();
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

    let result = run_import_with_base(
        Some(project_dir.path()),
        &input_path,
        None,
        false,
        false,
        base_dir.path(),
    );
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
    let (project_dir, base_dir, db_path) = setup_project();
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

    run_import_with_base(
        Some(project_dir.path()),
        &input_path,
        None,
        false,
        false,
        base_dir.path(),
    )
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
    let (project_dir, base_dir, db_path) = setup_project();
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

    run_import_with_base(
        Some(project_dir.path()),
        &input_path,
        None,
        false,
        false,
        base_dir.path(),
    )
    .expect("import should succeed");

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
    let (project_a, base_dir_a, db_a) = setup_project();
    let store_a = open_store(&db_a);
    populate_representative_data(store_a.write_pool_server()).await;
    store_a.close().await.unwrap();

    // Export (includes audit_log entries with event_ids 1-3)
    let tmp = TempDir::new().unwrap();
    let export_path = tmp.path().join("export.jsonl");
    run_export_with_base(
        Some(project_a.path()),
        Some(&export_path),
        base_dir_a.path(),
        None,
        false,
        false,
    )
    .unwrap();

    // Import into fresh DB
    let (project_b, base_dir_b, db_b) = setup_project();
    run_import_with_base(
        Some(project_b.path()),
        &export_path,
        None,
        false,
        false,
        base_dir_b.path(),
    )
    .expect("import should succeed");

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
    let (project_a, base_dir_a, db_a) = setup_project();
    let store_a = open_store(&db_a);
    populate_representative_data(store_a.write_pool_server()).await;
    store_a.close().await.unwrap();

    // Export
    let tmp = TempDir::new().unwrap();
    let export_path = tmp.path().join("export.jsonl");
    run_export_with_base(
        Some(project_a.path()),
        Some(&export_path),
        base_dir_a.path(),
        None,
        false,
        false,
    )
    .unwrap();

    // Import into fresh DB
    let (project_b, base_dir_b, db_b) = setup_project();
    run_import_with_base(
        Some(project_b.path()),
        &export_path,
        None,
        false,
        false,
        base_dir_b.path(),
    )
    .expect("import should succeed");

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
    let (project_a, base_dir_a, db_a) = setup_project();
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
    run_export_with_base(
        Some(project_a.path()),
        Some(&export_path),
        base_dir_a.path(),
        None,
        false,
        false,
    )
    .unwrap();

    // Import into fresh DB
    let (project_b, base_dir_b, db_b) = setup_project();
    // Use skip_hash_validation because previous_hash="prev-hash" is intentionally
    // a non-matching value to test that it gets preserved exactly.
    run_import_with_base(
        Some(project_b.path()),
        &export_path,
        None,
        true,
        false,
        base_dir_b.path(),
    )
    .expect("import should succeed");

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
    let (project_dir, base_dir, db_path) = setup_project();
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

    run_import_with_base(
        Some(project_dir.path()),
        &input_path,
        None,
        true,
        true,
        base_dir.path(),
    )
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

// ---------------------------------------------------------------------------
// nxs-012: Helpers for new tables
// ---------------------------------------------------------------------------

async fn insert_test_graph_edge(
    pool: &sqlx::SqlitePool,
    source_id: i64,
    target_id: i64,
    relation_type: &str,
    weight: f64,
) {
    sqlx::query(
        "INSERT INTO graph_edges (source_id, target_id, relation_type, weight,
             created_at, created_by, source, bootstrap_only, metadata)
         VALUES (?1, ?2, ?3, ?4, 1700000000, 'test-agent', 'integration-test', 0, NULL)",
    )
    .bind(source_id)
    .bind(target_id)
    .bind(relation_type)
    .bind(weight)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_test_observation(
    pool: &sqlx::SqlitePool,
    id: i64,
    session_id: &str,
    hook: &str,
    ts: i64,
) {
    sqlx::query(
        "INSERT INTO observations (id, session_id, ts_millis, hook)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(id)
    .bind(session_id)
    .bind(ts)
    .bind(hook)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_test_cycle_event(
    pool: &sqlx::SqlitePool,
    id: i64,
    cycle_id: &str,
    seq: i64,
    event_type: &str,
) {
    sqlx::query(
        "INSERT INTO cycle_events (id, cycle_id, seq, event_type, timestamp)
         VALUES (?1, ?2, ?3, ?4, 1700000000)",
    )
    .bind(id)
    .bind(cycle_id)
    .bind(seq)
    .bind(event_type)
    .execute(pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// nxs-012: R-02 -- drop_all_data clears new tables + FK-dependent metric tables
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_force_import_clears_observation_metric_tables() {
    // R-02: --force import must clear observations, graph_edges, and cycle_events.
    // FK-dependent tables (observation_phase_metrics, observation_metrics) are
    // also cleared by the explicit DELETE ordering in drop_all_data (ADR-001).
    // We verify:
    // 1. observations, graph_edges, cycle_events are empty after force import.
    // 2. The force succeeds without FK constraint errors, proving the DELETE ordering
    //    (phase_metrics before metrics before observations) is correct.
    let (project_dir, base_dir, db_path) = setup_project();
    let store = open_store(&db_path);
    let sv = get_schema_version(&store).await;
    let pool = store.write_pool_server();

    // Populate all 3 new tables
    insert_test_observation(pool, 1, "sess-1", "on_tool", 1700000001).await;
    insert_test_observation(pool, 2, "sess-2", "on_result", 1700000002).await;
    insert_full_entry(pool, 1).await;
    insert_test_graph_edge(pool, 1, 1, "SelfRef", 1.0).await;
    insert_test_cycle_event(pool, 1, "nxs-012", 1, "cycle_start").await;

    // Populate observation_phase_metrics (FK child of observations in some schema versions).
    // Use INSERT OR IGNORE to handle schemas where this table has a different structure.
    // The feature_cycle PK variant:
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO observation_phase_metrics
         (feature_cycle, phase, computed_at)
         VALUES ('nxs-012', 'assimilate', 1700000000)",
    )
    .execute(pool)
    .await;

    sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES ('next_audit_id', 100)")
        .execute(pool)
        .await
        .unwrap();
    store.close().await.unwrap();

    // Force-import with empty data (no rows, just counters)
    let tmp = TempDir::new().unwrap();
    let lines = vec![
        make_header(sv, 2, 0),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 1),
        make_counter_line("next_audit_id", 200),
    ];
    let input_path = write_jsonl(&tmp, &lines);

    // This must succeed — proving DELETE ordering is correct (no FK constraint errors)
    run_import_with_base(
        Some(project_dir.path()),
        &input_path,
        None,
        false,
        true, // force
        base_dir.path(),
    )
    .expect("R-02: force import must succeed with correct FK-safe DELETE ordering");

    // Verify all 3 new tables are empty after force
    let store = open_store(&db_path);
    let pool = store.write_pool_server();

    let obs_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM observations")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(obs_count, 0, "R-02: observations cleared after --force");

    let ge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(ge_count, 0, "R-02: graph_edges cleared after --force");

    let ce_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cycle_events")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(ce_count, 0, "R-02: cycle_events cleared after --force");

    // observation_phase_metrics should also be cleared
    let opm_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM observation_phase_metrics")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    assert_eq!(
        opm_count, 0,
        "R-02: observation_phase_metrics cleared after --force"
    );
}

// ---------------------------------------------------------------------------
// nxs-012: R-05 -- observations.id PRIMARY KEY collision on force import with existing ID
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_observations_id_collision_via_plain_insert() {
    // R-05: importing observations with id=1 into a DB that already has
    // observations id=1 WITHOUT --force must fail (non-empty DB guard kicks in first).
    // With --force (which clears the table first) it must succeed.
    let (project_dir, base_dir, db_path) = setup_project();
    let store = open_store(&db_path);
    let sv = get_schema_version(&store).await;
    let pool = store.write_pool_server();

    // Pre-populate: an entry (to trigger non-empty guard) + observation id=1
    insert_full_entry(pool, 1).await;
    insert_test_observation(pool, 1, "existing", "on_tool", 1700000000).await;
    sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES ('next_entry_id', 2)")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES ('next_audit_id', 1)")
        .execute(pool)
        .await
        .unwrap();
    store.close().await.unwrap();

    // Craft import file with observations id=1 (would collide if table not cleared)
    let tmp = TempDir::new().unwrap();
    let obs_line = serde_json::json!({
        "_table": "observations",
        "id": 1i64,
        "session_id": "new-sess",
        "ts_millis": 1700001000i64,
        "hook": "on_result",
        "tool": null,
        "input": null,
        "response_size": null,
        "response_snippet": null,
        "topic_signal": null,
        "phase": null
    })
    .to_string();
    let lines_no_force = vec![
        make_header(sv, 2, 0),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 1),
        make_counter_line("next_audit_id", 10),
        obs_line.clone(),
    ];
    let input_path = write_jsonl(&tmp, &lines_no_force);

    // Without --force: fails because DB is non-empty (entries table has rows)
    let result = run_import_with_base(
        Some(project_dir.path()),
        &input_path,
        None,
        false,
        false, // no force
        base_dir.path(),
    );
    assert!(
        result.is_err(),
        "R-05: non-empty DB without --force must fail"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("--force"),
        "R-05: error should mention --force: {err_msg}"
    );

    // With --force: clears table first, import succeeds (no ID collision)
    let tmp2 = TempDir::new().unwrap();
    let lines_force = vec![
        make_header(sv, 2, 0),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 1),
        make_counter_line("next_audit_id", 100),
        obs_line,
    ];
    let input_path2 = write_jsonl(&tmp2, &lines_force);
    let result2 = run_import_with_base(
        Some(project_dir.path()),
        &input_path2,
        None,
        false,
        true, // force
        base_dir.path(),
    );
    assert!(
        result2.is_ok(),
        "R-05: --force clears table before insert, must succeed: {result2:?}"
    );

    // Verify the new observation (from import) is present
    let store = open_store(&db_path);
    let hook: String = sqlx::query_scalar("SELECT hook FROM observations WHERE id = 1")
        .fetch_one(store.write_pool_server())
        .await
        .unwrap();
    assert_eq!(
        hook, "on_result",
        "R-05: imported observation present after force import"
    );
}

// ---------------------------------------------------------------------------
// nxs-012: AC-15, AC-16, AC-17 -- full round-trip with all 11 tables
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_round_trip_all_11_tables() {
    // AC-15: export DB with all 11 tables populated, import into fresh DB,
    // re-export, compare (excluding exported_at and goal_embedding).
    // AC-16: observations.id preserved through export/import.
    // AC-17: cycle_events.id preserved through export/import.
    let (project_a, base_dir_a, db_a) = setup_project();
    let store_a = open_store(&db_a);

    {
        let pool = store_a.write_pool_server();
        // Populate original 8 tables
        populate_representative_data(pool).await;

        // Add new tables
        insert_test_graph_edge(pool, 1, 2, "Supports", 0.85).await;
        insert_test_graph_edge(pool, 2, 3, "Relates", 0.5).await;
        insert_test_observation(pool, 1, "sess-a", "context_store", 1700000100).await;
        insert_test_observation(pool, 2, "sess-b", "context_search", 1700000200).await;
        insert_test_cycle_event(pool, 1, "nxs-012", 1, "cycle_start").await;
        insert_test_cycle_event(pool, 2, "nxs-012", 2, "cycle_end").await;
    }
    store_a.close().await.unwrap();

    let tmp = TempDir::new().unwrap();
    let export1_path = tmp.path().join("export1.jsonl");
    let export1 = run_export_to_string(project_a.path(), base_dir_a.path(), &export1_path);

    // Import into fresh DB
    let (project_b, base_dir_b, db_b) = setup_project();
    run_import_with_base(
        Some(project_b.path()),
        &export1_path,
        None,
        false,
        false,
        base_dir_b.path(),
    )
    .expect("import should succeed");

    // AC-16: verify observations.id preserved
    let store_b = open_store(&db_b);
    let pool_b = store_b.write_pool_server();
    let obs_ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM observations ORDER BY id")
        .fetch_all(pool_b)
        .await
        .unwrap();
    assert_eq!(obs_ids, vec![1i64, 2], "AC-16: observations.id preserved");

    // AC-17: verify cycle_events.id preserved
    let ce_ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM cycle_events ORDER BY id")
        .fetch_all(pool_b)
        .await
        .unwrap();
    assert_eq!(ce_ids, vec![1i64, 2], "AC-17: cycle_events.id preserved");

    // Verify graph_edges present
    let ge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(pool_b)
        .await
        .unwrap();
    assert_eq!(ge_count, 2, "graph_edges preserved through import");
    store_b.close().await.unwrap();

    // Re-export and compare (AC-15)
    let export2_path = tmp.path().join("export2.jsonl");
    let export2 = run_export_to_string(project_b.path(), base_dir_b.path(), &export2_path);

    let normalize = |s: &str| -> Vec<String> {
        let mut result: Vec<String> = Vec::new();
        for line in s.lines() {
            if line.is_empty() {
                continue;
            }
            let mut val: serde_json::Value = serde_json::from_str(line).unwrap();
            if let Some(obj) = val.as_object_mut() {
                if obj.contains_key("_header") {
                    obj.insert("exported_at".into(), serde_json::Value::Number(0.into()));
                }
                // Normalize next_audit_id counter (incremented by provenance write)
                if obj.get("_table").and_then(|t| t.as_str()) == Some("counters")
                    && obj.get("name").and_then(|n| n.as_str()) == Some("next_audit_id")
                {
                    obj.insert("value".into(), serde_json::Value::Number(0.into()));
                }
            }
            result.push(serde_json::to_string(&val).unwrap());
        }
        result
    };

    let lines1 = normalize(&export1);
    let lines2_all = normalize(&export2);

    // Filter out provenance audit entry added during import
    let lines2: Vec<String> = lines2_all
        .iter()
        .filter(|l| {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(l)
                && v.get("_table").and_then(|t| t.as_str()) == Some("audit_log")
                && v.get("operation").and_then(|o| o.as_str()) == Some("import")
            {
                return false;
            }
            true
        })
        .cloned()
        .collect();

    assert_eq!(
        lines1.len(),
        lines2.len(),
        "AC-15: line count mismatch export1={} export2={}",
        lines1.len(),
        lines2.len()
    );
    for (i, (a, b)) in lines1.iter().zip(lines2.iter()).enumerate() {
        assert_eq!(
            a, b,
            "AC-15: line {i} differs:\n  export1: {a}\n  export2: {b}"
        );
    }
}

// ===========================================================================
// vnc-048: `import --slug` branch — pre-flight gates + vector redirect
// (Component 3). Restore into the runtime's literal-slug store
// (`{base}/<slug>/unimatrix.db` + `/vector`) resolved through the shared funnel.
// ===========================================================================

const SLUG_DB_NAME: &str = "unimatrix.db";
const SLUG_VECTOR_DIR: &str = "vector";

use std::sync::Arc;
use unimatrix_embed::{EmbedConfig, OnnxProvider, embed_entry};
use unimatrix_store::pool_config::PoolConfig;
use unimatrix_vector::{VectorConfig, VectorIndex};

/// Metadata file the daemon probes at boot to decide load-vs-build
/// (`http_provision.rs:196`; `unimatrix-vector` `METADATA_FILENAME`).
const SLUG_VECTOR_META: &str = "unimatrix-vector.meta";

/// Seed a freshly-registered (empty, audit-empty) slug store at
/// `{base}/<slug>/unimatrix.db` + `{base}/<slug>/vector`, mirroring what
/// `project register` lays down. `open` creates + migrates the schema (empty
/// entries, empty audit_log). Returns the slug dir.
async fn seed_slug_store(base: &Path, slug: &str) -> std::path::PathBuf {
    let slug_dir = base.join(slug);
    std::fs::create_dir_all(slug_dir.join(SLUG_VECTOR_DIR)).expect("create slug vector dir");
    let db = slug_dir.join(SLUG_DB_NAME);
    let store = SqlxStore::open(&db, PoolConfig::default())
        .await
        .expect("seed slug store open");
    store.close().await.expect("close seeded slug store");
    slug_dir
}

/// Count regular files directly under `dir` (0 if `dir` is absent).
fn file_count(dir: &Path) -> usize {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .count(),
        Err(_) => 0,
    }
}

/// Build an audit_log JSONL line (explicit event_id — the restore path uses a
/// plain INSERT that would collide on a non-empty target, C-5).
fn make_audit_line(event_id: i64, operation: &str, detail: &str) -> String {
    serde_json::json!({
        "_table": "audit_log",
        "event_id": event_id,
        "timestamp": 1700000000i64 + event_id,
        "session_id": "seed-sess",
        "agent_id": "system",
        "operation": operation,
        "target_ids": "[]",
        "outcome": 0,
        "detail": detail
    })
    .to_string()
}

/// Build a graph_edges JSONL line.
fn make_graph_edge_line(
    source_id: i64,
    target_id: i64,
    relation_type: &str,
    weight: f64,
) -> String {
    serde_json::json!({
        "_table": "graph_edges",
        "source_id": source_id,
        "target_id": target_id,
        "relation_type": relation_type,
        "weight": weight,
        "created_at": 1700000000i64,
        "created_by": "test-agent",
        "source": "runtime",
        "bootstrap_only": 0,
        "metadata": null
    })
    .to_string()
}

/// An all-tables restore dump (entries w/ full columns, entry_tags, co_access,
/// feature_entries, graph_edges, audit_log, counters). Confidence carries a
/// bit-exact f64 sentinel for type-fidelity assertions.
fn all_tables_dump(sv: i64) -> Vec<String> {
    let mut lines = vec![
        make_header(sv, 2, 3),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 4),
        make_counter_line("next_audit_id", 3),
    ];
    for id in 1i64..=3 {
        // make_entry_line emits confidence 0.5; craft a full entry for id 1 with a
        // bit-exact confidence + JSON-in-TEXT to exercise type fidelity.
        if id == 1 {
            let title = "Entry 1";
            let content = "Content for entry 1";
            let hash = compute_content_hash(title, content);
            lines.push(
                serde_json::json!({
                    "_table": "entries", "id": 1, "title": title, "content": content,
                    "topic": "testing", "category": "pattern", "source": "seed",
                    "status": 0, "confidence": 0.87654321f64,
                    "created_at": 1700000000i64, "updated_at": 1700000001i64,
                    "last_accessed_at": 0, "access_count": 0,
                    "supersedes": null, "superseded_by": null, "correction_count": 0,
                    "embedding_dim": 384, "created_by": "agent", "modified_by": "agent",
                    "content_hash": hash, "previous_hash": "",
                    "version": 1, "feature_cycle": "", "trust_source": "direct",
                    "helpful_count": 0, "unhelpful_count": 0, "pre_quarantine_status": null
                })
                .to_string(),
            );
        } else {
            lines.push(make_entry_line(
                id,
                &format!("Entry {id}"),
                &format!("Content for entry {id}"),
                "",
            ));
        }
    }
    lines.push(serde_json::json!({"_table":"entry_tags","entry_id":1,"tag":"rust"}).to_string());
    lines.push(serde_json::json!({"_table":"entry_tags","entry_id":2,"tag":"testing"}).to_string());
    lines.push(
        serde_json::json!({"_table":"co_access","entry_id_a":1,"entry_id_b":2,"count":5,"last_updated":1700000000i64}).to_string(),
    );
    lines.push(
        serde_json::json!({"_table":"feature_entries","feature_id":"vnc-048","entry_id":1})
            .to_string(),
    );
    lines.push(make_graph_edge_line(1, 2, "Supports", 0.85));
    lines.push(make_audit_line(1, "context_store", "seed 1"));
    lines.push(make_audit_line(2, "context_search", "seed 2"));
    lines.push(make_audit_line(3, "context_store", "seed 3"));
    lines
}

/// Full `ProjectPaths` for the isolated (project_dir, base_dir) pair.
fn paths_for(project_dir: &Path, base_dir: &Path) -> project::ProjectPaths {
    project::ensure_data_directory(Some(project_dir), Some(base_dir)).unwrap()
}

// ── R-11 / AC-13 — live-PID hard-error (live-only predicate) ────────────────

/// Copy `sleep` to a file named `unimatrix` and spawn it, so
/// `/proc/<pid>/cmdline` argv[0] has `file_name() == "unimatrix"` — the exact
/// shape `is_unimatrix_process` matches. Returns the pid + a kill-on-drop guard.
#[cfg(target_os = "linux")]
struct DaemonGuard(std::process::Child);
#[cfg(target_os = "linux")]
impl Drop for DaemonGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(target_os = "linux")]
fn spawn_fake_daemon(tmp: &Path) -> (u32, DaemonGuard) {
    use std::os::unix::fs::PermissionsExt;
    let sleep = ["/bin/sleep", "/usr/bin/sleep"]
        .iter()
        .find(|p| Path::new(p).exists())
        .expect("a sleep binary must exist");
    let fake = tmp.join("unimatrix");
    std::fs::copy(sleep, &fake).expect("copy sleep -> unimatrix");
    let mut perm = std::fs::metadata(&fake).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&fake, perm).unwrap();
    let child = std::process::Command::new(&fake)
        .arg("120")
        .spawn()
        .expect("spawn fake unimatrix daemon");
    (child.id(), DaemonGuard(child))
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread")]
async fn test_import_slug_live_pid_hard_errors_no_vector_write() {
    let project_dir = TempDir::new().unwrap();
    let base_dir = TempDir::new().unwrap();
    let daemon_tmp = TempDir::new().unwrap();
    let paths = paths_for(project_dir.path(), base_dir.path());
    let sv = {
        let s = open_store(&paths.db_path);
        let v = get_schema_version(&s).await;
        s.close().await.unwrap();
        // Remove the incidental path-hash db so its presence can't mask the slug write.
        let _ = std::fs::remove_file(&paths.db_path);
        v
    };

    let slug_dir = seed_slug_store(base_dir.path(), "live-slug").await;

    // Fabricate a LIVE daemon PID at the base-scoped pid_path and prove the
    // fixture satisfies BOTH predicates the gate uses.
    let (pid, _guard) = spawn_fake_daemon(daemon_tmp.path());
    assert!(unimatrix_server::infra::pidfile::is_process_alive(pid));
    assert!(unimatrix_server::infra::pidfile::is_unimatrix_process(pid));
    std::fs::write(&paths.pid_path, format!("{pid}\n")).unwrap();

    let tmp = TempDir::new().unwrap();
    let input = write_jsonl(&tmp, &all_tables_dump(sv));

    let result = run_import_with_base(
        Some(project_dir.path()),
        &input,
        Some("live-slug"),
        false,
        false,
        base_dir.path(),
    );
    assert!(result.is_err(), "live-PID must hard-error");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("live unimatrix daemon"),
        "must name a live daemon: {err}"
    );
    assert!(
        err.contains(&paths.pid_path.display().to_string()),
        "must name the resolved PID path: {err}"
    );
    assert!(
        err.contains("stop"),
        "must name the stop→import→start remedy: {err}"
    );

    // Refused before any write: the clobber-path {slug}/vector is never entered.
    assert_eq!(
        file_count(&slug_dir.join(SLUG_VECTOR_DIR)),
        0,
        "no vector index may be written when the live-PID gate refuses"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_import_slug_stale_pid_does_not_block() {
    let project_dir = TempDir::new().unwrap();
    let base_dir = TempDir::new().unwrap();
    let paths = paths_for(project_dir.path(), base_dir.path());
    let sv = {
        let s = open_store(&paths.db_path);
        let v = get_schema_version(&s).await;
        s.close().await.unwrap();
        v
    };
    seed_slug_store(base_dir.path(), "stale-slug").await;

    // Dead PID (predicate is live-PID-only) must NOT block.
    std::fs::write(&paths.pid_path, "4000000\n").unwrap();

    let tmp = TempDir::new().unwrap();
    let input = write_jsonl(&tmp, &all_tables_dump(sv));

    let result = run_import_with_base(
        Some(project_dir.path()),
        &input,
        Some("stale-slug"),
        false,
        false,
        base_dir.path(),
    );
    // The gate must not fire; import proceeds (embedding may Err if the model is
    // absent, but never the live-daemon refusal).
    if let Err(e) = &result {
        let msg = e.to_string();
        assert!(
            !msg.contains("live unimatrix daemon"),
            "stale PID must not trip the live-PID gate: {msg}"
        );
    }
}

// ── R-07 — non-empty-audit pre-flight refusal (C-5, AC-10/FR-13) ────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_import_slug_nonempty_audit_refuses_preflight() {
    let project_dir = TempDir::new().unwrap();
    let base_dir = TempDir::new().unwrap();
    let paths = paths_for(project_dir.path(), base_dir.path());
    let sv = {
        let s = open_store(&paths.db_path);
        let v = get_schema_version(&s).await;
        s.close().await.unwrap();
        v
    };

    // Seed a slug then dirty its audit_log with a row (append-only; INSERT ok).
    let slug_dir = seed_slug_store(base_dir.path(), "dirty-slug").await;
    let db = slug_dir.join(SLUG_DB_NAME);
    {
        let s = SqlxStore::open(&db, PoolConfig::default()).await.unwrap();
        sqlx::query(
            "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
             operation, target_ids, outcome, detail)
             VALUES (1, 1700000000, 'prior', 'system', 'store', '[]', 0, 'prior')",
        )
        .execute(s.write_pool_server())
        .await
        .unwrap();
        s.close().await.unwrap();
    }

    let tmp = TempDir::new().unwrap();
    let input = write_jsonl(&tmp, &all_tables_dump(sv));

    // Without --force.
    let result = run_import_with_base(
        Some(project_dir.path()),
        &input,
        Some("dirty-slug"),
        false,
        false,
        base_dir.path(),
    );
    assert!(result.is_err(), "non-empty audit_log must refuse");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("audit rows"),
        "actionable audit message: {err}"
    );
    assert!(
        err.contains("register"),
        "must direct to register a fresh slug: {err}"
    );
    assert!(
        err.contains(&db.display().to_string()),
        "must name the resolved slug db path: {err}"
    );
    assert!(
        !err.to_uppercase().contains("UNIQUE"),
        "must NEVER surface the raw SQLite UNIQUE error: {err}"
    );
    // Refused before write: no vector index written.
    assert_eq!(file_count(&slug_dir.join(SLUG_VECTOR_DIR)), 0);

    // --force must NOT bypass the audit refusal (no such override exists).
    let forced = run_import_with_base(
        Some(project_dir.path()),
        &input,
        Some("dirty-slug"),
        false,
        true,
        base_dir.path(),
    );
    assert!(forced.is_err(), "--force must not bypass the audit refusal");
    assert!(
        forced.unwrap_err().to_string().contains("audit rows"),
        "force path still hits the audit gate"
    );
}

// ── R-02 / R-14 — missing store fails loud, creates nothing (AC-03) ─────────

#[tokio::test(flavor = "multi_thread")]
async fn test_import_slug_missing_store_fails_loud_fs_unchanged() {
    let project_dir = TempDir::new().unwrap();
    let base_dir = TempDir::new().unwrap();
    let paths = paths_for(project_dir.path(), base_dir.path());
    let sv = {
        let s = open_store(&paths.db_path);
        let v = get_schema_version(&s).await;
        s.close().await.unwrap();
        v
    };
    // No slug store seeded for "ghost".
    let ghost_dir = base_dir.path().join("ghost");
    let expected_db = ghost_dir.join(SLUG_DB_NAME);

    let tmp = TempDir::new().unwrap();
    let input = write_jsonl(&tmp, &all_tables_dump(sv));

    let result = run_import_with_base(
        Some(project_dir.path()),
        &input,
        Some("ghost"),
        false,
        false,
        base_dir.path(),
    );
    assert!(result.is_err(), "missing slug store must fail loud");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains(&expected_db.display().to_string()),
        "must name the fully-resolved absolute db path: {err}"
    );
    // Existence gate (a pure read) created nothing under the slug dir.
    assert!(
        !ghost_dir.exists(),
        "existence gate must not create the slug dir"
    );
}

// ── R-08 — validation at the CLI edge (AC-04) ───────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_import_slug_invalid_rejected_no_fs_touch() {
    let project_dir = TempDir::new().unwrap();
    let base_dir = TempDir::new().unwrap();
    let paths = paths_for(project_dir.path(), base_dir.path());
    let sv = {
        let s = open_store(&paths.db_path);
        let v = get_schema_version(&s).await;
        s.close().await.unwrap();
        v
    };
    let tmp = TempDir::new().unwrap();
    let input = write_jsonl(&tmp, &all_tables_dump(sv));

    for bad in ["UPPER", "tools", "../escape"] {
        let result = run_import_with_base(
            Some(project_dir.path()),
            &input,
            Some(bad),
            false,
            false,
            base_dir.path(),
        );
        assert!(result.is_err(), "invalid slug {bad:?} must be rejected");
    }
}

// ── AC-10 / AC-02 — all-tables restore into a fresh slug B; A untouched ─────

#[tokio::test(flavor = "multi_thread")]
async fn test_import_slug_all_tables_into_fresh_slug_b_vector_redirect() {
    let project_dir = TempDir::new().unwrap();
    let base_dir = TempDir::new().unwrap();
    let paths = paths_for(project_dir.path(), base_dir.path());
    let sv = {
        let s = open_store(&paths.db_path);
        let v = get_schema_version(&s).await;
        s.close().await.unwrap();
        // Remove the incidental path-hash db so we can assert the slug is the target.
        let _ = std::fs::remove_file(&paths.db_path);
        v
    };

    // Slug A seeded with a sentinel entry (id 999) — the resolver must NOT read it.
    let a_dir = seed_slug_store(base_dir.path(), "aslug").await;
    {
        let a = SqlxStore::open(&a_dir.join(SLUG_DB_NAME), PoolConfig::default())
            .await
            .unwrap();
        insert_full_entry(a.write_pool_server(), 999).await;
        a.close().await.unwrap();
    }

    // Slug B: fresh, audit-empty restore target.
    let b_dir = seed_slug_store(base_dir.path(), "bslug").await;

    let tmp = TempDir::new().unwrap();
    let input = write_jsonl(&tmp, &all_tables_dump(sv));

    // Hash validation ON (skip=false) — Ok proves content_hash + chain_verify clean.
    run_import_with_base(
        Some(project_dir.path()),
        &input,
        Some("bslug"),
        false,
        false,
        base_dir.path(),
    )
    .expect("AC-10: restore into fresh slug B must succeed");

    // All tables restored into B's db.
    let b = open_store(&b_dir.join(SLUG_DB_NAME));
    let pool = b.write_pool_server();
    let entries: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entries")
        .fetch_one(pool)
        .await
        .unwrap();
    let tags: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entry_tags")
        .fetch_one(pool)
        .await
        .unwrap();
    let co: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM co_access")
        .fetch_one(pool)
        .await
        .unwrap();
    let fe: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM feature_entries")
        .fetch_one(pool)
        .await
        .unwrap();
    let ge: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(pool)
        .await
        .unwrap();
    let audit: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(entries, 3, "entries restored into B");
    assert_eq!(tags, 2, "entry_tags restored");
    assert_eq!(co, 1, "co_access restored");
    assert_eq!(fe, 1, "feature_entries restored");
    assert_eq!(ge, 1, "graph_edges restored");
    assert_eq!(audit, 3 + 1, "3 imported audit rows + 1 import provenance");

    // Type fidelity: f64 confidence bit-exact through the round-trip.
    let conf: f64 = sqlx::query_scalar("SELECT confidence FROM entries WHERE id = 1")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(
        conf.to_bits(),
        0.87654321f64.to_bits(),
        "confidence bit-exact"
    );
    b.close().await.unwrap();

    // Resolver targeted B, NOT A: A still holds only its sentinel entry (id 999).
    let a = open_store(&a_dir.join(SLUG_DB_NAME));
    let a_entries: Vec<i64> = sqlx::query_scalar("SELECT id FROM entries ORDER BY id")
        .fetch_all(a.write_pool_server())
        .await
        .unwrap();
    assert_eq!(
        a_entries,
        vec![999i64],
        "slug A must be untouched by an import into B"
    );
    a.close().await.unwrap();

    // AC-02 vector redirect: fresh HNSW under {base}/bslug/vector; nothing under
    // the path-hash vector/.
    assert!(
        file_count(&b_dir.join(SLUG_VECTOR_DIR)) > 0,
        "a fresh HNSW index must be written under the slug's vector dir"
    );
    assert_eq!(
        file_count(&paths.vector_dir),
        0,
        "nothing may be written to the path-hash vector dir in slug mode"
    );
}

// ── R-03 S2 / AC-12 — served vector search from `start` (gate non-negotiable) ─

/// Insert one semantically-distinct entry (valid content_hash, unchained
/// `previous_hash = ""`) so the reconstructed HNSW is searchable by meaning.
async fn insert_semantic_entry(pool: &sqlx::SqlitePool, id: i64, title: &str, content: &str) {
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
            ?1, ?2, ?3, 'testing', 'pattern', 'seed',
            1, 0.5,
            1700000000, 1700000001, 0, 0,
            NULL, NULL, 0, 384,
            'agent', 'agent', ?4, '',
            1, '', 'direct',
            0, 0, NULL
        )",
    )
    .bind(id)
    .bind(title)
    .bind(content)
    .bind(&hash)
    .execute(pool)
    .await
    .unwrap();
}

/// R-03 S2 / AC-12 — the gate non-negotiable. Drives the assembled restore
/// sequence (register A → seed corpus → export --slug A → register B →
/// import --slug B), then simulates `start` by loading the POST-IMPORT
/// `{slug}/vector` through the SAME boot-time per-slug vector-load path the
/// daemon runs (`build_project_server`, http_provision.rs:186-224:
/// `SqlxStore::open` the slug db, probe `unimatrix-vector.meta`, then
/// `VectorIndex::load(store, VectorConfig::default(), &vector_dir)`), and
/// issues a SERVED vector query through that freshly-loaded index.
///
/// This proves the OUTCOME from `start`, not disk state: no in-memory index
/// built before import is reused, and no `file_count`/presence proxy is
/// asserted — the rebuilt index must be the one that actually serves, so the
/// two-resolver / stale-index failure modes (SR-10) could surface here.
#[tokio::test(flavor = "multi_thread")]
async fn test_restore_sequence_serves_vector_search_from_start() {
    let project_dir = TempDir::new().unwrap();
    let base_dir = TempDir::new().unwrap();
    let paths = paths_for(project_dir.path(), base_dir.path());

    // 1. register slug A + seed a semantically-distinct source corpus.
    let a_dir = seed_slug_store(base_dir.path(), "aslug").await;
    {
        let a = SqlxStore::open(&a_dir.join(SLUG_DB_NAME), PoolConfig::default())
            .await
            .unwrap();
        let pool = a.write_pool_server();
        insert_semantic_entry(
            pool,
            1,
            "Tokio asynchronous runtime for Rust",
            "Tokio provides an asynchronous runtime with async I/O, futures, and task \
             scheduling for concurrent Rust programs.",
        )
        .await;
        insert_semantic_entry(
            pool,
            2,
            "Sourdough bread baking",
            "Baking sourdough bread needs a fermented starter of flour and water left \
             to rise overnight before the loaf goes in the oven.",
        )
        .await;
        insert_semantic_entry(
            pool,
            3,
            "Alpine mountain hiking trails",
            "Hiking alpine mountain trails means steep elevation gain, rocky switchbacks, \
             and thin air at high altitude.",
        )
        .await;
        sqlx::query("INSERT OR REPLACE INTO counters (name, value) VALUES ('next_entry_id', 4)")
            .execute(pool)
            .await
            .unwrap();
        a.close().await.unwrap();
    }

    // 2. export --slug A → dump (the operator's real export invocation).
    let dump = base_dir.path().join("aslug-export.jsonl");
    run_export_with_base(
        Some(project_dir.path()),
        Some(&dump),
        base_dir.path(),
        Some("aslug"),
        false,
        false,
    )
    .expect("export --slug A must succeed");

    // 3. register a FRESH slug B, then import --slug B (the register → stop →
    //    import path; the live-PID gate is clear because no daemon is up).
    let b_dir = seed_slug_store(base_dir.path(), "bslug").await;
    run_import_with_base(
        Some(project_dir.path()),
        &dump,
        Some("bslug"),
        false, // hash validation ON — faithful restore
        false,
        base_dir.path(),
    )
    .expect("import --slug B must succeed");

    // Guard: the import wrote the rebuilt HNSW into B's slug vector dir, not the
    // path-hash dir (a precondition for the boot path to load the RIGHT index).
    let b_vector_dir = b_dir.join(SLUG_VECTOR_DIR);
    assert_eq!(
        file_count(&paths.vector_dir),
        0,
        "slug-mode import must not touch the path-hash vector dir"
    );

    // 4. Simulate `start` FAITHFULLY — reuse the daemon's boot-time per-slug
    //    vector-load path (build_project_server, http_provision.rs:186-224)
    //    against the POST-IMPORT on-disk state. Open the slug store the boot
    //    way, then load the index the boot way (meta probe → VectorIndex::load).
    //    No pre-import in-memory index exists to reuse.
    let b_db = b_dir.join(SLUG_DB_NAME);
    let boot_store = Arc::new(
        SqlxStore::open(&b_db, PoolConfig::default())
            .await
            .expect("boot: open the restored slug store"),
    );
    let meta_path = b_vector_dir.join(SLUG_VECTOR_META);
    assert!(
        meta_path.exists(),
        "boot precondition: import must have dumped a vector index the daemon loads at start"
    );
    let boot_index = VectorIndex::load(
        Arc::clone(&boot_store),
        VectorConfig::default(),
        &b_vector_dir,
    )
    .await
    .expect("boot: load the rebuilt per-slug index the SAME way build_project_server does");

    // The rebuilt index actually carries the restored corpus (not an empty
    // fallback) — the load-then-serve path is live, not a stat.
    assert_eq!(
        boot_index.point_count(),
        3,
        "the boot-loaded index must hold all 3 restored entries"
    );

    // 5. Issue a SERVED vector search through the freshly-loaded index. Embed
    //    the query with the SAME ONNX model the reconstruct path used, so the
    //    query lands in the index's embedding space.
    let provider = OnnxProvider::new(EmbedConfig::default())
        .expect("initialize the ONNX embedding model for the served query");
    let query = embed_entry(
        &provider,
        "asynchronous runtime and concurrency in Rust with tokio",
        "",
        ": ",
    )
    .expect("embed the served query");

    let results = boot_index
        .search(&query, 3, 32)
        .expect("served vector search must run against the boot-loaded index");

    assert!(
        !results.is_empty(),
        "served vector search from `start` must return the restored corpus's hits"
    );
    assert_eq!(
        results[0].entry_id, 1,
        "the top served hit for an async-Rust query must be the restored async-runtime \
         entry (id 1) — proving the daemon serves the REBUILT index, not a stale/empty one; \
         got {results:?}"
    );
}

// ── R-09 — no-`--slug` fallthrough parity (AC-05) ───────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_import_no_slug_writes_to_path_hash_data_dir() {
    let project_dir = TempDir::new().unwrap();
    let base_dir = TempDir::new().unwrap();
    let paths = paths_for(project_dir.path(), base_dir.path());
    let sv = {
        let s = open_store(&paths.db_path);
        let v = get_schema_version(&s).await;
        s.close().await.unwrap();
        v
    };

    let tmp = TempDir::new().unwrap();
    let lines = vec![
        make_header(sv, 1, 1),
        make_counter_line("schema_version", sv),
        make_counter_line("next_entry_id", 2),
        make_entry_line(1, "NoSlug", "path-hash target", ""),
    ];
    let input = write_jsonl(&tmp, &lines);

    run_import_with_base(
        Some(project_dir.path()),
        &input,
        None,
        false,
        false,
        base_dir.path(),
    )
    .expect("no-slug import must succeed into the path-hash data dir");

    // The row landed in the path-hash db (unchanged behavior, AC-05).
    let s = open_store(&paths.db_path);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entries")
        .fetch_one(s.write_pool_server())
        .await
        .unwrap();
    assert_eq!(count, 1);
}
