//! Import path: JSON-lines intermediate file -> SQLite database (ADR-004).
//!
//! Only compiled under the SQLite backend (`#[cfg(feature = "backend-sqlite")]`).
//! Creates the database via `Store::open()` for correct schema, then inserts
//! data using raw SQL on the underlying connection.

use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use super::format::{DataRow, KeyType, TableHeader, ValueType, decode_blob, read_line, validate_i64_range};
use super::{ALL_TABLES, MigrateError, MigrationSummary};

/// Import all tables from a JSON-lines file into a new SQLite database.
///
/// Creates the database via `Store::open()` (ADR-004) to get correct schema
/// and PRAGMAs, then inserts data using direct SQL on the underlying connection.
pub fn import(input_path: &Path, output_path: &Path) -> Result<MigrationSummary, MigrateError> {
    // Precondition: output must not exist
    if output_path.exists() {
        return Err(MigrateError::Validation(format!(
            "output file already exists: {}",
            output_path.display()
        )));
    }

    // Create database with correct schema via Store::open()
    let store = crate::sqlite::db::Store::open(output_path)?;

    // Clear auto-initialized counters (Store::open sets defaults like next_entry_id=1).
    // These will be overwritten by imported values.
    {
        let conn = store.lock_conn();
        conn.execute("DELETE FROM counters", [])
            .map_err(crate::error::StoreError::Sqlite)?;
    }

    // Read and parse intermediate file
    let file = File::open(input_path)?;
    let mut reader = BufReader::new(file);
    let mut summary = MigrationSummary::new();
    let mut tables_seen = HashSet::new();

    loop {
        // Read next line -- should be a table header or EOF
        let line = match read_line(&mut reader)? {
            Some(v) => v,
            None => break,
        };

        // Parse as TableHeader
        let header: TableHeader = serde_json::from_value(line)?;
        tables_seen.insert(header.table.clone());

        // Import all rows for this table in a transaction
        let conn = store.lock_conn();
        conn.execute_batch("BEGIN")
            .map_err(crate::error::StoreError::Sqlite)?;

        let mut actual_count = 0u64;
        for _ in 0..header.row_count {
            let row_line = read_line(&mut reader)?.ok_or_else(|| {
                MigrateError::Validation(format!(
                    "unexpected EOF in table {}, expected {} rows, got {}",
                    header.table, header.row_count, actual_count
                ))
            })?;
            let row: DataRow = serde_json::from_value(row_line)?;

            insert_row(&conn, &header, &row)?;
            actual_count += 1;
        }

        conn.execute_batch("COMMIT")
            .map_err(crate::error::StoreError::Sqlite)?;
        drop(conn);

        if actual_count != header.row_count {
            // Delete partial database on error
            let _ = std::fs::remove_file(output_path);
            return Err(MigrateError::RowCountMismatch {
                table: header.table,
                expected: header.row_count,
                actual: actual_count,
            });
        }

        summary.add(&header.table, actual_count);
    }

    // Verify all 17 tables were present in the file
    for desc in ALL_TABLES {
        if !tables_seen.contains(desc.name()) {
            let _ = std::fs::remove_file(output_path);
            return Err(MigrateError::Validation(format!(
                "missing table in intermediate file: {}",
                desc.name()
            )));
        }
    }

    // Post-import verification
    if let Err(e) = verify_import(&store) {
        let _ = std::fs::remove_file(output_path);
        return Err(e);
    }

    Ok(summary)
}

/// Insert a single row into the appropriate SQLite table via raw SQL.
fn insert_row(
    conn: &rusqlite::Connection,
    header: &TableHeader,
    row: &DataRow,
) -> Result<(), MigrateError> {
    match (&header.key_type, &header.value_type, header.multimap) {
        // u64 + blob: entries, audit_log, signal_queue, injection_log
        (KeyType::U64, ValueType::Blob, false) => {
            let key = row
                .key
                .as_u64()
                .ok_or_else(|| MigrateError::Validation("expected u64 key".into()))?;
            validate_i64_range(key)?;
            let blob = decode_blob(
                row.value
                    .as_str()
                    .ok_or_else(|| MigrateError::Validation("expected base64 string value".into()))?,
            )?;
            let sql = match header.table.as_str() {
                "entries" => "INSERT INTO entries (id, data) VALUES (?1, ?2)",
                "audit_log" => "INSERT INTO audit_log (event_id, data) VALUES (?1, ?2)",
                "signal_queue" => "INSERT INTO signal_queue (signal_id, data) VALUES (?1, ?2)",
                "injection_log" => "INSERT INTO injection_log (log_id, data) VALUES (?1, ?2)",
                other => {
                    return Err(MigrateError::Validation(format!(
                        "unexpected U64Blob table: {other}"
                    )))
                }
            };
            conn.execute(sql, rusqlite::params![key as i64, blob])
                .map_err(crate::error::StoreError::Sqlite)?;
        }

        // str + blob: agent_registry, observation_metrics, sessions
        (KeyType::Str, ValueType::Blob, false) => {
            let key = row
                .key
                .as_str()
                .ok_or_else(|| MigrateError::Validation("expected string key".into()))?;
            let blob = decode_blob(
                row.value
                    .as_str()
                    .ok_or_else(|| MigrateError::Validation("expected base64 string value".into()))?,
            )?;
            let sql = match header.table.as_str() {
                "agent_registry" => {
                    "INSERT INTO agent_registry (agent_id, data) VALUES (?1, ?2)"
                }
                "observation_metrics" => {
                    "INSERT INTO observation_metrics (feature_cycle, data) VALUES (?1, ?2)"
                }
                "sessions" => "INSERT INTO sessions (session_id, data) VALUES (?1, ?2)",
                other => {
                    return Err(MigrateError::Validation(format!(
                        "unexpected StrBlob table: {other}"
                    )))
                }
            };
            conn.execute(sql, rusqlite::params![key, blob])
                .map_err(crate::error::StoreError::Sqlite)?;
        }

        // str + u64 (non-multimap): counters
        (KeyType::Str, ValueType::U64, false) => {
            let key = row
                .key
                .as_str()
                .ok_or_else(|| MigrateError::Validation("expected string key".into()))?;
            let val = row
                .value
                .as_u64()
                .ok_or_else(|| MigrateError::Validation("expected u64 value".into()))?;
            validate_i64_range(val)?;
            conn.execute(
                "INSERT OR REPLACE INTO counters (name, value) VALUES (?1, ?2)",
                rusqlite::params![key, val as i64],
            )
            .map_err(crate::error::StoreError::Sqlite)?;
        }

        // (str, u64) + unit: topic_index, category_index, outcome_index
        (KeyType::StrU64, ValueType::Unit, false) => {
            let arr = row
                .key
                .as_array()
                .ok_or_else(|| MigrateError::Validation("expected array key".into()))?;
            if arr.len() != 2 {
                return Err(MigrateError::Validation(format!(
                    "expected 2-element array key, got {}",
                    arr.len()
                )));
            }
            let s = arr[0]
                .as_str()
                .ok_or_else(|| MigrateError::Validation("expected string in key[0]".into()))?;
            let id = arr[1]
                .as_u64()
                .ok_or_else(|| MigrateError::Validation("expected u64 in key[1]".into()))?;
            validate_i64_range(id)?;
            let sql = match header.table.as_str() {
                "topic_index" => {
                    "INSERT INTO topic_index (topic, entry_id) VALUES (?1, ?2)"
                }
                "category_index" => {
                    "INSERT INTO category_index (category, entry_id) VALUES (?1, ?2)"
                }
                "outcome_index" => {
                    "INSERT INTO outcome_index (feature_cycle, entry_id) VALUES (?1, ?2)"
                }
                other => {
                    return Err(MigrateError::Validation(format!(
                        "unexpected StrU64Unit table: {other}"
                    )))
                }
            };
            conn.execute(sql, rusqlite::params![s, id as i64])
                .map_err(crate::error::StoreError::Sqlite)?;
        }

        // (u64, u64) + unit: time_index
        (KeyType::U64U64, ValueType::Unit, false) => {
            let arr = row
                .key
                .as_array()
                .ok_or_else(|| MigrateError::Validation("expected array key".into()))?;
            if arr.len() != 2 {
                return Err(MigrateError::Validation(format!(
                    "expected 2-element array key, got {}",
                    arr.len()
                )));
            }
            let k0 = arr[0]
                .as_u64()
                .ok_or_else(|| MigrateError::Validation("expected u64 in key[0]".into()))?;
            let k1 = arr[1]
                .as_u64()
                .ok_or_else(|| MigrateError::Validation("expected u64 in key[1]".into()))?;
            validate_i64_range(k0)?;
            validate_i64_range(k1)?;
            conn.execute(
                "INSERT INTO time_index (timestamp, entry_id) VALUES (?1, ?2)",
                rusqlite::params![k0 as i64, k1 as i64],
            )
            .map_err(crate::error::StoreError::Sqlite)?;
        }

        // (u8, u64) + unit: status_index
        (KeyType::U8U64, ValueType::Unit, false) => {
            let arr = row
                .key
                .as_array()
                .ok_or_else(|| MigrateError::Validation("expected array key".into()))?;
            if arr.len() != 2 {
                return Err(MigrateError::Validation(format!(
                    "expected 2-element array key, got {}",
                    arr.len()
                )));
            }
            let k0 = arr[0]
                .as_u64()
                .ok_or_else(|| MigrateError::Validation("expected u64 in key[0]".into()))?;
            let k1 = arr[1]
                .as_u64()
                .ok_or_else(|| MigrateError::Validation("expected u64 in key[1]".into()))?;
            validate_i64_range(k1)?;
            conn.execute(
                "INSERT INTO status_index (status, entry_id) VALUES (?1, ?2)",
                rusqlite::params![k0 as i64, k1 as i64],
            )
            .map_err(crate::error::StoreError::Sqlite)?;
        }

        // u64 + u64: vector_map
        (KeyType::U64, ValueType::U64, false) => {
            let key = row
                .key
                .as_u64()
                .ok_or_else(|| MigrateError::Validation("expected u64 key".into()))?;
            let val = row
                .value
                .as_u64()
                .ok_or_else(|| MigrateError::Validation("expected u64 value".into()))?;
            validate_i64_range(key)?;
            validate_i64_range(val)?;
            conn.execute(
                "INSERT INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)",
                rusqlite::params![key as i64, val as i64],
            )
            .map_err(crate::error::StoreError::Sqlite)?;
        }

        // (u64, u64) + blob: co_access
        (KeyType::U64U64, ValueType::Blob, false) => {
            let arr = row
                .key
                .as_array()
                .ok_or_else(|| MigrateError::Validation("expected array key".into()))?;
            if arr.len() != 2 {
                return Err(MigrateError::Validation(format!(
                    "expected 2-element array key, got {}",
                    arr.len()
                )));
            }
            let k0 = arr[0]
                .as_u64()
                .ok_or_else(|| MigrateError::Validation("expected u64 in key[0]".into()))?;
            let k1 = arr[1]
                .as_u64()
                .ok_or_else(|| MigrateError::Validation("expected u64 in key[1]".into()))?;
            validate_i64_range(k0)?;
            validate_i64_range(k1)?;
            let blob = decode_blob(
                row.value
                    .as_str()
                    .ok_or_else(|| MigrateError::Validation("expected base64 string value".into()))?,
            )?;
            conn.execute(
                "INSERT INTO co_access (entry_id_a, entry_id_b, data) VALUES (?1, ?2, ?3)",
                rusqlite::params![k0 as i64, k1 as i64, blob],
            )
            .map_err(crate::error::StoreError::Sqlite)?;
        }

        // str + u64 multimap: tag_index, feature_entries
        (KeyType::Str, ValueType::U64, true) => {
            let key = row
                .key
                .as_str()
                .ok_or_else(|| MigrateError::Validation("expected string key".into()))?;
            let val = row
                .value
                .as_u64()
                .ok_or_else(|| MigrateError::Validation("expected u64 value".into()))?;
            validate_i64_range(val)?;
            let sql = match header.table.as_str() {
                "tag_index" => {
                    "INSERT OR IGNORE INTO tag_index (tag, entry_id) VALUES (?1, ?2)"
                }
                "feature_entries" => {
                    "INSERT OR IGNORE INTO feature_entries (feature_id, entry_id) VALUES (?1, ?2)"
                }
                other => {
                    return Err(MigrateError::Validation(format!(
                        "unexpected multimap table: {other}"
                    )))
                }
            };
            conn.execute(sql, rusqlite::params![key, val as i64])
                .map_err(crate::error::StoreError::Sqlite)?;
        }

        _ => {
            return Err(MigrateError::Validation(format!(
                "unsupported table type: key={:?} value={:?} multimap={}",
                header.key_type, header.value_type, header.multimap
            )));
        }
    }
    Ok(())
}

/// Post-import verification (FR-02 step 6).
fn verify_import(store: &crate::sqlite::db::Store) -> Result<(), MigrateError> {
    let conn = store.lock_conn();

    // Check schema_version
    let schema_ver: i64 = conn
        .query_row(
            "SELECT value FROM counters WHERE name = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .map_err(crate::error::StoreError::Sqlite)?;
    if schema_ver != 5 {
        return Err(MigrateError::Validation(format!(
            "schema_version is {schema_ver}, expected 5"
        )));
    }

    // Check next_entry_id > MAX(entries.id)
    let max_id: Option<i64> = conn
        .query_row("SELECT MAX(id) FROM entries", [], |row| row.get(0))
        .map_err(crate::error::StoreError::Sqlite)?;
    let next_id: i64 = conn
        .query_row(
            "SELECT value FROM counters WHERE name = 'next_entry_id'",
            [],
            |row| row.get(0),
        )
        .map_err(crate::error::StoreError::Sqlite)?;
    if let Some(max) = max_id {
        if next_id <= max {
            return Err(MigrateError::Validation(format!(
                "next_entry_id ({next_id}) <= MAX(entries.id) ({max})"
            )));
        }
    }

    Ok(())
}
