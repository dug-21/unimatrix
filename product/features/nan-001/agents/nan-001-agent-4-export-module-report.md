# Agent Report: nan-001-agent-4-export-module

## Component
export-module -- orchestration layer in `crates/unimatrix-server/src/export.rs`

## Files Modified
- `crates/unimatrix-server/src/export.rs` (created, 639 lines)
- `crates/unimatrix-server/Cargo.toml` (already had `preserve_order` feature added by prior agent)

## Implementation Summary

Implemented the full export orchestration layer per the validated pseudocode:

1. **`run_export()`** -- Public entry point. Resolves project paths via `ensure_data_directory`, opens store via `Store::open()`, acquires connection mutex, begins `BEGIN DEFERRED` transaction (ADR-001), sets up `BufWriter<File>` or `BufWriter<StdoutLock>` based on output param, calls `do_export()`, commits transaction, propagates errors.

2. **`do_export()`** -- Internal helper that calls `write_header()` then all 8 table export functions in dependency order, then flushes the writer. Separated from `run_export` to handle the writer type variance (file vs stdout) without boxing.

3. **`write_header()`** -- Queries `schema_version` from counters, `COUNT(*)` from entries, captures unix timestamp. Writes header JSON line with keys in insertion order: `_header`, `schema_version`, `exported_at`, `entry_count`, `format_version`.

4. **8 per-table export functions** -- All fully implemented with SQL queries, explicit column-to-JSON mapping via `serde_json::Map`, correct `ORDER BY` clauses, nullable column handling, and JSON-in-TEXT columns emitted as raw strings (ADR-002).

5. **Helper functions** -- `write_row()`, `nullable_int()`, `nullable_text()` for DRY row serialization.

### Key Design Decisions Followed
- ADR-001: `BEGIN DEFERRED` for snapshot isolation, COMMIT after all reads
- ADR-002: Explicit column mapping, SQL NULL to JSON null, JSON-in-TEXT as raw strings
- ADR-003: `preserve_order` feature on serde_json for insertion-order key determinism
- Sync-only, no tokio (matches hook subcommand pattern)
- Direct SQL via `conn` (not Store API)
- NaN safety on confidence f64 via `Number::from_f64` with fallback to 0

## Tests
- 6 passed, 0 failed
  - `test_write_header_fields_correct` (T-EM-09: header fields)
  - `test_write_header_exported_at_recent` (T-EM-09: timestamp recency)
  - `test_do_export_empty_db` (T-EM-08: fresh database)
  - `test_do_export_all_lines_valid_json` (T-EM-08: all lines valid JSON)
  - `test_run_export_to_file` (placeholder -- run_export requires project root detection)
  - `test_header_key_order_preserved` (ADR-003: insertion order preserved)

## Build Verification
- `cargo build --workspace` passes (zero errors, pre-existing warnings only)
- `cargo test --workspace` passes (zero new failures)
- `cargo clippy` clean for export.rs
- `cargo fmt` applied

## Issues
- File is 639 lines (exceeds 500-line guideline). 500 lines are non-test code, 139 lines are tests. The pseudocode overview designed this as a single file. Splitting would require making helper functions (`nullable_int`, `nullable_text`, `write_row`) public, which leaks internal implementation. Acceptable for v1.
- The `test_run_export_to_file` test is a placeholder because `run_export` uses `ensure_data_directory` which requires project root detection. Full file output testing belongs in integration tests (T-EM-03, T-EM-11).

## Knowledge Stewardship
- Queried: /query-patterns not available (knowledge server not running in worktree)
- Stored: nothing novel to store -- implementation followed pseudocode directly with no surprises. The `preserve_order` feature on serde_json was already documented in ADR-003 and worked as expected with no test regressions.
