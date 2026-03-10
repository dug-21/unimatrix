# Agent Report: col-020-agent-5-store-api

## Component: C4 -- Store API Extensions

## Files Modified

- `/workspaces/unimatrix/.claude/worktrees/agent-a517f895/crates/unimatrix-store/src/query_log.rs`
- `/workspaces/unimatrix/.claude/worktrees/agent-a517f895/crates/unimatrix-store/src/injection_log.rs`
- `/workspaces/unimatrix/.claude/worktrees/agent-a517f895/crates/unimatrix-store/src/read.rs`
- `/workspaces/unimatrix/.claude/worktrees/agent-a517f895/crates/unimatrix-store/src/topic_deliveries.rs`

## Methods Added

1. **`scan_query_log_by_sessions(&self, session_ids: &[&str]) -> Result<Vec<QueryLogRecord>>`** (query_log.rs)
   - Batch query with IN clause, chunked to 50 session IDs per batch
   - Returns empty Vec for empty input (no SQL executed)
   - Results ordered by ts ASC within each chunk

2. **`scan_injection_log_by_sessions(&self, session_ids: &[&str]) -> Result<Vec<InjectionLogRecord>>`** (injection_log.rs)
   - Same chunking pattern as query_log variant
   - Results ordered by log_id within each chunk

3. **`count_active_entries_by_category(&self) -> Result<HashMap<String, u64>>`** (read.rs)
   - SQL GROUP BY with WHERE status = 0 (Active only)
   - Excludes Deprecated (1), Proposed (2), Quarantined (3)

4. **`set_topic_delivery_counters(&self, topic: &str, total_sessions: i64, total_tool_calls: i64, total_duration_secs: i64) -> Result<()>`** (topic_deliveries.rs)
   - Absolute-set UPDATE (ADR-002), not additive
   - Returns StoreError::Deserialization if topic not found (same pattern as update_topic_delivery_counters)
   - Does not touch non-counter fields (status, github_issue, phases_completed, etc.)

## Tests

- **13 new tests, all passing**
- 86 total unimatrix-store tests pass (0 failures)

| Test | File |
|------|------|
| test_scan_query_log_by_sessions_returns_matching | query_log.rs |
| test_scan_query_log_by_sessions_empty_ids | query_log.rs |
| test_scan_query_log_by_sessions_no_matching | query_log.rs |
| test_scan_injection_log_by_sessions_returns_matching | injection_log.rs |
| test_scan_injection_log_by_sessions_empty_ids | injection_log.rs |
| test_count_active_entries_by_category_basic | read.rs |
| test_count_active_entries_by_category_excludes_quarantined | read.rs |
| test_count_active_entries_by_category_empty_store | read.rs |
| test_set_topic_delivery_counters_basic | topic_deliveries.rs |
| test_set_topic_delivery_counters_idempotent | topic_deliveries.rs |
| test_set_topic_delivery_counters_overwrite | topic_deliveries.rs |
| test_set_topic_delivery_counters_missing_record | topic_deliveries.rs |
| test_set_topic_delivery_counters_preserves_non_counter_fields | topic_deliveries.rs |

## Issues / Blockers

None. All four methods implemented per pseudocode, all tests pass per test plan.

## Verification

- [x] `cargo build -p unimatrix-store` passes
- [x] `cargo test -p unimatrix-store` passes (86 tests, 0 failures)
- [x] `cargo clippy -p unimatrix-store` clean
- [x] `cargo fmt -p unimatrix-store` applied
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK`
- [x] No `.unwrap()` in non-test code
- [x] All new methods follow existing error handling patterns
