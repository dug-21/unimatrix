# Test Plan: uds-dispatch

## Unit Tests

### Async dispatch migration (R-02)

All existing dispatch tests must pass with `.await` added. The test function signatures become `#[tokio::test] async fn`.

| Test | Input | Expected | Risk |
|------|-------|----------|------|
| `test_dispatch_ping_returns_pong` | HookRequest::Ping | Pong { server_version: "0.1.0" } | R-02 |
| `test_dispatch_session_register_returns_ack` | SessionRegister { ... } | Ack | R-02 |
| `test_dispatch_session_close_returns_ack` | SessionClose { ... } | Ack | R-02 |
| `test_dispatch_record_event_returns_ack` | RecordEvent { ... } | Ack | R-02 |
| `test_dispatch_record_events_returns_ack` | RecordEvents { ... } | Ack | R-02 |
| `test_dispatch_unknown_returns_error` | Briefing { ... } (still a stub) | Error { ERR_UNKNOWN_REQUEST } | R-02 |

### ContextSearch handler

| Test | Input | Expected | Risk |
|------|-------|----------|------|
| `test_dispatch_context_search_returns_entries` | ContextSearch with populated store + warm embed | Entries with items | R-01 |
| `test_dispatch_context_search_embed_not_ready` | ContextSearch with EmbedNotReady | Entries { items: [], total_tokens: 0 } | R-05 |
| `test_context_search_filters_by_similarity_floor` | Entries with sim < 0.5 | Excluded from results | R-04 |
| `test_context_search_filters_by_confidence_floor` | Entries with confidence < 0.3 | Excluded from results | R-04 |
| `test_context_search_at_floor_boundaries` | Entry with sim=0.5, conf=0.3 | Included (>= comparison) | R-04 |

### CoAccessDedup

| Test | Input | Expected | Risk |
|------|-------|----------|------|
| `test_coaccess_dedup_new_set_returns_true` | check_and_insert("s1", [1, 2, 3]) first time | true | R-06 |
| `test_coaccess_dedup_duplicate_returns_false` | check_and_insert("s1", [1, 2, 3]) second time | false | R-06 |
| `test_coaccess_dedup_different_set_returns_true` | check_and_insert("s1", [1, 2, 4]) | true | R-06 |
| `test_coaccess_dedup_different_session_returns_true` | check_and_insert("s2", [1, 2, 3]) | true | R-06 |
| `test_coaccess_dedup_clear_session` | clear_session("s1") then check_and_insert("s1", same set) | true (re-inserted) | R-06 |
| `test_coaccess_dedup_canonical_ordering` | check_and_insert("s1", [3, 1, 2]) then [1, 2, 3] | second returns false (same canonical) | R-06 |

### SessionClose dedup cleanup

| Test | Input | Expected | Risk |
|------|-------|----------|------|
| `test_session_close_clears_dedup` | SessionClose with session_id "s1" | coaccess_dedup has no "s1" entries | R-06 |

## Integration Tests (Rust-level)

These require a full server setup (Store + embed service + vector index).

| Test | Scenario | Expected | Risk |
|------|----------|----------|------|
| `test_context_search_end_to_end` | Populate KB, send ContextSearch via UDS | Entries with matching items | R-01 |
| `test_context_search_empty_kb` | Empty knowledge base, send ContextSearch | Entries { items: [] } | R-04 |
| `test_concurrent_context_search` | 3 concurrent ContextSearch from different connections | All complete independently | R-11 |

## Assertions

- All existing dispatch tests pass after async migration
- ContextSearch returns Entries (not Error, not Ack)
- Entries.items contains EntryPayload with valid fields (id > 0, non-empty title/content)
- Similarity floor: entries with sim < 0.5 excluded
- Confidence floor: entries with conf < 0.3 excluded
- CoAccessDedup: canonical sort ensures [3,1,2] == [1,2,3]
- SessionClose clears dedup state for that session only
- EmbedNotReady results in empty Entries, not Error

## Edge Cases

- ContextSearch with empty query string: embedding succeeds (empty string is valid), search returns results
- ContextSearch with k=0: returns empty results
- ContextSearch with k=100: capped by HNSW index size
- All search results quarantined: returns empty Entries
