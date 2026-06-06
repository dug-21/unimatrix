# Pseudocode: cycle-review-purge (`mcp/tools.rs` handler + `server.rs`/`main.rs` field plumbing)

ADRs: ADR-004 (seam + emission), ADR-006 (policy surface). FRs: FR-15, FR-16. Risk: R-10.

## Purpose

Wire the third purge point: `context_cycle_review` clears the buffers of sessions attributed
to the reviewed `feature_cycle`, gated on an exhaustive `TranscriptRetention` match, with
content-free audit. Review output is otherwise unchanged (no distillation — crt-052 inserts
its snapshot step inside `clear_transcripts_for_feature` later).

## Prerequisite Plumbing: `retention_config` on `UnimatrixServer`

The handler needs `transcript_retention` at runtime; `UnimatrixServer` does not currently hold
retention config (it goes only to the background tick, `main.rs:698/:794`). Follow the
`store_config` precedent (#561, `server.rs:242/:346`, `main.rs:758/:1180`):

```
// server.rs — struct field:
pub retention_config: Arc<crate::infra::config::RetentionConfig>,

// server.rs test ctor (~:346):
retention_config: Arc::new(RetentionConfig::default()),

// main.rs daemon path (~:758) and stdio path (~:1180):
server.retention_config = Arc::new(config.retention.clone());
// (an Arc<RetentionConfig> already exists at main.rs:698/:1120 — reuse via Arc::clone)
```

## Handler Change (`context_cycle_review`, `mcp/tools.rs:~1918`)

Insertion point: after the retrospective report is successfully computed (or returned from
cache) and the success response is built — i.e., the last step before returning `Ok(result)`.
Error paths (validation failure, no observation data, SQL failure) return early ABOVE the
purge and do not clear transcripts (review failed ⇒ transcripts stay for the retry; see
OVERVIEW open questions).

```
// vnc-025 (#670, FR-15/FR-16): purge transcript buffers for the reviewed cycle.
// EXHAUSTIVE match — enterprise seam (Constraint 7). Never a hardcoded assumption,
// never `if ... == PurgeOnCycleClose`, never a `_` arm.
match self.retention_config.transcript_retention {
    TranscriptRetention::PurgeOnCycleClose => {
        // in-place clear; sessions stay registered; records are non-zero-bytes only
        let records = self.session_registry
            .clear_transcripts_for_feature(&params.feature_cycle);
        for record in records {
            // ADR-004: emit after lock release (clear_transcripts_for_feature already
            // released everything), fire-and-forget via the existing helper
            // (audit_fire_and_forget = tokio::spawn + log_event_async, server.rs:508).
            self.audit_fire_and_forget(AuditEvent {
                event_id: 0,
                timestamp: 0,
                session_id: record.session_id,
                agent_id: "server".to_string(),
                operation: "transcript_session_purged".to_string(),
                target_ids: vec![],
                outcome: Outcome::Success,
                detail: format!("bytes={} trigger=cycle_review", record.bytes_purged),
                ..AuditEvent::default()
            });
        }
    }
    TranscriptRetention::RetainDays(_) => {
        // Enterprise-only; OSS validate() rejects this value at startup, so this arm is
        // unreachable in OSS — but it MUST exist and MUST NOT purge (the whole point of
        // the seam). No-op.
    }
}
// review response construction/return — UNCHANGED (AC-09: output byte-identical pre/post)
```

Imports: `use crate::infra::config::TranscriptRetention;` (AuditEvent/Outcome already imported
in tools.rs).

## Data Flow

```
params.feature_cycle ──► clear_transcripts_for_feature (registry-wiring)
                              │ matches state.feature == Some(feature_cycle); None never matches
                              ▼
                     Vec<TranscriptPurgeRecord> (bytes_purged > 0 only)
                              │
                              ▼
                     audit_fire_and_forget × N  ──► transcript_session_purged rows
```

- Idempotent on cached re-review: second call finds empty buffers → empty record vec → no
  emission.
- Zero attributed sessions: empty vec, no audit, no error (R-10.5).
- Sweep-before-review (SR-09 accepted hazard): swept sessions are gone from the registry —
  clear is a no-op for them; silent loss by design.

## Error Handling

- `clear_transcripts_for_feature` cannot fail (no Result); poisoned buffer mutexes are
  recovered inside registry-wiring with best-effort counts.
- Audit failures: warn-and-continue inside `audit_fire_and_forget`; the clear stands (FR-14).
- The purge block introduces NO new error path into the handler — it cannot change the
  review's result or error surface.

## Key Test Scenarios (AC-09, R-10)

1. Through the tool handler: sessions with `feature == Some(cycle)` cleared (empty afterward,
   still registered); `Some(other)` and `None` untouched; audit rows match returned records.
2. Review output snapshot pre/post vnc-025: byte-identical (AC-09).
3. Retention gate: exhaustive match compiles (compile-level guarantee); `RetainDays` arm does
   not purge (unit-test the match via a registry fixture if reachable in test builds).
4. Cached second call: no new audit rows (buffers already empty).
5. Zero attributed sessions: no-op, no audit, no error (R-10.5).
6. Post-clear resumed stream: deltas at high offsets merge cleanly (pinned by
   transcript-buffer `clear()` semantics: `base_offset = high_water`) (R-10.2/.3).
7. Review error path (e.g. no observation data): transcripts NOT cleared.
