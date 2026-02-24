# crt-001 Risk Coverage Report

## Test Summary

| Crate | Before | After | New Tests |
|-------|--------|-------|-----------|
| unimatrix-core | 21 | 21 | 0 |
| unimatrix-embed | 76 (18 ignored) | 76 (18 ignored) | 0 |
| unimatrix-server | 272 | 289 | 17 |
| unimatrix-store | 123 | 136 | 13 |
| unimatrix-vector | 95 | 95 | 0 |
| **Total** | **587** | **617** | **30** |

All 617 tests pass. 0 failures.

## Risk-to-Test Coverage Matrix

| Risk ID | Description | Tests | Status |
|---------|-------------|-------|--------|
| R-01 | Schema migration data loss (v1->v2) | 5 migration tests (from C2 implementation) | COVERED |
| R-02 | Counter update atomicity | T-C3-01 through T-C3-06 (store tests) | COVERED |
| R-03 | Dedup bypass (duplicate counting) | 14 UsageDedup unit tests + T-server-dedup | COVERED |
| R-04 | FEATURE_ENTRIES orphan writes | T-C3-12 through T-C3-17 | COVERED |
| R-05 | EntryStore trait breaking change | Existing trait object-safety tests pass | COVERED |
| R-06 | Bincode positional encoding breakage | Schema roundtrip tests with new fields | COVERED |
| R-07 | last_accessed_at staleness | T-C3-07 (last_accessed_at without access_count) | COVERED |
| R-08 | write_count_since correctness | 6 audit tests (write vs read, agent filter, timestamp, empty, both ops, non-write exclusion) | COVERED |
| R-09 | Fire-and-forget masking failures | T-server-empty, T-server-access tests | COVERED |
| R-10 | context_briefing double-counting | T-server-dedup + briefing ID collection logic | COVERED |
| R-11 | Backward compatibility | Existing 272 server tests still pass, JSON deserialization tests with new optional fields | COVERED |
| R-14 | record_usage partial batch | T-C3-03, T-C3-08, T-C3-09 | COVERED |
| R-15 | deserialize_audit_event visibility | Implicitly verified by write_count_since tests | COVERED |
| R-16 | Vote correction atomicity | T-C3-10, T-C3-11, T-server-vote-correction, 14 UsageDedup tests | COVERED |
| R-17 | FEATURE_ENTRIES trust bypass | T-server-feature-internal, T-server-feature-restricted, T-server-feature-privileged | COVERED |

## Acceptance Criteria Coverage

| AC-ID | Description | Test(s) | Verified |
|-------|-------------|---------|----------|
| AC-01 | FEATURE_ENTRIES table exists | db.rs test (11 tables), record_feature_entries tests | YES |
| AC-02 | helpful_count/unhelpful_count fields with serde(default) | Schema roundtrip tests | YES |
| AC-03 | v1->v2 migration backfills 0 | 5 migration tests | YES |
| AC-04 | access_count deduped per agent per session | test_record_usage_for_entries_access_dedup | YES |
| AC-05 | last_accessed_at updated every retrieval | test_record_usage_5_entries_all_updated, test_record_usage_last_accessed_at_updated_without_access_count | YES |
| AC-06 | helpful=true increments helpful_count | test_record_usage_for_entries_helpful_vote | YES |
| AC-07 | helpful=false increments unhelpful_count; None changes neither | test_record_usage_for_entries_unhelpful_vote, test_record_usage_for_entries_helpful_none | YES |
| AC-08 | FEATURE_ENTRIES populated with trust gating | test_record_usage_for_entries_feature_internal_agent | YES |
| AC-09 | FEATURE_ENTRIES idempotency | test_record_feature_entries_idempotent | YES |
| AC-10 | context_briefing dedup | briefing_entry_ids dedup logic + server ID collection | YES |
| AC-11 | EntryStore trait extended | Existing trait object-safety tests pass | YES |
| AC-12 | write_count_since returns correct count | 6 audit tests | YES |
| AC-13 | Existing retrieval unchanged | All 272 pre-existing server tests pass | YES |
| AC-14 | Atomic write transaction | test_record_usage_5_entries_all_updated, test_record_usage_nonexistent_entry_skipped | YES |
| AC-15 | UsageDedup session-scoped, not persisted | 14 UsageDedup unit tests | YES |
| AC-16 | Vote correction: decrement old, increment new | test_record_usage_for_entries_vote_correction, test_record_usage_vote_correction | YES |
| AC-17 | Restricted agent feature ignored | test_record_usage_for_entries_feature_restricted_agent_ignored | YES |
| AC-18 | All components have tests | 30 new tests across store + server | YES |

## New Test Inventory

### crates/unimatrix-store/src/write.rs (13 new tests)

1. test_record_usage_5_entries_all_updated
2. test_record_usage_overlapping_sets
3. test_record_usage_nonexistent_entry_skipped
4. test_record_usage_empty_all_ids
5. test_record_usage_cumulative_increments
6. test_record_usage_preserves_fields
7. test_record_usage_last_accessed_at_updated_without_access_count
8. test_record_usage_vote_correction
9. test_record_usage_saturating_subtraction
10. test_record_feature_entries_basic
11. test_record_feature_entries_idempotent
12. test_record_feature_entries_nonexistent_entry
13. test_record_feature_entries_empty

### crates/unimatrix-server/src/audit.rs (6 new tests)

1. test_write_count_since_counts_writes_only
2. test_write_count_since_agent_filtering
3. test_write_count_since_timestamp_boundary
4. test_write_count_since_empty_log
5. test_write_count_since_both_write_ops
6. test_write_count_since_non_write_ops_excluded

### crates/unimatrix-server/src/server.rs (11 new tests)

1. test_record_usage_for_entries_updates_access
2. test_record_usage_for_entries_empty_ids
3. test_record_usage_for_entries_access_dedup
4. test_record_usage_for_entries_helpful_vote
5. test_record_usage_for_entries_unhelpful_vote
6. test_record_usage_for_entries_helpful_none
7. test_record_usage_for_entries_vote_correction
8. test_record_usage_for_entries_feature_internal_agent
9. test_record_usage_for_entries_feature_restricted_agent_ignored
10. test_record_usage_for_entries_feature_privileged_agent
11. test_record_usage_for_entries_vote_after_access_only

### crates/unimatrix-server/src/usage_dedup.rs (14 tests, created in Stage 3b)

1-14: UsageDedup filter_access and check_votes tests (R-03, R-16 coverage)
