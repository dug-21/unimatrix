# nan-001 Pseudocode Overview

## Components

| Component | File | Purpose |
|-----------|------|---------|
| cli-extension | `crates/unimatrix-server/src/main.rs` | Add `Export` variant to `Command` enum, dispatch to `export::run_export()` |
| export-module | `crates/unimatrix-server/src/export.rs` (new) | Orchestrate export: open DB, transaction, iterate tables, write JSONL |
| row-serialization | Within `export.rs` | Per-table SQL-to-JSON mapping using `serde_json::Map<String, Value>` |

## Data Flow

```
CLI parse (main.rs)
  |
  v
Command::Export { output }
  |
  v
export::run_export(project_dir, output)
  |
  +-> ensure_data_directory(project_dir, None) -> ProjectPaths
  +-> Store::open(&paths.db_path) -> Store
  +-> store.lock_conn() -> MutexGuard<Connection>
  +-> conn.execute_batch("BEGIN DEFERRED")
  |
  +-> write_header(&conn, &mut writer)        // header line
  +-> export_counters(&conn, &mut writer)      // table 1
  +-> export_entries(&conn, &mut writer)        // table 2
  +-> export_entry_tags(&conn, &mut writer)     // table 3
  +-> export_co_access(&conn, &mut writer)      // table 4
  +-> export_feature_entries(&conn, &mut writer) // table 5
  +-> export_outcome_index(&conn, &mut writer)  // table 6
  +-> export_agent_registry(&conn, &mut writer) // table 7
  +-> export_audit_log(&conn, &mut writer)      // table 8
  |
  +-> conn.execute_batch("COMMIT")
  +-> writer.flush()
  +-> Ok(())
```

## Shared Types

No new shared types introduced. All types are existing:

- `serde_json::Map<String, Value>` -- used with `preserve_order` feature for insertion-order key determinism
- `serde_json::Value` -- `Value::Number`, `Value::String`, `Value::Null` for SQL-to-JSON conversion
- `rusqlite::Connection` -- direct SQL access via `Store::lock_conn()`
- `rusqlite::Row` -- per-row column extraction

## Cargo.toml Change

`crates/unimatrix-server/Cargo.toml`: Change `serde_json = "1"` to `serde_json = { version = "1", features = ["preserve_order"] }`.

This enables `indexmap`-backed `Map` for insertion-order key determinism (ADR-003). The feature applies crate-wide but is compatible with existing code since insertion-order is a superset of the previous arbitrary order behavior.

## Sequencing

1. **Cargo.toml** -- enable `preserve_order` feature on serde_json (prerequisite for deterministic output)
2. **export-module + row-serialization** -- implement `export.rs` with all functions (single file, well under 500 lines)
3. **cli-extension** -- wire `Command::Export` in `main.rs` and add `mod export;` to lib.rs or main.rs

Components 2 and 3 can be implemented in parallel since the interface (`run_export` signature) is defined.

## Module Registration

The export module must be declared. Following the codebase pattern where `hook` is accessed as `unimatrix_server::uds::hook::run`, the export module should be a top-level module in the server crate. Check whether the crate uses `lib.rs` for module declarations or `main.rs` directly. The hook module is under `uds/` because it uses UDS transport. Export has no transport dependency, so `export.rs` as a peer to `uds/` is appropriate. Add `pub mod export;` in `lib.rs`.
