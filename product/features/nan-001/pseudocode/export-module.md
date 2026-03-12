# export-module: Export Orchestration

## Purpose

Implement `run_export()` as the entry point for the export subcommand. Handles database opening, transaction management, writer setup, header emission, and sequential table export. All logic lives in `crates/unimatrix-server/src/export.rs`.

## File Created

- `crates/unimatrix-server/src/export.rs`

## Public Function: run_export

Source: Architecture Integration Surface.

```
pub fn run_export(
    project_dir: Option<&Path>,
    output: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>>
```

### Pseudocode

```
fn run_export(project_dir, output):
    // 1. Resolve project paths
    paths = project::ensure_data_directory(project_dir, None)?

    // 2. Open database (triggers migration if needed)
    store = Store::open(&paths.db_path)?

    // 3. Acquire connection mutex
    conn = store.lock_conn()

    // 4. Begin snapshot transaction (ADR-001)
    conn.execute_batch("BEGIN DEFERRED")?

    // 5. Set up writer (file or stdout)
    //    Use BufWriter for performance on file output
    if output is Some(path):
        file = File::create(path)?
        writer = BufWriter::new(file)
        result = do_export(&conn, &mut writer)
    else:
        stdout = io::stdout()
        lock = stdout.lock()
        writer = BufWriter::new(lock)
        result = do_export(&conn, &mut writer)

    // 6. Commit transaction regardless of export result
    //    Use a helper to ensure commit happens even on error
    //    (deferred transaction is read-only so COMMIT vs ROLLBACK is equivalent)
    let _ = conn.execute_batch("COMMIT")

    // 7. Propagate any export error
    result
```

Note on transaction cleanup: Since this is a read-only DEFERRED transaction, both COMMIT and ROLLBACK have the same effect. We attempt COMMIT after export completes. If the export itself failed, we still try COMMIT (which releases the snapshot) and then propagate the original error.

### Helper: do_export

Separated to allow the writer type to vary (file vs stdout) while keeping transaction logic in one place.

```
fn do_export(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>:
    write_header(conn, writer)?
    export_counters(conn, writer)?
    export_entries(conn, writer)?
    export_entry_tags(conn, writer)?
    export_co_access(conn, writer)?
    export_feature_entries(conn, writer)?
    export_outcome_index(conn, writer)?
    export_agent_registry(conn, writer)?
    export_audit_log(conn, writer)?
    writer.flush()?
    Ok(())
```

## Internal Function: write_header

```
fn write_header(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>:
    // Query schema version from counters table
    schema_version = conn.query_row(
        "SELECT value FROM counters WHERE name = 'schema_version'",
        [],
        |row| row.get::<_, i64>(0)
    )?

    // Query entry count
    entry_count = conn.query_row(
        "SELECT COUNT(*) FROM entries",
        [],
        |row| row.get::<_, i64>(0)
    )?

    // Get current unix timestamp (seconds)
    exported_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64

    // Build header JSON object with insertion-order keys
    // Key order: _header, schema_version, exported_at, entry_count, format_version
    map = serde_json::Map::new()
    map.insert("_header", Value::Bool(true))
    map.insert("schema_version", Value::Number(schema_version))
    map.insert("exported_at", Value::Number(exported_at))
    map.insert("entry_count", Value::Number(entry_count))
    map.insert("format_version", Value::Number(1))

    // Serialize and write as single line
    line = serde_json::to_string(&Value::Object(map))?
    writeln!(writer, "{}", line)?

    Ok(())
```

## Writer Setup Pattern

The writer is always a `BufWriter` wrapping either:
- `File` (when `--output` is specified)
- `StdoutLock` (when writing to stdout)

Since `BufWriter<File>` and `BufWriter<StdoutLock>` are different types, the dispatch uses an `if/else` calling `do_export` with `&mut impl Write` in both branches. This avoids boxing or trait objects.

## Error Handling

All errors propagate via `Box<dyn std::error::Error>`:

| Error Source | Type | Propagation |
|-------------|------|-------------|
| `ensure_data_directory` | `io::Error` | `?` to caller |
| `Store::open` | `StoreError` | `?` to caller |
| `execute_batch("BEGIN DEFERRED")` | `rusqlite::Error` | `?` to caller |
| `File::create` | `io::Error` | `?` to caller |
| `query_row` (header queries) | `rusqlite::Error` | `?` to caller |
| `serde_json::to_string` | `serde_json::Error` | `?` to caller |
| `writeln!` | `io::Error` | `?` to caller |
| `writer.flush()` | `io::Error` | `?` to caller |

On any error, `main()` prints the error (via `Display`) to stderr and exits with non-zero code. The partial output file (if `--output` was specified) may remain on disk -- this is explicitly accepted per the specification.

## Module Registration

Add `pub mod export;` to the server crate's `lib.rs` (check actual module declaration location -- it may be `lib.rs` based on how `unimatrix_server::uds::hook::run` is accessed from `main.rs`).

## Key Test Scenarios

1. **Full export with representative data**: Populate all 8 tables, export, verify header + all rows present with correct values
2. **Empty database**: Fresh Store::open, export, verify header with entry_count=0 and counter rows only
3. **File output**: Export with `--output`, verify file exists and content matches stdout export
4. **Stdout output**: Export without `--output`, capture stdout, verify valid JSONL
5. **Transaction isolation**: Verify BEGIN DEFERRED is executed before reads and COMMIT after
6. **Database open failure**: Invalid db_path, verify error message to stderr, non-zero exit
7. **Write failure**: Read-only output path, verify error propagation
8. **project-dir respected**: Create DB in non-default dir, export with project_dir, verify correct data
