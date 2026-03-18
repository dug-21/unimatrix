# Investigator Report: 302-investigator

## Root Cause

Both bugs share one root cause: `write_pool` (max_connections=1, correct by ADR-001 nxs-011) is starved by the analytics drain task.

`StoreService::insert()` in `store_ops.rs:215` calls `self.audit.log_event()` synchronously via `block_in_place`, which races with the drain task holding the single write connection. After 5s the pool acquire times out → `StoreError::PoolTimeout` → `ServerError::Core` → `-32603`.

Every other audit-emission site uses fire-and-forget. This call site was not converted during nxs-011.

**Bug B (auto_enroll → -32003)**: `agent_resolve_or_enroll()` INSERT also contends for the same write pool. If it times out, the agent is never enrolled → capability check finds no record → `-32003`. Same root cause.

## Affected Files

| File | Function | Role in Bug |
|------|----------|-------------|
| `crates/unimatrix-server/src/services/store_ops.rs:215` | `StoreService::insert()` | Synchronous `audit.log_event()` call — primary fix target |
| `crates/unimatrix-server/src/infra/audit.rs` | `AuditLog::log_event()` | Acquires `write_pool.begin()` — contention point |
| `crates/unimatrix-store/src/registry.rs:125–164` | `agent_resolve_or_enroll()`, `agent_update_last_seen()` | Direct write pool acquires — also contend with drain task |
| `crates/unimatrix-store/src/analytics.rs` | `run_drain_task()` | Holds `write_pool.begin()` while committing batches |
| `crates/unimatrix-server/src/services/store_correct.rs` | correct path | Also calls `audit.log_event()` synchronously |

## Proposed Fix Approach

Change `store_ops.rs` and `store_correct.rs` audit calls from synchronous to fire-and-forget, consistent with all other audit sites. Add `log_event_async()` method to `audit.rs` for use from async context.

## Risk Assessment

Low. Audit writes become best-effort — same trade-off accepted at all other audit sites. A crash immediately post-store could lose one audit record. No data loss for actual entries.

## Missing Test

Integration test that floods the analytics queue (to keep drain task busy), then calls `context_store` and asserts success. The race is deterministic with max_connections=1 and an active drain task.

## Knowledge Stewardship

- Queried: Unimatrix entries #2130 (write_pool max_connections=1 lesson from nxs-011), #2059 (ADR-002 nxs-011 SqliteWriteTransaction retirement), #2060 (ADR-003 nxs-011 migration connection sequencing). These established design intent but did not anticipate the synchronous audit write contention.
- Declined: attempted `context_store` for "write_pool max_connections=1 + drain task + synchronous block_in_place audit write = deadlock" lesson — server returned `-32603` (the live server has the unfixed code). Will be stored after deployment.
