# Test Plan: schema-migration (C4)

**Files under test**: `crates/unimatrix-store/src/migration.rs`, `crates/unimatrix-store/src/sessions.rs`
**Risks covered**: R-03, R-05, R-06 (partial)

## Unit Tests

### Migration v11 -> v12 (R-05)

Follow established migration test pattern.

```
test_migration_v11_to_v12_adds_keywords_column
  Arrange: Create a v11 database with sessions table (no keywords column)
  Act: Run migration to v12
  Assert: PRAGMA table_info(sessions) includes "keywords" column of type TEXT
  Assert: CURRENT_SCHEMA_VERSION == 12

test_migration_v12_existing_sessions_have_null_keywords
  Arrange: Create v11 database with 3 existing session rows
  Act: Run migration to v12
  Assert: All 3 sessions have keywords = NULL

test_migration_v12_idempotency_or_graceful_failure
  Arrange: Create v12 database (keywords column already exists)
  Act: Run migration again
  Assert: Either succeeds silently (idempotent) or returns descriptive error
  Note: SQLite ALTER TABLE ADD COLUMN fails if column exists. Migration should
        check schema version first and skip if already at v12.
```

### SessionRecord round-trip (R-03 -- High Priority)

```
test_session_record_round_trip_with_keywords
  Arrange: Insert session via upsert_session() with all fields populated,
           keywords = Some(r#"["attr","lifecycle"]"#)
  Act: Read back via get_session() or list_sessions()
  Assert: Every field matches input, including:
    - session_id, feature_cycle, agent_role, started_at, ended_at
    - status, compaction_count, outcome, total_injections
    - keywords == Some(r#"["attr","lifecycle"]"#)

test_session_record_round_trip_without_keywords
  Arrange: Insert session with keywords = None
  Act: Read back
  Assert: keywords == None (not Some("null") or Some(""))
  Assert: All other fields correct (column index not shifted)

test_session_record_round_trip_empty_keywords
  Arrange: Insert session with keywords = Some("[]")
  Act: Read back
  Assert: keywords == Some("[]")

test_session_columns_count_matches_from_row
  Arrange: Count tokens in SESSION_COLUMNS constant string
  Act: Compare to number of column index accesses in session_from_row
  Assert: Counts match
  Note: This is a structural test to catch R-03 at compile/test time
```

### Keywords column persistence

```
test_update_session_keywords_writes_to_column
  Arrange: Insert session row (keywords NULL)
  Act: update_session_keywords(store, session_id, r#"["a","b"]"#)
  Assert: Read session, keywords == Some(r#"["a","b"]"#)

test_update_session_keywords_overwrites_existing
  Arrange: Insert session with keywords = Some(r#"["old"]"#)
  Act: update_session_keywords(store, session_id, r#"["new"]"#)
  Assert: keywords == Some(r#"["new"]"#)
```

### Keywords JSON fidelity (R-06)

```
test_keywords_json_round_trip_special_chars
  Arrange: keywords_json = r#"["has \"quotes\"","back\\slash"]"#
  Act: Store, read back, serde_json::from_str::<Vec<String>>
  Assert: Deserialized vec matches original values

test_keywords_json_unicode
  Arrange: keywords_json = r#"["\u00e9","emoji\u2764"]"#
  Act: Store, read back, deserialize
  Assert: Round-trip fidelity

test_keywords_null_vs_empty_distinction
  Arrange A: Insert with keywords = None -> NULL in SQLite
  Arrange B: Insert with keywords = Some("[]") -> "[]" in SQLite
  Act: Read both
  Assert A: keywords == None
  Assert B: keywords == Some("[]")
```

## Edge Cases

- Session inserted by old code (pre-v12) read by new code: keywords column is NULL, `session_from_row` must handle this via `row.get_optional()` or equivalent
- Migration on empty database (no sessions table rows): should succeed
- Very large valid JSON in keywords column (e.g., 5 keywords of 64 chars each): fits in TEXT column
