## ADR-002: Explicit Column-to-JSON Mapping with serde_json::Value

### Context

The export must serialize SQL rows to JSON lines. There are several approaches (SR-01, SR-03 from the Scope Risk Assessment):

1. **Derive from Rust types**: Use the existing `EntryRecord` and similar structs with `#[derive(Serialize)]`. Problem: the Store API types may not include all 26 columns (e.g., `embedding_dim` may be excluded from the Rust struct), field names may differ from SQL column names, and `#[serde(skip)]` annotations could silently drop fields. Any mismatch between the Rust type and the SQL schema produces a lossy export.

2. **SELECT * with dynamic column names**: Read column names from `stmt.column_names()` and build JSON dynamically. Problem: column order in `SELECT *` is not guaranteed to be stable across SQLite versions, and there is no compile-time verification that all expected columns exist.

3. **Explicit column list with explicit Value construction**: Write each table's SELECT with a hardcoded column list, extract each column by index, and build a `serde_json::Value::Object` (backed by `serde_json::Map`) with explicit key-value pairs.

### Decision

Use approach (3): explicit column list per table, explicit `serde_json::Value` construction.

Each per-table export function:
1. Prepares a SQL statement with an explicit column list: `SELECT id, title, content, ... FROM entries ORDER BY id`
2. Iterates rows, extracting each column by index using rusqlite's typed getters (`row.get::<_, i64>(0)`, `row.get::<_, Option<i64>>(12)`, etc.)
3. Builds a `serde_json::Map<String, Value>` by inserting keys in the canonical order (same order as the SQL column list)
4. Inserts `"_table"` as the first key
5. Serializes the map as a single JSON line

For f64 values (confidence), `serde_json` serializes with sufficient precision by default. Specifically, serde_json uses the `ryu` crate which produces the shortest decimal representation that round-trips exactly through `f64`. This satisfies SR-03 (float precision) without additional configuration.

For JSON-in-TEXT columns (`capabilities`, `allowed_topics`, `allowed_categories`, `target_ids`), the raw TEXT value is emitted as a JSON string. No parsing or re-encoding. On import (nan-002), the importer writes the string value directly to the SQL column. This avoids double-encoding and preserves the exact original string.

SQL NULLs are represented as `serde_json::Value::Null`. The column is always present in the JSON object — never omitted. This is critical for nan-002 to distinguish "NULL" from "not exported."

### Consequences

- **Positive**: The column-to-JSON mapping is explicit and auditable. Every column is named in the SQL query and in the JSON construction code. Nothing is silently skipped.
- **Positive**: No dependency on Rust struct field names or serde annotations. The export format is decoupled from internal type representations.
- **Positive**: f64 round-trip fidelity is guaranteed by serde_json/ryu without additional work.
- **Negative**: The column list is hardcoded per table. If a schema migration adds or removes a column, the export code must be updated manually. This is acceptable for v1 — the scope explicitly assumes schema v11 stability (SR-04).
- **Negative**: More verbose code than a `#[derive(Serialize)]` approach. Each table function is ~30-50 lines. This is a worthwhile tradeoff for format correctness.
