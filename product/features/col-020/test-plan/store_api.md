# Test Plan: C4 -- store_api

Modules:
- `crates/unimatrix-store/src/query_log.rs`
- `crates/unimatrix-store/src/injection_log.rs`
- `crates/unimatrix-store/src/read.rs`
- `crates/unimatrix-store/src/topic_deliveries.rs`

## Unit Tests (with real Store)

All tests use `Store::open_tmp()` or equivalent ephemeral DB.

### scan_query_log_by_sessions

#### test_scan_query_log_by_sessions_returns_matching
- **Setup**: Insert 5 query_log rows across 3 session_ids. Query with 2 of those session_ids.
- **Assert**: Returns rows only for the 2 requested sessions.

#### test_scan_query_log_by_sessions_empty_ids
- **Setup**: Some query_log rows.
- **Assert**: scan_query_log_by_sessions(&[]) returns empty Vec, no SQL error.
- **Risks**: R-11

#### test_scan_query_log_by_sessions_no_matching
- **Setup**: query_log rows for session "s1".
- **Assert**: scan_query_log_by_sessions(&["s99"]) returns empty Vec.

### scan_injection_log_by_sessions

#### test_scan_injection_log_by_sessions_returns_matching
- **Setup**: Insert injection_log rows for sessions "s1", "s2", "s3". Query for "s1" and "s3".
- **Assert**: Returns rows for "s1" and "s3" only.

#### test_scan_injection_log_by_sessions_empty_ids
- **Setup**: Some injection_log rows.
- **Assert**: scan_injection_log_by_sessions(&[]) returns empty Vec.
- **Risks**: R-11

### count_active_entries_by_category

#### test_count_active_entries_by_category_basic
- **Setup**: Store 3 active entries: 2 "convention", 1 "pattern". Deprecate 1 "convention".
- **Assert**: Returns {"convention": 1, "pattern": 1} (deprecated excluded).

#### test_count_active_entries_by_category_excludes_quarantined
- **Setup**: Store 2 entries. Quarantine 1.
- **Assert**: Quarantined entry not counted.

#### test_count_active_entries_by_category_empty_store
- **Setup**: Empty store.
- **Assert**: Returns empty HashMap.

### set_topic_delivery_counters

#### test_set_topic_delivery_counters_basic
- **Setup**: upsert_topic_delivery for topic "test-topic". Then set_topic_delivery_counters("test-topic", 5, 100, 3600).
- **Assert**: get_topic_delivery("test-topic") returns record with total_sessions=5, total_tool_calls=100, total_duration_secs=3600.
- **Risks**: R-05

#### test_set_topic_delivery_counters_idempotent
- **Setup**: Set counters to (5, 100, 3600). Set again to (5, 100, 3600).
- **Assert**: Values unchanged after second call.
- **Risks**: R-05
- **AC**: AC-12

#### test_set_topic_delivery_counters_overwrite
- **Setup**: Set counters to (5, 100, 3600). Then set to (10, 200, 7200).
- **Assert**: Values are (10, 200, 7200), not (15, 300, 10800) -- absolute set, not additive.
- **Risks**: R-05
- **AC**: AC-12

#### test_set_topic_delivery_counters_missing_record
- **Setup**: Call set_topic_delivery_counters for a topic with no existing record.
- **Assert**: Returns error or creates record (per implementation decision). Does not panic.
- **Risks**: R-05
