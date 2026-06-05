## ADR-003: Tee Deltas to the Merge Before the Untouched Batch Filter; Reuse the Sanitized RecordEvent Entry; Always-Ack

### Context

vnc-024's two drop points must become merge points without weakening two load-bearing
properties: (1) the batch-arm filter (`listener.rs:1009`,
`.filter(|e| e.event_type != TRANSCRIPT_DELTA_EVENT)`) is the proof that delta bytes never
reach `insert_observations_batch` (vnc-024 ADR-004 R-04; SR-07 — a refactor slip here reopens
delta-bytes → durable-row); (2) adding registry calls inside UDS dispatch arms has previously
caused unexpected `sanitize_session_id` audit interactions (SR-08, lesson #3902, crt-026).
The fire-and-forget contract (always `Ack`, never `Error`, silent no-op for unregistered
sessions, no content in logs) is PoC-validated and frozen.

### Decision

- **Single-event arm** (`listener.rs:765-786`): keep the early-return shape; replace the
  drop body. On successful `TranscriptDeltaPayload` parse, call
  `session_registry.apply_transcript_delta(&event.session_id, delta.offset,
  delta.bytes.as_bytes())`; on parse failure keep today's content-free `tracing::debug!`
  drop. Both paths `return HookResponse::Ack`. The arm stays an early return so the
  persistence paths below remain provably unreachable for a delta.
- **Batch arm** (`listener.rs:996-1025`): add a tee loop *immediately before* the `obs_batch`
  construction: for each event with `event_type == TRANSCRIPT_DELTA_EVENT`, parse and
  `apply_transcript_delta` (parse failure = content-free skip). The existing filter line is
  **not edited, not moved, not "simplified"** — it remains the second, independent guarantee
  that deltas never enter `obs_batch`. vnc-024's zero-rows test (AC-12 gate) runs unmodified
  with the buffer active (AC-05).
- **No new entry path (SR-08)**: both merge calls sit inside the existing dispatch arms,
  *after* the existing `sanitize_session_id` guard (`listener.rs:748`) and `SessionWrite`
  capability check (`listener.rs:738`). No re-sanitization, no parallel registry entry point,
  no new auth surface (goal 6). HTTP `/observe` needs zero changes: `prefix_session_id`
  rewrites the session_id pre-dispatch and preserves `event_type`; convergence is asserted by
  per-transport tests per pattern #4725 (shared-arm behavior proven once via direct dispatch).
- **Contract**: every delta outcome — merged, unregistered session, malformed payload,
  over-cap clipping — returns `Ack`. No `Error` reaches the client for a delta; no log line
  carries payload bytes (AC-03, AC-04, AC-12).

### Consequences

- Easier: SR-07 reduces to "did the filter line change?" in review — a one-line diff check;
  transport convergence needs no new HTTP code; the merge inherits all existing gating.
- Harder: deltas in a batch are parsed in the tee loop in addition to the filter's
  `event_type` comparison — a second pass over delta elements. Negligible (string compare +
  one serde parse per delta) and worth the untouched-filter guarantee.
- Cross-references: ADR-001 (what `apply_transcript_delta` does under which lock), ADR-002
  (merge semantics).
