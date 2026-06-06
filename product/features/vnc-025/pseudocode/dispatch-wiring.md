# Pseudocode: dispatch-wiring (`uds/listener.rs` — MODIFIED, thin wiring only)

ADRs: ADR-003 (tee-before-filter, always-Ack), ADR-005 (PreCompact integration), ADR-008
(poison degradation). FRs: FR-06, FR-07, FR-08, FR-09, FR-17, FR-18, FR-19.
`http/router.rs` is UNCHANGED (`prefix_session_id` already preserves `event_type` — pattern #4725).

## Purpose

Replace vnc-024's two accept-and-drop guard points with merge calls, and build the
server-side PreCompact tail block. Three edit sites in `listener.rs`; everything else
(sanitize, capability checks, batch filter, persistence paths) is untouched.

## Imports

```
use crate::infra::session_transcript::TranscriptBuffer;   // only if the local poison-recovery
                                                           // pattern needs the type name
use crate::uds::transcript_block::{
    extract_transcript_block_from_bytes, prepend_transcript,
    MAX_PRECOMPACT_BYTES, TAIL_MULTIPLIER,
};
// TRANSCRIPT_DELTA_EVENT + TranscriptDeltaPayload already imported (vnc-024).
```

## Edit 1 — Single-event arm (`listener.rs:~774`, inside `HookRequest::RecordEvent`)

The arm sits AFTER the existing `SessionWrite` capability check (`:738`) and
`sanitize_session_id` guard (`:748`) — no new entry path, no re-sanitize (FR-09/SR-08).
Keep the EARLY-RETURN shape (persistence below stays provably unreachable for a delta).
Replace the vnc-024 drop body:

```
if event.event_type == TRANSCRIPT_DELTA_EVENT {
    // vnc-025 (#670): merge into the per-session in-memory buffer (ADR-003). Replaces the
    // vnc-024 accept-and-drop guard. EARLY RETURN — persistence at :793/:849 below remains
    // provably unreachable for a delta. Fire-and-forget: every outcome Acks; no payload
    // content in logs (AC-04/AC-12).
    match serde_json::from_value::<TranscriptDeltaPayload>(event.payload.clone()) {
        Ok(delta) => {
            session_registry.apply_transcript_delta(
                &event.session_id, delta.offset, delta.bytes.as_bytes());
        }
        Err(e) => {
            // content-free: serde error Display carries position/type info, not payload bytes
            tracing::debug!(error = %e, "transcript_delta dropped (unparsed payload)");
        }
    }
    return HookResponse::Ack;
}
```

Note: the existing `tracing::info!(event_type, session_id, "UDS: event recorded")` above the
arm logs metadata only — unchanged, acceptable. The vnc-024 `offset` debug line is replaced by
the merge call.

## Edit 2 — Batch arm tee (`listener.rs`, inside `HookRequest::RecordEvents`, immediately before the `obs_batch` construction at `:~1007`)

```
// vnc-025 (#670, ADR-003): tee transcript deltas to the in-memory merge BEFORE the
// vnc-024 non-persistence filter below. The filter line is NOT edited/moved/simplified —
// it remains the second, independent guarantee that deltas never enter obs_batch
// (SR-07 hard gate; vnc-024 zero-rows test runs unmodified).
for event in events.iter().filter(|e| e.event_type == TRANSCRIPT_DELTA_EVENT) {
    match serde_json::from_value::<TranscriptDeltaPayload>(event.payload.clone()) {
        Ok(delta) => session_registry.apply_transcript_delta(
            &event.session_id, delta.offset, delta.bytes.as_bytes()),
        Err(e) => tracing::debug!(error = %e, "transcript_delta dropped (unparsed payload)"),
    }
}

// existing code below — BYTE-IDENTICAL (review-diff gate R-04.3):
let obs_batch: Vec<ObservationRow> = events
    .iter()
    .filter(|event| event.event_type != TRANSCRIPT_DELTA_EVENT)
    ...
```

The batch arm's existing per-batch validation/gating is reused as-is; the tee adds one
string-compare + one serde parse per delta element (ADR-003 accepted cost). The arm still
returns `HookResponse::Ack` at its existing tail — unchanged.

## Edit 3 — PreCompact tail block (`handle_compact_payload`, `listener.rs:~1504`)

Insertion point: after step 7 (`format_compaction_payload` → `content: Option<String>`) and
BEFORE `token_count` is computed (R-09.5: token_count reflects the prepended block).
The step-2 snapshot (`session_state = session_registry.get_state(session_id)` at `:~1521`)
already shares the live buffer via the Arc — NO new registry read (ADR-001/ADR-005).

```
// ... step 7 produces: let content = format_compaction_payload(...)  // Option<String>

// vnc-025 (#670, ADR-005): server-built transcript tail block. Read a point-in-time
// contiguous tail under the buffer lock (≤12,000 bytes copied; never crosses a hole,
// never zero-fill — FR-19). Poison → treat-as-empty (ADR-008): degrade to empty path.
let tail: Option<Vec<u8>> = session_state.as_ref().and_then(|s| {
    let mut buf = match s.transcript.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            let mut g = poisoned.into_inner();
            let _ = g.clear();                     // ADR-008 Layer 2 (see registry-wiring)
            g
        }
    };
    buf.contiguous_tail(MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER)   // window = 12,000
});  // buffer lock released before any await/formatting below

let block: Option<String> = tail.as_deref().and_then(extract_transcript_block_from_bytes);

// Empty buffer / absent session / None tail / None block → `content` flows through
// UNTOUCHED — byte-identical to pre-vnc-025 (AC-11/FR-18, the no-double-prepend guard).
let content: Option<String> = match block {
    Some(b) => Some(prepend_transcript(Some(&b), content.as_deref().unwrap_or(""))),
    None => content,
};

// existing code — unchanged relative order:
session_registry.increment_compaction(session_id);
let token_count = content.as_ref().map(|c| (c.len() / 4) as u32).unwrap_or(0);
HookResponse::BriefingContent { content: content.unwrap_or_default(), token_count }
```

Note the `(block = Some, content = None)` case: result is `Some(block)` (prepend_transcript's
transcript-only arm) — token_count becomes non-zero. Correct: a session with transcript but no
briefing entries still gets its tail block.

## Edit 4 — Drain/sweep call-site updates (`process_session_close`, `listener.rs:~1796/:1814`)

Tuple destructuring only; purge-record CONSUMPTION (audit emission) is specified in
purge-audit.md:

```
// :~1796 — sweep:
let (stale_outputs, sweep_purges) = session_registry.sweep_stale_sessions();
for sweep_result in &stale_outputs { ... existing body unchanged ... }
emit_purge_audits(&audit_log, sweep_purges, "stale_sweep");          // see purge-audit.md

// :~1814 — drain:
let maybe_drained = session_registry.drain_and_signal_session(session_id, hook_outcome);
let (maybe_output, drain_purge) = match maybe_drained {
    Some((out, purge)) => (Some(out), purge),
    None => (None, None),
};
if let Some(rec) = drain_purge {
    emit_purge_audits(&audit_log, vec![rec], "session_close");       // see purge-audit.md
}
// existing `if let Some(ref output) = maybe_output { ... }` body unchanged
```

`process_session_close` gains an `audit_log: &Arc<AuditLog>` parameter, threaded from the
dispatch caller (already a dispatch param at `listener.rs:265/319/385` — purge-audit.md).

## Error Handling

- Always-Ack (Constraint 4): merged / unregistered / malformed / over-cap / poison-recovered —
  all return `Ack` from both arms. No `Error` reaches a client for a delta.
- Parse failures: content-free `tracing::debug!` only (no `event.payload` interpolation
  anywhere — R-05).
- PreCompact: every failure direction (no session, empty buffer, poisoned mutex, `None` tail,
  `None` block) degrades to the unchanged pre-vnc-025 response.

## Key Test Scenarios (AC-01..AC-06, AC-11, R-04, R-05, R-09, R-12)

1. Direct-dispatch delta for registered session → `Ack` + buffer content equals streamed
   bytes (AC-01).
2. Unknown session → `Ack`, registry size unchanged, other buffers unchanged (AC-03).
3. Malformed payload (sentinel inside) → `Ack`; sentinel absent from captured tracing output
   across single arm, batch tee, merge, overflow, purge paths (AC-04, R-05.2).
4. vnc-024 zero-rows test runs UNMODIFIED with the buffer active (AC-05 hard gate).
5. Mixed batch (UDS + HTTP): non-delta events persist with exact row counts; zero
   delta-derived rows; delta bytes absent from every persisted column (R-04.2).
6. HTTP `/observe` delta lands in the `http-{id}` buffer, not the bare-id buffer;
   `prefix_session_id` preserves `event_type` single + batch; missing bearer/`SessionWrite` →
   rejected before dispatch, no merge (AC-06, R-12).
7. No new audit events fire on a normal delta dispatch (#3902 regression signature).
8. AC-11 golden parity through `handle_compact_payload` (see transcript-block.md) + empty-buffer
   byte-identity snapshot (HARD GATE).
9. Hole inside the last 12 KB → shorter well-formed block, never pre-hole bytes (R-09.3).
10. Concurrent deltas during compact read → point-in-time tail, block parses (R-09.6).
11. token_count reflects the prepended block (R-09.5).
