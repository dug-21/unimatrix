# Test Plan: row-serialization

Component: Per-table column mapping and type encoding in `crates/unimatrix-server/src/export.rs`
Risks covered: R-01, R-02, R-03, R-04, R-06, R-11, R-13, R-14

All tests below are unit tests within `export.rs` `#[cfg(test)]` module unless otherwise noted. They create in-memory SQLite databases, insert rows via raw SQL, and call the per-table export functions with a `Vec<u8>` writer.

## Column Completeness

### T-RS-01: All 26 entry columns present with correct values (R-01, AC-06)

**Risks**: R-01 (hardcoded column list divergence)
**Setup**: Create an in-memory SQLite database with the entries table schema. Insert one entry with non-default values for ALL 26 columns:
```
id=42, title="Test Entry", content="Content here", topic="testing",
category="pattern", source="unit-test", status=1, confidence=0.87654321,
created_at=1700000000, updated_at=1700000001, last_accessed_at=1700000002,
access_count=15, supersedes=10, superseded_by=50,
correction_count=3, embedding_dim=384, created_by="agent-x",
modified_by="agent-y", content_hash="abc123", previous_hash="def456",
version=7, feature_cycle="crt-002", trust_source="human",
helpful_count=12, unhelpful_count=2, pre_quarantine_status=0
```
**Action**: Call `export_entries(&conn, &mut buf)`.
**Assert**:
- Parse the output line as JSON.
- Assert exactly 27 keys present (26 columns + `_table`).
- Assert `_table` equals "entries".
- Assert every column value matches the inserted value exactly:
  - `id` == 42 (number)
  - `title` == "Test Entry" (string)
  - `confidence` == 0.87654321 (number, f64)
  - `supersedes` == 10 (number, not null)
  - `superseded_by` == 50 (number, not null)
  - `pre_quarantine_status` == 0 (number, not null)
  - (all other columns similarly verified)

### T-RS-02: Column count matches PRAGMA table_info (R-01)

**Risks**: R-01 (missing columns)
**Setup**: Create a real database via `Store::open()` (to get the actual schema).
**Action**: Run `PRAGMA table_info(entries)` and count columns. Export one entry.
**Assert**: The JSON key count minus 1 (for `_table`) equals the PRAGMA column count.
**Repeat** for all 8 exported tables: counters, entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log.

### T-RS-03: Per-table key count matches SQL column count plus _table (R-01)

**Risks**: R-01 (extra or missing keys)
**Setup**: Create a database with at least one row in each of the 8 tables.
**Action**: Export. For each `_table` group, count the JSON keys.
**Assert**:
- counters rows: 3 keys (_table, name, value)
- entries rows: 27 keys (_table + 26 columns)
- entry_tags rows: 3 keys (_table, entry_id, tag)
- co_access rows: 5 keys (_table, entry_id_a, entry_id_b, count, last_updated)
- feature_entries rows: 3 keys (_table, feature_id, entry_id)
- outcome_index rows: 3 keys (_table, feature_cycle, entry_id)
- agent_registry rows: 9 keys (_table + 8 columns)
- audit_log rows: 9 keys (_table + 8 columns)

## Float Precision

### T-RS-04: f64 confidence round-trip fidelity (R-02)

**Risks**: R-02 (precision loss)
**Setup**: Create entries with confidence values:
- 0.0
- 1.0
- 0.123456789012345
- f64::MIN_POSITIVE (2.2250738585072014e-308)
- 0.1 + 0.2 (= 0.30000000000000004, a classic IEEE 754 edge case)
**Action**: Export each entry. Parse the JSON confidence value back to f64.
**Assert**:
- For each value, `parsed_f64 == original_f64` (bitwise equality via `to_bits()`).
- The serialized form uses shortest-exact representation (ryu), not fixed decimal.

## JSON-in-TEXT Columns

### T-RS-05: JSON-in-TEXT columns emitted as raw strings (R-03)

**Risks**: R-03 (double encoding)
**Setup**: Create agent_registry rows:
1. `capabilities = '["Admin","Read"]'`, `allowed_topics = '["security"]'`, `allowed_categories = '["decision"]'`
2. `capabilities = '[]'` (empty array), `allowed_topics = NULL`, `allowed_categories = NULL`

Create audit_log row:
3. `target_ids = '[1,2,3]'`

**Action**: Export.
**Assert**:
- Row 1: `capabilities` is a JSON string value `"[\"Admin\",\"Read\"]"` -- when you parse the JSON line, `row["capabilities"]` is a `Value::String` containing `["Admin","Read"]`. It is NOT a JSON array `["Admin","Read"]`.
- Row 1: `allowed_topics` is a JSON string `"[\"security\"]"`, not a JSON array.
- Row 2: `capabilities` is a JSON string `"[]"`, not an empty JSON array.
- Row 2: `allowed_topics` is JSON `null`.
- Row 3: `target_ids` is a JSON string `"[1,2,3]"`, not a JSON array.
- Round-trip: for each JSON-in-TEXT value, `json_string_value.as_str()` equals the original SQLite TEXT value byte-for-byte.

## NULL Handling

### T-RS-06: NULL columns serialized as JSON null (R-04, AC-09)

**Risks**: R-04 (NULL omission)
**Setup**: Create an entry with:
- `supersedes = NULL`
- `superseded_by = NULL`
- `pre_quarantine_status = NULL`

Create an agent_registry row with:
- `allowed_topics = NULL`
- `allowed_categories = NULL`

**Action**: Export.
**Assert**:
- Entry row: `supersedes` key IS present and value IS `Value::Null`.
- Entry row: `superseded_by` key IS present and value IS `Value::Null`.
- Entry row: `pre_quarantine_status` key IS present and value IS `Value::Null`.
- Agent row: `allowed_topics` key IS present and value IS `Value::Null`.
- Agent row: `allowed_categories` key IS present and value IS `Value::Null`.
- No row has fewer keys than expected (key count assertions from T-RS-03 still hold).

### T-RS-06b: Empty strings are NOT null

**Setup**: Create an entry with `created_by = ""`, `content_hash = ""`, `feature_cycle = ""`.
**Action**: Export.
**Assert**:
- `created_by` is `Value::String("")`, NOT `Value::Null`.
- `content_hash` is `Value::String("")`, NOT `Value::Null`.

## Key Ordering

### T-RS-07: _table is first key, columns follow declaration order (R-06)

**Risks**: R-06 (non-deterministic ordering)
**Setup**: Create an entry. Export.
**Action**: Read the raw JSON string (before parsing into a Map which may reorder).
**Assert**:
- The first key in the JSON object is `"_table"`.
- For entries rows, the key order is: _table, id, title, content, topic, category, source, status, confidence, created_at, updated_at, last_accessed_at, access_count, supersedes, superseded_by, correction_count, embedding_dim, created_by, modified_by, content_hash, previous_hash, version, feature_cycle, trust_source, helpful_count, unhelpful_count, pre_quarantine_status.
- Verify using regex or string scanning on the raw line, not on a parsed JSON object.

## Regression

### T-RS-08: preserve_order feature does not break existing tests (R-11)

**Risks**: R-11 (global side-effect)
**Verification**: Run `cargo test --workspace` after enabling the `preserve_order` feature on `serde_json` in `unimatrix-server/Cargo.toml`. All existing tests must pass. This is not a new test to write -- it is verified by the full test suite passing in Stage 3c.

## Unicode

### T-RS-09: Unicode content preserved (R-13)

**Risks**: R-13 (corruption)
**Setup**: Create entries with:
1. `title` containing CJK: "\u{77E5}\u{8B58}" (knowledge in Japanese)
2. `content` containing emoji: "Status: \u{2705} approved"
3. Tag with accented characters: "resume\u{0301}" (e + combining accent)
4. `content` with embedded newline: "line1\nline2"
5. `content` with JSON-special characters: `He said "hello" and used a \backslash`

**Action**: Export.
**Assert**:
- Title parses back to the original CJK string.
- Content parses back to the original emoji string.
- Tag parses back to the original accented string.
- Content with newline: the JSONL line is a single line (newline is escaped as `\n` in JSON), and parsing yields the original two-line string.
- Content with quotes/backslashes: parsing yields the original string with literal quotes and backslash.

## Large Integers

### T-RS-10: Large integer values preserved (R-14)

**Risks**: R-14 (precision loss)
**Setup**: Create an entry with:
- `created_at = 9999999999` (year 2286, large but realistic)
- `version = 2147483647` (i32::MAX)
- `access_count = 1000000`

Create a counter with `value = 9223372036854775807` (i64::MAX).

**Action**: Export.
**Assert**:
- All integer values parse back to the exact original value.
- Counter with i64::MAX: the JSON number is exact (serde_json handles i64 natively).
- No scientific notation for integer values.

## Edge Cases

### T-RS-11: Entry with all nullable fields NULL simultaneously

**Setup**: Create entry with supersedes=NULL, superseded_by=NULL, pre_quarantine_status=NULL.
**Action**: Export.
**Assert**: All three are JSON null. Row has exactly 27 keys.

### T-RS-12: Timestamp of 0 is not treated as NULL

**Setup**: Create entry with `created_at = 0`, `last_accessed_at = 0`.
**Action**: Export.
**Assert**: Both are JSON number 0, not null, not omitted.

### T-RS-13: JSONL line integrity -- no raw newlines in output lines

**Setup**: Create entry with `content = "line1\nline2\nline3"`.
**Action**: Export to a string buffer. Split by newlines.
**Assert**: Each line (split by `\n`) is a complete, valid JSON object. The content field's newlines are escaped within the JSON string, not literal newlines breaking the JSONL format.
