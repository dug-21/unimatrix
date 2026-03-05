# Pseudocode: migrate-module

## Files

### `crates/unimatrix-store/src/migrate/mod.rs`

```
pub mod format;

#[cfg(not(feature = "backend-sqlite"))]
pub mod export;

#[cfg(feature = "backend-sqlite")]
pub mod import;

/// Classification of each table's key/value schema.
/// Used by export and import to dispatch correct serialization.
enum TableDescriptor {
    U64Blob { name: &'static str },          // entries, audit_log, signal_queue, injection_log
    StrBlob { name: &'static str },          // agent_registry, observation_metrics, sessions
    StrU64 { name: &'static str },           // counters
    StrU64Unit { name: &'static str },       // topic_index, category_index, outcome_index
    U64U64Unit { name: &'static str },       // time_index
    U8U64Unit { name: &'static str },        // status_index
    U64U64 { name: &'static str },           // vector_map
    U64U64Blob { name: &'static str },       // co_access
    MultimapStrU64 { name: &'static str },   // tag_index, feature_entries
}

impl TableDescriptor {
    fn name(&self) -> &'static str { ... }
    fn key_type(&self) -> KeyType { ... }
    fn value_type(&self) -> ValueType { ... }
    fn is_multimap(&self) -> bool { ... }
}

const ALL_TABLES: &[TableDescriptor] = &[
    // exactly 17 entries, matching schema.rs order:
    // entries, topic_index, category_index, tag_index, time_index,
    // status_index, vector_map, counters, agent_registry, audit_log,
    // feature_entries, co_access, outcome_index, observation_metrics,
    // signal_queue, sessions, injection_log
];
```

### `crates/unimatrix-store/src/migrate/format.rs`

```
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
enum KeyType { U64, Str, StrU64, U64U64, U8U64 }

#[derive(Serialize, Deserialize)]
enum ValueType { Blob, U64, Unit }

#[derive(Serialize, Deserialize)]
struct TableHeader {
    table: String,
    key_type: KeyType,
    value_type: ValueType,
    #[serde(default, skip_serializing_if = "is_false")]
    multimap: bool,
    row_count: u64,
}

#[derive(Serialize, Deserialize)]
struct DataRow {
    key: serde_json::Value,
    value: serde_json::Value,
}

/// Write a table header as JSON line to writer
fn write_header(writer: &mut impl Write, header: &TableHeader) -> Result<()> {
    serde_json::to_writer(&mut *writer, header)?;
    writer.write_all(b"\n")?;
    Ok(())
}

/// Write a data row as JSON line to writer
fn write_row(writer: &mut impl Write, row: &DataRow) -> Result<()> {
    serde_json::to_writer(&mut *writer, row)?;
    writer.write_all(b"\n")?;
    Ok(())
}

/// Read one line from reader, parse as JSON
fn read_line(reader: &mut impl BufRead) -> Result<Option<serde_json::Value>> {
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line)?;
    if bytes_read == 0 { return Ok(None); }
    let trimmed = line.trim();
    if trimmed.is_empty() { return Ok(None); }
    let value = serde_json::from_str(trimmed)?;
    Ok(Some(value))
}

/// Encode bytes as standard base64
fn encode_blob(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Decode base64 string to bytes
fn decode_blob(s: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s)
        .map_err(|e| MigrateError::Base64Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    // T-03: base64 round-trip for blob sizes 0, 1, 2, 3, 100, 100000
    // TableHeader serde round-trip
    // DataRow serde round-trip with all key types
}
```

### `crates/unimatrix-store/src/migrate/export.rs`

```
#[cfg(not(feature = "backend-sqlite"))]
// Only compiled under redb backend

use crate::schema::*; // redb table definitions
use super::format::*;
use super::{ALL_TABLES, TableDescriptor};

/// Error type for migration operations.
enum MigrateError {
    Io(io::Error),
    Redb(redb::Error),
    Serde(serde_json::Error),
    Base64Decode(String),
    RowCountMismatch { table: String, expected: u64, actual: u64 },
    Validation(String),
}

/// Export all 17 tables from a redb database to a JSON-lines file.
///
/// Opens the database with redb::Builder::new().create() directly (ADR-003)
/// to avoid running migrations. Read-only operation.
fn export(db_path: &Path, output_path: &Path) -> Result<ExportSummary, MigrateError> {
    let db = redb::Builder::new().create(db_path)?;
    let txn = db.begin_read()?;

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    let mut summary = ExportSummary::new();

    for descriptor in ALL_TABLES {
        match descriptor {
            TableDescriptor::U64Blob { name } => {
                let table = txn.open_table(table_def_for(name))?;
                let count = table.len()?;
                write_header(&mut writer, &make_header(descriptor, count))?;
                for result in table.iter()? {
                    let (key, value) = result?;
                    let k = key.value();
                    let v = value.value();
                    write_row(&mut writer, &DataRow {
                        key: json!(k),
                        value: json!(encode_blob(v)),
                    })?;
                }
                summary.add(name, count);
            }
            TableDescriptor::StrU64Unit { name } => {
                let table = txn.open_table(table_def_for(name))?;
                let count = table.len()?;
                write_header(&mut writer, &make_header(descriptor, count))?;
                for result in table.iter()? {
                    let (key, _) = result?;
                    let (s, id) = key.value();
                    write_row(&mut writer, &DataRow {
                        key: json!([s, id]),
                        value: json!(null),
                    })?;
                }
                summary.add(name, count);
            }
            // ... similar for all other TableDescriptor variants
            TableDescriptor::MultimapStrU64 { name } => {
                let table = txn.open_multimap_table(multimap_def_for(name))?;
                // Count ALL (key, value) pairs, not just unique keys
                let mut total_pairs = 0u64;
                let mut rows = Vec::new();
                for result in table.iter()? {
                    let (key, values) = result?;
                    let k = key.value().to_string();
                    for val_result in values {
                        let val = val_result?;
                        rows.push(DataRow {
                            key: json!(k),
                            value: json!(val.value()),
                        });
                        total_pairs += 1;
                    }
                }
                write_header(&mut writer, &make_header_with_count(descriptor, total_pairs))?;
                for row in &rows {
                    write_row(&mut writer, row)?;
                }
                summary.add(name, total_pairs);
            }
        }
    }

    writer.flush()?;
    Ok(summary)
}

/// Maps table name to the correct redb TableDefinition constant.
/// Uses the constants from schema.rs (ENTRIES, TOPIC_INDEX, etc.)
fn table_def_for_u64_blob(name: &str) -> TableDefinition<u64, &[u8]> {
    match name {
        "entries" => ENTRIES,
        "audit_log" => AUDIT_LOG,
        "signal_queue" => SIGNAL_QUEUE,
        "injection_log" => INJECTION_LOG,
        _ => unreachable!("unknown u64_blob table: {name}"),
    }
}
// similar lookup functions for each key/value type combination

struct ExportSummary {
    tables: Vec<(String, u64)>,
}
impl ExportSummary {
    fn print_to_stderr(&self) { ... }
    fn total_rows(&self) -> u64 { ... }
}
```

### `crates/unimatrix-store/src/migrate/import.rs`

```
#[cfg(feature = "backend-sqlite")]
// Only compiled under SQLite backend

use crate::sqlite::db::Store;
use super::format::*;
use super::{ALL_TABLES, TableDescriptor};

/// Import all tables from a JSON-lines file into a new SQLite database.
///
/// Creates the database via Store::open() (ADR-004) to get correct schema,
/// then inserts data using raw SQL on the underlying connection.
fn import(input_path: &Path, output_path: &Path) -> Result<ImportSummary, MigrateError> {
    // Precondition: output must not exist
    if output_path.exists() {
        return Err(MigrateError::Validation("output file already exists".into()));
    }

    // Create database with correct schema
    let store = Store::open(output_path)?;

    // Clear auto-initialized counters (Store::open sets defaults like next_entry_id=1)
    // These will be overwritten by imported values
    {
        let conn = store.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM counters", [])?;
    }

    // Read and parse intermediate file
    let file = File::open(input_path)?;
    let mut reader = BufReader::new(file);
    let mut summary = ImportSummary::new();

    // Track which tables we've seen (for completeness check)
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

        // Import all rows for this table
        let conn = store.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute_batch("BEGIN")?;

        let mut actual_count = 0u64;
        for _ in 0..header.row_count {
            let row_line = read_line(&mut reader)?
                .ok_or(MigrateError::Validation("unexpected EOF mid-table"))?;
            let row: DataRow = serde_json::from_value(row_line)?;

            insert_row(&conn, &header, &row)?;
            actual_count += 1;
        }

        conn.execute_batch("COMMIT")?;
        drop(conn);

        if actual_count != header.row_count {
            // Delete partial database on error
            let _ = std::fs::remove_file(output_path);
            return Err(MigrateError::RowCountMismatch { ... });
        }

        summary.add(&header.table, actual_count);
    }

    // Verify all 17 tables were present
    for desc in ALL_TABLES {
        if !tables_seen.contains(desc.name()) {
            let _ = std::fs::remove_file(output_path);
            return Err(MigrateError::Validation(format!("missing table: {}", desc.name())));
        }
    }

    // Post-import verification
    verify_import(&store)?;

    Ok(summary)
}

/// Insert a single row into the appropriate SQLite table.
fn insert_row(conn: &Connection, header: &TableHeader, row: &DataRow) -> Result<()> {
    match (header.key_type, header.value_type, header.multimap) {
        // u64 + blob tables (entries, audit_log, signal_queue, injection_log)
        (KeyType::U64, ValueType::Blob, false) => {
            let key = row.key.as_u64().ok_or(...)?;
            validate_i64_range(key)?;
            let blob = decode_blob(row.value.as_str().ok_or(...)?)?;
            let sql = match header.table.as_str() {
                "entries" => "INSERT INTO entries (id, data) VALUES (?1, ?2)",
                "audit_log" => "INSERT INTO audit_log (event_id, data) VALUES (?1, ?2)",
                "signal_queue" => "INSERT INTO signal_queue (signal_id, data) VALUES (?1, ?2)",
                "injection_log" => "INSERT INTO injection_log (log_id, data) VALUES (?1, ?2)",
                _ => unreachable!(),
            };
            conn.execute(sql, rusqlite::params![key as i64, blob])?;
        }
        // str + blob tables (agent_registry, observation_metrics, sessions)
        (KeyType::Str, ValueType::Blob, false) => {
            let key = row.key.as_str().ok_or(...)?;
            let blob = decode_blob(row.value.as_str().ok_or(...)?)?;
            let sql = match header.table.as_str() {
                "agent_registry" => "INSERT INTO agent_registry (agent_id, data) VALUES (?1, ?2)",
                "observation_metrics" => "INSERT INTO observation_metrics (feature_cycle, data) VALUES (?1, ?2)",
                "sessions" => "INSERT INTO sessions (session_id, data) VALUES (?1, ?2)",
                _ => unreachable!(),
            };
            conn.execute(sql, rusqlite::params![key, blob])?;
        }
        // str + u64 (counters)
        (KeyType::Str, ValueType::U64, false) => {
            let key = row.key.as_str().ok_or(...)?;
            let val = row.value.as_u64().ok_or(...)?;
            validate_i64_range(val)?;
            conn.execute(
                "INSERT OR REPLACE INTO counters (name, value) VALUES (?1, ?2)",
                rusqlite::params![key, val as i64],
            )?;
        }
        // (str, u64) + unit (topic_index, category_index, outcome_index)
        (KeyType::StrU64, ValueType::Unit, false) => {
            let arr = row.key.as_array().ok_or(...)?;
            let s = arr[0].as_str().ok_or(...)?;
            let id = arr[1].as_u64().ok_or(...)?;
            validate_i64_range(id)?;
            let sql = match header.table.as_str() {
                "topic_index" => "INSERT INTO topic_index (topic, entry_id) VALUES (?1, ?2)",
                "category_index" => "INSERT INTO category_index (category, entry_id) VALUES (?1, ?2)",
                "outcome_index" => "INSERT INTO outcome_index (feature_cycle, entry_id) VALUES (?1, ?2)",
                _ => unreachable!(),
            };
            conn.execute(sql, rusqlite::params![s, id as i64])?;
        }
        // (u64, u64) + unit (time_index)
        (KeyType::U64U64, ValueType::Unit, false) => {
            let arr = row.key.as_array().ok_or(...)?;
            let k0 = arr[0].as_u64().ok_or(...)?;
            let k1 = arr[1].as_u64().ok_or(...)?;
            validate_i64_range(k0)?;
            validate_i64_range(k1)?;
            conn.execute(
                "INSERT INTO time_index (timestamp, entry_id) VALUES (?1, ?2)",
                rusqlite::params![k0 as i64, k1 as i64],
            )?;
        }
        // (u8, u64) + unit (status_index)
        (KeyType::U8U64, ValueType::Unit, false) => {
            let arr = row.key.as_array().ok_or(...)?;
            let k0 = arr[0].as_u64().ok_or(...)? as u8;
            let k1 = arr[1].as_u64().ok_or(...)?;
            validate_i64_range(k1)?;
            conn.execute(
                "INSERT INTO status_index (status, entry_id) VALUES (?1, ?2)",
                rusqlite::params![k0 as i64, k1 as i64],
            )?;
        }
        // u64 + u64 (vector_map)
        (KeyType::U64, ValueType::U64, false) => {
            let key = row.key.as_u64().ok_or(...)?;
            let val = row.value.as_u64().ok_or(...)?;
            validate_i64_range(key)?;
            validate_i64_range(val)?;
            conn.execute(
                "INSERT INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)",
                rusqlite::params![key as i64, val as i64],
            )?;
        }
        // (u64, u64) + blob (co_access)
        (KeyType::U64U64, ValueType::Blob, false) => {
            let arr = row.key.as_array().ok_or(...)?;
            let k0 = arr[0].as_u64().ok_or(...)?;
            let k1 = arr[1].as_u64().ok_or(...)?;
            validate_i64_range(k0)?;
            validate_i64_range(k1)?;
            let blob = decode_blob(row.value.as_str().ok_or(...)?)?;
            conn.execute(
                "INSERT INTO co_access (entry_id_a, entry_id_b, data) VALUES (?1, ?2, ?3)",
                rusqlite::params![k0 as i64, k1 as i64, blob],
            )?;
        }
        // str + u64 multimap (tag_index, feature_entries)
        (KeyType::Str, ValueType::U64, true) => {
            let key = row.key.as_str().ok_or(...)?;
            let val = row.value.as_u64().ok_or(...)?;
            validate_i64_range(val)?;
            let sql = match header.table.as_str() {
                "tag_index" => "INSERT OR IGNORE INTO tag_index (tag, entry_id) VALUES (?1, ?2)",
                "feature_entries" => "INSERT OR IGNORE INTO feature_entries (feature_id, entry_id) VALUES (?1, ?2)",
                _ => unreachable!(),
            };
            conn.execute(sql, rusqlite::params![key, val as i64])?;
        }
        _ => return Err(MigrateError::Validation("unknown table type combo")),
    }
    Ok(())
}

/// Validate u64 value fits in i64 range for SQLite storage.
fn validate_i64_range(val: u64) -> Result<()> {
    if val > i64::MAX as u64 {
        return Err(MigrateError::Validation(format!("value {val} exceeds i64::MAX")));
    }
    Ok(())
}

/// Post-import verification:
/// 1. For each table: SELECT COUNT(*) matches expected
/// 2. next_entry_id > MAX(entries.id)
/// 3. schema_version == 5
fn verify_import(store: &Store) -> Result<()> {
    let conn = store.conn.lock().unwrap_or_else(|e| e.into_inner());

    // Check schema_version
    let schema_ver: i64 = conn.query_row(
        "SELECT value FROM counters WHERE name = 'schema_version'",
        [],
        |row| row.get(0),
    )?;
    if schema_ver != 5 {
        return Err(MigrateError::Validation(
            format!("schema_version is {schema_ver}, expected 5")
        ));
    }

    // Check next_entry_id > MAX(entries.id)
    let max_id: Option<i64> = conn.query_row(
        "SELECT MAX(id) FROM entries",
        [],
        |row| row.get(0),
    )?;
    let next_id: i64 = conn.query_row(
        "SELECT value FROM counters WHERE name = 'next_entry_id'",
        [],
        |row| row.get(0),
    )?;
    if let Some(max) = max_id {
        if next_id <= max {
            return Err(MigrateError::Validation(
                format!("next_entry_id ({next_id}) <= MAX(entries.id) ({max})")
            ));
        }
    }

    Ok(())
}
```

### Cargo.toml changes (unimatrix-store)

```
# Add to [dependencies]:
base64 = "0.22"
serde_json = { workspace = true }

# Add to [features]:
default = ["backend-sqlite"]
```

### lib.rs change

```
pub mod migrate;
```
