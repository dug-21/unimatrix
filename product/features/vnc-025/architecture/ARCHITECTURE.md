# vnc-025 Architecture: Server-Side Session Transcript Buffer

All code references verified against the workspace 2026-06-05 (post-vnc-024, main).

## System Overview

vnc-024 (F1) shipped the `transcript_delta` wire surface and deliberately drops every delta at
two guard points. vnc-025 replaces those drops with an in-memory, never-persisted, per-session
`TranscriptBuffer`, adds its purge lifecycle with content-free audit, and uses the buffer to
close the remote PreCompact-fidelity gap (server-built transcript-tail block). Nothing else
reads the buffer; distillation is crt-052.

```
client (F3, future)                         server
  transcript_delta ──UDS──┐
  transcript_delta ──HTTP /observe──prefix_session_id──┐
                          │                            │
                          ▼                            ▼
              dispatch (listener.rs) ── single arm + batch arm
                          │  (tee deltas; batch filter UNTOUCHED)
                          ▼
              SessionRegistry.apply_transcript_delta(session_id, offset, bytes)
                          │  Arc clone under registry lock; memcpy under buffer lock
                          ▼
              TranscriptBuffer (infra/session_transcript.rs)
                  │ contiguous-tail read           │ purge (3 points)
                  ▼                                ▼
       handle_compact_payload            transcript_session_purged audit
       (shared extraction core,          (metadata collected under lock,
        prepend to BriefingContent)       emitted after release, fire-and-forget)
```

## Component Breakdown

| Component | File | Responsibility | New/Modified |
|-----------|------|----------------|--------------|
| `TranscriptBuffer` | `infra/session_transcript.rs` (new) | Bytes, `high_water`, hole tracking, idempotent `apply_delta`, ring-tail overflow, contiguous-tail reader, content-opaque Debug | New |
| `TranscriptPurgeRecord` | `infra/session_transcript.rs` | `{ session_id, bytes_purged }` metadata struct for audit emission | New |
| `session_key()` seam | `infra/session_transcript.rs` | Documented `(tenant, project, session)` → string collapse (ADR-007) | New |
| `SessionRegistry` methods | `infra/session.rs` | `apply_transcript_delta`, `clear_transcripts_for_feature`, purge-record returns from drain/sweep — thin wiring only (file is over the 500-line cap) | Modified |
| `SessionState.transcript` | `infra/session.rs` | `Arc<Mutex<TranscriptBuffer>>` field (ADR-001) | Modified |
| Transcript-block extraction core | `uds/transcript_block.rs` (new) | `ExchangeTurn`, `build_exchange_pairs`, `format_turn`, constants, `extract_transcript_block` (path), `extract_transcript_block_from_bytes`, `prepend_transcript` — moved out of `hook.rs` (ADR-005) | New (moved) |
| Dispatch arms | `uds/listener.rs` | Single arm: drop guard body → merge call (`:774`); batch arm: tee loop before the untouched filter (`:1009`) (ADR-003) | Modified |
| `handle_compact_payload` | `uds/listener.rs:1504` | Build tail block from buffer when non-empty; prepend to content (ADR-005) | Modified |
| Cycle-review purge | `mcp/tools.rs:1918` handler | Call `clear_transcripts_for_feature` gated on `TranscriptRetention` match; emit audit via `log_event_async` | Modified |
| `RetentionConfig.transcript_buffer_max_bytes` | `infra/config.rs` | 4 MiB default knob beside `transcript_retention` (ADR-006) | Modified |
| Audit emission | `uds/listener.rs`, `mcp/tools.rs` | `transcript_session_purged` events, content-free, fire-and-forget (ADR-004) | Modified |
| HTTP path | `http/router.rs` | No changes — `prefix_session_id` already preserves `event_type`; convergence proven by tests (pattern #4725) | Unchanged |

## Component Interactions and Data Flow

### Delta ingest (both transports, both arms)

1. UDS `RecordEvent`/`RecordEvents` or HTTP `/observe` (post `prefix_session_id`) reach dispatch.
   `SessionWrite` capability and `sanitize_session_id` already ran (`listener.rs:738-758`) —
   **no new sanitize or entry path is added** (SR-08, ADR-003).
2. `event_type == TRANSCRIPT_DELTA_EVENT` → parse `TranscriptDeltaPayload`; on parse failure:
   drop, `Ack`, no content in logs (fire-and-forget contract unchanged).
3. `registry.apply_transcript_delta(&session_id, offset, bytes)`:
   - registry lock: look up session (absent → silent no-op), clone the `Arc` handle, bump
     `last_activity_at`, release — microseconds, no memcpy under the registry lock.
   - buffer lock: idempotent offset-bounded merge, cap enforcement, `high_water` update.
4. Always `Ack`. Batch arm: deltas additionally remain excluded from `obs_batch` by the
   **untouched** vnc-024 filter (`listener.rs:1009`) — merge happens in a tee loop *before* it
   (SR-07, ADR-003).

### PreCompact read (the only reader)

`handle_compact_payload` already holds a `SessionState` snapshot (`listener.rs:1521`); the
snapshot's `transcript` Arc shares the live buffer. After step 7 formatting:

1. `snapshot.transcript.lock()` → `contiguous_tail(TAIL_WINDOW_BYTES)` → `Option<Vec<u8>>`
   (≤12,000 bytes copied; never serves bytes spanning a hole — resolved decision 2).
2. `extract_transcript_block_from_bytes(&tail)` — same core as the local hook (ADR-005).
3. `prepend_transcript(block, content)` before `token_count` is computed.
4. Empty/absent buffer or `None` block → content byte-identical to pre-vnc-025 (AC-11; the
   legacy local hook never streams deltas, so no double-prepend — A2 is an F3 contract).

### Purge lifecycle (three points, all → audit)

| Trigger | Mechanism | Purge-record source |
|---------|-----------|---------------------|
| `drain_and_signal_session` (SessionClose) | key removal drops the Arc | drain returns `Option<TranscriptPurgeRecord>` alongside `SignalOutput` (SignalOutput shape untouched — it feeds the persisted signal queue) |
| `sweep_stale_sessions` (4 h idle) | key removal | sweep returns `Vec<TranscriptPurgeRecord>` alongside `Vec<SweepResult>` — covers silently-evicted sessions (empty `injection_history`, non-empty buffer) too, or AC-08 misses them |
| `context_cycle_review` | `clear_transcripts_for_feature(feature_cycle)` — in-place clear, session stays registered, gated on `match transcript_retention { PurgeOnCycleClose => ... }` | method return value |

Audit: callers emit `transcript_session_purged` **after lock release**, fire-and-forget
(`log_event_async` in async contexts per GH #302); purge success never depends on audit success
(SR-03, ADR-004). Event carries session_id, byte count, trigger — never content. GC'd by
existing `gc_audit_log` (crt-036).

### Lock discipline

- Order: registry lock → (release or hold briefly) → buffer lock. **Never** acquire the
  registry lock while holding a buffer lock.
- Registry lock: lookup + Arc clone + scalar bump only (existing microsecond contract,
  Constraint 3). The ≤1 MiB worst-case memcpy (Constraint 9: frame ceiling, not the client's
  64 KiB soft cap) happens under the per-session buffer lock only.
- An Arc cloned just before key removal may merge into an orphaned buffer — harmless; it frees
  on last drop (fire-and-forget: in-flight content is lost on close by design).

## Technology Decisions

| Decision | ADR |
|----------|-----|
| `Arc<Mutex<TranscriptBuffer>>` field shape; memcpy outside registry lock | ADR-001 |
| Contiguous-span + hole-ranges representation; ring-tail; metadata-only elision; content-opaque Debug | ADR-002 |
| Tee-before-filter wiring; reuse sanitized entry; always-Ack | ADR-003 |
| Collect-under-lock / emit-after-release audit; `clear_transcripts_for_feature` shaped for crt-052 | ADR-004 |
| Single shared extraction core, `from_path` + `from_bytes` | ADR-005 |
| `transcript_buffer_max_bytes` on `RetentionConfig`; cap injected at registry construction | ADR-006 |
| `session_key()` constructor seam, no re-key | ADR-007 |
| Checked offset arithmetic (overflowing delta dropped whole); treat-as-empty poison recovery at every buffer-mutex lock site; always-Ack preserved | ADR-008 |

No new runtime dependency (AC-13): std + existing `tokio`/`tracing` only.

## Integration Points

- **vnc-024 wire surface (frozen)**: `TRANSCRIPT_DELTA_EVENT` (`wire.rs:46`),
  `TranscriptDeltaPayload { offset: u64, bytes: String }` (`wire.rs:284`). Consume-only.
- **vnc-024 batch filter** (`listener.rs:1009`): load-bearing, untouched (ADR-003).
- **vnc-024 ADR-005 / #4721**: `transcript_retention` enum; OSS honors `PurgeOnCycleClose`
  only; purge code matches on the enum (enterprise seam), never assumes the variant.
- **Existing audit**: `AuditLog::log_event_async` (`infra/audit.rs`), `AuditEvent`
  (`unimatrix-store/src/schema.rs:360`), `uds_auth_failure` content-free precedent
  (`listener.rs:409`). `handle_session_close` needs the existing `Arc<AuditLog>` threaded in
  (already a dispatch param at `listener.rs:265/319/385`).
- **Local hook**: `hook.rs` extraction internals move to `uds/transcript_block.rs`; hook
  call sites (`hook.rs:220/:252/:295`) re-import — behavior unchanged, `hook.rs` shrinks.
- **Registry construction**: `server.rs:335`, `main.rs:645/:1068` switch to
  `SessionRegistry::with_transcript_cap(cfg.retention.transcript_buffer_max_bytes)`;
  `new()` keeps the 4 MiB default for tests (ADR-006).

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| Delta payload | `TranscriptDeltaPayload { offset: u64, bytes: String }` | `uds/wire.rs:284` (frozen) |
| Event-type constant | `TRANSCRIPT_DELTA_EVENT: &str = "transcript_delta"` | `uds/wire.rs:46` |
| New field | `SessionState.transcript: Arc<Mutex<TranscriptBuffer>>` | `infra/session.rs` (new) |
| Buffer merge | `TranscriptBuffer::apply_delta(&mut self, offset: u64, bytes: &[u8])` — no return value carries content; never errors, never panics for any wire-reachable input (checked arithmetic, ADR-008) | `infra/session_transcript.rs` (new) |
| Buffer read | `TranscriptBuffer::contiguous_tail(&self, window: usize) -> Option<Vec<u8>>` | `infra/session_transcript.rs` (new) |
| Buffer metadata | `TranscriptBuffer::{len() -> usize, high_water() -> u64, elided_bytes() -> u64, clear(&mut self) -> u64 /* bytes purged */}` | `infra/session_transcript.rs` (new) |
| Registry ingest | `SessionRegistry::apply_transcript_delta(&self, session_id: &str, offset: u64, bytes: &[u8])` — silent no-op when unregistered | `infra/session.rs` (new method) |
| Cycle-review purge | `SessionRegistry::clear_transcripts_for_feature(&self, feature_cycle: &str) -> Vec<TranscriptPurgeRecord>` | `infra/session.rs` (new method; crt-052 insertion seam) |
| Purge record | `TranscriptPurgeRecord { session_id: String, bytes_purged: u64 }` | `infra/session_transcript.rs` (new) |
| Drain change | `drain_and_signal_session(...) -> Option<(SignalOutput, Option<TranscriptPurgeRecord>)>` (`SignalOutput` itself unchanged) | `infra/session.rs:475` (modified) |
| Sweep change | `sweep_stale_sessions(&self) -> (Vec<SweepResult>, Vec<TranscriptPurgeRecord>)` | `infra/session.rs:501` (modified) |
| Extraction (path) | `extract_transcript_block(path: &str) -> Option<String>` (moved, signature unchanged) | `uds/transcript_block.rs` (from `hook.rs:1383`) |
| Extraction (bytes) | `extract_transcript_block_from_bytes(bytes: &[u8]) -> Option<String>` | `uds/transcript_block.rs` (new) |
| Prepend | `prepend_transcript(transcript: Option<&str>, briefing: &str) -> String` (moved, unchanged) | `uds/transcript_block.rs` (from `hook.rs:1442`) |
| Constants | `MAX_PRECOMPACT_BYTES: usize = 3000`, `TAIL_MULTIPLIER: usize = 4` (window = 12,000) | `uds/transcript_block.rs` (from `hook.rs:39/:50`) |
| Config knob | `RetentionConfig.transcript_buffer_max_bytes: usize` (serde default `4_194_304`; `validate()` rejects `< 65_536`) | `infra/config.rs` (new field, beside `transcript_retention` at `:1561`) |
| Retention gate | `TranscriptRetention::PurgeOnCycleClose` match arm | `infra/config.rs:1506` |
| Audit op | `AuditEvent { operation: "transcript_session_purged", agent_id: "server", session_id, detail: "bytes=<n> trigger=<session_close\|stale_sweep\|cycle_review>", outcome: Success, target_ids: [], .. }` via `log_event_async` | `infra/audit.rs:45`, `schema.rs:360` |
| Registry ctor | `SessionRegistry::with_transcript_cap(max_bytes: usize)`; `new()` = 4 MiB default | `infra/session.rs` (new ctor) |
| Key seam | `session_key(tenant: &str, project: &str, session_id: &str) -> String` — OSS returns `session_id` unchanged | `infra/session_transcript.rs` (new, ADR-007) |

## Risk Dispositions (SR-XX)

- **SR-01**: resolved first, structurally — ADR-001 (`Arc` handle; `get_state()` clone cost is
  one Arc clone; AC-10 satisfiable by structure + a clone-cost guard test).
- **SR-02**: ADR-002 — manual `Debug` printing metadata only; no `Display`; `apply_delta`
  returns nothing content-bearing; no error type carries bytes; AC-12 grep gate on new modules.
- **SR-03**: ADR-004 — metadata under lock, audit after release, purge independent of audit.
- **SR-04**: ADR-004 — `clear_transcripts_for_feature` is the named crt-052 seam (becomes
  `take`-shaped later; counts-only today, deliberately not returning bytes with no consumer).
- **SR-05**: ADR-005 — parity by shared code, golden test compares `from_path` vs streamed
  `from_bytes` on the same fixture.
- **SR-06**: accepted at scope review. Evidence trigger for a global cap: sustained
  `>32` concurrent registered sessions or resident transcript memory observed `>256 MiB`
  in `context_status`/ops review — revisit then; 4 h sweep is the backstop until.
- **SR-07**: ADR-003 — filter line untouched; merge is a tee *before* it; vnc-024 zero-rows
  test runs unmodified with the buffer active (AC-05).
- **SR-08**: ADR-003 — merge call sits after the existing `sanitize_session_id` guard inside
  the same dispatch arms; no parallel entry path, no re-sanitize.
- **SR-09**: accepted — sweep-before-review transcript loss degrades to crt-052
  reconstruction (by design, principle 8). The empty-buffer/no-double-prepend invariant
  ("a client never both streams deltas and runs the local-hook prepend") is recorded here as
  an **F3 contract obligation** (A2) — F3's scope must own it.

## Open Questions (for spec / downstream)

1. **AC-02 × overflow (A1)**: order-independence is guaranteed below the cap. For
   cap-crossing sequences, convergence holds for the final tail window, not the full
   content (a late head-fill arriving after ring-tail advanced the base is clipped). Spec
   should phrase the AC-02/AC-07 interaction test as tail-window equivalence; flag to human
   if full-content convergence under overflow is required (it would force covered-range
   replay buffering — rejected as speculative, resolved decision 2).
2. **Exact drain/sweep return shapes**: tuple returns specified above are the minimal-churn
   choice; spec may substitute named structs (`SweepOutcome`) if call-site readability wins —
   the constraint that `SignalOutput` (persisted queue shape) stays untouched is firm.
3. **Hole-metadata bound**: ADR-002 caps hole-range tracking at 64 ranges (collapse-to-newest
   + elision beyond). The constant is a guess; spec/test may tune it — the bounded-metadata
   property, not the number, is the requirement.
