# row-serialization: Per-Table SQL-to-JSON Mapping

## Purpose

Each of the 8 exported tables has a dedicated function that runs a SELECT query and writes one JSONL line per row. Each function builds a `serde_json::Map<String, Value>` with keys inserted in column declaration order (ADR-003: `preserve_order` feature ensures insertion order is preserved in serialization). The `_table` key is always inserted first.

All functions live in `crates/unimatrix-server/src/export.rs` alongside the orchestration code. The file is estimated at ~350 lines total, well within the 500-line limit.

## SQL Type Conversion Rules

These rules apply uniformly across all table functions.

| SQL Type | Rust Extraction | JSON Value |
|----------|----------------|------------|
| INTEGER NOT NULL | `row.get::<_, i64>(idx)?` | `Value::Number(n.into())` |
| INTEGER (nullable) | `row.get::<_, Option<i64>>(idx)?` | `Some(n) -> Value::Number(n.into())`, `None -> Value::Null` |
| REAL NOT NULL | `row.get::<_, f64>(idx)?` | `Value::Number(Number::from_f64(v).unwrap())` -- serde_json/ryu provides lossless f64 precision |
| TEXT NOT NULL | `row.get::<_, String>(idx)?` | `Value::String(s)` |
| TEXT (nullable) | `row.get::<_, Option<String>>(idx)?` | `Some(s) -> Value::String(s)`, `None -> Value::Null` |

**Critical: JSON-in-TEXT columns** (capabilities, allowed_topics, allowed_categories, target_ids) use the TEXT extraction path. The value is emitted as a JSON string, NOT parsed/re-encoded. This means the JSON array stored in SQLite appears as a string value in the export JSONL. ADR-002 mandates this to avoid double-encoding.

**Critical: f64 via Number::from_f64**: `serde_json::Number::from_f64` returns `None` for NaN/Infinity. Confidence values should never be NaN/Infinity (constrained to [0.0, 1.0]), so `.unwrap()` is safe here. If the database contains a corrupt NaN, the unwrap will panic and the export aborts with a clear error via the panic hook.

## Shared Row-Writing Helper

To avoid repeating the serialize-and-write logic in every function:

```
fn write_row(map: serde_json::Map<String, Value>, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>:
    line = serde_json::to_string(&Value::Object(map))?
    writeln!(writer, "{}", line)?
    Ok(())
```

## Per-Table Functions

### export_counters

```
fn export_counters(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>:
    stmt = conn.prepare("SELECT name, value FROM counters ORDER BY name")?
    rows = stmt.query_map([], |row|:
        name: String = row.get(0)?
        value: i64 = row.get(1)?
        Ok((name, value))
    )?

    for result in rows:
        (name, value) = result?
        map = Map::new()
        map.insert("_table", Value::String("counters"))
        map.insert("name", Value::String(name))
        map.insert("value", Value::Number(value.into()))
        write_row(map, writer)?

    Ok(())
```

### export_entries

The largest function -- 26 columns plus `_table` discriminator.

```
fn export_entries(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>:
    stmt = conn.prepare(
        "SELECT id, title, content, topic, category, source, status, confidence,
                created_at, updated_at, last_accessed_at, access_count,
                supersedes, superseded_by, correction_count, embedding_dim,
                created_by, modified_by, content_hash, previous_hash,
                version, feature_cycle, trust_source,
                helpful_count, unhelpful_count, pre_quarantine_status
         FROM entries ORDER BY id"
    )?

    rows = stmt.query_map([], |row|:
        // Extract all 26 columns by index
        // Using a closure that returns Result<Map, rusqlite::Error>

        map = Map::new()
        map.insert("_table", Value::String("entries"))

        // INTEGER NOT NULL columns
        map.insert("id", Value::Number(row.get::<_, i64>(0)?.into()))
        // TEXT NOT NULL columns
        map.insert("title", Value::String(row.get::<_, String>(1)?))
        map.insert("content", Value::String(row.get::<_, String>(2)?))
        map.insert("topic", Value::String(row.get::<_, String>(3)?))
        map.insert("category", Value::String(row.get::<_, String>(4)?))
        map.insert("source", Value::String(row.get::<_, String>(5)?))
        // INTEGER NOT NULL
        map.insert("status", Value::Number(row.get::<_, i64>(6)?.into()))
        // REAL NOT NULL (f64)
        let confidence: f64 = row.get(7)?
        map.insert("confidence", Value::Number(Number::from_f64(confidence).unwrap()))
        // INTEGER NOT NULL timestamps
        map.insert("created_at", Value::Number(row.get::<_, i64>(8)?.into()))
        map.insert("updated_at", Value::Number(row.get::<_, i64>(9)?.into()))
        map.insert("last_accessed_at", Value::Number(row.get::<_, i64>(10)?.into()))
        map.insert("access_count", Value::Number(row.get::<_, i64>(11)?.into()))
        // INTEGER nullable
        map.insert("supersedes", match row.get::<_, Option<i64>>(12)?:
            Some(v) => Value::Number(v.into())
            None => Value::Null
        )
        map.insert("superseded_by", match row.get::<_, Option<i64>>(13)?:
            Some(v) => Value::Number(v.into())
            None => Value::Null
        )
        // INTEGER NOT NULL
        map.insert("correction_count", Value::Number(row.get::<_, i64>(14)?.into()))
        map.insert("embedding_dim", Value::Number(row.get::<_, i64>(15)?.into()))
        // TEXT NOT NULL
        map.insert("created_by", Value::String(row.get::<_, String>(16)?))
        map.insert("modified_by", Value::String(row.get::<_, String>(17)?))
        map.insert("content_hash", Value::String(row.get::<_, String>(18)?))
        map.insert("previous_hash", Value::String(row.get::<_, String>(19)?))
        // INTEGER NOT NULL
        map.insert("version", Value::Number(row.get::<_, i64>(20)?.into()))
        // TEXT NOT NULL
        map.insert("feature_cycle", Value::String(row.get::<_, String>(21)?))
        map.insert("trust_source", Value::String(row.get::<_, String>(22)?))
        // INTEGER NOT NULL
        map.insert("helpful_count", Value::Number(row.get::<_, i64>(23)?.into()))
        map.insert("unhelpful_count", Value::Number(row.get::<_, i64>(24)?.into()))
        // INTEGER nullable
        map.insert("pre_quarantine_status", match row.get::<_, Option<i64>>(25)?:
            Some(v) => Value::Number(v.into())
            None => Value::Null
        )

        Ok(map)
    )?

    for result in rows:
        map = result?
        write_row(map, writer)?

    Ok(())
```

**Important**: The column order in the SELECT must match the index numbers used for extraction. The key insertion order into the Map determines JSON key order (via `preserve_order`).

### export_entry_tags

```
fn export_entry_tags(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>:
    stmt = conn.prepare("SELECT entry_id, tag FROM entry_tags ORDER BY entry_id, tag")?
    rows = stmt.query_map([], |row|:
        map = Map::new()
        map.insert("_table", Value::String("entry_tags"))
        map.insert("entry_id", Value::Number(row.get::<_, i64>(0)?.into()))
        map.insert("tag", Value::String(row.get::<_, String>(1)?))
        Ok(map)
    )?

    for result in rows:
        write_row(result?, writer)?

    Ok(())
```

### export_co_access

```
fn export_co_access(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>:
    stmt = conn.prepare(
        "SELECT entry_id_a, entry_id_b, count, last_updated
         FROM co_access ORDER BY entry_id_a, entry_id_b"
    )?
    rows = stmt.query_map([], |row|:
        map = Map::new()
        map.insert("_table", Value::String("co_access"))
        map.insert("entry_id_a", Value::Number(row.get::<_, i64>(0)?.into()))
        map.insert("entry_id_b", Value::Number(row.get::<_, i64>(1)?.into()))
        map.insert("count", Value::Number(row.get::<_, i64>(2)?.into()))
        map.insert("last_updated", Value::Number(row.get::<_, i64>(3)?.into()))
        Ok(map)
    )?

    for result in rows:
        write_row(result?, writer)?

    Ok(())
```

### export_feature_entries

```
fn export_feature_entries(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>:
    stmt = conn.prepare(
        "SELECT feature_id, entry_id FROM feature_entries ORDER BY feature_id, entry_id"
    )?
    rows = stmt.query_map([], |row|:
        map = Map::new()
        map.insert("_table", Value::String("feature_entries"))
        map.insert("feature_id", Value::String(row.get::<_, String>(0)?))
        map.insert("entry_id", Value::Number(row.get::<_, i64>(1)?.into()))
        Ok(map)
    )?

    for result in rows:
        write_row(result?, writer)?

    Ok(())
```

### export_outcome_index

```
fn export_outcome_index(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>:
    stmt = conn.prepare(
        "SELECT feature_cycle, entry_id FROM outcome_index ORDER BY feature_cycle, entry_id"
    )?
    rows = stmt.query_map([], |row|:
        map = Map::new()
        map.insert("_table", Value::String("outcome_index"))
        map.insert("feature_cycle", Value::String(row.get::<_, String>(0)?))
        map.insert("entry_id", Value::Number(row.get::<_, i64>(1)?.into()))
        Ok(map)
    )?

    for result in rows:
        write_row(result?, writer)?

    Ok(())
```

### export_agent_registry

Contains JSON-in-TEXT columns. These are extracted as TEXT and emitted as JSON strings (not parsed).

```
fn export_agent_registry(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>:
    stmt = conn.prepare(
        "SELECT agent_id, trust_level, capabilities, allowed_topics,
                allowed_categories, enrolled_at, last_seen_at, active
         FROM agent_registry ORDER BY agent_id"
    )?
    rows = stmt.query_map([], |row|:
        map = Map::new()
        map.insert("_table", Value::String("agent_registry"))
        // TEXT NOT NULL (PK)
        map.insert("agent_id", Value::String(row.get::<_, String>(0)?))
        // INTEGER NOT NULL
        map.insert("trust_level", Value::Number(row.get::<_, i64>(1)?.into()))
        // TEXT NOT NULL -- JSON-in-TEXT, emitted as string
        map.insert("capabilities", Value::String(row.get::<_, String>(2)?))
        // TEXT nullable -- JSON-in-TEXT, emitted as string or null
        map.insert("allowed_topics", match row.get::<_, Option<String>>(3)?:
            Some(s) => Value::String(s)
            None => Value::Null
        )
        map.insert("allowed_categories", match row.get::<_, Option<String>>(4)?:
            Some(s) => Value::String(s)
            None => Value::Null
        )
        // INTEGER NOT NULL
        map.insert("enrolled_at", Value::Number(row.get::<_, i64>(5)?.into()))
        map.insert("last_seen_at", Value::Number(row.get::<_, i64>(6)?.into()))
        map.insert("active", Value::Number(row.get::<_, i64>(7)?.into()))
        Ok(map)
    )?

    for result in rows:
        write_row(result?, writer)?

    Ok(())
```

### export_audit_log

Contains a JSON-in-TEXT column (target_ids).

```
fn export_audit_log(conn: &Connection, writer: &mut impl Write) -> Result<(), Box<dyn std::error::Error>>:
    stmt = conn.prepare(
        "SELECT event_id, timestamp, session_id, agent_id, operation,
                target_ids, outcome, detail
         FROM audit_log ORDER BY event_id"
    )?
    rows = stmt.query_map([], |row|:
        map = Map::new()
        map.insert("_table", Value::String("audit_log"))
        // INTEGER NOT NULL (PK)
        map.insert("event_id", Value::Number(row.get::<_, i64>(0)?.into()))
        // INTEGER NOT NULL
        map.insert("timestamp", Value::Number(row.get::<_, i64>(1)?.into()))
        // TEXT NOT NULL
        map.insert("session_id", Value::String(row.get::<_, String>(2)?))
        map.insert("agent_id", Value::String(row.get::<_, String>(3)?))
        map.insert("operation", Value::String(row.get::<_, String>(4)?))
        // TEXT NOT NULL -- JSON-in-TEXT, emitted as string
        map.insert("target_ids", Value::String(row.get::<_, String>(5)?))
        // INTEGER NOT NULL
        map.insert("outcome", Value::Number(row.get::<_, i64>(6)?.into()))
        // TEXT NOT NULL
        map.insert("detail", Value::String(row.get::<_, String>(7)?))
        Ok(map)
    )?

    for result in rows:
        write_row(result?, writer)?

    Ok(())
```

## Implementation Note: query_map vs query Pattern

The pseudocode uses `query_map` for clarity. In practice, rusqlite's `query_map` returns `Rows` which borrows the statement. The implementation may need to use the `query` + manual `while let Some(row) = rows.next()?` pattern instead, since `query_map` closures cannot return `Map` without ownership issues. The implementation agent should choose whichever pattern compiles cleanly. The logical flow is identical either way.

Alternative pattern (recommended for implementation):

```
let mut stmt = conn.prepare(SQL)?;
let mut rows = stmt.query([])?;
while let Some(row) = rows.next()? {
    let mut map = Map::new();
    // ... insert columns ...
    write_row(map, writer)?;
}
```

This avoids the double-Result nesting of `query_map` and writes rows as they are read (true streaming).

## Nullable Column Helper

To reduce repetition for nullable integer columns, a small helper can be used:

```
fn nullable_int(row: &Row, idx: usize) -> Result<Value, rusqlite::Error>:
    match row.get::<_, Option<i64>>(idx)?:
        Some(v) => Ok(Value::Number(v.into()))
        None => Ok(Value::Null)
```

Similarly for nullable text:

```
fn nullable_text(row: &Row, idx: usize) -> Result<Value, rusqlite::Error>:
    match row.get::<_, Option<String>>(idx)?:
        Some(s) => Ok(Value::String(s))
        None => Ok(Value::Null)
```

These are optional ergonomic helpers, not required.

## Key Test Scenarios

### Per-Table Completeness (R-01)
1. Insert row with non-default values for ALL columns in each table. Export. Verify every column present in JSONL with correct value.
2. For entries table: verify all 26 columns + `_table` = 27 keys per row.

### NULL Handling (R-04)
3. Insert entry with `supersedes = NULL`, `superseded_by = NULL`, `pre_quarantine_status = NULL`. Verify all three are JSON `null` (not omitted, not empty string).
4. Insert agent_registry with `allowed_topics = NULL`, `allowed_categories = NULL`. Verify both are JSON `null`.

### f64 Precision (R-02)
5. Export entries with confidence values: 0.0, 1.0, 0.123456789012345, f64::MIN_POSITIVE, 0.1 + 0.2. Parse back, verify bitwise equality.

### JSON-in-TEXT (R-03)
6. Insert agent with `capabilities = '["Admin","Read"]'`. Verify exported value is `"[\"Admin\",\"Read\"]"` (JSON string, not JSON array).
7. Insert audit_log with `target_ids = '[1,2,3]'`. Verify exported value is a string.

### Row Ordering (R-08)
8. Insert entries with IDs 5, 2, 8. Verify export order is 2, 5, 8.
9. Insert entry_tags (entry_id=1, tag="z") then (entry_id=1, tag="a"). Verify order is (1, "a"), (1, "z").

### Empty Tables (R-12)
10. Fresh database. Verify counters present, no rows for other tables.

### Unicode (R-13)
11. Insert entry with CJK title, emoji content, accented tag. Verify round-trip fidelity.

### Large Integers (R-14)
12. Insert entry with `created_at = i64::MAX`. Verify exact number in JSON output.

### Determinism (R-06)
13. Export same database twice (with mocked/fixed timestamp). Verify byte-identical output.
14. Verify `_table` is always first key in every data row.

### Excluded Tables (R-07)
15. Populate excluded tables. Verify no `_table` values from excluded set appear in output.
