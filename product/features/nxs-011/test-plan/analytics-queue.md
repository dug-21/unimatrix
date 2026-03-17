# Test Plan: AnalyticsQueue + AnalyticsWrite (analytics.rs)

**Component**: `crates/unimatrix-store/src/analytics.rs`
**Risks**: R-02 (drain task teardown race), R-04 (shed silent data loss), R-06 (integrity write contamination), R-13 (variant field mismatch)
**ACs**: AC-06, AC-07, AC-08, AC-15, AC-18, AC-19

---

## Unit Tests (`#[tokio::test]` in `unimatrix-store/src/analytics.rs` or `tests/analytics_tests.rs`)

### AQ-U-01: `test_shed_counter_increments_on_full_queue` — (AC-06, AC-15)
- **Arrange**: Open store with `PoolConfig { analytics_queue_capacity: 1000 }` (or rely on `ANALYTICS_QUEUE_CAPACITY`); pause the drain task (use a long sleep or suspend drain task for test isolation)
- **Act**: Enqueue exactly 1000 events (fills channel); enqueue one more
- **Assert**: `store.shed_events_total() == 1`
- **Assert**: A WARN log is emitted containing: variant name string, `queue_len == 1000`, `capacity == 1000`
- **Teardown**: `store.close().await`
- **Risk**: R-04

### AQ-U-02: `test_shed_counter_cumulates_multiple_events`
- **Arrange**: Saturate queue (1000 events); pause drain task
- **Act**: Enqueue 5 more events
- **Assert**: `store.shed_events_total() == 5`
- **Teardown**: `store.close().await`
- **Risk**: R-04

### AQ-U-03: `test_shed_counter_zero_on_fresh_store`
- **Arrange**: `SqlxStore::open(temp_db, PoolConfig::test_default()).await`
- **Act**: Call `store.shed_events_total()`
- **Assert**: Returns `0`
- **Teardown**: `store.close().await`
- **Risk**: R-04

### AQ-U-04: `test_enqueue_analytics_does_not_acquire_write_pool`
- **Arrange**: Saturate write_pool (hold both connections in open transactions)
- **Act**: Call `store.enqueue_analytics(AnalyticsWrite::CoAccess { id_a: 1, id_b: 2 })` — must be `fn`, not `async fn`
- **Assert**: Returns immediately (does not block waiting for write_pool connection); no timeout error
- **Assert**: `shed_events_total()` reflects queue state, not pool state
- **Teardown**: `store.close().await`
- **Risk**: R-06 (enqueue path must not acquire a pool connection)

### AQ-U-05: `test_drain_batch_size_exactly_50_commits_once` — (AC-06)
- **Arrange**: Open store; pause drain task until exactly 50 events are in channel
- **Act**: Resume drain task; wait for batch commit
- **Assert**: Exactly 50 rows in the target analytics table; the drain task committed in one transaction (verify via single `txn.commit()` log or row count)
- **Teardown**: `store.close().await`
- **Risk**: R-04 (batch size limit)

### AQ-U-06: `test_drain_batch_size_51_commits_in_two_batches` — (AC-06)
- **Arrange**: Enqueue 51 events; allow drain to run
- **Act**: `store.close().await` (forces final flush)
- **Assert**: All 51 rows committed; shed_counter == 0
- **Teardown**: (close already called)
- **Risk**: R-04

### AQ-U-07: `test_drain_single_event_batch`
- **Arrange**: Enqueue 1 event
- **Act**: `store.close().await`
- **Assert**: 1 row committed; no duplication
- **Teardown**: (close already called)
- **Risk**: Edge case 2 from RISK-TEST-STRATEGY.md

### AQ-U-08: `test_drain_flush_interval_partial_batch`
- **Arrange**: Enqueue 10 events; do NOT send more; wait 600ms (> `DRAIN_FLUSH_INTERVAL` 500ms)
- **Assert**: All 10 events committed within 700ms without calling `store.close()`
- **Teardown**: `store.close().await`
- **Risk**: NF-04 (flush interval guarantee)

### AQ-U-09: `test_drain_idle_does_not_spin_loop`
- **Arrange**: Open store with empty queue; wait 2s
- **Assert**: No events were committed (no spurious commits); drain task did not spin (measure by absence of excessive log output or CPU-check via side-channel if feasible)
- **Teardown**: `store.close().await`
- **Risk**: Edge case 1 from RISK-TEST-STRATEGY.md

---

## Integration Tests (`#[tokio::test]` in `unimatrix-store/tests/`)

### AQ-I-01: `test_store_close_awaits_drain_task_exit` — (AC-19)
- **Arrange**: `SqlxStore::open(temp_db, PoolConfig::test_default()).await`; enqueue 10 `AnalyticsWrite::CoAccess` events
- **Act**: `store.close().await`
- **Assert**: All 10 co_access rows present in database after close returns
- **Assert**: `drain_handle` join future resolved (no live drain task)
- **Risk**: R-02

### AQ-I-02: `test_store_close_commits_events_before_returning` — (AC-19)
- **Arrange**: Enqueue 30 events; call `store.close().await`
- **Assert**: All 30 rows in analytics tables; no partial commit
- **Risk**: R-02

### AQ-I-03: `test_shutdown_signal_during_active_batch_completes_gracefully`
- **Arrange**: Enqueue 40 events (below batch limit); send close signal while drain is mid-batch
- **Act**: `store.close().await`
- **Assert**: All 40 events committed; no panic; returns within 5s grace period
- **Risk**: R-02 (edge case 5 from RISK-TEST-STRATEGY.md)

### AQ-I-04: `test_integrity_write_survives_full_analytics_queue` — (AC-08)
- **Arrange**: Open store; pause drain task; enqueue 1000 events (fill queue to capacity)
- **Act**: Call `store.write_entry(new_entry)` (integrity path)
- **Assert**: `write_entry` returns `Ok(entry_id)`; entry is readable via `store.get_entry(entry_id).await`
- **Assert**: `shed_events_total() == 0` (no integrity write was shed)
- **Teardown**: `store.close().await`
- **Risk**: R-06 (AC-08)

### AQ-I-05: `test_audit_log_write_survives_full_analytics_queue` — (AC-08)
- **Arrange**: Same queue saturation as AQ-I-04
- **Act**: Call an integrity write that inserts into `audit_log`
- **Assert**: `audit_log` row is present; no error returned to caller
- **Teardown**: `store.close().await`
- **Risk**: R-06 (AC-08 for audit_log)

### AQ-I-06: `test_analytics_queue_routing_co_access` — (AC-07)
- **Arrange**: Open store; enqueue `AnalyticsWrite::CoAccess { id_a: 1, id_b: 2 }`
- **Act**: `store.close().await` (forces drain flush)
- **Assert**: Row exists in `co_access` table with `id_a=1`, `id_b=2`
- **Risk**: R-06, R-13 (field mapping correctness)

### AQ-I-07: `test_analytics_queue_routing_session_update` — (AC-07)
- **Arrange**: Enqueue `AnalyticsWrite::SessionUpdate { .. }` with known field values
- **Act**: `store.close().await`
- **Assert**: Corresponding row in `sessions` table with correct field values
- **Risk**: R-13

### AQ-I-08: `test_analytics_queue_routing_query_log` — (AC-07)
- **Arrange**: Enqueue `AnalyticsWrite::QueryLog { session_id: "s1", query_text: "test", .. }`
- **Act**: `store.close().await`
- **Assert**: Row in `query_log` with matching fields
- **Risk**: R-13

### AQ-I-09: `test_analytics_queue_routing_outcome_index` — (AC-07)
- **Arrange**: Enqueue `AnalyticsWrite::OutcomeIndex { feature_cycle: "nxs-011", entry_id: 42 }`
- **Act**: `store.close().await`
- **Assert**: Row in `outcome_index` with `feature_cycle="nxs-011"`, `entry_id=42`
- **Risk**: R-13

### AQ-I-10: `test_shed_events_visible_in_context_status` — (AC-18)
- **Arrange**: Start server (via SqlxStore); pause drain task; enqueue 1000 events; enqueue 5 more (induces 5 shed events)
- **Act**: Call `context_status` MCP tool (or call `store.shed_events_total()` directly if integration at Rust level)
- **Assert**: Response contains `shed_events_total == 5`
- **Teardown**: `store.close().await`
- **Risk**: R-04 (AC-18)

### AQ-I-11: `test_drain_task_panic_surfaces_as_drain_task_panic_error`
- **Arrange**: Use a mock/test version of drain task that panics on first batch
- **Act**: `store.close().await`
- **Assert**: `close()` returns (does not deadlock); panic is caught by JoinHandle; result is `StoreError::DrainTaskPanic` or a similar indication
- **Risk**: R-02 (drain task panic handling)

---

## Static Verification (grep, code review gates)

### AQ-S-01: Analytics methods call `enqueue_analytics`, not `write_pool` — (AC-07)
- **Check**: `grep -rn "enqueue_analytics" crates/unimatrix-store/src/write.rs` matches entries for each analytics table
- **Check**: `grep -rn "write_pool" crates/unimatrix-store/src/write.rs` does NOT match any analytics table write

### AQ-S-02: Integrity methods call `write_pool`, not `enqueue_analytics` — (AC-08)
- **Check**: `grep -n "enqueue_analytics" crates/unimatrix-store/src/write.rs` must NOT contain lines for: `entries`, `entry_tags`, `audit_log`, `agent_registry`, `vector_map`, `counters`

---

## Notes

- TC-05 applies to AQ-I-06 through AQ-I-09: always call `store.close().await` before asserting analytics rows.
- For queue saturation tests (AQ-U-01, AQ-I-04, AQ-I-05): the drain task must be suspended or slowed so the queue actually fills. Use a test-only config or channel mock if needed.
- AQ-I-10 is the formal AC-18 gate. If `context_status` is only accessible via MCP, use the server integration test harness; if `shed_events_total()` is accessible directly, test at the Rust level.
