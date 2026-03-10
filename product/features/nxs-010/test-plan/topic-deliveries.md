# Test Plan: topic-deliveries (C3)

## Component

`crates/unimatrix-store/src/topic_deliveries.rs` -- `TopicDeliveryRecord` struct and Store API: `upsert_topic_delivery`, `get_topic_delivery`, `update_topic_delivery_counters`, `list_topic_deliveries`.

## Risks Covered

| Risk ID | Priority | Scenarios |
|---------|----------|-----------|
| R-07 | High | update_counters on nonexistent topic |
| R-10 | Critical | INSERT OR REPLACE semantics |

## Unit Tests

### test_upsert_topic_delivery_insert

**Arrange**: Open fresh `TestDb`. Build a `TopicDeliveryRecord` with topic="nxs-010", created_at=1000, status="active", total_sessions=0.
**Act**: Call `store.upsert_topic_delivery(&record)`.
**Assert**:
- Returns `Ok(())`.
- `store.get_topic_delivery("nxs-010")` returns `Some(record)` with all fields matching.

**AC**: AC-07

### test_upsert_topic_delivery_replace

**Arrange**: Open fresh `TestDb`. Insert a topic delivery with total_sessions=5, status="active".
**Act**: Call `upsert_topic_delivery` with the same topic but status="completed", total_sessions=0.
**Assert**:
- Returns `Ok(())`.
- `get_topic_delivery` returns record with status="completed" and total_sessions=0 (replaced, not merged).

**AC**: AC-07 (R-10 -- documents replace semantics)

### test_upsert_replace_overwrites_counters

**Arrange**: Insert topic with total_sessions=5, total_tool_calls=10. Call `update_topic_delivery_counters` to set counters to 8, 15.
**Act**: Call `upsert_topic_delivery` with total_sessions=0, total_tool_calls=0.
**Assert**:
- `get_topic_delivery` shows total_sessions=0, total_tool_calls=0.
- Confirms INSERT OR REPLACE destroys previous counter values.

**AC**: R-10 documentation test

### test_get_topic_delivery_not_found

**Arrange**: Open fresh `TestDb`.
**Act**: Call `store.get_topic_delivery("nonexistent")`.
**Assert**: Returns `Ok(None)`.

**AC**: AC-08

### test_get_topic_delivery_all_fields

**Arrange**: Insert a `TopicDeliveryRecord` with all fields populated including `completed_at=Some(2000)`, `github_issue=Some(42)`, `phases_completed=Some("design,delivery")`.
**Act**: Call `get_topic_delivery`.
**Assert**: All fields round-trip correctly. `completed_at` is `Some(2000)`. `github_issue` is `Some(42)`. `phases_completed` is `Some("design,delivery")`.

**AC**: AC-07 (full field coverage)

### test_update_topic_delivery_counters_increment

**Arrange**: Insert topic with total_sessions=2, total_tool_calls=10, total_duration_secs=500.
**Act**: Call `update_topic_delivery_counters("topic", 3, 5, 100)`.
**Assert**:
- `get_topic_delivery` returns total_sessions=5, total_tool_calls=15, total_duration_secs=600.

**AC**: AC-09

### test_update_topic_delivery_counters_decrement

**Arrange**: Insert topic with total_sessions=10, total_tool_calls=20, total_duration_secs=1000.
**Act**: Call `update_topic_delivery_counters("topic", -3, -5, -100)`.
**Assert**:
- `get_topic_delivery` returns total_sessions=7, total_tool_calls=15, total_duration_secs=900.

**AC**: AC-09 (correction scenario)

### test_update_topic_delivery_counters_nonexistent_topic_returns_error

**Arrange**: Open fresh `TestDb` (no topic deliveries).
**Act**: Call `update_topic_delivery_counters("missing", 1, 1, 1)`.
**Assert**: Returns `Err(...)`. Not `Ok(())`.

**AC**: AC-09 (R-07 mitigation)

### test_list_topic_deliveries_empty

**Arrange**: Open fresh `TestDb`.
**Act**: Call `store.list_topic_deliveries()`.
**Assert**: Returns `Ok(vec![])`.

### test_list_topic_deliveries_ordered_by_created_at_desc

**Arrange**: Insert 3 topics with created_at values 1000, 3000, 2000.
**Act**: Call `store.list_topic_deliveries()`.
**Assert**:
- Returns 3 records.
- Order: created_at=3000 first, then 2000, then 1000.

**AC**: FR-04.5

### test_upsert_topic_delivery_nullable_fields

**Arrange**: Insert topic with completed_at=None, github_issue=None, phases_completed=None.
**Act**: Read back via `get_topic_delivery`.
**Assert**: All optional fields are `None`.

## Edge Cases

| Edge Case | Test |
|-----------|------|
| Nonexistent topic for counter update | test_update_topic_delivery_counters_nonexistent_topic_returns_error |
| Replace overwrites counters | test_upsert_replace_overwrites_counters |
| Negative counter deltas | test_update_topic_delivery_counters_decrement |
| All nullable fields None | test_upsert_topic_delivery_nullable_fields |
| Empty list | test_list_topic_deliveries_empty |
