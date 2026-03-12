//! Integration tests for the knowledge export module (nan-001).
//!
//! These tests exercise `run_export` end-to-end: real database, real file I/O,
//! real project directory resolution. They verify acceptance criteria AC-01
//! through AC-18 and cover risks R-01 through R-15 from RISK-TEST-STRATEGY.md.

use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;
use tempfile::TempDir;
use unimatrix_server::export::run_export;
use unimatrix_server::project;
use unimatrix_store::Store;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Set up a project directory and return (project_dir, db_path).
///
/// Uses `base_dir=None` so the database is resolved to `~/.unimatrix/{hash}/`,
/// matching what `run_export` does internally. Each temp dir has a unique path
/// hash so tests do not collide.
fn setup_project() -> (TempDir, std::path::PathBuf) {
    let project_dir = TempDir::new().expect("create project temp dir");
    let paths =
        project::ensure_data_directory(Some(project_dir.path()), None).unwrap();
    (project_dir, paths.db_path)
}

/// Run export to a buffer by writing to a file then reading it back.
/// Returns the raw output string.
fn run_export_to_string(project_dir: &Path, output_file: &Path) -> String {
    run_export(Some(project_dir), Some(output_file)).expect("run_export should succeed");
    std::fs::read_to_string(output_file).expect("read output file")
}

/// Parse all lines from export output.
fn parse_lines(output: &str) -> Vec<Value> {
    output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("invalid JSON: {e}: {l}")))
        .collect()
}

/// Insert a representative entry with all 26 columns filled.
fn insert_full_entry(conn: &rusqlite::Connection, id: i64) {
    conn.execute(
        "INSERT INTO entries (
            id, title, content, topic, category, source, status, confidence,
            created_at, updated_at, last_accessed_at, access_count,
            supersedes, superseded_by, correction_count, embedding_dim,
            created_by, modified_by, content_hash, previous_hash,
            version, feature_cycle, trust_source,
            helpful_count, unhelpful_count, pre_quarantine_status
        ) VALUES (
            ?1, 'Entry ' || ?1, 'Content for entry ' || ?1, 'testing', 'pattern', 'integration-test',
            1, 0.87654321,
            1700000000, 1700000001, 1700000002, 15,
            NULL, NULL, 3, 384,
            'agent-x', 'agent-y', 'hash_' || ?1, 'prev_' || ?1,
            7, 'nan-001', 'human',
            12, 2, NULL
        )",
        rusqlite::params![id],
    )
    .unwrap();
}

/// Populate a database with representative data across all 8 tables.
fn populate_representative_data(conn: &rusqlite::Connection) {
    // 3 entries
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
    ).unwrap();
    conn.execute(
        "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (2, 3, 3, 1700000001)",
        [],
    ).unwrap();

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
    ).unwrap();
    conn.execute(
        "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
         allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
         VALUES ('bot-2', 1, '[]', NULL, NULL, 1700000002, 1700000003, 1)",
        [],
    ).unwrap();

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
}

use unimatrix_store::rusqlite;

// ---------------------------------------------------------------------------
// T-EM-11 / AC-17: Full export with representative data across all 8 tables
// ---------------------------------------------------------------------------
#[test]
fn test_full_export_representative_data() {
    let (project_dir, db_path) = setup_project();
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        populate_representative_data(&conn);
    }
    drop(store);

    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().join("export.jsonl");
    let output = run_export_to_string(project_dir.path(), &output_path);
    let lines = parse_lines(&output);

    // Header present
    assert!(lines[0]["_header"].as_bool().unwrap());

    // Collect table groups
    let data_lines: Vec<&Value> = lines.iter().skip(1).collect();
    let mut table_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for line in &data_lines {
        let table = line["_table"].as_str().unwrap().to_string();
        *table_counts.entry(table).or_insert(0) += 1;
    }

    // All 8 tables present
    let expected_tables: HashSet<&str> = [
        "counters",
        "entries",
        "entry_tags",
        "co_access",
        "feature_entries",
        "outcome_index",
        "agent_registry",
        "audit_log",
    ]
    .iter()
    .copied()
    .collect();
    let actual_tables: HashSet<&str> = table_counts.keys().map(|s| s.as_str()).collect();
    assert_eq!(actual_tables, expected_tables, "All 8 tables must be present");

    // Verify row counts
    assert_eq!(table_counts["entries"], 3);
    assert_eq!(table_counts["entry_tags"], 4);
    assert_eq!(table_counts["co_access"], 2);
    assert_eq!(table_counts["feature_entries"], 2);
    assert_eq!(table_counts["outcome_index"], 2);
    assert_eq!(table_counts["agent_registry"], 2);
    assert_eq!(table_counts["audit_log"], 3);
    assert!(table_counts["counters"] >= 1, "At least schema_version counter");
}

// ---------------------------------------------------------------------------
// T-EM-08 / AC-10: Empty database export
// ---------------------------------------------------------------------------
#[test]
fn test_empty_database_export() {
    let (project_dir, db_path) = setup_project();
    // Just opening the store creates the schema
    let _store = Store::open(&db_path).unwrap();
    drop(_store);

    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().join("export.jsonl");
    let output = run_export_to_string(project_dir.path(), &output_path);
    let lines = parse_lines(&output);

    // Header present with entry_count: 0
    assert!(lines[0]["_header"].as_bool().unwrap());
    assert_eq!(lines[0]["entry_count"].as_i64().unwrap(), 0);

    // Counter rows present, no data rows for non-counter tables
    let data_lines: Vec<&Value> = lines.iter().skip(1).collect();
    for line in &data_lines {
        assert_eq!(
            line["_table"].as_str().unwrap(),
            "counters",
            "Only counters should be present in empty DB export"
        );
    }

    // Every line is valid JSON (already verified by parse_lines)
    assert!(data_lines.len() >= 1, "At least schema_version counter");
}

// ---------------------------------------------------------------------------
// T-EM-03 / AC-14: Determinism -- two exports produce identical output
// ---------------------------------------------------------------------------
#[test]
fn test_deterministic_output() {
    let (project_dir, db_path) = setup_project();
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        populate_representative_data(&conn);
    }
    drop(store);

    let output_dir = TempDir::new().unwrap();

    // Run export 3 times
    let mut outputs: Vec<String> = Vec::new();
    for i in 0..3 {
        let output_path = output_dir.path().join(format!("export_{i}.jsonl"));
        let output = run_export_to_string(project_dir.path(), &output_path);
        outputs.push(output);
    }

    // Normalize exported_at (replace the timestamp with a fixed value for comparison)
    let normalize = |s: &str| -> String {
        let mut result = String::new();
        for line in s.lines() {
            if line.contains("\"_header\"") {
                let mut val: Value = serde_json::from_str(line).unwrap();
                val.as_object_mut()
                    .unwrap()
                    .insert("exported_at".into(), Value::Number(0.into()));
                result.push_str(&serde_json::to_string(&val).unwrap());
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }
        result
    };

    let normalized: Vec<String> = outputs.iter().map(|o| normalize(o)).collect();
    assert_eq!(
        normalized[0], normalized[1],
        "First and second exports must be byte-identical (excluding exported_at)"
    );
    assert_eq!(
        normalized[1], normalized[2],
        "Second and third exports must be byte-identical (excluding exported_at)"
    );
}

// ---------------------------------------------------------------------------
// T-EM-04 / AC-18: Excluded tables not present in output
// ---------------------------------------------------------------------------
#[test]
fn test_excluded_tables_not_present() {
    let (project_dir, db_path) = setup_project();
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        insert_full_entry(&conn, 1);
        // Try inserting into excluded tables that exist in the schema.
        // These may or may not exist depending on schema version, so we
        // silently ignore errors from non-existent tables.
        let excluded_tables_inserts = [
            "INSERT OR IGNORE INTO sessions (session_id, agent_id, started_at) VALUES ('s1', 'a1', 1)",
            "INSERT OR IGNORE INTO observations (id, session_id, tool_name, timestamp) VALUES (1, 's1', 'test', 1)",
            "INSERT OR IGNORE INTO query_log (id, session_id, query, timestamp) VALUES (1, 's1', 'test', 1)",
        ];
        for sql in &excluded_tables_inserts {
            let _ = conn.execute(sql, []);
        }
    }
    drop(store);

    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().join("export.jsonl");
    let output = run_export_to_string(project_dir.path(), &output_path);
    let lines = parse_lines(&output);

    let allowed: HashSet<&str> = [
        "counters",
        "entries",
        "entry_tags",
        "co_access",
        "feature_entries",
        "outcome_index",
        "agent_registry",
        "audit_log",
    ]
    .iter()
    .copied()
    .collect();

    for line in lines.iter().skip(1) {
        let table = line["_table"].as_str().unwrap();
        assert!(
            allowed.contains(table),
            "Excluded table '{table}' found in export output"
        );
    }
}

// ---------------------------------------------------------------------------
// T-EM-12 / AC-08: Table emission order
// ---------------------------------------------------------------------------
#[test]
fn test_table_emission_order() {
    let (project_dir, db_path) = setup_project();
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        populate_representative_data(&conn);
    }
    drop(store);

    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().join("export.jsonl");
    let output = run_export_to_string(project_dir.path(), &output_path);
    let lines = parse_lines(&output);

    // Collect unique _table values in order of first appearance
    let mut seen_order: Vec<String> = Vec::new();
    let mut seen_set: HashSet<String> = HashSet::new();
    for line in lines.iter().skip(1) {
        let table = line["_table"].as_str().unwrap().to_string();
        if seen_set.insert(table.clone()) {
            seen_order.push(table);
        }
    }

    let expected_order = vec![
        "counters",
        "entries",
        "entry_tags",
        "co_access",
        "feature_entries",
        "outcome_index",
        "agent_registry",
        "audit_log",
    ];

    assert_eq!(
        seen_order, expected_order,
        "Tables must appear in dependency order"
    );
}

// ---------------------------------------------------------------------------
// T-EM-05 / AC-07: Row ordering within tables
// ---------------------------------------------------------------------------
#[test]
fn test_row_ordering_within_tables() {
    let (project_dir, db_path) = setup_project();
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        // Insert entries out of order
        for id in [5, 2, 8, 1] {
            insert_full_entry(&conn, id);
        }

        // Insert tags out of order
        conn.execute(
            "INSERT INTO entry_tags (entry_id, tag) VALUES (1, 'zebra')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entry_tags (entry_id, tag) VALUES (1, 'apple')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entry_tags (entry_id, tag) VALUES (2, 'mango')",
            [],
        )
        .unwrap();

        // Insert co_access out of order
        conn.execute(
            "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (3, 5, 1, 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (1, 2, 1, 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (2, 4, 1, 1)",
            [],
        ).unwrap();
    }
    drop(store);

    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().join("export.jsonl");
    let output = run_export_to_string(project_dir.path(), &output_path);
    let lines = parse_lines(&output);

    // Entries ordered by id
    let entry_ids: Vec<i64> = lines
        .iter()
        .filter(|l| l.get("_table").and_then(|t| t.as_str()) == Some("entries"))
        .map(|l| l["id"].as_i64().unwrap())
        .collect();
    assert_eq!(entry_ids, vec![1, 2, 5, 8], "Entries must be ordered by id ASC");

    // Entry tags ordered by (entry_id, tag)
    let tag_pairs: Vec<(i64, String)> = lines
        .iter()
        .filter(|l| l.get("_table").and_then(|t| t.as_str()) == Some("entry_tags"))
        .map(|l| {
            (
                l["entry_id"].as_i64().unwrap(),
                l["tag"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        tag_pairs,
        vec![
            (1, "apple".to_string()),
            (1, "zebra".to_string()),
            (2, "mango".to_string()),
        ],
        "Tags must be ordered by (entry_id, tag)"
    );

    // Co-access ordered by (entry_id_a, entry_id_b)
    let co_pairs: Vec<(i64, i64)> = lines
        .iter()
        .filter(|l| l.get("_table").and_then(|t| t.as_str()) == Some("co_access"))
        .map(|l| {
            (
                l["entry_id_a"].as_i64().unwrap(),
                l["entry_id_b"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        co_pairs,
        vec![(1, 2), (2, 4), (3, 5)],
        "Co-access must be ordered by (entry_id_a, entry_id_b)"
    );
}

// ---------------------------------------------------------------------------
// T-CL-02 / AC-02: --output file path
// ---------------------------------------------------------------------------
#[test]
fn test_output_file_path() {
    let (project_dir, db_path) = setup_project();
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        insert_full_entry(&conn, 1);
    }
    drop(store);

    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().join("export.jsonl");
    assert!(!output_path.exists(), "Output file should not exist yet");

    run_export(Some(project_dir.path()), Some(&output_path)).expect("export should succeed");

    assert!(output_path.exists(), "Output file should have been created");
    let content = std::fs::read_to_string(&output_path).unwrap();
    assert!(!content.is_empty(), "Output file should not be empty");

    let lines = parse_lines(&content);
    assert!(lines[0]["_header"].as_bool().unwrap());
}

// ---------------------------------------------------------------------------
// T-EM-09 / AC-03: Header validation
// ---------------------------------------------------------------------------
#[test]
fn test_header_validation() {
    let (project_dir, db_path) = setup_project();
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        for id in 1..=3 {
            insert_full_entry(&conn, id);
        }
    }
    drop(store);

    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().join("export.jsonl");
    let output = run_export_to_string(project_dir.path(), &output_path);
    let lines = parse_lines(&output);
    let header = &lines[0];
    let obj = header.as_object().unwrap();

    assert_eq!(obj["_header"], Value::Bool(true));
    assert!(obj["schema_version"].as_i64().unwrap() > 0);
    // exported_at should be a recent timestamp (within 120 seconds of now)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let exported_at = obj["exported_at"].as_i64().unwrap();
    assert!(
        (now - exported_at).abs() < 120,
        "exported_at should be recent, got {exported_at} vs now {now}"
    );
    assert_eq!(obj["entry_count"].as_i64().unwrap(), 3);
    assert_eq!(obj["format_version"].as_i64().unwrap(), 1);
    assert_eq!(obj.len(), 5, "Header should have exactly 5 keys");
}

// ---------------------------------------------------------------------------
// T-RS-01 / AC-06: Entries with all 26 columns including confidence/learned signals
// ---------------------------------------------------------------------------
#[test]
fn test_entries_all_26_columns() {
    let (project_dir, db_path) = setup_project();
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at, last_accessed_at, access_count,
                supersedes, superseded_by, correction_count, embedding_dim,
                created_by, modified_by, content_hash, previous_hash,
                version, feature_cycle, trust_source,
                helpful_count, unhelpful_count, pre_quarantine_status
            ) VALUES (
                42, 'Test Entry', 'Content here', 'testing', 'pattern', 'integration-test',
                1, 0.87654321,
                1700000000, 1700000001, 1700000002, 15,
                10, 50, 3, 384,
                'agent-x', 'agent-y', 'abc123', 'def456',
                7, 'crt-002', 'human',
                12, 2, 0
            )",
            [],
        )
        .unwrap();
    }
    drop(store);

    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().join("export.jsonl");
    let output = run_export_to_string(project_dir.path(), &output_path);
    let lines = parse_lines(&output);

    let entry_row = lines
        .iter()
        .find(|l| l.get("_table").and_then(|t| t.as_str()) == Some("entries"))
        .expect("Should have an entries row");

    let obj = entry_row.as_object().unwrap();
    assert_eq!(obj.len(), 27, "26 columns + _table");
    assert_eq!(obj["id"].as_i64().unwrap(), 42);
    assert_eq!(obj["title"].as_str().unwrap(), "Test Entry");
    assert_eq!(obj["content"].as_str().unwrap(), "Content here");
    assert_eq!(obj["topic"].as_str().unwrap(), "testing");
    assert_eq!(obj["category"].as_str().unwrap(), "pattern");
    assert_eq!(obj["source"].as_str().unwrap(), "integration-test");
    assert_eq!(obj["status"].as_i64().unwrap(), 1);
    // f64 precision check
    assert_eq!(obj["confidence"].as_f64().unwrap().to_bits(), 0.87654321_f64.to_bits());
    assert_eq!(obj["created_at"].as_i64().unwrap(), 1_700_000_000);
    assert_eq!(obj["updated_at"].as_i64().unwrap(), 1_700_000_001);
    assert_eq!(obj["last_accessed_at"].as_i64().unwrap(), 1_700_000_002);
    assert_eq!(obj["access_count"].as_i64().unwrap(), 15);
    assert_eq!(obj["supersedes"].as_i64().unwrap(), 10);
    assert_eq!(obj["superseded_by"].as_i64().unwrap(), 50);
    assert_eq!(obj["correction_count"].as_i64().unwrap(), 3);
    assert_eq!(obj["embedding_dim"].as_i64().unwrap(), 384);
    assert_eq!(obj["created_by"].as_str().unwrap(), "agent-x");
    assert_eq!(obj["modified_by"].as_str().unwrap(), "agent-y");
    assert_eq!(obj["content_hash"].as_str().unwrap(), "abc123");
    assert_eq!(obj["previous_hash"].as_str().unwrap(), "def456");
    assert_eq!(obj["version"].as_i64().unwrap(), 7);
    assert_eq!(obj["feature_cycle"].as_str().unwrap(), "crt-002");
    assert_eq!(obj["trust_source"].as_str().unwrap(), "human");
    assert_eq!(obj["helpful_count"].as_i64().unwrap(), 12);
    assert_eq!(obj["unhelpful_count"].as_i64().unwrap(), 2);
    assert_eq!(obj["pre_quarantine_status"].as_i64().unwrap(), 0);
}

// ---------------------------------------------------------------------------
// T-RS-06 / AC-09: Null handling for nullable columns
// ---------------------------------------------------------------------------
#[test]
fn test_null_handling_nullable_columns() {
    let (project_dir, db_path) = setup_project();
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        // Entry with all nullable fields NULL
        conn.execute(
            "INSERT INTO entries (
                id, title, content, topic, category, source, created_at, updated_at,
                supersedes, superseded_by, pre_quarantine_status
            ) VALUES (1, 'test', 'c', 't', 'p', 's', 1, 1, NULL, NULL, NULL)",
            [],
        )
        .unwrap();

        // Agent with nullable fields NULL
        conn.execute(
            "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
             allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
             VALUES ('bot-null', 0, '[]', NULL, NULL, 1, 1, 1)",
            [],
        )
        .unwrap();
    }
    drop(store);

    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().join("export.jsonl");
    let output = run_export_to_string(project_dir.path(), &output_path);
    let lines = parse_lines(&output);

    // Check entry nullable fields
    let entry_row = lines
        .iter()
        .find(|l| l.get("_table").and_then(|t| t.as_str()) == Some("entries"))
        .expect("entries row");
    let obj = entry_row.as_object().unwrap();
    assert!(obj.contains_key("supersedes"), "supersedes key must be present");
    assert!(obj["supersedes"].is_null(), "supersedes must be JSON null");
    assert!(obj.contains_key("superseded_by"), "superseded_by key must be present");
    assert!(obj["superseded_by"].is_null(), "superseded_by must be JSON null");
    assert!(
        obj.contains_key("pre_quarantine_status"),
        "pre_quarantine_status key must be present"
    );
    assert!(
        obj["pre_quarantine_status"].is_null(),
        "pre_quarantine_status must be JSON null"
    );
    // Key count still 27 (no keys omitted)
    assert_eq!(obj.len(), 27);

    // Check agent nullable fields
    let agent_row = lines
        .iter()
        .find(|l| l.get("_table").and_then(|t| t.as_str()) == Some("agent_registry"))
        .expect("agent_registry row");
    let aobj = agent_row.as_object().unwrap();
    assert!(aobj["allowed_topics"].is_null());
    assert!(aobj["allowed_categories"].is_null());
    assert_eq!(aobj.len(), 9);
}

// ---------------------------------------------------------------------------
// T-EM-10 / AC-04: Every non-header line has _table
// ---------------------------------------------------------------------------
#[test]
fn test_every_non_header_line_has_table() {
    let (project_dir, db_path) = setup_project();
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        populate_representative_data(&conn);
    }
    drop(store);

    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().join("export.jsonl");
    let output = run_export_to_string(project_dir.path(), &output_path);
    let lines = parse_lines(&output);

    let allowed_tables: HashSet<&str> = [
        "counters",
        "entries",
        "entry_tags",
        "co_access",
        "feature_entries",
        "outcome_index",
        "agent_registry",
        "audit_log",
    ]
    .iter()
    .copied()
    .collect();

    for (i, line) in lines.iter().enumerate().skip(1) {
        let table = line
            .get("_table")
            .unwrap_or_else(|| panic!("Line {i} missing _table key"));
        let table_str = table
            .as_str()
            .unwrap_or_else(|| panic!("Line {i} _table is not a string"));
        assert!(
            allowed_tables.contains(table_str),
            "Line {i} has unexpected _table: {table_str}"
        );
    }
}

// ---------------------------------------------------------------------------
// T-EM-11 / AC-05: All 8 table types present with correct row counts
// ---------------------------------------------------------------------------
#[test]
fn test_all_8_tables_with_row_counts() {
    let (project_dir, db_path) = setup_project();
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        populate_representative_data(&conn);
    }
    drop(store);

    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().join("export.jsonl");
    let output = run_export_to_string(project_dir.path(), &output_path);
    let lines = parse_lines(&output);

    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for line in lines.iter().skip(1) {
        let table = line["_table"].as_str().unwrap().to_string();
        *counts.entry(table).or_insert(0) += 1;
    }

    assert_eq!(counts.len(), 8, "Exactly 8 table types should be present");
    assert_eq!(counts["entries"], 3);
    assert_eq!(counts["entry_tags"], 4);
    assert_eq!(counts["co_access"], 2);
    assert_eq!(counts["feature_entries"], 2);
    assert_eq!(counts["outcome_index"], 2);
    assert_eq!(counts["agent_registry"], 2);
    assert_eq!(counts["audit_log"], 3);
}

// ---------------------------------------------------------------------------
// T-CL-05 / AC-15: Error on non-writable output path
// ---------------------------------------------------------------------------
#[test]
fn test_error_on_invalid_output_path() {
    let (project_dir, db_path) = setup_project();
    let _store = Store::open(&db_path).unwrap();
    drop(_store);

    let bad_path = std::path::Path::new("/nonexistent_dir_12345/export.jsonl");
    let result = run_export(Some(project_dir.path()), Some(bad_path));
    assert!(result.is_err(), "Export to non-writable path should fail");
}

// ---------------------------------------------------------------------------
// T-CL-05: Error on non-existent database
// ---------------------------------------------------------------------------
#[test]
fn test_error_on_nonexistent_database() {
    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().join("export.jsonl");

    // Pass a project_dir that cannot be canonicalized -- ensure_data_directory fails.
    let result = run_export(
        Some(std::path::Path::new("/nonexistent_path_xyz_12345")),
        Some(&output_path),
    );
    assert!(result.is_err(), "Export with non-canonicalizable project dir should fail");
}

// ---------------------------------------------------------------------------
// T-CL-03 / AC-13: --project-dir flag resolves to correct database
// ---------------------------------------------------------------------------
#[test]
fn test_project_dir_isolation() {
    // Create two separate project dirs with different data
    let (project_a, db_a) = setup_project();
    let (project_b, db_b) = setup_project();

    // Populate A with "alpha" entry
    let store_a = Store::open(&db_a).unwrap();
    {
        let conn = store_a.lock_conn();
        conn.execute(
            "INSERT INTO entries (id, title, content, topic, category, source, created_at, updated_at)
             VALUES (1, 'alpha', 'alpha content', 't', 'p', 's', 1, 1)",
            [],
        )
        .unwrap();
    }
    drop(store_a);

    // Populate B with "beta" entry
    let store_b = Store::open(&db_b).unwrap();
    {
        let conn = store_b.lock_conn();
        conn.execute(
            "INSERT INTO entries (id, title, content, topic, category, source, created_at, updated_at)
             VALUES (1, 'beta', 'beta content', 't', 'p', 's', 1, 1)",
            [],
        )
        .unwrap();
    }
    drop(store_b);

    let output_dir = TempDir::new().unwrap();

    // Export A
    let output_a = output_dir.path().join("export_a.jsonl");
    let content_a = run_export_to_string(project_a.path(), &output_a);
    let lines_a = parse_lines(&content_a);
    let entry_a = lines_a
        .iter()
        .find(|l| l.get("_table").and_then(|t| t.as_str()) == Some("entries"))
        .expect("entries in A");
    assert_eq!(entry_a["title"].as_str().unwrap(), "alpha");

    // Export B
    let output_b = output_dir.path().join("export_b.jsonl");
    let content_b = run_export_to_string(project_b.path(), &output_b);
    let lines_b = parse_lines(&content_b);
    let entry_b = lines_b
        .iter()
        .find(|l| l.get("_table").and_then(|t| t.as_str()) == Some("entries"))
        .expect("entries in B");
    assert_eq!(entry_b["title"].as_str().unwrap(), "beta");
}

// ---------------------------------------------------------------------------
// T-EM-13 / AC-11: Performance -- 500 entries under 5 seconds
// ---------------------------------------------------------------------------
#[test]
fn test_performance_500_entries() {
    let (project_dir, db_path) = setup_project();
    let store = Store::open(&db_path).unwrap();
    {
        let conn = store.lock_conn();
        for id in 1..=500 {
            conn.execute(
                "INSERT INTO entries (
                    id, title, content, topic, category, source, status, confidence,
                    created_at, updated_at
                ) VALUES (?1, 'Entry ' || ?1, 'Content for entry ' || ?1,
                          'topic', 'pattern', 'perf-test', 0, 0.5, 1700000000, 1700000000)",
                rusqlite::params![id],
            )
            .unwrap();
        }
        // Add tags (2 per entry)
        for id in 1..=500 {
            conn.execute(
                "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, 'tag-a')",
                rusqlite::params![id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, 'tag-b')",
                rusqlite::params![id],
            )
            .unwrap();
        }
        // Add co_access (1 per entry pair for first 100)
        for id in 1..=100 {
            conn.execute(
                "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (?1, ?2, 1, 1)",
                rusqlite::params![id, id + 1],
            )
            .unwrap();
        }
    }
    drop(store);

    let output_dir = TempDir::new().unwrap();
    let output_path = output_dir.path().join("export.jsonl");

    let start = std::time::Instant::now();
    run_export(Some(project_dir.path()), Some(&output_path)).expect("export should succeed");
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 5,
        "Export of 500 entries should complete in under 5 seconds, took {elapsed:?}"
    );

    // Verify output is complete
    let content = std::fs::read_to_string(&output_path).unwrap();
    let lines = parse_lines(&content);
    let entry_count = lines
        .iter()
        .filter(|l| l.get("_table").and_then(|t| t.as_str()) == Some("entries"))
        .count();
    assert_eq!(entry_count, 500);
}
