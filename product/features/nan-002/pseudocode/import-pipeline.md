# nan-002: import-pipeline -- Pseudocode

## Purpose

Implement the full import pipeline in `import.rs`: header validation, pre-flight checks, JSONL ingestion via direct SQL INSERT, hash validation, and transaction management. This is the core of nan-002.

## File Created

- `crates/unimatrix-server/src/import.rs`

## Module Structure (target ~400 lines, under 500-line limit)

```
import.rs
  run_import()                    -- public entry point
  parse_header()                  -- header line parsing + validation
  check_preflight()               -- DB empty check, PID file warning
  drop_all_data()                 -- --force data clearing
  ingest_rows()                   -- JSONL line-by-line ingestion loop
  insert_counter()                -- per-table INSERT functions
  insert_entry()
  insert_entry_tag()
  insert_co_access()
  insert_feature_entry()
  insert_outcome_index()
  insert_agent_registry()
  insert_audit_log()
  validate_hashes()               -- content hash + chain validation
  record_provenance()             -- audit log provenance entry
  print_summary()                 -- stderr summary
```

If the file exceeds ~450 lines during implementation, split the 8 `insert_*` functions into a private submodule `import/inserters.rs`.

## run_import() -- Public Entry Point

```
pub fn run_import(
    project_dir: Option<&Path>,
    input: &Path,
    skip_hash_validation: bool,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>>

FUNCTION BODY:

    // Phase 1: Setup
    let paths = project::ensure_data_directory(project_dir, None)?
    let store = Arc::new(Store::open(&paths.db_path)?)

    // Phase 2: Open and parse header
    let file = File::open(input)?    // fail fast if file missing
    let reader = BufReader::new(file)
    let mut lines = reader.lines()

    let header_line = lines.next()
        .ok_or("empty file: no header line")??
    let header = parse_header(&header_line)?

    // Phase 3: Pre-flight checks
    let conn = store.lock_conn()
    let db_schema_version = conn.query_row(
        "SELECT value FROM counters WHERE name = 'schema_version'",
        [], |row| row.get::<_, i64>(0)
    )?
    check_preflight(&conn, force, &paths)?

    // Phase 4: Validate header against DB
    if header.format_version != 1 {
        return Err(format!("unsupported format_version: {}. Only format_version 1 is supported.", header.format_version))
    }
    if header.schema_version > db_schema_version {
        return Err(format!(
            "export schema_version ({}) is newer than this binary's schema_version ({}). Upgrade unimatrix-server.",
            header.schema_version, db_schema_version
        ))
    }

    // Phase 5: Force-drop if needed (before transaction, needs its own writes)
    if force {
        let entry_count = conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get::<_, i64>(0))?
        if entry_count > 0 {
            eprintln!("WARNING: --force specified. Dropping {} existing entries and all associated data in {}.",
                entry_count, paths.data_dir.display())
        }
        drop_all_data(&conn)?
    }

    // Phase 6: BEGIN IMMEDIATE transaction
    conn.execute_batch("BEGIN IMMEDIATE")?

    // Phase 7: Ingest JSONL
    let counts = match ingest_rows(&conn, lines, skip_hash_validation) {
        Ok(counts) => counts,
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK")
            return Err(e)
        }
    }

    // Phase 8: Hash validation (inside transaction, before commit)
    if !skip_hash_validation {
        match validate_hashes(&conn) {
            Ok(()) => {},
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK")
                return Err(e)
            }
        }
    } else {
        eprintln!("WARNING: hash validation skipped (--skip-hash-validation)")
    }

    // Phase 9: COMMIT
    conn.execute_batch("COMMIT")?

    // Phase 10: Drop the MutexGuard before embedding (embedding needs store access)
    drop(conn)

    // Phase 11: Re-embed and build vector index (see embedding-reconstruction.md)
    reconstruct_embeddings(&store, &paths)?

    // Phase 12: Record provenance
    record_provenance(&store, input, &counts)?

    // Phase 13: Summary
    print_summary(&counts, skip_hash_validation)

    Ok(())
```

## parse_header()

```
fn parse_header(line: &str) -> Result<ExportHeader, Box<dyn std::error::Error>>

FUNCTION BODY:
    let header: ExportHeader = serde_json::from_str(line)
        .map_err(|e| format!("invalid header line: {e}"))?

    if !header._header {
        return Err("header line: _header must be true")
    }

    Ok(header)
```

## check_preflight()

```
fn check_preflight(
    conn: &Connection,
    force: bool,
    paths: &ProjectPaths,
) -> Result<(), Box<dyn std::error::Error>>

FUNCTION BODY:
    // Check if DB is non-empty
    let entry_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entries", [], |row| row.get(0)
    )?

    if entry_count > 0 && !force {
        return Err(format!(
            "database is not empty ({} entries). Use --force to drop existing data, or use a fresh --project-dir.",
            entry_count
        ))
    }

    // PID file check -- warning only, do not block (SR-07)
    if paths.pid_path.exists() {
        eprintln!("WARNING: PID file exists at {}. A server may be running. Consider stopping it before import.",
            paths.pid_path.display())
    }

    Ok(())
```

## drop_all_data()

```
fn drop_all_data(conn: &Connection) -> Result<(), Box<dyn std::error::Error>>

FUNCTION BODY:
    // Delete from all 8 importable tables + vector_map
    // Order: FK-dependent tables first, then parent tables
    conn.execute_batch("
        DELETE FROM entry_tags;
        DELETE FROM co_access;
        DELETE FROM feature_entries;
        DELETE FROM outcome_index;
        DELETE FROM audit_log;
        DELETE FROM agent_registry;
        DELETE FROM vector_map;
        DELETE FROM entries;
        DELETE FROM counters;
    ")?

    Ok(())
```

Note: `DELETE FROM` (not `DROP TABLE`) preserves the schema. 9 tables deleted (8 importable + vector_map). Order respects FK constraints: child tables before parent tables.

## ImportCounts (tracking struct)

```
struct ImportCounts {
    counters: u64,
    entries: u64,
    entry_tags: u64,
    co_access: u64,
    feature_entries: u64,
    outcome_index: u64,
    agent_registry: u64,
    audit_log: u64,
}
// Initialize all to 0
```

## ingest_rows()

```
fn ingest_rows(
    conn: &Connection,
    lines: impl Iterator<Item = io::Result<String>>,
    skip_hash_validation: bool,
) -> Result<ImportCounts, Box<dyn std::error::Error>>

FUNCTION BODY:
    let mut counts = ImportCounts::default()
    let mut line_number: u64 = 1  // header was line 1

    for line_result in lines {
        line_number += 1
        let line = line_result
            .map_err(|e| format!("I/O error reading line {line_number}: {e}"))?

        let row: ExportRow = serde_json::from_str(&line)
            .map_err(|e| format!("JSON parse error on line {line_number}: {e}"))?

        match row {
            ExportRow::Counter(r) => {
                insert_counter(conn, &r)?
                counts.counters += 1
            }
            ExportRow::Entry(r) => {
                insert_entry(conn, &r)?
                counts.entries += 1
                // Progress: report every 100 entries
                if counts.entries % 100 == 0 {
                    eprintln!("  Inserted {} entries...", counts.entries)
                }
            }
            ExportRow::EntryTag(r) => {
                insert_entry_tag(conn, &r)?
                counts.entry_tags += 1
            }
            ExportRow::CoAccess(r) => {
                insert_co_access(conn, &r)?
                counts.co_access += 1
            }
            ExportRow::FeatureEntry(r) => {
                insert_feature_entry(conn, &r)?
                counts.feature_entries += 1
            }
            ExportRow::OutcomeIndex(r) => {
                insert_outcome_index(conn, &r)?
                counts.outcome_index += 1
            }
            ExportRow::AgentRegistry(r) => {
                insert_agent_registry(conn, &r)?
                counts.agent_registry += 1
            }
            ExportRow::AuditLog(r) => {
                insert_audit_log(conn, &r)?
                counts.audit_log += 1
            }
        }
    }

    eprintln!("  Inserted {} entries", counts.entries)

    Ok(counts)
```

## Per-Table INSERT Functions

All use `rusqlite::params![]` for parameterized queries. No string interpolation.

### insert_counter()

```
fn insert_counter(conn: &Connection, r: &CounterRow) -> Result<(), Box<dyn std::error::Error>>

    conn.execute(
        "INSERT OR REPLACE INTO counters (name, value) VALUES (?1, ?2)",
        params![r.name, r.value]
    )?
    Ok(())
```

`INSERT OR REPLACE` handles counters auto-initialized by `Store::open()`.

### insert_entry()

```
fn insert_entry(conn: &Connection, r: &EntryRow) -> Result<(), Box<dyn std::error::Error>>

    conn.execute(
        "INSERT INTO entries (
            id, title, content, topic, category, source, status, confidence,
            created_at, updated_at, last_accessed_at, access_count,
            supersedes, superseded_by, correction_count, embedding_dim,
            created_by, modified_by, content_hash, previous_hash,
            version, feature_cycle, trust_source,
            helpful_count, unhelpful_count, pre_quarantine_status
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
            ?9, ?10, ?11, ?12,
            ?13, ?14, ?15, ?16,
            ?17, ?18, ?19, ?20,
            ?21, ?22, ?23,
            ?24, ?25, ?26
        )",
        params![
            r.id, r.title, r.content, r.topic, r.category, r.source, r.status, r.confidence,
            r.created_at, r.updated_at, r.last_accessed_at, r.access_count,
            r.supersedes, r.superseded_by, r.correction_count, r.embedding_dim,
            r.created_by, r.modified_by, r.content_hash, r.previous_hash,
            r.version, r.feature_cycle, r.trust_source,
            r.helpful_count, r.unhelpful_count, r.pre_quarantine_status,
        ]
    )?
    Ok(())
```

All 26 columns. `Option<i64>` fields (`supersedes`, `superseded_by`, `pre_quarantine_status`) serialize to SQL NULL when None via rusqlite's `params!` macro.

### insert_entry_tag()

```
fn insert_entry_tag(conn: &Connection, r: &EntryTagRow) -> Result<(), Box<dyn std::error::Error>>

    conn.execute(
        "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
        params![r.entry_id, r.tag]
    )?
    Ok(())
```

### insert_co_access()

```
fn insert_co_access(conn: &Connection, r: &CoAccessRow) -> Result<(), Box<dyn std::error::Error>>

    conn.execute(
        "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (?1, ?2, ?3, ?4)",
        params![r.entry_id_a, r.entry_id_b, r.count, r.last_updated]
    )?
    Ok(())
```

### insert_feature_entry()

```
fn insert_feature_entry(conn: &Connection, r: &FeatureEntryRow) -> Result<(), Box<dyn std::error::Error>>

    conn.execute(
        "INSERT INTO feature_entries (feature_id, entry_id) VALUES (?1, ?2)",
        params![r.feature_id, r.entry_id]
    )?
    Ok(())
```

Note: DDL column is `feature_id`, struct field is `feature_id`. Matches export JSON key.

### insert_outcome_index()

```
fn insert_outcome_index(conn: &Connection, r: &OutcomeIndexRow) -> Result<(), Box<dyn std::error::Error>>

    conn.execute(
        "INSERT INTO outcome_index (feature_cycle, entry_id) VALUES (?1, ?2)",
        params![r.feature_cycle, r.entry_id]
    )?
    Ok(())
```

### insert_agent_registry()

```
fn insert_agent_registry(conn: &Connection, r: &AgentRegistryRow) -> Result<(), Box<dyn std::error::Error>>

    conn.execute(
        "INSERT INTO agent_registry (
            agent_id, trust_level, capabilities, allowed_topics,
            allowed_categories, enrolled_at, last_seen_at, active
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            r.agent_id, r.trust_level, r.capabilities, r.allowed_topics,
            r.allowed_categories, r.enrolled_at, r.last_seen_at, r.active,
        ]
    )?
    Ok(())
```

`allowed_topics` and `allowed_categories` are `Option<String>` -- None becomes SQL NULL.

### insert_audit_log()

```
fn insert_audit_log(conn: &Connection, r: &AuditLogRow) -> Result<(), Box<dyn std::error::Error>>

    conn.execute(
        "INSERT INTO audit_log (
            event_id, timestamp, session_id, agent_id,
            operation, target_ids, outcome, detail
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            r.event_id, r.timestamp, r.session_id, r.agent_id,
            r.operation, r.target_ids, r.outcome, r.detail,
        ]
    )?
    Ok(())
```

## validate_hashes()

```
fn validate_hashes(conn: &Connection) -> Result<(), Box<dyn std::error::Error>>

FUNCTION BODY:
    let mut errors: Vec<String> = Vec::new()

    // Query all entries for hash validation
    let mut stmt = conn.prepare(
        "SELECT id, title, content, content_hash, previous_hash FROM entries ORDER BY id"
    )?

    // Collect all content hashes for chain validation
    let mut known_hashes: HashSet<String> = HashSet::new()
    let mut entries_to_check: Vec<(i64, String, String, String, String)> = Vec::new()

    let mut rows = stmt.query([])?
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?
        let title: String = row.get(1)?
        let content: String = row.get(2)?
        let content_hash: String = row.get(3)?
        let previous_hash: String = row.get(4)?

        known_hashes.insert(content_hash.clone())
        entries_to_check.push((id, title, content, content_hash, previous_hash))
    }
    drop(rows)
    drop(stmt)

    for (id, title, content, stored_hash, previous_hash) in &entries_to_check {
        // Content hash validation
        let computed = compute_content_hash(title, content)
        if computed != *stored_hash {
            errors.push(format!(
                "content hash mismatch for entry {id}: computed={computed}, stored={stored_hash}"
            ))
        }

        // Chain integrity validation
        if !previous_hash.is_empty() && !known_hashes.contains(previous_hash) {
            errors.push(format!(
                "broken hash chain for entry {id}: previous_hash '{previous_hash}' not found in imported entries"
            ))
        }
    }

    if !errors.is_empty() {
        let msg = format!("hash validation failed:\n{}", errors.join("\n"))
        return Err(msg.into())
    }

    Ok(())
```

## record_provenance()

```
fn record_provenance(
    store: &Store,
    input_path: &Path,
    counts: &ImportCounts,
) -> Result<(), Box<dyn std::error::Error>>

FUNCTION BODY:
    let conn = store.lock_conn()

    // Get next event_id from counters (restored counters ensure no collision)
    // If no next_event_id counter exists, use MAX(event_id) + 1 from audit_log
    let next_event_id: i64 = conn.query_row(
        "SELECT COALESCE(MAX(event_id), 0) + 1 FROM audit_log",
        [], |row| row.get(0)
    )?

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64

    let detail = format!(
        "Imported from '{}': {} entries, {} tags, {} co-access pairs, {} counters",
        input_path.display(),
        counts.entries, counts.entry_tags, counts.co_access, counts.counters
    )

    conn.execute(
        "INSERT INTO audit_log (
            event_id, timestamp, session_id, agent_id,
            operation, target_ids, outcome, detail
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            next_event_id,
            now,
            "import",           // session_id
            "system",           // agent_id
            "import",           // operation
            "[]",               // target_ids (JSON array, empty)
            1i64,               // outcome: success
            detail,
        ]
    )?

    Ok(())
```

## print_summary()

```
fn print_summary(counts: &ImportCounts, skip_hash_validation: bool)

FUNCTION BODY:
    eprintln!("Import complete:")
    eprintln!("  Counters:        {}", counts.counters)
    eprintln!("  Entries:         {}", counts.entries)
    eprintln!("  Entry tags:      {}", counts.entry_tags)
    eprintln!("  Co-access pairs: {}", counts.co_access)
    eprintln!("  Feature entries: {}", counts.feature_entries)
    eprintln!("  Outcome index:   {}", counts.outcome_index)
    eprintln!("  Agent registry:  {}", counts.agent_registry)
    eprintln!("  Audit log:       {}", counts.audit_log)

    if skip_hash_validation {
        eprintln!("  Hash validation: SKIPPED")
    } else {
        eprintln!("  Hash validation: PASSED")
    }
```

## Error Handling

| Error Source | Function | Behavior |
|---|---|---|
| File not found | `run_import` (File::open) | Propagate io::Error, exit 1 |
| Empty file | `run_import` (lines.next()) | "empty file: no header line", exit 1 |
| Invalid header JSON | `parse_header` | "invalid header line: {serde error}", exit 1 |
| format_version != 1 | `run_import` | "unsupported format_version: N", exit 1 |
| schema_version > current | `run_import` | "export schema_version (N) is newer... Upgrade unimatrix-server.", exit 1 |
| Non-empty DB without --force | `check_preflight` | "database is not empty (N entries)...", exit 1 |
| JSON parse error on line N | `ingest_rows` | "JSON parse error on line N: {serde error}", ROLLBACK, exit 1 |
| SQL FK/PK violation | `insert_*` functions | rusqlite error propagated, ROLLBACK, exit 1 |
| Hash mismatch | `validate_hashes` | Lists all mismatches, ROLLBACK, exit 1 |

All errors that occur inside the transaction trigger explicit ROLLBACK before returning.

## Key Test Scenarios

1. Full round-trip: export populated DB, import into fresh DB, re-export. Compare output (excluding `exported_at`).
2. `--force` on populated DB: old entries gone, new entries present, correct entry count warning on stderr.
3. Non-empty DB without `--force`: rejected with actionable error.
4. `--force` on empty DB: proceeds without error (no-op drop).
5. Header with `format_version: 2`: rejected with error naming version 2.
6. Header with `schema_version: 999`: rejected with upgrade suggestion.
7. Malformed JSON on line 5: error message includes "line 5", transaction rolled back.
8. Content hash mismatch: validation fails with entry ID.
9. Broken chain: validation fails with entry ID and unresolved hash.
10. `--skip-hash-validation` with tampered content: import succeeds with warning on stderr.
11. All 26 entry columns preserved exactly through import (per-column comparison).
12. Counter restoration: post-import insert gets ID > max imported ID.
13. FK violation (entry_tags before entries): SQL error, transaction rolled back.
14. Empty export (header + counters only): valid empty database with correct counters.
15. SQL injection in entry title `'; DROP TABLE entries; --`: parameterized queries prevent execution.
16. Audit provenance entry written with event_id that does not collide with imported audit entries.
17. PID file exists: warning emitted to stderr, import proceeds.
18. `--project-dir` respected: database created at specified path.
