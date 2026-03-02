# Pseudocode: session-gc

Component: Session GC with INJECTION_LOG Cascade (P0)
Files: `crates/unimatrix-server/src/tools.rs` (maintain path), `crates/unimatrix-store/src/sessions.rs` (gc_sessions)

---

## Purpose

When the MCP `context_status` tool is called with `maintain=true`, trigger GC of stale sessions. Mark Active sessions older than 24h as TimedOut. Delete sessions older than 30 days and cascade-delete their INJECTION_LOG records — all in one atomic write transaction.

---

## 1. GC Logic (sessions.rs) — Already Documented in storage-layer.md

The `gc_sessions` function is defined on `Store` in `sessions.rs`. See `storage-layer.md` §2 for the full 5-phase pseudocode. Key points:

- All 5 phases run in one `WriteTransaction`.
- Returns `GcStats { timed_out_count, deleted_session_count, deleted_injection_log_count }`.
- Uses named constants: `TIMED_OUT_THRESHOLD_SECS = 24 * 3600`, `DELETE_THRESHOLD_SECS = 30 * 24 * 3600`.

---

## 2. tools.rs — maintain=true Integration

Location: The `maintain=true` handling block in `handle_context_status` (or equivalent maintain path in `tools.rs`).

The existing maintain path (from crt-005) runs:
- Confidence refresh (batch 100)
- Graph compaction
- Co-access cleanup

Add session GC after these operations:

```
// NEW (col-010): GC sessions in maintain=true path
let store_gc = Arc::clone(&store)
let gc_result = tokio::task::spawn_blocking(move || {
    store_gc.gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS)
}).await

match gc_result:
    Ok(Ok(stats)) =>
        tracing::info!(
            timed_out = %stats.timed_out_count,
            deleted_sessions = %stats.deleted_session_count,
            deleted_log_entries = %stats.deleted_injection_log_count,
            "Session GC complete"
        )
    Ok(Err(e)) =>
        tracing::warn!(error = %e, "Session GC failed")
    Err(join_err) =>
        tracing::warn!(error = %join_err, "Session GC task panicked")
```

GC failure does not fail the `context_status` response. The maintain operation continues.

---

## 3. GC Atomicity (ADR-002)

The 5-phase write transaction ensures:
- If INJECTION_LOG delete completes but SESSIONS delete fails → transaction rolls back → orphan log entries not created.
- If only some SESSIONS records are deleted → transaction rolls back → all or nothing.
- TimedOut marking and deletion are separate phases — a session cannot be TimedOut AND scheduled for deletion in the same GC run (delete_boundary is 30 days; timed_out_boundary is 24h, so all sessions scheduled for deletion are already beyond TimedOut threshold).

---

## 4. GC Threshold Constants Import in tools.rs

```
use unimatrix_store::sessions::{TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS}
```

These constants are defined in `sessions.rs` and imported at the call site.

---

## Error Handling

| Error | Handling |
|-------|---------|
| `gc_sessions` returns Err | Log warn; maintain continues |
| `spawn_blocking` JoinError | Log warn; maintain continues |
| Partial transaction failure | redb rolls back automatically |

---

## Key Test Scenarios

1. Active session at started_at = now - 25h → GC marks TimedOut; session still present.
2. Active session at started_at = now - 23h → GC does NOT time out.
3. Session at started_at = now - 31 days → GC deletes session; all associated InjectionLogRecords deleted; `GcStats.deleted_session_count = 1`, `deleted_injection_log_count = N`.
4. Session at started_at = now - 29 days → NOT deleted.
5. Abandoned session at 31 days → deleted (any status is eligible for 30-day deletion).
6. GcStats: run GC with 2 timed-out and 1 deleted → `timed_out_count=2, deleted_session_count=1`.
7. GC cascade atomicity: insert session + 3 injection records → delete via GC → both session and 3 log entries gone in same operation.
8. `maintain=true` context_status call completes even if GC fails (graceful degradation).
