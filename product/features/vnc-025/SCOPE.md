# vnc-025: Server-Side Session Transcript Buffer — Stream Wiring + Like-for-Like Delivery (F2)

## Problem Statement

The server can now *receive* client-streamed transcript deltas — vnc-024 (F1) shipped the
`transcript_delta` wire type, the typed `TranscriptDeltaPayload { offset, bytes }` binding, and
the `transcript_retention` policy knob — but it deliberately **accepts-and-drops every delta**
(vnc-024 ADR-004, `listener.rs:774` single-event arm, `listener.rs:1009` batch arm). The guard
exists because raw conversation bytes may contain secrets and the only legitimate destination —
an in-memory, never-persisted buffer — does not exist yet.

Consequences of the missing buffer:

1. **Remote PreCompact-fidelity gap (#4676)**: the local Rust hook reads a 12 KB transcript tail
   from `transcript_path` and prepends it to the compaction-defense injection client-side
   (`hook.rs:246-255`, `prepend_transcript`). A server-side formatted path (F1 content
   negotiation) and any future thin TS client have no transcript source — the server holds
   nothing to restore from.
2. **Distillation gap** *(deferred — crt-052, #689)*: cycle review (`context_cycle_review`)
   analyzes structured `ObservationRecord`s only. The conversational narrative — the *why*
   behind decisions, rework reasoning, phase intent — is never captured. vnc-025 builds the
   buffer that makes distillation possible; the distillation itself is independently scoped as
   crt-052, informed by the ass-070 extractor-quality spike (#683).

Who is affected: every remote (HTTP) session today, and both transports once the F3 TS client
streams deltas; the self-learning pipeline, which loses all conversational context at session end.

Why now: F1 shipped (vnc-024, verified in code 2026-06-05); the attribution gate passed
(ass-069 Q1 PoC: 0 mis-attribution across 128 concurrent mixed-transport sessions with ≤50%
drop, reorder, duplicates); the offset-bounded idempotent merge is PoC-validated. vnc-025 is
independently shippable — it ships "dark" (no client streams deltas until F3) but is fully
testable via direct dispatch. It is wiring + like-for-like delivery only: behavior with a
populated buffer matches what the local Rust hook delivers today (PreCompact parity); new
capabilities built on the buffer (distillation) are crt-052.

## Goals

1. **Buffer + merge**: Add a per-session in-memory transcript buffer to `SessionState`
   (`transcript` + `transcript_high_water: u64`), populated by an offset-bounded idempotent
   merge of incoming `transcript_delta` events — out-of-order and duplicate safe — replacing the
   vnc-024 accept-and-drop guard in **both** dispatch arms (single `RecordEvent` and
   `RecordEvents` batch) on **both** transports (UDS + HTTP `/observe`).
2. **Accumulated-buffer bound**: enforce a configurable per-session cap on the *accumulated*
   buffer (the 64 KiB cap bounds individual deltas only — ass-069 Unanswered Q3). Overflow
   policy keeps the most recent content (tail), preserves `transcript_high_water`, and records
   an elision marker + dropped-byte count. Memory is bounded for a multi-hour session.
3. **Purge lifecycle**: raw transcript is purged on (a) `drain_and_signal_session`
   (SessionClose) and (b) `sweep_stale_sessions` — both automatic via existing key removal —
   and (c) explicitly cleared at `context_cycle_review`, honoring
   `TranscriptRetention::PurgeOnCycleClose` (the only OSS value). The buffer never touches disk.
   (crt-052 later inserts a distill step ahead of the cycle-review clear.)
4. **Content-free audit**: emit a `transcript_session_purged` audit event (session_id, byte
   count, timestamp — never content) through the existing append-only `AuditLog`, garbage-
   collected by the existing `gc_audit_log` retention policy.
5. **Server-side PreCompact transcript block**: when a `CompactPayload` arrives for a session
   whose buffer is non-empty, the server builds the transcript-tail block from the buffer
   (server-side equivalent of `extract_transcript_block`/`prepend_transcript`), closing the
   remote PreCompact-fidelity gap. Sessions with empty buffers (today's Rust hook) are
   unaffected — no double-prepend. This is the like-for-like centerpiece: parity with what
   the local hook delivers client-side today.
6. **Enterprise seams kept warm**: a single key-construction seam documents the
   `(tenant, project, session)` composite dimension (OSS `tenant = "default"`); the purge
   lifecycle reads `transcript_retention` from config rather than hardcoding; deltas continue to
   inherit `SessionWrite` capability + bearer gating (no new auth surface).

## Non-Goals

- **Distillation at cycle review and the reconstruction fallback** — crt-052 (#689),
  independently scoped, informed by the ass-070 extractor-quality spike (#683). vnc-025
  delivers the buffer and purge lifecycle only; nothing reads the buffer except the
  PreCompact block.
- **The TS client / delta production** (per-session `last_offset` tracking, ship-since-offset,
  64 KiB head+tail client truncation) — F3. vnc-025 ships dark and is exercised by tests.
- **Enterprise acknowledged-delivery / at-least-once audit path** — named gap (ass-069 Q7);
  fire-and-forget delivery stands. Not designed here.
- **Honoring `TranscriptRetention::RetainDays`** — OSS `validate()` rejects it (vnc-024
  ADR-005); no durable transcript persistence of any kind is built.
- **Any disk spill, crash recovery, or persistence of raw transcript** — a crash loses in-flight
  transcript by design; behavior degrades to reconstruction (principle 8).
- **Changes to the 23 detection rules** — they stay on `ObservationRecord`; nothing in vnc-025
  touches cycle review output.
- **Any transcript parsing or extraction** — LLM-based or rule-based, single- or multi-provider —
  the buffer stores opaque bytes; all interpretation is crt-052 (#689) / ass-070 (#683).
- **Activating `CompactPayload.transcript_excerpt`** — remains ignored legacy (vnc-022 ADR-005);
  streamed deltas supersede it (vnc-024 FR-15). If a future feature activates it, the F-07
  per-field size cap (#670 comment, PR #671 review) applies *there*, not here.
- **Full registry re-key to a composite key type** — OSS keeps the string `session_id` key
  (already transport-namespaced via `http-` prefix); vnc-025 adds the documented seam only.
- **Retirement/demotion of the feature-attribution heuristics** (`enrich_topic_signal`,
  `check_eager_attribution`, topic-tally voting) — a separate simplification feature
  (ass-069 Q3); transcript-read attribution may inform it later.
- **OAuth / `http-{subject_hash}` multi-tenant prefixing** — enterprise.

## Background Research

All claims verified against the workspace 2026-06-05 (post-vnc-024 merge, commit `70b3aeb7`).

### F1 landing verification (vnc-024 — what this feature consumes)

- `TRANSCRIPT_DELTA_EVENT = "transcript_delta"` (`wire.rs:46`) and typed
  `TranscriptDeltaPayload { offset: u64, bytes: String }` (`wire.rs:284-289`, ts-rs exported).
  Documented precedence: streamed deltas supersede `transcript_excerpt` (`wire.rs:217-221`).
- **Accept-and-drop guard, single-event arm** (`listener.rs:765-786`): early-return `Ack` on
  `event_type == TRANSCRIPT_DELTA_EVENT`; typed parse is observability-only and never changes
  control flow (malformed payload still drops with `Ack`). This is the branch vnc-025 replaces
  with the buffer merge.
- **Batch arm** (`listener.rs:999-1009`): deltas are filtered out of `obs_batch` before
  `insert_observations_batch`. vnc-025 must route filtered deltas into the merge **while
  keeping them out of durable observation rows** — the filter's non-persistence property is
  load-bearing and must survive.
- **HTTP path**: `/observe` reaches the same dispatch via `router.rs`; `prefix_session_id`
  rewrites `session_id → http-{id}` pre-dispatch and preserves `event_type` (tested per the
  transport-convergence pattern, Unimatrix #4725). `SessionWrite` capability + bearer gating
  inherited (`listener.rs:737-743`).
- **`TranscriptRetention`** (`config.rs:1506`): enum `PurgeOnCycleClose` (default) |
  `RetainDays(u32)`; OSS `validate()` **rejects** `RetainDays` (`config.rs:1634-1640`);
  project-wins merge arm at `config.rs:3376-3384` (vnc-024 ADR-005, Unimatrix #4721). Field is
  currently config-only — vnc-025 is its first consumer.

### SessionRegistry / SessionState (`infra/session.rs`, 2030 lines)

- `SessionRegistry` is `Mutex<HashMap<String, SessionState>>` (`session.rs:159-161`), Arc-shared
  between the UDS listener and the MCP server (`server.rs:217`, ptr-eq tested). All record paths
  are silent no-ops for unregistered sessions — the PoC-validated contract a delta merge must
  follow (no auto-registration).
- **`SessionState` derives `Clone` and `get_state()` clones the whole struct**
  (`session.rs:223-226`); `get_state` runs on hot paths (`tools.rs:747`, `:1404` — every
  context_search with a session). A multi-MB `Vec<u8>` cloned per search is a regression trap.
  The design must keep the transcript out of wholesale clones (e.g., `Arc<[u8]>`/shared handle,
  a sibling map, or non-cloning accessors). This is the single biggest structural constraint
  discovered.
- Purge points exist: `drain_and_signal_session` removes the key under one lock
  (`session.rs:475-490`); `sweep_stale_sessions` evicts after 4 h idle (`session.rs:501-534`,
  swept from `listener.rs:1796`); `clear_session` (`session.rs:301-304`). Key removal drops the
  buffer — purge is structural, with audit emission to be added at these points.
- Precedent for in-memory-only fields documented on the struct: `category_counts`,
  `confirmed_entries` ("in-memory only; reset on register_session; never persisted").

### Cycle review purge insertion point

- `context_cycle_review` (`mcp/tools.rs:1918`, ~1000-line handler in a 9,926-line file) is keyed
  by `feature_cycle` and has `self.session_registry` in scope. **There is no registry index from
  feature → sessions**; the cycle-review purge needs a new registry method that clears the
  transcripts of sessions whose `state.feature == feature_cycle` (single lock, clear in place,
  session stays registered). crt-052 will later extend this same method to snapshot bytes out
  before clearing (distill-before-purge).
- Distillation-specific research (`synthesis.rs` / `phase_narrative.rs` insertion points,
  `ObservationRecord` shape for reconstruction) moved to crt-052 (#689).

### PreCompact mechanism (the fidelity gap)

- Local: the Rust hook reads `transcript_path`, runs `extract_transcript_block` (12 KB tail) and
  prepends it to the `BriefingContent` **client-side** (`hook.rs:246-255`, `:291-300`). The
  server's `CompactPayload` handler (`listener.rs:1499`) builds knowledge payload only and
  ignores `transcript_excerpt` (`listener.rs:1204`). A buffer-fed server-side block is the gap
  closure; the empty-buffer condition naturally prevents double-prepend for the legacy local
  hook (which never streams deltas).

### Audit log + retention

- `AuditLog` (`infra/audit.rs`) wraps `SqlxStore::log_audit_event`; `AuditEvent`
  (`store/schema.rs:360`): `event_id`, `timestamp`, `session_id`, `agent_id`, `operation`,
  `target_ids`, `outcome`, detail. `transcript_session_purged` fits as an `operation` with byte
  count in detail — mirrors the content-free `uds_auth_failure` pattern. `gc_audit_log`
  (`store/retention.rs:271`, crt-036) already GCs audit rows — no new retention machinery.

### Prior decisions consulted (Unimatrix)

- **#4721** (vnc-024 ADR-005): `transcript_retention` governs the raw ephemeral transcript only,
  not distilled knowledge/observations/audit; no content secret-scanner exists — in-memory +
  purge-on-close **is** the secrets guarantee; OSS rejects `RetainDays`.
- **#4725** (transport-convergence testing): per-transport tests assert the pre-dispatch
  transform preserves the routing key; the shared-arm behavior is proven once via direct
  dispatch. Applies directly to the merge replacing the drop guard.
- **#3922** (SessionState scalar-counter pattern), **#759** (col-017 topic-tally choice):
  precedent for SessionState field-shape decisions.

## Proposed Approach

1. **Buffer module** (new focused file, e.g. `infra/session_transcript.rs`): a
   `TranscriptBuffer` owning the bytes, `high_water`, gap awareness (tail-contiguity check,
   representation encapsulated — resolved decision 2), elision/drop accounting, and the
   `apply_delta(offset, bytes)` idempotent merge (PoC semantics: writes bounded by offset;
   duplicates/reorder are no-ops or in-place rewrites; never grows past the accumulated cap).
   `SessionState` holds it behind a shared handle so `get_state()` clones stay O(small).
2. **Dispatch wiring**: replace the single-event accept-and-drop early-return with
   `registry.apply_transcript_delta(session_id, payload)` → still returns `Ack` always
   (fire-and-forget contract: malformed payload, unregistered session, over-cap — all `Ack`,
   no `Error`, no content logging). Batch arm: route deltas to the merge before the existing
   filter; the filter itself stays (deltas must never enter `obs_batch`).
3. **Bound**: `transcript_buffer_max_bytes` config knob, alongside `transcript_retention`,
   default 4 MiB (resolved decision 1). Overflow keeps tail (ring-tail),
   inserts an elision marker, increments a dropped-bytes counter, preserves `high_water`.
4. **Purge + audit**: emit `transcript_session_purged` (bytes, session_id) at the three purge
   points when the buffer is non-empty; cycle-review clear gated on
   `TranscriptRetention::PurgeOnCycleClose` (the only OSS arm; the match is the enterprise
   seam). crt-052 later inserts its distillation snapshot ahead of this clear.
5. **PreCompact block**: in the `CompactPayload` handler, when the session buffer is non-empty,
   build the tail block (reuse the 12 KB tail-window constant family) server-side and prepend to
   `BriefingContent` content. Empty buffer → today's behavior, byte-identical.
6. **Key seam**: a `session_key()` constructor (or newtype) documenting
   `(tenant="default", project, session)` collapse to today's string — one place to change when
   enterprise lands; no call-site re-key.

## Acceptance Criteria

- AC-01: A `transcript_delta` event dispatched for a registered session (UDS direct dispatch)
  merges `{offset, bytes}` into that session's in-memory buffer and returns `Ack`; the buffer
  content equals the streamed bytes.
- AC-02: Out-of-order, duplicate, and overlapping deltas converge to identical buffer content
  regardless of arrival order (idempotent offset-bounded merge); `transcript_high_water` equals
  max(offset + len) seen.
  *Annotation (human-accepted at design review 2026-06-05, per ADR-002): under ring-tail overflow,
  equivalence means tail-window equivalence — full-content equality below the 4 MiB cap; below-floor
  deltas are defined no-ops.*
- AC-03: A delta for an unregistered session is a silent no-op: `Ack` returned, no registry slot
  created, no other session's buffer affected.
- AC-04: A malformed `transcript_delta` payload still returns `Ack` (fire-and-forget contract);
  no `Error` reaches the client and no payload content appears in logs.
- AC-05: No `transcript_delta` — single-event or batch element, UDS or HTTP — ever produces a
  durable observation row or any other disk write (the vnc-024 AC-12 zero-rows gate is preserved
  with the buffer active); a batch containing deltas + normal events persists only the normal
  events while the deltas merge into the buffer.
- AC-06: The HTTP `/observe` path merges deltas into the `http-`prefixed session's buffer;
  `prefix_session_id` continues to preserve `event_type` for single and batch shapes
  (transport-convergence pattern #4725); `SessionWrite` capability is still enforced.
- AC-07: The accumulated buffer never exceeds the configured `transcript_buffer_max_bytes`; on
  overflow the most recent content is retained, an elision marker and dropped-byte count are
  recorded, and subsequent merges remain correct (`high_water` monotonic).
- AC-08: `drain_and_signal_session` and `sweep_stale_sessions` free the buffer (key removal) and
  emit a content-free `transcript_session_purged` audit event (session_id, byte count,
  timestamp) when the purged buffer was non-empty; the event row contains no transcript content.
- AC-09: `context_cycle_review` clears the buffers of all registry sessions attributed to the
  reviewed `feature_cycle` (buffer empty afterward, session still registered), emitting the
  purge audit event; behavior is gated on `TranscriptRetention::PurgeOnCycleClose` read from
  config (not hardcoded). Cycle review output is otherwise unchanged (no distillation — crt-052).
- AC-10: `get_state()` and other hot-path registry reads do not deep-copy transcript bytes
  (demonstrated structurally — e.g., shared handle — or by a clone-cost guard test).
- AC-11: A `CompactPayload` for a session with a non-empty buffer returns `BriefingContent`
  with a server-built transcript-tail block prepended; with an empty buffer the response is
  byte-identical to pre-vnc-025 behavior (no double-prepend for legacy local-hook sessions).
- AC-12: Raw transcript bytes never reach disk: no SQL write, no file write, no log line carries
  buffer content (code-review gate + grep/test assertion on tracing calls in new code paths).
- AC-13: `cargo audit` passes; no new runtime dependency is required for the buffer.

## Constraints

1. **Principle 8 / secrets posture is architectural, not scanner-based** (#4721): there is no
   content redactor; in-memory-only + purge IS the guarantee. Any design that persists, spills,
   or logs raw transcript is rejected at review. Distilled *output* flows through the existing
   `context_store` write-path sanitization.
2. **`SessionState: Clone` + hot-path `get_state()` clones** (`session.rs:223`): the buffer must
   not ride wholesale clones. This constrains the field shape (shared handle / sibling structure
   / accessor design) before any other decision.
3. **Registry mutex discipline**: all existing record paths hold the lock for microseconds with
   no I/O/await. Delta merge (≤64 KiB memcpy) fits; the cycle-review clear is in-place key work
   only. (crt-052's distillation must snapshot-and-release — never parse under the lock.)
4. **Fire-and-forget contract**: deltas ride `RecordEvent` — always `Ack`, never `Error`, silent
   no-op for unregistered sessions, drops lose content but never corrupt (PoC-validated).
5. **The batch-filter non-persistence property is load-bearing** (vnc-024 ADR-004 R-04): wiring
   the buffer must not reopen the path from delta bytes to `insert_observations_batch`.
6. **500-line file rule**: `session.rs` (2,030), `listener.rs` (8,567), `tools.rs` (9,926) are
   already over; the buffer lands as a new focused module with thin call-site wiring.
7. **`transcript_retention` consumption**: OSS has exactly one reachable variant
   (`PurgeOnCycleClose`; `RetainDays` rejected at `validate()`). The purge code must match on the
   enum (enterprise seam), not assume it.
8. **Ships dark until F3**: no production client streams deltas yet. All verification is
   test-driven (direct dispatch + fixtures); the PreCompact empty-buffer path is the only
   behavior live before F3.
9. **1 MiB frame ceiling** (`MAX_PAYLOAD_SIZE`, `wire.rs:16`) remains the hard per-event bound;
   the server does not trust the client's 64 KiB soft cap (F3 concern) and must tolerate deltas
   up to the frame limit.
10. **Wire contract is frozen** (F1): no changes to `TranscriptDeltaPayload`, event-type string,
    or ts-rs bindings; vnc-025 is consume-only on the wire surface.
11. **Aggregate memory envelope** (decided at scope review): the per-session cap bounds one
    session; the aggregate is per-session cap (4 MiB) × concurrent session count. For the
    personal-cloud single-container posture with a handful of concurrent sessions this is tens
    of MiB worst case — acceptable. A global cap is **deliberately out of scope**: the 4 h
    `sweep_stale_sessions` eviction is the existing backstop, and no global bound is built until
    evidence demands one.

## Resolved Decisions

All three open questions were resolved at scope review (uni-zero, 2026-06-05, human-approved).
Downstream design treats these as settled.

1. **Buffer bound: 4 MiB default; knob beside `transcript_retention`; ring-tail overflow.**
   `transcript_buffer_max_bytes` defaults to 4 MiB and lives next to `transcript_retention` —
   they are the two halves of one transcript-policy surface, and the enterprise seam (goal 6)
   reads that section as a unit. PreCompact needs only the 12 KB tail, so 4 MiB is generous
   headroom for crt-052's future distillation window; ass-070 will tell us if distillation wants
   more, and the knob makes that a config change, not a redesign. Overflow is ring-tail: the
   simplest policy that satisfies the only current reader, and crt-052's reconstruction fallback
   already covers the lost-head case.
2. **Gap handling: tail-contiguity check, representation encapsulated in `TranscriptBuffer`.**
   PreCompact is the only buffer reader in vnc-025; the sole hard requirement is never serving
   NUL-filled holes in the tail block, and the contiguity check meets it. Full covered-range
   tracking now would be speculative design for crt-052's fallback trigger before ass-070 has
   reported. The representation stays encapsulated in the new module so range tracking is a
   local retrofit if crt-052 wants it. Matches the feature's "like-for-like only" framing.
3. **Composite-key seam: documented constructor seam; re-key deferred to enterprise.**
   Re-keying every registry call site for a dimension OSS never populates (`tenant = "default"`)
   is churn with zero OSS behavior change and a real regression surface across
   `session.rs`/`listener.rs`/`tools.rs` hot paths. The constructor seam gives enterprise exactly
   one place to change. Consistent with principle 6's zero-required-infrastructure posture and
   with how every other enterprise seam in this scope is handled.

## Tracking

GitHub Issue: #670 (re-scoped per ass-069 Q6, then narrowed 2026-06-05 to stream wiring +
like-for-like delivery — distillation and reconstruction split out to crt-052 #689; F1
dependency #672 shipped in vnc-024, verified in-code 2026-06-05).

Follow-on: crt-052 (#689) — transcript-fed cycle review distillation, immediately following
vnc-025, informed by ass-070 (#683).
