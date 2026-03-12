//! Knowledge base import from JSONL format (nan-002).
//!
//! Restores a Unimatrix knowledge base from a nan-001 JSONL export dump,
//! preserving all learned signals (confidence, helpful/unhelpful counts,
//! co-access pairs, correction chains). Runs synchronously with no tokio
//! runtime, following the existing `hook`/`export` subcommand pattern.
//!
//! The import runs in two phases (ADR-004):
//! 1. Database restore: header validation, pre-flight, JSONL ingestion, hash check
//! 2. Embedding reconstruction: re-embed all entries, build HNSW index (separate component)

mod inserters;

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use unimatrix_store::rusqlite::{self, Connection, params};
use unimatrix_store::{Store, compute_content_hash};

use crate::format::{ExportHeader, ExportRow};
use crate::project;

use inserters::{
    insert_agent_registry, insert_audit_log, insert_co_access, insert_counter, insert_entry,
    insert_entry_tag, insert_feature_entry, insert_outcome_index,
};

/// Tracking struct for per-table insert counts.
#[derive(Debug, Default)]
pub struct ImportCounts {
    pub counters: u64,
    pub entries: u64,
    pub entry_tags: u64,
    pub co_access: u64,
    pub feature_entries: u64,
    pub outcome_index: u64,
    pub agent_registry: u64,
    pub audit_log: u64,
}

/// Run the import pipeline.
///
/// Opens the database via `Store::open()`, validates the JSONL header,
/// ingests all rows in a single IMMEDIATE transaction, validates hashes,
/// then commits. Embedding reconstruction is called after DB commit.
pub fn run_import(
    project_dir: Option<&Path>,
    input: &Path,
    skip_hash_validation: bool,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Phase 1: Setup
    let paths = project::ensure_data_directory(project_dir, None)?;
    let store = Arc::new(Store::open(&paths.db_path)?);

    // Phase 2: Open and parse header
    let file = File::open(input)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let header_line = lines.next().ok_or("empty file: no header line")??;
    let header = parse_header(&header_line)?;

    // Phase 3: Pre-flight checks
    let conn = store.lock_conn();
    let db_schema_version: i64 = conn.query_row(
        "SELECT value FROM counters WHERE name = 'schema_version'",
        [],
        |row| row.get(0),
    )?;
    check_preflight(&conn, force, &paths)?;

    // Phase 4: Validate header against DB
    if header.format_version != 1 {
        return Err(format!(
            "unsupported format_version: {}. Only format_version 1 is supported.",
            header.format_version
        )
        .into());
    }
    if header.schema_version > db_schema_version {
        return Err(format!(
            "export schema_version ({}) is newer than this binary's schema_version ({}). Upgrade unimatrix.",
            header.schema_version, db_schema_version
        )
        .into());
    }

    // Phase 5: Force-drop if needed
    if force {
        let entry_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;
        if entry_count > 0 {
            eprintln!(
                "WARNING: --force specified. Dropping {} existing entries and all associated data in {}.",
                entry_count,
                paths.data_dir.display()
            );
        }
        drop_all_data(&conn)?;
    }

    // Phase 6: BEGIN IMMEDIATE transaction
    conn.execute_batch("BEGIN IMMEDIATE")?;

    // Phase 7: Ingest JSONL
    let counts = match ingest_rows(&conn, lines) {
        Ok(counts) => counts,
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    };

    // Phase 8: Hash validation (inside transaction, before commit)
    if !skip_hash_validation {
        if let Err(e) = validate_hashes(&conn) {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    } else {
        eprintln!("WARNING: hash validation skipped (--skip-hash-validation)");
    }

    // Phase 9: COMMIT
    conn.execute_batch("COMMIT")?;

    // Phase 10: Drop the MutexGuard before embedding (embedding needs store access)
    drop(conn);

    // Phase 11: Re-embed and build vector index (ADR-004: after DB commit)
    crate::embed_reconstruct::reconstruct_embeddings(&store, &paths.vector_dir)?;

    // Phase 12: Record provenance
    record_provenance(&store, input, &counts)?;

    // Phase 13: Summary
    print_summary(&counts, skip_hash_validation);

    Ok(())
}

/// Parse and validate the JSONL header line.
fn parse_header(line: &str) -> Result<ExportHeader, Box<dyn std::error::Error>> {
    let header: ExportHeader =
        serde_json::from_str(line).map_err(|e| format!("invalid header line: {e}"))?;

    if !header._header {
        return Err("header line: _header must be true".into());
    }

    Ok(header)
}

/// Pre-flight checks: DB empty check, PID file warning.
fn check_preflight(
    conn: &Connection,
    force: bool,
    paths: &project::ProjectPaths,
) -> Result<(), Box<dyn std::error::Error>> {
    let entry_count: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;

    if entry_count > 0 && !force {
        return Err(format!(
            "database is not empty ({} entries). Use --force to drop existing data, or use a fresh --project-dir.",
            entry_count
        )
        .into());
    }

    // PID file check -- warning only, do not block (SR-07)
    if paths.pid_path.exists() {
        eprintln!(
            "WARNING: PID file exists at {}. A server may be running. Consider stopping it before import.",
            paths.pid_path.display()
        );
    }

    Ok(())
}

/// Drop all data from 8 importable tables + vector_map.
///
/// Uses DELETE (not DROP TABLE) to preserve schema.
/// FK-dependent tables deleted first, then parent tables.
fn drop_all_data(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute_batch(
        "DELETE FROM entry_tags;
         DELETE FROM co_access;
         DELETE FROM feature_entries;
         DELETE FROM outcome_index;
         DELETE FROM audit_log;
         DELETE FROM agent_registry;
         DELETE FROM vector_map;
         DELETE FROM entries;
         DELETE FROM counters;",
    )?;
    Ok(())
}

/// Ingest JSONL data lines into the database.
///
/// Reads lines one-by-one, deserializes via `ExportRow`, and routes to
/// per-table INSERT functions. Tracks counts for progress reporting.
fn ingest_rows(
    conn: &Connection,
    lines: impl Iterator<Item = io::Result<String>>,
) -> Result<ImportCounts, Box<dyn std::error::Error>> {
    let mut counts = ImportCounts::default();
    let mut line_number: u64 = 1; // header was line 1

    for line_result in lines {
        line_number += 1;
        let line = line_result.map_err(|e| format!("I/O error reading line {line_number}: {e}"))?;

        if line.is_empty() {
            continue;
        }

        let row: ExportRow = serde_json::from_str(&line)
            .map_err(|e| format!("JSON parse error on line {line_number}: {e}"))?;

        match row {
            ExportRow::Counter(r) => {
                insert_counter(conn, &r)?;
                counts.counters += 1;
            }
            ExportRow::Entry(r) => {
                insert_entry(conn, &r)?;
                counts.entries += 1;
                if counts.entries % 100 == 0 {
                    eprintln!("  Inserted {} entries...", counts.entries);
                }
            }
            ExportRow::EntryTag(r) => {
                insert_entry_tag(conn, &r)?;
                counts.entry_tags += 1;
            }
            ExportRow::CoAccess(r) => {
                insert_co_access(conn, &r)?;
                counts.co_access += 1;
            }
            ExportRow::FeatureEntry(r) => {
                insert_feature_entry(conn, &r)?;
                counts.feature_entries += 1;
            }
            ExportRow::OutcomeIndex(r) => {
                insert_outcome_index(conn, &r)?;
                counts.outcome_index += 1;
            }
            ExportRow::AgentRegistry(r) => {
                insert_agent_registry(conn, &r)?;
                counts.agent_registry += 1;
            }
            ExportRow::AuditLog(r) => {
                insert_audit_log(conn, &r)?;
                counts.audit_log += 1;
            }
        }
    }

    eprintln!("  Inserted {} entries", counts.entries);
    Ok(counts)
}

// ---------------------------------------------------------------------------
// Hash validation
// ---------------------------------------------------------------------------

/// Validate content hashes and chain integrity for all imported entries.
///
/// Content hash: recompute via `compute_content_hash()` and compare.
/// Chain integrity: verify `previous_hash` references an existing entry's hash.
fn validate_hashes(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let mut errors: Vec<String> = Vec::new();

    let mut stmt = conn.prepare(
        "SELECT id, title, content, content_hash, previous_hash FROM entries ORDER BY id",
    )?;

    let mut known_hashes: HashSet<String> = HashSet::new();
    let mut entries_to_check: Vec<(i64, String, String, String, String)> = Vec::new();

    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let title: String = row.get(1)?;
        let content: String = row.get(2)?;
        let content_hash: String = row.get(3)?;
        let previous_hash: String = row.get(4)?;

        known_hashes.insert(content_hash.clone());
        entries_to_check.push((id, title, content, content_hash, previous_hash));
    }
    drop(rows);
    drop(stmt);

    for (id, title, content, stored_hash, previous_hash) in &entries_to_check {
        // Content hash validation
        let computed = compute_content_hash(title, content);
        if computed != *stored_hash {
            errors.push(format!(
                "content hash mismatch for entry {id}: computed={computed}, stored={stored_hash}"
            ));
        }

        // Chain integrity validation
        if !previous_hash.is_empty() && !known_hashes.contains(previous_hash) {
            errors.push(format!(
                "broken hash chain for entry {id}: previous_hash '{previous_hash}' not found in imported entries"
            ));
        }
    }

    if !errors.is_empty() {
        let msg = format!("hash validation failed:\n{}", errors.join("\n"));
        return Err(msg.into());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Provenance and summary
// ---------------------------------------------------------------------------

/// Record an audit log entry documenting the import operation.
fn record_provenance(
    store: &Store,
    input_path: &Path,
    counts: &ImportCounts,
) -> Result<(), Box<dyn std::error::Error>> {
    let conn = store.lock_conn();

    let next_event_id: i64 = conn.query_row(
        "SELECT COALESCE(MAX(event_id), 0) + 1 FROM audit_log",
        [],
        |row| row.get(0),
    )?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let detail = format!(
        "Imported from '{}': {} entries, {} tags, {} co-access pairs, {} counters",
        input_path.display(),
        counts.entries,
        counts.entry_tags,
        counts.co_access,
        counts.counters
    );

    conn.execute(
        "INSERT INTO audit_log (
            event_id, timestamp, session_id, agent_id,
            operation, target_ids, outcome, detail
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            next_event_id,
            now,
            "import",
            "system",
            "import",
            "[]",
            1i64,
            detail,
        ],
    )?;

    Ok(())
}

/// Print import summary to stderr.
fn print_summary(counts: &ImportCounts, skip_hash_validation: bool) {
    eprintln!("Import complete:");
    eprintln!("  Counters:        {}", counts.counters);
    eprintln!("  Entries:         {}", counts.entries);
    eprintln!("  Entry tags:      {}", counts.entry_tags);
    eprintln!("  Co-access pairs: {}", counts.co_access);
    eprintln!("  Feature entries: {}", counts.feature_entries);
    eprintln!("  Outcome index:   {}", counts.outcome_index);
    eprintln!("  Agent registry:  {}", counts.agent_registry);
    eprintln!("  Audit log:       {}", counts.audit_log);

    if skip_hash_validation {
        eprintln!("  Hash validation: SKIPPED");
    } else {
        eprintln!("  Hash validation: PASSED");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    // --- Helpers ---

    /// Create a project dir and return (project_dir, db_path).
    fn setup_project() -> (TempDir, std::path::PathBuf) {
        let project_dir = TempDir::new().expect("create project temp dir");
        let paths = project::ensure_data_directory(Some(project_dir.path()), None).unwrap();
        (project_dir, paths.db_path)
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
        let mut f = File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
        path
    }

    /// Get the current schema version from a fresh Store.
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

    // --- Header Validation ---

    #[test]
    fn test_validate_header_valid() {
        let json = make_header(11, 1, 5);
        let h = parse_header(&json).unwrap();
        assert!(h._header);
        assert_eq!(h.format_version, 1);
        assert_eq!(h.schema_version, 11);
    }

    #[test]
    fn test_validate_header_bad_format_version() {
        let (project_dir, db_path) = setup_project();
        let sv = get_schema_version(&db_path);
        let output_dir = TempDir::new().unwrap();
        let lines = vec![make_header(sv, 2, 0)];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(Some(project_dir.path()), &input_path, false, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("2"), "should mention version 2: {err}");
        assert!(err.contains("format"), "should mention format: {err}");
    }

    #[test]
    fn test_validate_header_future_schema_version() {
        let (project_dir, db_path) = setup_project();
        let _sv = get_schema_version(&db_path);
        let output_dir = TempDir::new().unwrap();
        let lines = vec![make_header(999, 1, 0)];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(Some(project_dir.path()), &input_path, false, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.to_lowercase().contains("upgrade"),
            "should suggest upgrade: {err}"
        );
    }

    #[test]
    fn test_validate_header_missing_header_flag() {
        let json = r#"{"_header":false,"schema_version":11,"exported_at":1,"entry_count":0,"format_version":1}"#;
        let result = parse_header(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("_header"), "should mention _header: {err}");
    }

    #[test]
    fn test_validate_header_format_version_zero() {
        let (project_dir, db_path) = setup_project();
        let sv = get_schema_version(&db_path);
        let output_dir = TempDir::new().unwrap();
        let lines = vec![make_header(sv, 0, 0)];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(Some(project_dir.path()), &input_path, false, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("0"), "should mention version 0: {err}");
    }

    // --- Hash Validation ---

    #[test]
    fn test_hash_validation_valid_chain() {
        let (project_dir, db_path) = setup_project();
        let sv = get_schema_version(&db_path);

        let hash_a = compute_content_hash("Entry A", "Content A");
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 2),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 3),
            make_entry_line(1, "Entry A", "Content A", ""),
            make_entry_line(2, "Entry B", "Content B", &hash_a),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(Some(project_dir.path()), &input_path, false, false);
        assert!(result.is_ok(), "valid chain should pass: {result:?}");
    }

    #[test]
    fn test_hash_validation_broken_chain() {
        let (project_dir, db_path) = setup_project();
        let sv = get_schema_version(&db_path);

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, "Entry A", "Content A", "nonexistent_hash"),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(Some(project_dir.path()), &input_path, false, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("1"), "should mention entry ID: {err}");
        assert!(
            err.contains("nonexistent_hash"),
            "should mention broken hash: {err}"
        );
    }

    #[test]
    fn test_hash_validation_content_mismatch() {
        let (project_dir, db_path) = setup_project();
        let sv = get_schema_version(&db_path);

        // Build entry with wrong content_hash
        let mut entry_json: serde_json::Value =
            serde_json::from_str(&make_entry_line(1, "Title", "Content", "")).unwrap();
        entry_json["content_hash"] = serde_json::Value::String("wrong_hash".to_string());

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            entry_json.to_string(),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(Some(project_dir.path()), &input_path, false, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("1"), "should mention entry ID: {err}");
        assert!(err.contains("mismatch"), "should mention mismatch: {err}");
    }

    #[test]
    fn test_hash_validation_empty_previous_hash() {
        let (project_dir, db_path) = setup_project();
        let sv = get_schema_version(&db_path);

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, "Entry A", "Content A", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(Some(project_dir.path()), &input_path, false, false);
        assert!(
            result.is_ok(),
            "empty previous_hash should pass: {result:?}"
        );
    }

    #[test]
    fn test_hash_validation_empty_title_edge_case() {
        let (project_dir, db_path) = setup_project();
        let sv = get_schema_version(&db_path);

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, "", "some text", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(Some(project_dir.path()), &input_path, false, false);
        assert!(result.is_ok(), "empty title should pass: {result:?}");
    }

    #[test]
    fn test_hash_validation_empty_both() {
        let (project_dir, db_path) = setup_project();
        let sv = get_schema_version(&db_path);

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, "", "", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(Some(project_dir.path()), &input_path, false, false);
        assert!(
            result.is_ok(),
            "empty title+content should pass: {result:?}"
        );
    }

    // --- Malformed Input ---

    #[test]
    fn test_malformed_jsonl_line_with_line_number() {
        let (project_dir, db_path) = setup_project();
        let sv = get_schema_version(&db_path);

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 3),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 4),
            make_entry_line(1, "A", "A", ""),
            "THIS IS NOT VALID JSON".to_string(), // line 5
            make_entry_line(3, "C", "C", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(Some(project_dir.path()), &input_path, false, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("line 5"), "should mention line 5: {err}");
    }

    #[test]
    fn test_empty_file_errors() {
        let (project_dir, _db_path) = setup_project();
        let output_dir = TempDir::new().unwrap();
        let path = output_dir.path().join("empty.jsonl");
        File::create(&path).unwrap();

        let result = run_import(Some(project_dir.path()), &path, false, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("empty") || err.contains("header"),
            "should mention empty/header: {err}"
        );
    }

    #[test]
    fn test_header_only_file() {
        let (project_dir, db_path) = setup_project();
        let sv = get_schema_version(&db_path);

        let output_dir = TempDir::new().unwrap();
        let lines = vec![make_header(sv, 1, 0)];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(Some(project_dir.path()), &input_path, false, false);
        assert!(
            result.is_ok(),
            "header-only should be valid empty import: {result:?}"
        );
    }

    // --- SQL Injection Prevention ---

    #[test]
    fn test_sql_injection_in_title() {
        let (project_dir, db_path) = setup_project();
        let sv = get_schema_version(&db_path);

        let malicious_title = "'; DROP TABLE entries; --";
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, malicious_title, "safe content", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(
            Some(project_dir.path()),
            &input_path,
            true, // skip hash -- hash won't match the SQL injection string
            false,
        );
        assert!(
            result.is_ok(),
            "SQL injection in title should be safe: {result:?}"
        );

        // Verify entry exists with the literal string
        let store = Store::open(&db_path).unwrap();
        let conn = store.lock_conn();
        let title: String = conn
            .query_row("SELECT title FROM entries WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, malicious_title);

        // Verify entries table still exists
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_sql_injection_in_content() {
        let (project_dir, db_path) = setup_project();
        let sv = get_schema_version(&db_path);

        let malicious = "Robert'); DROP TABLE entries;--";
        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 1),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 2),
            make_entry_line(1, "Safe title", malicious, ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(Some(project_dir.path()), &input_path, true, false);
        assert!(
            result.is_ok(),
            "SQL injection in content should be safe: {result:?}"
        );

        let store = Store::open(&db_path).unwrap();
        let conn = store.lock_conn();
        let content: String = conn
            .query_row("SELECT content FROM entries WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(content, malicious);
    }

    #[test]
    fn test_duplicate_entry_ids() {
        let (project_dir, db_path) = setup_project();
        let sv = get_schema_version(&db_path);

        let output_dir = TempDir::new().unwrap();
        let lines = vec![
            make_header(sv, 1, 2),
            make_counter_line("schema_version", sv),
            make_counter_line("next_entry_id", 3),
            make_entry_line(1, "First", "First content", ""),
            make_entry_line(1, "Duplicate", "Duplicate content", ""),
        ];
        let input_path = write_jsonl(&output_dir, &lines);

        let result = run_import(Some(project_dir.path()), &input_path, true, false);
        assert!(result.is_err(), "duplicate PK should fail");
    }
}
