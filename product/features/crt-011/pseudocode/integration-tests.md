# crt-011: Pseudocode — integration-tests

## Component: Integration Tests

### New Consumer Dedup Unit Tests (listener.rs)

All tests in existing `mod tests` in listener.rs, using `make_store()` and `make_pending()` helpers.

#### T-CON-01: test_confidence_consumer_dedup_same_session

```pseudocode
fn test_confidence_consumer_dedup_same_session():
    store = make_store()
    pending = make_pending()
    (_, entry_store, _) = make_dispatch_deps(store)

    // Insert a real entry so fetch succeeds
    entry_id = store.insert(test_entry)

    // Insert TWO signals with SAME session_id, both referencing entry_id
    signal_1 = SignalRecord { session_id: "sess-A", entry_ids: [entry_id], type: Helpful, ... }
    signal_2 = SignalRecord { session_id: "sess-A", entry_ids: [entry_id], type: Helpful, ... }
    store.insert_signal(signal_1)
    store.insert_signal(signal_2)

    // Run consumer
    run_confidence_consumer(store, entry_store, pending).await

    // Assert: success_session_count = 1 (not 2)
    guard = pending.lock()
    analysis = guard.entries[entry_id]
    assert_eq!(analysis.success_session_count, 1)
```

#### T-CON-02: test_confidence_consumer_different_sessions_count_separately

```pseudocode
fn test_confidence_consumer_different_sessions_count_separately():
    // Same setup but TWO DIFFERENT session_ids
    signal_1 = SignalRecord { session_id: "sess-A", entry_ids: [entry_id], type: Helpful, ... }
    signal_2 = SignalRecord { session_id: "sess-B", entry_ids: [entry_id], type: Helpful, ... }

    run_confidence_consumer(...)

    // Assert: success_session_count = 2 (each session counts)
    assert_eq!(analysis.success_session_count, 2)
```

#### T-CON-03: test_retrospective_consumer_rework_session_dedup

```pseudocode
fn test_retrospective_consumer_rework_session_dedup():
    // TWO signals, SAME session_id, same entry_id, type Flagged
    store.insert_signal(SignalRecord { session_id: "sess-A", entry_ids: [entry_id], type: Flagged })
    store.insert_signal(SignalRecord { session_id: "sess-A", entry_ids: [entry_id], type: Flagged })

    run_retrospective_consumer(...)

    // Assert: rework_session_count = 1 (deduped)
    assert_eq!(analysis.rework_session_count, 1)
    // Assert: rework_flag_count = 2 (NOT deduped, per ADR-002)
    assert_eq!(analysis.rework_flag_count, 2)
```

#### T-CON-04: test_retrospective_consumer_flag_count_not_deduped

```pseudocode
fn test_retrospective_consumer_flag_count_not_deduped():
    // Same as T-CON-03 but explicitly focused on flag_count
    // THREE signals, same session_id, same entry_id
    store.insert_signal(SignalRecord { session_id: "sess-A", entry_ids: [entry_id], type: Flagged })
    store.insert_signal(SignalRecord { session_id: "sess-A", entry_ids: [entry_id], type: Flagged })
    store.insert_signal(SignalRecord { session_id: "sess-A", entry_ids: [entry_id], type: Flagged })

    run_retrospective_consumer(...)

    // Assert: rework_flag_count = 3 (every event counted)
    assert_eq!(analysis.rework_flag_count, 3)
    // Assert: rework_session_count = 1 (only one session)
    assert_eq!(analysis.rework_session_count, 1)
```

### Handler-Level Integration Tests

#### T-INT-01: test_mcp_usage_confidence_recomputed (usage.rs)

```pseudocode
fn test_mcp_usage_confidence_recomputed():
    (service, store, _dir) = make_usage_service()
    id = insert_test_entry(store)

    // Record usage via MCP path
    service.record_access([id], AccessSource::McpTool, UsageContext {
        agent_id: "agent-1", helpful: Some(true), ...
    })
    sleep(50ms)  // wait for spawn_blocking

    entry = store.get(id)
    assert!(entry.confidence > 0.0)
    assert_eq!(entry.access_count, 1)
    assert_eq!(entry.helpful_count, 1)
```

#### T-INT-02: test_mcp_usage_dedup_prevents_double_access (usage.rs)

```pseudocode
fn test_mcp_usage_dedup_prevents_double_access():
    (service, store, _dir) = make_usage_service()
    id = insert_test_entry(store)

    // Record SAME agent+entry TWICE
    service.record_access([id], AccessSource::McpTool, ctx_agent_1)
    service.record_access([id], AccessSource::McpTool, ctx_agent_1)
    sleep(50ms)

    entry = store.get(id)
    assert_eq!(entry.access_count, 1)  // deduped by UsageDedup
```

#### T-INT-03 and T-INT-04: server.rs

Check if existing tests `test_confidence_updated_on_retrieval` and `test_record_usage_for_entries_access_dedup` already cover these scenarios. If they do, document the mapping. If not, add new tests following the same make_server() pattern.

## Dependencies

- `SignalRecord` struct from `unimatrix_store::signal`
- `Store::insert_signal()` method
- `Store::drain_signals()` method
- Existing test helpers in each module
