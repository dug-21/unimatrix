# C8: Query Test Plan

## R7/AC-17: QueryFilter Combined Query

### test_empty_filter_returns_all_active
- Insert Active + Deprecated entries
- query(QueryFilter::default()) returns only Active entries

### test_single_field_topic
- QueryFilter { topic: Some("auth"), ..default }
- Returns entries matching topic (regardless of status)

### test_single_field_status
- QueryFilter { status: Some(Deprecated), ..default }
- Returns only Deprecated entries

### test_two_fields_topic_and_status
- Filter: topic="auth" + status=Active
- Returns only entries that are both topic="auth" AND Active

### test_two_fields_tags_and_status
- Filter: tags=["rust"] + status=Active
- Returns intersection

### test_three_fields_topic_tags_time
- Filter: topic + tags + time_range
- Returns intersection of all three

### test_all_fields_populated
- Filter with all 5 fields set
- Returns only entries matching ALL criteria

### test_disjoint_filters_empty_result
- Topic exists, category exists, but no single entry matches both
- Returns empty vec

### test_nonexistent_topic_filter
- Filter with topic that matches nothing
- Returns empty vec, not error

### test_status_deprecated_filter
- Filter with status=Deprecated
- Returns only deprecated entries

### test_50_entries_varied_subsets
- Insert 50 entries with varied topics/categories/tags/statuses
- Apply various QueryFilters and verify results match expected subsets

### test_query_filter_time_range_inverted
- QueryFilter with time_range where start > end
- That filter dimension effectively contributes nothing (or is skipped)
