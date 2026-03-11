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
        map.insert(
            "feature_id".into(),
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
        map.insert(
            "agent_id".into(),
            Value::String(row.get::<_, String>(0)?),
        );
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
        map.insert(
            "session_id".into(),
            Value::String(row.get::<_, String>(2)?),
        );
        map.insert(
            "agent_id".into(),
            Value::String(row.get::<_, String>(3)?),
        );
        map.insert(
            "operation".into(),
            Value::String(row.get::<_, String>(4)?),
        );
        // JSON-in-TEXT: emitted as string, not parsed
        map.insert(
            "target_ids".into(),
            Value::String(row.get::<_, String>(5)?),
        );
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

    /// Helper: create a fresh database in a temp directory and return (store, temp_dir).
    fn setup_test_db() -> (Store, TempDir) {
        let tmp = TempDir::new().expect("create temp dir");
        let db_path = tmp.path().join("unimatrix.db");
        let store = Store::open(&db_path).expect("open store");
        (store, tmp)
    }

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
        // Exactly 5 keys
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

        // At minimum, the header line should be present
        assert!(!lines.is_empty(), "should have at least a header line");

        // First line is a valid header
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
        let tmp = TempDir::new().expect("create temp dir");
        let db_path = tmp.path().join("unimatrix.db");
        let _store = Store::open(&db_path).expect("open store");
        drop(_store);

        let output_path = tmp.path().join("export.jsonl");

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

        // With preserve_order, keys should be in insertion order
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
}
