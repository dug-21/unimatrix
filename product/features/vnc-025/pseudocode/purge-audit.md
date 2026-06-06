# Pseudocode: purge-audit (`uds/listener.rs` helper + emission at three purge points)

ADR: ADR-004. FRs: FR-12, FR-13, FR-14. Risks: R-07 (emission context, #4379/#302 cluster),
R-08 (silently-evicted gap).

## Purpose

Emit the content-free `transcript_session_purged` audit event for every non-empty purged
buffer, at all three purge points, strictly AFTER lock release, fire-and-forget. Purge success
never depends on audit success. Zero-byte purges emit nothing (already filtered at the source:
registry-wiring's `purge_record_for` and `clear_transcripts_for_feature` only return records
with `bytes_purged > 0`).

## Pinned Event Shape (ADR-004, mirrors `uds_auth_failure` precedent at `listener.rs:409`)

```
AuditEvent {
    event_id: 0,                                   // assigned by store
    timestamp: 0,                                  // assigned by store
    session_id: record.session_id,                 // already routed through session_key()
    agent_id: "server",
    operation: "transcript_session_purged",
    target_ids: vec![],
    outcome: Outcome::Success,
    detail: format!("bytes={} trigger={}", record.bytes_purged, trigger),
                                                   // trigger ∈ {session_close, stale_sweep, cycle_review}
    ..AuditEvent::default()                        // credential_type/capability/attribution/metadata defaults
}
// NEVER content. detail interpolates ONLY the u64 count and the static trigger token (R-05.3).
```

Rows are GC'd by existing `gc_audit_log` retention (crt-036) — no new retention machinery.

## Emission Helper (listener.rs — UDS-side purge points)

Async contexts only; per GH #302/#4379 use `log_event_async` inside `tokio::spawn` — never
`log_event`/`block_in_place` from async code:

```
fn emit_purge_audits(
    audit_log: &Arc<AuditLog>,
    records: Vec<TranscriptPurgeRecord>,
    trigger: &'static str,                         // "session_close" | "stale_sweep"
) {
    for record in records {                        // records are already non-zero-bytes only
        let audit = Arc::clone(audit_log);
        let event = build event per pinned shape above;
        // fire-and-forget: JoinHandle dropped; purge already completed before this runs
        tokio::spawn(async move {
            if let Err(e) = audit.log_event_async(event).await {
                // content-free warn; byte count only; NO retry loop (FR-14)
                tracing::warn!(error = %e, "transcript purge audit write failed");
            }
        });
    }
}
```

Sweep burst note (R-07.3): one spawned task per record; `log_event_async` calls
`SqlxStore::log_audit_event` directly (no blocking bridge), proven non-starving by the
GH #302 regression test — 20+ concurrent purge audits are the same pattern.

## Wiring per Purge Point

### 1. `session_close` — `process_session_close` (listener.rs)

- `process_session_close` gains `audit_log: &Arc<AuditLog>` (or owned `Arc`) as a parameter.
- Thread the existing `Arc<AuditLog>` from the dispatch function (already a parameter at
  `listener.rs:265/319/385`) through `handle_request`/the call chain into
  `process_session_close`. Mechanical parameter threading; no new Arc creation.
- After `drain_and_signal_session` returns (registry lock already released inside the method):
  `if let Some(rec) = drain_purge { emit_purge_audits(audit_log, vec![rec], "session_close") }`
  (call-site shape in dispatch-wiring.md Edit 4).

### 2. `stale_sweep` — same function, sweep step

- After `sweep_stale_sessions()` returns:
  `emit_purge_audits(audit_log, sweep_purges, "stale_sweep")`.
- The purge vec includes silently-evicted sessions (empty `injection_history`) by
  construction in registry-wiring — emission here is uniform; no special case (R-08).

### 3. `cycle_review` — `mcp/tools.rs` handler

- Emission uses the existing `self.audit_fire_and_forget(event)` helper (`server.rs:508` —
  already `tokio::spawn` + `log_event_async`, the exact ADR-004 pattern). Trigger token
  `"cycle_review"`. Call-site shape in cycle-review-purge.md.

## Ordering Guarantees (FR-14 — structural)

1. `clear()` / key removal completes under its lock.
2. Lock(s) released (drain/sweep release the registry lock at method return;
   `clear_transcripts_for_feature` clears after releasing the registry lock).
3. THEN emission is spawned. Audit failure → content-free `tracing::warn!`; the purge stands.
   No retry, no rollback, no dependency in either direction.

## Error Handling

- Audit store unavailable: purge path completes normally; one warn per failed row (R-07.1/.2).
- No emission for `bytes_purged == 0` — guaranteed at record-construction time, asserted in
  tests (R-07.4).
- The helper takes `TranscriptPurgeRecord` values only — it is structurally incapable of
  touching transcript content.

## Key Test Scenarios (AC-08, AC-09, R-05.3, R-07, R-08)

1. Both UDS purge points: audit row fields match the pinned shape (operation, agent_id
   "server", session_id, `bytes=<n> trigger=<t>`, Success, empty target_ids); absence of
   content (AC-08).
2. Sentinel test: purge a buffer containing a sentinel string; assert the audit row — ALL
   columns including detail — is sentinel-free (R-05.3).
3. Empty-buffer purge at each point: NO audit row (R-07.4).
4. Audit-write failure injected: purge completed (buffer gone/cleared), content-free warn
   fired, no retry (FR-14, R-07.2).
5. Close path completes when the audit store is unavailable (R-07.1).
6. Sweep burst: 20+ non-empty-buffer sessions swept in one pass → all rows eventually land;
   write pool does not starve (R-07.3, #2266 precedent).
7. Silently-evicted session emits an audit row (R-08.1 — mandatory named case; record
   produced in registry-wiring, emitted here).
8. Emission-context review: `log_event_async` + `tokio::spawn` only; no `log_event` /
   `block_in_place` from async contexts (#4379 pattern).
