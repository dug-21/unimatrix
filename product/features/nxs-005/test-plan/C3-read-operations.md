# Test Plan: C3 Read Operations

## Existing Tests (run unchanged on both backends)

From read.rs:
- test_get_returns_inserted_entry
- test_get_nonexistent_returns_error
- test_query_by_topic_returns_matching
- test_query_by_topic_nonexistent
- test_query_by_category_returns_matching
- test_query_by_category_nonexistent
- test_query_single_tag
- test_query_two_tag_intersection
- test_query_three_tag_intersection
- test_query_nonexistent_tag
- test_query_empty_tags
- test_time_range_inclusive
- test_time_range_inverted
- test_time_range_empty
- test_query_by_status_active
- test_query_by_status_deprecated
- test_exists_true
- test_exists_false
- test_read_counter_missing_key
- test_iter_vector_mappings_empty
- test_iter_vector_mappings_populated
- test_iter_vector_mappings_after_overwrite
- test_iter_vector_mappings_consistency_with_get
- test_iter_vector_mappings_after_delete
- test_read_counter_after_inserts
- test_get_co_access_partners_as_min
- test_get_co_access_partners_as_max
- test_get_co_access_partners_staleness_filter
- test_get_co_access_partners_no_partners
- test_co_access_stats
- test_top_co_access_pairs_ordering_and_limit
- test_co_access_stats_empty_table
- test_store_and_get_metrics_roundtrip
- test_get_metrics_nonexistent
- test_list_all_metrics_empty
- test_list_all_metrics_multiple
- test_store_metrics_overwrites

From query.rs:
- test_empty_filter_returns_all_active
- test_single_field_topic
- test_single_field_status
- test_two_fields_topic_and_status
- test_two_fields_tags_and_status
- test_disjoint_filters_empty_result
- test_nonexistent_topic_filter
- test_all_fields_populated
- test_50_entries_varied_subsets

## Co-Access Test Adjustments

Several co-access read tests in read.rs seed data by directly accessing `store.db.begin_write()` to insert into the CO_ACCESS table. Under SQLite, these need to use the Store's public API `record_co_access_pairs()` instead, or use a test helper that seeds co-access data through the store's connection.

Strategy:
- Add a `#[cfg(feature = "backend-sqlite")]` block in each test that uses the public API
- OR add a test-only helper method to Store for seeding co-access data
- Prefer the public API approach since record_co_access_pairs already exists

## New SQLite-Specific Tests

### AC-04: Vector Mapping Roundtrip
```
test_vector_mapping_full_roundtrip:
  put_vector_mapping(1, 100)
  put_vector_mapping(2, 200)
  get_vector_mapping(1) -> Some(100)
  get_vector_mapping(2) -> Some(200)
  iter_vector_mappings -> [(1, 100), (2, 200)] (sorted by entry_id)
```

### R-01: Edge Case Queries
```
test_query_by_status_with_all_four_statuses:
  insert entries with Active, Deprecated, Proposed, Quarantined
  query each status -> correct count

test_query_boundary_timestamps:
  time_range with start=0, end=u64::MAX -> returns all
  time_range with start=u64::MAX, end=0 -> returns empty (inverted)
```

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-01 | All existing read tests + edge case queries |
| R-06 | iter_vector_mappings ordering matches redb |
