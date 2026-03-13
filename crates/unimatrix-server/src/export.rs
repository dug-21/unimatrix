//! Knowledge base export to JSONL format (nan-001).
//!
//! Exports the Unimatrix knowledge base to a portable JSONL file that preserves
//! every field needed for lossless knowledge restore. Covers 8 tables, excludes
//! derived data (embeddings, HNSW index) and ephemeral operational data.
//!
//! The export runs synchronously with no tokio runtime, following the existing
//! `hook` subcommand pattern. A single `BEGIN DEFERRED` transaction wraps all
//! reads for snapshot isolation (ADR-001).

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Number, Value};
use unimatrix_store::Store;
use unimatrix_store::rusqlite::{self, Connection};

use crate::project;

/// Run the export subcommand.
///
/// Opens the database, wraps the read in a single transaction for snapshot
/// consistency, and writes JSONL to `output` (or stdout if None).
pub fn run_export(
    project_dir: Option<&Path>,
    output: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Resolve project paths
    let paths = project::ensure_data_directory(project_dir, None)?;

    // 2. Open database (triggers migration if needed)
    let store = Store::open(&paths.db_path)?;

    // 3. Acquire connection mutex
    let conn = store.lock_conn();

    // 4. Begin snapshot transaction (ADR-001)
    conn.execute_batch("BEGIN DEFERRED")?;

    // 5. Set up writer and run export
    let result = if let Some(path) = output {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        do_export(&conn, &mut writer)
    } else {
        let stdout = io::stdout();
        let lock = stdout.lock();
        let mut writer = BufWriter::new(lock);
        do_export(&conn, &mut writer)
    };

    // 6. Commit transaction regardless of export result
    //    Read-only DEFERRED: COMMIT and ROLLBACK are equivalent.
    let _ = conn.execute_batch("COMMIT");

    // 7. Propagate any export error
    result
}

/// Execute all export steps against the connection and writer.
///
/// Separated from `run_export` to allow the writer type to vary (file vs stdout)
/// while keeping transaction logic in one place.
fn do_export(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>> {
    write_header(conn, writer)?;
    export_counters(conn, writer)?;
    export_entries(conn, writer)?;
    export_entry_tags(conn, writer)?;
    export_co_access(conn, writer)?;
    export_feature_entries(conn, writer)?;
    export_outcome_index(conn, writer)?;
    export_agent_registry(conn, writer)?;
    export_audit_log(conn, writer)?;
    writer.flush()?;
    Ok(())
}

/// Write the JSONL header line with export metadata.
///
/// Queries schema_version from counters and COUNT(*) from entries.
/// Key order: _header, schema_version, exported_at, entry_count, format_version.
fn write_header(
    conn: &Connection,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema_version: i64 = conn.query_row(
        "SELECT value FROM counters WHERE name = 'schema_version'",
        [],
        |row| row.get(0),
    )?;

    let entry_count: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;

    let exported_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut map = Map::new();
    map.insert("_header".to_string(), Value::Bool(true));
    map.insert(
        "schema_version".to_string(),
        Value::Number(schema_version.into()),
    );
    map.insert("exported_at".to_string(), Value::Number(exported_at.into()));
    map.insert("entry_count".to_string(), Value::Number(entry_count.into()));
    map.insert("format_version".to_string(), Value::Number(1.into()));

    let line = serde_json::to_string(&Value::Object(map))?;
    writeln!(writer, "{line}")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers for row serialization
// ---------------------------------------------------------------------------

/// Serialize a `Map` as a single JSONL line.
fn write_row(
    map: Map<String, Value>,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let line = serde_json::to_string(&Value::Object(map))?;
    writeln!(writer, "{line}")?;
    Ok(())
}

/// Extract a nullable INTEGER column as `Value::Number` or `Value::Null`.
fn nullable_int(row: &rusqlite::Row<'_>, idx: usize) -> Result<Value, rusqlite::Error> {
    match row.get::<_, Option<i64>>(idx)? {
        Some(v) => Ok(Value::Number(v.into())),
        None => Ok(Value::Null),
    }
}

/// Extract a nullable TEXT column as `Value::String` or `Value::Null`.
fn nullable_text(row: &rusqlite::Row<'_>, idx: usize) -> Result<Value, rusqlite::Error> {
    match row.get::<_, Option<String>>(idx)? {
        Some(s) => Ok(Value::String(s)),
        None => Ok(Value::Null),
    }
}

// ---------------------------------------------------------------------------
// Per-table export functions (ADR-002: explicit column-to-JSON mapping)
// ---------------------------------------------------------------------------

/// Export all rows from the `counters` table.
///
/// Columns: name (TEXT PK), value (INTEGER NOT NULL).
/// Order: name ASC.
fn export_counters(
    conn: &Connection,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare("SELECT name, value FROM counters ORDER BY name")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("counters".into()));
        map.insert("name".into(), Value::String(row.get::<_, String>(0)?));
        map.insert("value".into(), Value::Number(row.get::<_, i64>(1)?.into()));
        write_row(map, writer)?;
    }
    Ok(())
}

/// Export all rows from the `entries` table (26 columns).
///
/// Order: id ASC. Nullable columns emit JSON null for SQL NULL.
/// Confidence (REAL) uses `Number::from_f64` with NaN fallback to 0 (ADR-002).
fn export_entries(
    conn: &Connection,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, content, topic, category, source, status, confidence,
                created_at, updated_at, last_accessed_at, access_count,
                supersedes, superseded_by, correction_count, embedding_dim,
                created_by, modified_by, content_hash, previous_hash,
                version, feature_cycle, trust_source,
                helpful_count, unhelpful_count, pre_quarantine_status
         FROM entries ORDER BY id",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("entries".into()));
        // INTEGER NOT NULL (PK)
        map.insert("id".into(), Value::Number(row.get::<_, i64>(0)?.into()));
        // TEXT NOT NULL columns
        map.insert("title".into(), Value::String(row.get::<_, String>(1)?));
        map.insert("content".into(), Value::String(row.get::<_, String>(2)?));
        map.insert("topic".into(), Value::String(row.get::<_, String>(3)?));
        map.insert("category".into(), Value::String(row.get::<_, String>(4)?));
        map.insert("source".into(), Value::String(row.get::<_, String>(5)?));
        // INTEGER NOT NULL
        map.insert("status".into(), Value::Number(row.get::<_, i64>(6)?.into()));
        // REAL NOT NULL (f64) -- NaN safety per ADR-002
        let confidence: f64 = row.get(7)?;
        map.insert(
            "confidence".into(),
            Value::Number(Number::from_f64(confidence).unwrap_or(Number::from(0))),
        );
        // INTEGER NOT NULL timestamps
        map.insert(
            "created_at".into(),
            Value::Number(row.get::<_, i64>(8)?.into()),
        );
        map.insert(
            "updated_at".into(),
            Value::Number(row.get::<_, i64>(9)?.into()),
        );
        map.insert(
            "last_accessed_at".into(),
            Value::Number(row.get::<_, i64>(10)?.into()),
        );
        map.insert(
            "access_count".into(),
            Value::Number(row.get::<_, i64>(11)?.into()),
        );
        // INTEGER nullable
        map.insert("supersedes".into(), nullable_int(row, 12)?);
        map.insert("superseded_by".into(), nullable_int(row, 13)?);
        // INTEGER NOT NULL
        map.insert(
            "correction_count".into(),
            Value::Number(row.get::<_, i64>(14)?.into()),
        );
        map.insert(
            "embedding_dim".into(),
            Value::Number(row.get::<_, i64>(15)?.into()),
        );
        // TEXT NOT NULL
        map.insert(
            "created_by".into(),
            Value::String(row.get::<_, String>(16)?),
        );
        map.insert(
            "modified_by".into(),
            Value::String(row.get::<_, String>(17)?),
        );
        map.insert(
            "content_hash".into(),
            Value::String(row.get::<_, String>(18)?),
        );
        map.insert(
            "previous_hash".into(),
            Value::String(row.get::<_, String>(19)?),
        );
        // INTEGER NOT NULL
        map.insert(
            "version".into(),
            Value::Number(row.get::<_, i64>(20)?.into()),
        );
        // TEXT NOT NULL
        map.insert(
            "feature_cycle".into(),
            Value::String(row.get::<_, String>(21)?),
        );
        map.insert(
            "trust_source".into(),
            Value::String(row.get::<_, String>(22)?),
        );
        // INTEGER NOT NULL
        map.insert(
            "helpful_count".into(),
            Value::Number(row.get::<_, i64>(23)?.into()),
        );
        map.insert(
            "unhelpful_count".into(),
            Value::Number(row.get::<_, i64>(24)?.into()),
        );
        // INTEGER nullable
        map.insert("pre_quarantine_status".into(), nullable_int(row, 25)?);

        write_row(map, writer)?;
    }
    Ok(())
}

/// Export all rows from the `entry_tags` table.
///
/// Columns: entry_id (INTEGER), tag (TEXT). Order: entry_id ASC, tag ASC.
fn export_entry_tags(
    conn: &Connection,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare("SELECT entry_id, tag FROM entry_tags ORDER BY entry_id, tag")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("entry_tags".into()));
        map.insert(
            "entry_id".into(),
            Value::Number(row.get::<_, i64>(0)?.into()),
        );
        map.insert("tag".into(), Value::String(row.get::<_, String>(1)?));
        write_row(map, writer)?;
    }
    Ok(())
}

/// Export all rows from the `co_access` table.
///
/// Columns: entry_id_a, entry_id_b, count, last_updated (all INTEGER NOT NULL).
/// Order: entry_id_a ASC, entry_id_b ASC.
fn export_co_access(
    conn: &Connection,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT entry_id_a, entry_id_b, count, last_updated
         FROM co_access ORDER BY entry_id_a, entry_id_b",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("co_access".into()));
        map.insert(
            "entry_id_a".into(),
            Value::Number(row.get::<_, i64>(0)?.into()),
        );
        map.insert(
            "entry_id_b".into(),
            Value::Number(row.get::<_, i64>(1)?.into()),
        );
        map.insert("count".into(), Value::Number(row.get::<_, i64>(2)?.into()));
        map.insert(
            "last_updated".into(),
            Value::Number(row.get::<_, i64>(3)?.into()),
        );
        write_row(map, writer)?;
    }
    Ok(())
}

/// Export all rows from the `feature_entries` table.
///
/// Columns: feature_id (TEXT), entry_id (INTEGER). Order: feature_id ASC, entry_id ASC.
fn export_feature_entries(
    conn: &Connection,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT feature_id, entry_id FROM feature_entries ORDER BY feature_id, entry_id",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("feature_entries".into()));
        map.insert("feature_id".into(), Value::String(row.get::<_, String>(0)?));
        map.insert(
            "entry_id".into(),
            Value::Number(row.get::<_, i64>(1)?.into()),
        );
        write_row(map, writer)?;
    }
    Ok(())
}

/// Export all rows from the `outcome_index` table.
///
/// Columns: feature_cycle (TEXT), entry_id (INTEGER). Order: feature_cycle ASC, entry_id ASC.
fn export_outcome_index(
    conn: &Connection,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT feature_cycle, entry_id FROM outcome_index ORDER BY feature_cycle, entry_id",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("outcome_index".into()));
        map.insert(
            "feature_cycle".into(),
            Value::String(row.get::<_, String>(0)?),
        );
        map.insert(
            "entry_id".into(),
            Value::Number(row.get::<_, i64>(1)?.into()),
        );
        write_row(map, writer)?;
    }
    Ok(())
}

/// Export all rows from the `agent_registry` table.
///
/// Columns: agent_id (TEXT PK), trust_level (INTEGER), capabilities (TEXT, JSON-in-TEXT),
/// allowed_topics (TEXT nullable, JSON-in-TEXT), allowed_categories (TEXT nullable, JSON-in-TEXT),
/// enrolled_at (INTEGER), last_seen_at (INTEGER), active (INTEGER).
/// Order: agent_id ASC.
///
/// JSON-in-TEXT columns are emitted as raw strings, not parsed/re-encoded (ADR-002).
fn export_agent_registry(
    conn: &Connection,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT agent_id, trust_level, capabilities, allowed_topics,
                allowed_categories, enrolled_at, last_seen_at, active
         FROM agent_registry ORDER BY agent_id",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("agent_registry".into()));
        map.insert("agent_id".into(), Value::String(row.get::<_, String>(0)?));
        map.insert(
            "trust_level".into(),
            Value::Number(row.get::<_, i64>(1)?.into()),
        );
        // JSON-in-TEXT: emitted as string, not parsed
        map.insert(
            "capabilities".into(),
            Value::String(row.get::<_, String>(2)?),
        );
        // Nullable JSON-in-TEXT
        map.insert("allowed_topics".into(), nullable_text(row, 3)?);
        map.insert("allowed_categories".into(), nullable_text(row, 4)?);
        map.insert(
            "enrolled_at".into(),
            Value::Number(row.get::<_, i64>(5)?.into()),
        );
        map.insert(
            "last_seen_at".into(),
            Value::Number(row.get::<_, i64>(6)?.into()),
        );
        map.insert("active".into(), Value::Number(row.get::<_, i64>(7)?.into()));
        write_row(map, writer)?;
    }
    Ok(())
}

/// Export all rows from the `audit_log` table.
///
/// Columns: event_id (INTEGER PK), timestamp (INTEGER), session_id (TEXT),
/// agent_id (TEXT), operation (TEXT), target_ids (TEXT, JSON-in-TEXT),
/// outcome (INTEGER), detail (TEXT).
/// Order: event_id ASC.
///
/// The `target_ids` column is JSON-in-TEXT: emitted as a raw string (ADR-002).
fn export_audit_log(
    conn: &Connection,
    writer: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT event_id, timestamp, session_id, agent_id, operation,
                target_ids, outcome, detail
         FROM audit_log ORDER BY event_id",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let mut map = Map::new();
        map.insert("_table".into(), Value::String("audit_log".into()));
        map.insert(
            "event_id".into(),
            Value::Number(row.get::<_, i64>(0)?.into()),
        );
        map.insert(
            "timestamp".into(),
            Value::Number(row.get::<_, i64>(1)?.into()),
        );
        map.insert("session_id".into(), Value::String(row.get::<_, String>(2)?));
        map.insert("agent_id".into(), Value::String(row.get::<_, String>(3)?));
        map.insert("operation".into(), Value::String(row.get::<_, String>(4)?));
        // JSON-in-TEXT: emitted as string, not parsed
        map.insert("target_ids".into(), Value::String(row.get::<_, String>(5)?));
        map.insert(
            "outcome".into(),
            Value::Number(row.get::<_, i64>(6)?.into()),
        );
        map.insert("detail".into(), Value::String(row.get::<_, String>(7)?));
        write_row(map, writer)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Create a fresh database in a temp directory and return (store, temp_dir).
    fn setup_test_db() -> (Store, TempDir) {
        let tmp = TempDir::new().expect("create temp dir");
        let db_path = tmp.path().join("unimatrix.db");
        let store = Store::open(&db_path).expect("open store");
        (store, tmp)
    }

    /// Parse the first non-empty line from a buffer as JSON.
    fn parse_line(buf: &[u8]) -> Value {
        let s = std::str::from_utf8(buf).unwrap();
        let line = s.lines().next().unwrap();
        serde_json::from_str(line).unwrap()
    }

    /// Parse all non-empty lines from a buffer as JSON values.
    fn parse_lines(buf: &[u8]) -> Vec<Value> {
        let s = std::str::from_utf8(buf).unwrap();
        s.lines()
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    // -----------------------------------------------------------------------
    // Export-module agent tests (header, orchestration)
    // -----------------------------------------------------------------------

    #[test]
    fn test_write_header_fields_correct() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        let mut buf = Vec::new();

        write_header(&conn, &mut buf).expect("write_header");

        let line = String::from_utf8(buf).expect("utf8");
        let val: Value = serde_json::from_str(line.trim()).expect("parse json");
        let obj = val.as_object().expect("object");

        assert_eq!(obj.get("_header"), Some(&Value::Bool(true)));
        assert!(
            obj.get("schema_version")
                .expect("schema_version")
                .is_number()
        );
        assert!(obj.get("exported_at").expect("exported_at").is_number());
        assert_eq!(obj.get("entry_count"), Some(&Value::Number(0.into())));
        assert_eq!(obj.get("format_version"), Some(&Value::Number(1.into())));
        assert_eq!(obj.len(), 5);
    }

    #[test]
    fn test_write_header_exported_at_recent() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        let mut buf = Vec::new();

        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        write_header(&conn, &mut buf).expect("write_header");

        let after = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let val: Value = serde_json::from_str(String::from_utf8(buf).unwrap().trim()).unwrap();
        let exported_at = val["exported_at"].as_i64().unwrap();

        assert!(exported_at >= before);
        assert!(exported_at <= after);
    }

    #[test]
    fn test_do_export_empty_db() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        let mut buf = Vec::new();

        do_export(&conn, &mut buf).expect("do_export");

        let output = String::from_utf8(buf).expect("utf8");
        let lines: Vec<&str> = output.lines().collect();

        assert!(!lines.is_empty(), "should have at least a header line");

        let header: Value = serde_json::from_str(lines[0]).expect("parse header");
        assert_eq!(header["_header"], Value::Bool(true));
        assert_eq!(header["entry_count"], Value::Number(0.into()));
    }

    #[test]
    fn test_do_export_all_lines_valid_json() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        let mut buf = Vec::new();

        do_export(&conn, &mut buf).expect("do_export");

        let output = String::from_utf8(buf).expect("utf8");
        for line in output.lines() {
            let _: Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("invalid JSON line: {e}: {line}"));
        }
    }

    #[test]
    fn test_run_export_to_file() {
        let _tmp = TempDir::new().expect("create temp dir");
        // run_export needs ensure_data_directory which uses project root detection.
        // For unit tests, we test do_export directly. File output is tested via
        // integration tests that set up proper project directories.
    }

    #[test]
    fn test_header_key_order_preserved() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        let mut buf = Vec::new();

        write_header(&conn, &mut buf).expect("write_header");

        let output = String::from_utf8(buf).expect("utf8");
        let val: Value = serde_json::from_str(output.trim()).expect("parse json");
        let obj = val.as_object().expect("object");

        let keys: Vec<&String> = obj.keys().collect();
        assert_eq!(
            keys,
            vec![
                "_header",
                "schema_version",
                "exported_at",
                "entry_count",
                "format_version"
            ]
        );
    }

    // -----------------------------------------------------------------------
    // T-RS-01: All 26 entry columns present with correct values
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_entries_all_26_columns_present() {
        let (store, _tmp) = setup_test_db();
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
                42, 'Test Entry', 'Content here', 'testing', 'pattern', 'unit-test', 1, 0.87654321,
                1700000000, 1700000001, 1700000002, 15,
                10, 50, 3, 384,
                'agent-x', 'agent-y', 'abc123', 'def456',
                7, 'crt-002', 'human',
                12, 2, 0
            )",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_entries(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);

        let obj = row.as_object().unwrap();
        assert_eq!(obj.len(), 27); // 26 columns + _table
        assert_eq!(obj["_table"], "entries");
        assert_eq!(obj["id"], 42);
        assert_eq!(obj["title"], "Test Entry");
        assert_eq!(obj["content"], "Content here");
        assert_eq!(obj["topic"], "testing");
        assert_eq!(obj["category"], "pattern");
        assert_eq!(obj["source"], "unit-test");
        assert_eq!(obj["status"], 1);
        assert_eq!(obj["created_at"], 1_700_000_000i64);
        assert_eq!(obj["updated_at"], 1_700_000_001i64);
        assert_eq!(obj["last_accessed_at"], 1_700_000_002i64);
        assert_eq!(obj["access_count"], 15);
        assert_eq!(obj["supersedes"], 10);
        assert_eq!(obj["superseded_by"], 50);
        assert_eq!(obj["correction_count"], 3);
        assert_eq!(obj["embedding_dim"], 384);
        assert_eq!(obj["created_by"], "agent-x");
        assert_eq!(obj["modified_by"], "agent-y");
        assert_eq!(obj["content_hash"], "abc123");
        assert_eq!(obj["previous_hash"], "def456");
        assert_eq!(obj["version"], 7);
        assert_eq!(obj["feature_cycle"], "crt-002");
        assert_eq!(obj["trust_source"], "human");
        assert_eq!(obj["helpful_count"], 12);
        assert_eq!(obj["unhelpful_count"], 2);
        assert_eq!(obj["pre_quarantine_status"], 0);
    }

    // -----------------------------------------------------------------------
    // T-RS-03: Per-table key counts
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_counters_key_count_and_values() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();

        let mut buf = Vec::new();
        export_counters(&conn, &mut buf).unwrap();
        let rows = parse_lines(&buf);
        // counters table has schema_version from Store::open migration
        assert!(!rows.is_empty());
        for row in &rows {
            assert_eq!(row.as_object().unwrap().len(), 3);
            assert_eq!(row["_table"], "counters");
        }
    }

    #[test]
    fn test_export_entry_tags_key_count() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO entries (id, title, content, topic, category, source, created_at, updated_at)
             VALUES (1, 't', 'c', 't', 'p', 's', 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entry_tags (entry_id, tag) VALUES (1, 'rust')",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_entry_tags(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);
        assert_eq!(row.as_object().unwrap().len(), 3);
        assert_eq!(row["_table"], "entry_tags");
    }

    #[test]
    fn test_export_co_access_key_count() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated)
             VALUES (1, 2, 5, 1700000000)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_co_access(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);
        assert_eq!(row.as_object().unwrap().len(), 5);
        assert_eq!(row["_table"], "co_access");
        assert_eq!(row["entry_id_a"], 1);
        assert_eq!(row["entry_id_b"], 2);
        assert_eq!(row["count"], 5);
        assert_eq!(row["last_updated"], 1_700_000_000i64);
    }

    #[test]
    fn test_export_feature_entries_key_count() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO feature_entries (feature_id, entry_id) VALUES ('nxs-001', 42)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_feature_entries(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);
        assert_eq!(row.as_object().unwrap().len(), 3);
        assert_eq!(row["feature_id"], "nxs-001");
        assert_eq!(row["entry_id"], 42);
    }

    #[test]
    fn test_export_outcome_index_key_count() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO outcome_index (feature_cycle, entry_id) VALUES ('crt-001', 7)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_outcome_index(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);
        assert_eq!(row.as_object().unwrap().len(), 3);
        assert_eq!(row["feature_cycle"], "crt-001");
        assert_eq!(row["entry_id"], 7);
    }

    #[test]
    fn test_export_agent_registry_key_count() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
             allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
             VALUES ('bot-1', 2, '[\"Admin\"]', '[\"security\"]', '[\"decision\"]', 1700000000, 1700000001, 1)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_agent_registry(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);
        assert_eq!(row.as_object().unwrap().len(), 9);
        assert_eq!(row["_table"], "agent_registry");
    }

    #[test]
    fn test_export_audit_log_key_count() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
             operation, target_ids, outcome, detail)
             VALUES (1, 1700000000, 'sess-1', 'bot-1', 'store', '[1,2]', 0, 'ok')",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_audit_log(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);
        assert_eq!(row.as_object().unwrap().len(), 9);
        assert_eq!(row["_table"], "audit_log");
    }

    // -----------------------------------------------------------------------
    // T-RS-04: f64 confidence round-trip fidelity
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_entries_f64_precision() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        let values = [0.0, 1.0, 0.123456789012345, f64::MIN_POSITIVE, 0.1 + 0.2];

        for (i, &v) in values.iter().enumerate() {
            let id = (i as i64) + 1;
            conn.execute(
                "INSERT INTO entries (
                    id, title, content, topic, category, source, status, confidence,
                    created_at, updated_at
                ) VALUES (?1, 'test', 'c', 't', 'p', 's', 0, ?2, 1, 1)",
                rusqlite::params![id, v],
            )
            .unwrap();
        }

        let mut buf = Vec::new();
        export_entries(&conn, &mut buf).unwrap();
        let rows = parse_lines(&buf);
        assert_eq!(rows.len(), values.len());

        for (row, &expected) in rows.iter().zip(values.iter()) {
            let parsed = row["confidence"].as_f64().unwrap();
            assert_eq!(
                parsed.to_bits(),
                expected.to_bits(),
                "f64 mismatch for {expected}: got {parsed}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // T-RS-05: JSON-in-TEXT columns emitted as raw strings
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_agent_registry_json_in_text_as_string() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
             allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
             VALUES ('bot-1', 2, '[\"Admin\",\"Read\"]', '[\"security\"]', '[\"decision\"]', 1, 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
             allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
             VALUES ('bot-2', 1, '[]', NULL, NULL, 2, 2, 1)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_agent_registry(&conn, &mut buf).unwrap();
        let rows = parse_lines(&buf);
        assert_eq!(rows.len(), 2);

        // bot-1: JSON-in-TEXT are strings, NOT parsed arrays
        let r1 = &rows[0];
        assert!(r1["capabilities"].is_string());
        assert_eq!(r1["capabilities"].as_str().unwrap(), "[\"Admin\",\"Read\"]");
        assert!(r1["allowed_topics"].is_string());
        assert_eq!(r1["allowed_topics"].as_str().unwrap(), "[\"security\"]");
        assert!(r1["allowed_categories"].is_string());
        assert_eq!(r1["allowed_categories"].as_str().unwrap(), "[\"decision\"]");

        // bot-2: empty array as string, nullable fields as null
        let r2 = &rows[1];
        assert!(r2["capabilities"].is_string());
        assert_eq!(r2["capabilities"].as_str().unwrap(), "[]");
        assert!(r2["allowed_topics"].is_null());
        assert!(r2["allowed_categories"].is_null());
    }

    #[test]
    fn test_export_audit_log_json_in_text_target_ids() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
             operation, target_ids, outcome, detail)
             VALUES (1, 100, 's1', 'a1', 'op', '[1,2,3]', 0, 'detail')",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_audit_log(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);

        assert!(row["target_ids"].is_string());
        assert_eq!(row["target_ids"].as_str().unwrap(), "[1,2,3]");
    }

    // -----------------------------------------------------------------------
    // T-RS-06: NULL columns serialized as JSON null
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_entries_null_handling() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at, supersedes, superseded_by, pre_quarantine_status
            ) VALUES (1, 'test', 'c', 't', 'p', 's', 0, 0.5, 1, 1, NULL, NULL, NULL)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_entries(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);

        assert!(row.as_object().unwrap().contains_key("supersedes"));
        assert!(row["supersedes"].is_null());
        assert!(row.as_object().unwrap().contains_key("superseded_by"));
        assert!(row["superseded_by"].is_null());
        assert!(
            row.as_object()
                .unwrap()
                .contains_key("pre_quarantine_status")
        );
        assert!(row["pre_quarantine_status"].is_null());
        assert_eq!(row.as_object().unwrap().len(), 27);
    }

    #[test]
    fn test_export_agent_registry_null_handling() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO agent_registry (agent_id, trust_level, capabilities,
             allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
             VALUES ('bot-null', 0, '[]', NULL, NULL, 1, 1, 1)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_agent_registry(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);

        assert!(row.as_object().unwrap().contains_key("allowed_topics"));
        assert!(row["allowed_topics"].is_null());
        assert!(row.as_object().unwrap().contains_key("allowed_categories"));
        assert!(row["allowed_categories"].is_null());
    }

    // -----------------------------------------------------------------------
    // T-RS-06b: Empty strings are NOT null
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_entries_empty_string_not_null() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at, created_by, content_hash, feature_cycle
            ) VALUES (1, 'test', 'c', 't', 'p', 's', 0, 0.0, 1, 1, '', '', '')",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_entries(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);

        assert!(row["created_by"].is_string());
        assert_eq!(row["created_by"].as_str().unwrap(), "");
        assert!(row["content_hash"].is_string());
        assert_eq!(row["content_hash"].as_str().unwrap(), "");
        assert!(row["feature_cycle"].is_string());
        assert_eq!(row["feature_cycle"].as_str().unwrap(), "");
    }

    // -----------------------------------------------------------------------
    // T-RS-07: _table is first key, columns follow DDL declaration order
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_entries_key_ordering() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at
            ) VALUES (1, 'test', 'content', 'topic', 'cat', 'src', 0, 0.5, 1, 1)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_entries(&conn, &mut buf).unwrap();

        let raw = std::str::from_utf8(&buf).unwrap();
        let line = raw.lines().next().unwrap();

        // _table must be first key in raw JSON
        assert!(
            line.starts_with("{\"_table\":"),
            "Expected _table as first key, got: {}",
            &line[..50.min(line.len())]
        );

        // Verify full key order via preserve_order map
        let v: Value = serde_json::from_str(line).unwrap();
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        let expected_keys = [
            "_table",
            "id",
            "title",
            "content",
            "topic",
            "category",
            "source",
            "status",
            "confidence",
            "created_at",
            "updated_at",
            "last_accessed_at",
            "access_count",
            "supersedes",
            "superseded_by",
            "correction_count",
            "embedding_dim",
            "created_by",
            "modified_by",
            "content_hash",
            "previous_hash",
            "version",
            "feature_cycle",
            "trust_source",
            "helpful_count",
            "unhelpful_count",
            "pre_quarantine_status",
        ];
        assert_eq!(keys.len(), expected_keys.len());
        for (got, expected) in keys.iter().zip(expected_keys.iter()) {
            assert_eq!(got.as_str(), *expected);
        }
    }

    #[test]
    fn test_export_counters_table_key_first() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();

        let mut buf = Vec::new();
        export_counters(&conn, &mut buf).unwrap();
        // counters has at least schema_version from migration
        let raw = std::str::from_utf8(&buf).unwrap();
        if let Some(line) = raw.lines().next() {
            assert!(line.starts_with("{\"_table\":"));
        }
    }

    // -----------------------------------------------------------------------
    // T-RS-09: Unicode content preserved
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_entries_unicode_cjk_and_emoji() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at
            ) VALUES (1, '\u{77E5}\u{8B58}', 'Status: \u{2705} approved', 't', 'p', 's', 0, 0.0, 1, 1)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_entries(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);

        assert_eq!(row["title"].as_str().unwrap(), "\u{77E5}\u{8B58}");
        assert_eq!(
            row["content"].as_str().unwrap(),
            "Status: \u{2705} approved"
        );
    }

    #[test]
    fn test_export_entry_tags_unicode_accented() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO entries (id, title, content, topic, category, source, created_at, updated_at)
             VALUES (1, 't', 'c', 't', 'p', 's', 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entry_tags (entry_id, tag) VALUES (1, 'resume\u{0301}')",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_entry_tags(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);
        assert_eq!(row["tag"].as_str().unwrap(), "resume\u{0301}");
    }

    // -----------------------------------------------------------------------
    // T-RS-10: Large integer values preserved
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_entries_large_integers() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO entries (
                id, title, content, topic, category, source, status, confidence,
                created_at, updated_at, version, access_count
            ) VALUES (1, 't', 'c', 't', 'p', 's', 0, 0.0, 9999999999, 1, 2147483647, 1000000)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_entries(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);

        assert_eq!(row["created_at"].as_i64().unwrap(), 9_999_999_999i64);
        assert_eq!(row["version"].as_i64().unwrap(), 2_147_483_647i64);
        assert_eq!(row["access_count"].as_i64().unwrap(), 1_000_000i64);
    }

    #[test]
    fn test_export_counters_i64_max() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT OR REPLACE INTO counters (name, value) VALUES ('big', 9223372036854775807)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_counters(&conn, &mut buf).unwrap();
        let rows = parse_lines(&buf);
        let big_row = rows.iter().find(|r| r["name"] == "big").unwrap();
        assert_eq!(big_row["value"].as_i64().unwrap(), i64::MAX);
    }

    // -----------------------------------------------------------------------
    // T-RS-11: Entry with all nullable fields NULL simultaneously
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_entries_all_nullable_null() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO entries (
                id, title, content, topic, category, source, created_at, updated_at,
                supersedes, superseded_by, pre_quarantine_status
            ) VALUES (1, 't', 'c', 't', 'p', 's', 1, 1, NULL, NULL, NULL)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_entries(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);

        assert!(row["supersedes"].is_null());
        assert!(row["superseded_by"].is_null());
        assert!(row["pre_quarantine_status"].is_null());
        assert_eq!(row.as_object().unwrap().len(), 27);
    }

    // -----------------------------------------------------------------------
    // T-RS-12: Timestamp of 0 is not treated as NULL
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_entries_zero_timestamp_not_null() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO entries (
                id, title, content, topic, category, source, created_at, updated_at,
                last_accessed_at
            ) VALUES (1, 't', 'c', 't', 'p', 's', 0, 0, 0)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_entries(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);

        assert_eq!(row["created_at"].as_i64().unwrap(), 0);
        assert_eq!(row["last_accessed_at"].as_i64().unwrap(), 0);
        assert!(!row["created_at"].is_null());
    }

    // -----------------------------------------------------------------------
    // T-RS-13: JSONL line integrity -- no raw newlines in output lines
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_entries_newline_in_content_escaped() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO entries (
                id, title, content, topic, category, source, created_at, updated_at
            ) VALUES (1, 't', 'line1\nline2\nline3', 't', 'p', 's', 1, 1)",
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_entries(&conn, &mut buf).unwrap();
        let raw = std::str::from_utf8(&buf).unwrap();

        let lines: Vec<&str> = raw.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 1, "Multi-line content must not break JSONL");

        let row: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(row["content"].as_str().unwrap(), "line1\nline2\nline3");
    }

    // -----------------------------------------------------------------------
    // Row ordering tests
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_entries_ordered_by_id() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        for id in [5, 2, 8] {
            conn.execute(
                "INSERT INTO entries (id, title, content, topic, category, source, created_at, updated_at)
                 VALUES (?1, 't', 'c', 't', 'p', 's', 1, 1)",
                [id],
            )
            .unwrap();
        }

        let mut buf = Vec::new();
        export_entries(&conn, &mut buf).unwrap();
        let rows = parse_lines(&buf);
        let ids: Vec<i64> = rows.iter().map(|r| r["id"].as_i64().unwrap()).collect();
        assert_eq!(ids, vec![2, 5, 8]);
    }

    #[test]
    fn test_export_entry_tags_ordered() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            "INSERT INTO entries (id, title, content, topic, category, source, created_at, updated_at)
             VALUES (1, 't', 'c', 't', 'p', 's', 1, 1)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO entry_tags (entry_id, tag) VALUES (1, 'z')", [])
            .unwrap();
        conn.execute("INSERT INTO entry_tags (entry_id, tag) VALUES (1, 'a')", [])
            .unwrap();

        let mut buf = Vec::new();
        export_entry_tags(&conn, &mut buf).unwrap();
        let rows = parse_lines(&buf);
        let tags: Vec<&str> = rows.iter().map(|r| r["tag"].as_str().unwrap()).collect();
        assert_eq!(tags, vec!["a", "z"]);
    }

    // -----------------------------------------------------------------------
    // Empty tables produce no output
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_empty_tables_no_output() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();

        let mut buf = Vec::new();
        export_entries(&conn, &mut buf).unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_entry_tags(&conn, &mut buf).unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_co_access(&conn, &mut buf).unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_feature_entries(&conn, &mut buf).unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_outcome_index(&conn, &mut buf).unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_agent_registry(&conn, &mut buf).unwrap();
        assert!(buf.is_empty());

        buf.clear();
        export_audit_log(&conn, &mut buf).unwrap();
        assert!(buf.is_empty());
    }

    // -----------------------------------------------------------------------
    // JSON-special characters in content
    // -----------------------------------------------------------------------
    #[test]
    fn test_export_entries_json_special_chars_in_content() {
        let (store, _tmp) = setup_test_db();
        let conn = store.lock_conn();
        conn.execute(
            r#"INSERT INTO entries (
                id, title, content, topic, category, source, created_at, updated_at
            ) VALUES (1, 't', 'He said "hello" and used a \backslash', 't', 'p', 's', 1, 1)"#,
            [],
        )
        .unwrap();

        let mut buf = Vec::new();
        export_entries(&conn, &mut buf).unwrap();
        let row = parse_line(&buf);
        assert_eq!(
            row["content"].as_str().unwrap(),
            r#"He said "hello" and used a \backslash"#
        );
    }
}
