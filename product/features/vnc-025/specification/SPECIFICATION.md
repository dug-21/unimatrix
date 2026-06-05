# SPECIFICATION — vnc-025: Server-Side Session Transcript Buffer (Stream Wiring + Like-for-Like PreCompact Delivery)

Source scope: `product/features/vnc-025/SCOPE.md` (Q1–Q3 resolved at scope review 2026-06-05, human-approved).
Risk input: `product/features/vnc-025/SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-09).

## Objective

Replace the vnc-024 accept-and-drop guard with a per-session, in-memory, never-persisted
transcript buffer fed by an offset-bounded idempotent merge of `transcript_delta` events, and use
that buffer to build the server-side PreCompact transcript-tail block — closing the remote
PreCompact-fidelity gap (#4676) with behavior matching what the local Rust hook delivers today.
The feature ships dark (no client streams deltas until F3) and builds nothing beyond wiring +
like-for-like delivery; distillation is crt-052.

## Functional Requirements

Each FR is testable; verification methods are bound at the AC level below.

### Buffer and merge

- **FR-01**: A new focused module (e.g. `infra/session_transcript.rs`) SHALL provide a
  `TranscriptBuffer` type owning: the byte buffer, `high_water: u64`, tail-contiguity gap
  awareness (representation encapsulated — resolved decision 2), and elision metadata
  (dropped-byte count). Per ADR-002, elision is metadata, never marker bytes spliced into
  content.
- **FR-02**: `TranscriptBuffer::apply_delta(offset, bytes)` SHALL be an offset-bounded idempotent
  merge: out-of-order, duplicate, and overlapping deltas converge to identical content regardless
  of arrival order; `high_water` is monotonic and equals max(offset + len) observed.
  Convergence is exact (full-content equality) below the accumulated cap. Once ring-tail
  overflow has advanced the buffer floor, bytes at offsets below the elided floor are a
  **defined no-op**: clipped, counted in the dropped-byte count, `high_water` still updated.
  Cap-crossing delta sequences therefore converge on the final tail window — **tail-window
  equivalence** — not on full content (ADR-002; resolves OQ-2).
- **FR-03**: `SessionState` SHALL hold the buffer behind a shared handle (or equivalent
  non-cloning shape) such that `get_state()` and other `SessionState` clones do not deep-copy
  transcript bytes (SR-01; pattern #4737).
- **FR-04**: The buffer SHALL be populated only for registered sessions. A delta for an
  unregistered session is a silent no-op — no auto-registration, no slot created (existing
  record-path contract).
- **FR-05**: The merge SHALL tolerate individual deltas up to the 1 MiB frame ceiling
  (`MAX_PAYLOAD_SIZE`); the server SHALL NOT assume the client's 64 KiB soft cap (Constraint 9).

### Dispatch wiring

- **FR-06**: The single-event `RecordEvent` arm (`listener.rs:765-786`) SHALL route
  `transcript_delta` events to the merge (e.g. `registry.apply_transcript_delta(session_id,
  payload)`) instead of dropping, and SHALL still return `Ack` unconditionally: malformed
  payload, unregistered session, over-cap — all `Ack`, never `Error`, no content logged.
- **FR-07**: The `RecordEvents` batch arm (`listener.rs:999-1009`) SHALL tee deltas to the merge
  while keeping the existing non-persistence filter intact: deltas never enter `obs_batch` or
  reach `insert_observations_batch`; non-delta events in the same batch persist normally (SR-07).
- **FR-08**: The HTTP `/observe` path SHALL merge deltas into the `http-`prefixed session's
  buffer via the same shared dispatch; `prefix_session_id` continues to preserve `event_type`
  for single and batch shapes; `SessionWrite` capability + bearer gating remain enforced —
  no new auth surface.
- **FR-09**: `apply_transcript_delta` SHALL enter the registry via the existing record-path entry
  pattern (same session-id handling as other record paths), not a parallel entry point — avoiding
  a second `sanitize_session_id` audit interaction (SR-08, #3902).

### Bound and overflow

- **FR-10**: A `transcript_buffer_max_bytes` config knob SHALL be added beside
  `transcript_retention` (same config section — they form one transcript-policy surface),
  default 4 MiB (resolved decision 1), with the same project-wins merge behavior as sibling
  fields.
- **FR-11**: On accumulated-cap overflow, the buffer SHALL retain the most recent content
  (ring-tail), record the dropped-byte count as buffer **metadata** (no marker bytes spliced
  into content — ADR-002), and preserve `high_water` monotonicity; merges after overflow
  remain correct under the tail-window-equivalence semantics of FR-02.

### Purge lifecycle and audit

- **FR-12**: The buffer SHALL be freed on (a) `drain_and_signal_session` and (b)
  `sweep_stale_sessions` — both via the existing key removal — with a content-free
  `transcript_session_purged` audit event emitted when the purged buffer was non-empty.
- **FR-13**: The audit event SHALL carry session_id, byte count, and timestamp only — never
  transcript content — through the existing append-only `AuditLog`
  (`SqlxStore::log_audit_event`), GC'd by the existing `gc_audit_log` retention policy. No new
  retention machinery.
- **FR-14**: Audit emission SHALL occur outside the registry mutex: purge points snapshot
  (session_id, byte_count) under lock and emit after release; purge success SHALL NOT depend on
  audit-write success (SR-03; an audit failure logs an error without content and the purge
  stands).
- **FR-15**: `context_cycle_review` SHALL clear the buffers of all registry sessions whose
  `state.feature == feature_cycle` via a new registry method (e.g.
  `clear_transcripts_for_feature`): single lock, clear in place, session stays registered,
  purge audit events emitted per FR-13/FR-14. The method SHALL return purged byte counts and be
  shaped so crt-052 can extend it to snapshot bytes before clearing without rewiring call sites
  (SR-04).
- **FR-16**: The cycle-review clear SHALL be gated on matching
  `TranscriptRetention::PurgeOnCycleClose` read from config — an exhaustive match on the enum
  (enterprise seam), not a hardcoded assumption (Constraint 7). Cycle-review output is otherwise
  unchanged.

### PreCompact block

- **FR-17**: In the `CompactPayload` handler, when the session's buffer is non-empty, the server
  SHALL build a transcript-tail block via the shared extraction core `uds/transcript_block.rs`
  (ADR-005): `extract_transcript_block_from_bytes` fed by `contiguous_tail` (12 KB tail-window
  constant family, `MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER`), then `prepend_transcript` onto
  `BriefingContent` content. The local hook's `extract_transcript_block(path)` re-imports the
  same core — parity is structural (resolves OQ-1). The block SHALL carry **no visible elision
  marker**: `contiguous_tail` is the sole content output and elision is metadata-only, so the
  server-built block mirrors local hook output exactly (ADR-002; resolves OQ-3).
- **FR-18**: When the session's buffer is empty (today's local Rust hook, which never streams
  deltas), the `CompactPayload` response SHALL be byte-identical to pre-vnc-025 behavior — the
  empty-buffer condition is the no-double-prepend guard (assumption A2; invariant owned by F3).
- **FR-19**: The tail block SHALL never contain unmerged gap filler (e.g. NUL-filled holes): the
  tail-contiguity check bounds the served window to contiguous received bytes.

### Enterprise seams

- **FR-20**: A single `session_key()` constructor (or newtype) SHALL document the
  `(tenant = "default", project, session)` composite-dimension collapse to today's string key —
  one place to change for enterprise; no call-site re-key (resolved decision 3).
- **FR-21**: `CompactPayload.transcript_excerpt` SHALL remain ignored (vnc-022 ADR-005;
  vnc-024 FR-15 precedence: streamed deltas supersede it).

## Non-Functional Requirements

- **NFR-01 (Secrets posture — SR-02)**: Raw transcript bytes never reach disk, SQL, or logs.
  In-memory + purge IS the guarantee (#4721) — no scanner exists. The buffer type SHALL NOT
  implement `Debug`/`Display` over content (or SHALL redact to length/metadata), and no
  `Result`/error type in the new paths SHALL embed buffer bytes. Hard review gate, not advisory.
- **NFR-02 (Hot-path cost — SR-01)**: No hot-path registry read (`get_state()`, context_search
  paths at `tools.rs:747`/`:1404`) deep-copies transcript bytes. Demonstrated structurally
  (shared handle) or by a clone-cost guard test.
- **NFR-03 (Lock discipline)**: All buffer operations under the registry mutex are bounded
  in-memory work (≤1 MiB memcpy worst case per delta); no I/O or `.await` under the lock.
- **NFR-04 (Memory envelope)**: Per-session memory is bounded by `transcript_buffer_max_bytes`
  (default 4 MiB). Aggregate is per-session cap × concurrent sessions; no global cap is built
  (Constraint 11, human-accepted SR-06). The 4 h `sweep_stale_sessions` eviction is the backstop.
  The evidence trigger for a future global cap SHALL be documented in the architecture artifact.
- **NFR-05 (Crash posture)**: A crash loses in-flight transcript by design (principle 8);
  behavior degrades to crt-052 reconstruction. No spill, no recovery.
- **NFR-06 (Dependencies)**: No new runtime dependency; `cargo audit` passes.
- **NFR-07 (File discipline)**: The buffer lands as a new focused module ≤500 lines; call-site
  changes in `session.rs`/`listener.rs`/`tools.rs` are thin wiring only.
- **NFR-08 (Wire freeze)**: No changes to `TranscriptDeltaPayload`, the
  `transcript_delta` event-type string, or ts-rs bindings — consume-only on the wire surface.
- **NFR-09 (Never-panics contract — ADR-008, R-02/R-06)**: No input reachable from the wire —
  any `offset: u64`, any `bytes` up to the 1 MiB frame ceiling — SHALL be able to cause a panic inside
  `TranscriptBuffer`. All internal offset arithmetic uses `checked_*`/`saturating_*`; on delta-end
  overflow (`offset.checked_add(len)` = `None`) the **whole delta is silently dropped** — no
  partial write, no state change, no `high_water` update, no elision accounting, no log line.
  Drop-whole is deliberate (ADR-008): it SHALL NOT be "improved" into partial-clip of the
  in-range prefix. Defense-in-depth: no `lock().unwrap()` on the buffer mutex anywhere — every
  lock site recovers from poison via `into_inner()` and `clear()`s the buffer (treat-as-empty,
  the only state with guaranteed invariants), preserving always-Ack on merge (ADR-003) and
  degrading PreCompact to the empty-buffer path — a poisoning event never bricks the session.
  Verification (per ADR-008/R-06.2): fuzz-ish randomized (offset, len) no-panic test including
  near-`u64::MAX` offsets; an explicit poisoned-mutex test (poison via a deliberately panicking
  closure in a test helper, then assert merge-resumes-empty with `Ack` and
  PreCompact-degrades-empty); plus grep-able review gate — no bare `unwrap()` on the buffer
  mutex, no unchecked offset arithmetic, no raw `offset as usize`.

## Acceptance Criteria

AC-IDs are verbatim from SCOPE.md. Verification methods per SR recommendations.

| AC | Criterion (summary) | Requirements | Verification |
|----|---------------------|--------------|--------------|
| AC-01 | UDS direct-dispatch delta for a registered session merges into that session's buffer and returns `Ack`; buffer content equals streamed bytes | FR-02, FR-06 | Integration test: direct dispatch, assert `Ack` + buffer bytes |
| AC-02 | Out-of-order/duplicate/overlapping deltas converge to identical content; `high_water` = max(offset+len) | FR-02 | Unit/property test on `apply_delta` permutations (PoC semantics). Below the cap: assert full-content equality across arrival orders. Overflow+reorder cases (assumption A1, per ADR-002): assert **tail-window equivalence** (final contiguous tail identical across orders) and below-floor deltas as defined no-ops — not full-content equality |
| AC-03 | Unregistered-session delta: silent no-op, `Ack`, no slot, no cross-session effect | FR-04, FR-06 | Integration test: dispatch to unknown session, assert registry size + other buffers unchanged |
| AC-04 | Malformed payload still `Ack`s; no `Error` to client; no payload content in logs | FR-06, NFR-01 | Integration test with log capture asserting no payload bytes in output |
| AC-05 | No delta — single/batch, UDS/HTTP — produces a durable row or disk write; mixed batch persists only non-delta events | FR-07, NFR-01 | vnc-024 AC-12 zero-rows test preserved unmodified + new mixed-batch test asserting row counts (SR-07) |
| AC-06 | HTTP `/observe` merges into `http-`prefixed session; `prefix_session_id` preserves `event_type` (single + batch); `SessionWrite` enforced | FR-08 | Per-transport pre-dispatch transform tests + one shared-arm direct-dispatch proof (transport-convergence pattern #4725) |
| AC-07 | Accumulated buffer never exceeds `transcript_buffer_max_bytes`; overflow keeps tail, records dropped-byte count as metadata (no content marker); `high_water` monotonic | FR-10, FR-11 | Unit test: drive past cap, assert size bound, tail content, dropped-byte counter; assert no marker bytes appear in content |
| AC-08 | `drain_and_signal_session` + `sweep_stale_sessions` free the buffer and emit content-free `transcript_session_purged` (session_id, bytes, timestamp) when non-empty | FR-12, FR-13, FR-14 | Integration tests at both purge points; assert audit row fields and absence of content; assert no emission for empty buffers |
| AC-09 | `context_cycle_review` clears buffers of sessions attributed to the reviewed `feature_cycle` (empty afterward, still registered), emits purge audit, gated on `PurgeOnCycleClose` from config; review output otherwise unchanged | FR-15, FR-16 | Integration test through the tool handler; snapshot/compare review output pre/post |
| AC-10 | `get_state()` and hot-path registry reads do not deep-copy transcript bytes | FR-03, NFR-02 | Structural demonstration (shared handle in code review) or clone-cost guard test (SR-01) |
| AC-11 | Non-empty buffer: `CompactPayload` returns `BriefingContent` with server-built tail block prepended; empty buffer: byte-identical to pre-vnc-025 (no double-prepend) | FR-17, FR-18, FR-19 | Golden-output parity test per ADR-005: expected = `extract_transcript_block(path)` on a fixture JSONL transcript; actual = stream the same file bytes as shuffled+duplicated deltas, then `extract_transcript_block_from_bytes(contiguous_tail(...))`; byte-for-byte equality, no hand-written expectation (SR-05, #3426); plus byte-identical empty-buffer snapshot |
| AC-12 | Raw transcript bytes never reach disk: no SQL write, no file write, no log line carries buffer content | NFR-01 | Code-review gate + grep/test assertion over `tracing` calls in new code paths (hard criterion per SR-02) |
| AC-13 | `cargo audit` passes; no new runtime dependency | NFR-06 | CI/protocol `cargo audit` run + `Cargo.toml` diff review |

## Domain Models

Ubiquitous language for downstream agents:

- **Transcript delta**: a `transcript_delta` wire event carrying
  `TranscriptDeltaPayload { offset: u64, bytes: String }` (frozen F1 contract, `wire.rs:284-289`).
  Fire-and-forget: always `Ack`d, never errored. **Delta-content contract (load-bearing for
  F3, per ADR-005)**: deltas carry raw transcript-file bytes (JSONL); offsets are file byte
  offsets.
- **TranscriptBuffer**: per-session in-memory accumulator of opaque transcript bytes. Fields
  (conceptual): bytes, `high_water` (max offset+len seen), dropped-byte counter (the elision
  record — metadata, never content), encapsulated contiguity state. Never persisted;
  content-opaque (no Debug/Display over bytes).
- **`high_water`**: monotonic high-water mark of streamed offsets. Survives overflow elision —
  it tracks what was *sent*, not what is *retained*.
- **Idempotent offset-bounded merge**: writes land at their declared offset; duplicates and
  reordering are no-ops or in-place rewrites; convergence is order-independent (ass-069 Q1 PoC
  semantics).
- **Ring-tail overflow**: on exceeding the accumulated cap, oldest bytes are dropped, newest
  retained; the discontinuity is recorded as metadata (dropped-byte count), never as marker
  bytes in content (ADR-002).
- **Tail-window equivalence**: the convergence guarantee under overflow — cap-crossing delta
  sequences converge on the same final contiguous tail window regardless of arrival order;
  full-content equality holds only below the cap (ADR-002).
- **Tail-contiguity**: the property that the served tail window contains only contiguously
  received bytes (no gap filler). The PreCompact block is bounded by it.
- **Purge**: removal of buffer bytes from memory. Three triggers: session close
  (`drain_and_signal_session`), staleness sweep (`sweep_stale_sessions`, 4 h), cycle review
  (`context_cycle_review`, gated on `PurgeOnCycleClose`).
- **`transcript_session_purged`**: content-free audit operation (session_id, byte count,
  timestamp) on the existing `AuditEvent` schema, mirroring the `uds_auth_failure` pattern.
- **Transcript-tail block**: the ~12 KB tail extract prepended to `BriefingContent` on
  PreCompact — today built client-side by the Rust hook; vnc-025 builds it server-side from
  the buffer via the shared `uds/transcript_block.rs` core (`from_path` for the hook,
  `from_bytes` for the server — ADR-005). Carries no elision marker.
- **Session key seam**: the documented `(tenant = "default", project, session)` →
  string-`session_id` collapse, constructed in exactly one place.
- **Ships dark**: no production client emits deltas until F3; all behavior except the
  empty-buffer PreCompact path is exercised only by tests.

Relationships: `SessionRegistry` (Mutex<HashMap<session_id, SessionState>>) → `SessionState`
holds a shared handle to → `TranscriptBuffer`. `AuditLog` receives purge events.
`CompactPayload` handler reads the buffer (the only reader in vnc-025).

## Workflows

### W1 — Delta ingestion (dark until F3; test-driven)
1. Client (F3, or test fixture) sends `transcript_delta` via UDS `RecordEvent`/`RecordEvents` or
   HTTP `/observe` (bearer + `SessionWrite` required on HTTP).
2. HTTP only: `prefix_session_id` rewrites to `http-{id}`, preserving `event_type`.
3. Dispatch routes the delta to `apply_transcript_delta`; batch arms tee before the
   non-persistence filter.
4. Registered session → idempotent merge into its buffer (cap-enforced, ring-tail on overflow).
   Unregistered/malformed/over-cap → silent no-op.
5. `Ack` returned in all cases. The delta never reaches observation persistence.

### W2 — PreCompact (the only live-before-F3 behavioral surface)
1. `CompactPayload` arrives for a session.
2. Buffer non-empty → server builds the contiguous tail block (12 KB window) via the shared
   extraction core (`extract_transcript_block_from_bytes`, ADR-005) and prepends to
   `BriefingContent`. No elision marker appears in the block.
3. Buffer empty (all current local-hook sessions) → response byte-identical to today; the local
   hook continues its client-side prepend untouched.

### W3 — Purge at session end / staleness
1. `drain_and_signal_session` or `sweep_stale_sessions` removes the session key (buffer freed
   structurally).
2. Under the lock: snapshot (session_id, byte_count) if buffer non-empty.
3. After lock release: emit `transcript_session_purged`. Audit failure does not undo or block
   the purge.

### W4 — Purge at cycle review
1. `context_cycle_review` runs for `feature_cycle`.
2. Config `transcript_retention` matched: `PurgeOnCycleClose` → call
   `clear_transcripts_for_feature(feature_cycle)`.
3. Buffers of attributed sessions cleared in place (sessions stay registered); byte counts
   returned; audit events emitted outside the lock.
4. Review output is unchanged (no distillation — crt-052 later inserts its snapshot step ahead
   of the clear via the same method).

### Accepted lifecycle hazard (SR-09 — explicit)
If a feature's sessions idle past the 4 h sweep before cycle review runs, their transcripts are
purged first and the cycle-review clear is a no-op for them. **This silent loss is accepted**:
it degrades to crt-052's reconstruction fallback, consistent with the crash posture (NFR-05).
No mitigation is built in vnc-025.

## Constraints

Carried verbatim from SCOPE.md Constraints 1–11 (binding on architecture and implementation),
with these spec-level emphases:

1. Secrets posture is architectural (no redactor): any persist/spill/log of raw transcript is a
   review rejection (Constraint 1, NFR-01).
2. Buffer field shape is the **first** architecture decision — everything downstream depends on
   it (SR-01, Constraint 2).
3. Registry mutex: bounded in-memory work only under lock (Constraint 3, FR-14, NFR-03).
4. Fire-and-forget: always `Ack`, never `Error` (Constraint 4, FR-06).
5. Batch-filter non-persistence is load-bearing: the existing filter stays untouched; merge tees
   before it (Constraint 5, SR-07, FR-07).
6. New focused module; over-limit files get thin wiring only (Constraint 6, NFR-07).
7. Exhaustive match on `TranscriptRetention` (Constraint 7, FR-16).
8. Ships dark; verification is test-driven (Constraint 8).
9. Tolerate deltas up to the 1 MiB frame ceiling (Constraint 9, FR-05).
10. Wire contract frozen (Constraint 10, NFR-08).
11. No global memory cap — per-session cap × N, sweep as backstop, human-accepted
    (Constraint 11, NFR-04).

## Dependencies

- **vnc-024 (F1, shipped, commit `70b3aeb7`)**: `transcript_delta` wire type,
  `TranscriptDeltaPayload`, accept-and-drop arms being replaced, `transcript_retention` config
  (first consumer is this feature), zero-rows test (AC-12 there → preserved by AC-05 here).
- **Existing components**: `SessionRegistry`/`SessionState` (`infra/session.rs`), UDS listener
  dispatch (`listener.rs`), HTTP `/observe` router (`router.rs`), `AuditLog`/`AuditEvent`
  (`infra/audit.rs`, `store/schema.rs`), `gc_audit_log` (`store/retention.rs`),
  `context_cycle_review` handler (`mcp/tools.rs:1918`), shared transcript-block extraction
  core (`uds/transcript_block.rs`, extracted from `hook.rs` per ADR-005 — same crate, hook
  call sites re-import; resolved OQ-1).
- **Crates**: none new (NFR-06).
- **Consumers (forward)**: crt-052 (#689) reads the buffer at cycle review via the FR-15 method;
  F3 TS client produces deltas; both out of scope here.

## NOT in Scope

Verbatim exclusions from SCOPE.md Non-Goals — restated to block scope creep:

- Distillation at cycle review and reconstruction fallback (crt-052 #689); nothing reads the
  buffer except the PreCompact block.
- TS client / delta production (offset tracking, client truncation) — F3.
- Enterprise acknowledged-delivery / at-least-once audit (ass-069 Q7 named gap).
- Honoring `TranscriptRetention::RetainDays` — OSS `validate()` rejects it; no durable
  transcript persistence of any kind.
- Disk spill, crash recovery, persistence of raw transcript.
- Changes to the 23 detection rules or cycle-review output.
- Any *interpretive* transcript parsing — knowledge extraction, distillation (crt-052 /
  ass-070). The mechanical JSONL→exchange-turn formatting in the shared extraction core is
  what the local hook already does and is required by like-for-like parity (goal 5); it is
  not this exclusion (ADR-005).
- Activating `CompactPayload.transcript_excerpt` (stays ignored legacy).
- Full registry re-key to a composite key type (seam only, FR-20).
- Retirement of feature-attribution heuristics (separate feature).
- OAuth / multi-tenant `http-{subject_hash}` prefixing (enterprise).
- A global aggregate memory cap (deliberately excluded, Constraint 11).

## Traceability

| Risk | Carried by |
|------|-----------|
| SR-01 | FR-03, NFR-02, AC-10; architecture's first decision |
| SR-02 | NFR-01, AC-04, AC-12 (hard gates) |
| SR-03 | FR-14, AC-08 |
| SR-04 | FR-15 (`clear_transcripts_for_feature` shape) |
| SR-05 | AC-11 golden-parity verification (#3426) |
| SR-06 | NFR-04 (accepted; evidence trigger documented by architect) |
| SR-07 | FR-07, AC-05 (vnc-024 zero-rows test unmodified) |
| SR-08 | FR-09 |
| SR-09 | W3/W4 accepted-hazard statement; FR-18 empty-buffer invariant flagged as F3 contract |

## Resolved Questions (architecture round, 2026-06-05)

All three open questions were resolved by the architect's ADRs; no conflicts with requirements.

- **OQ-1 (assumption A3) — resolved by ADR-005**: extraction is shared, not duplicated. The
  hook and server live in the same crate; the extraction core moves to `uds/transcript_block.rs`
  with two entry points — `extract_transcript_block(path)` (hook) and
  `extract_transcript_block_from_bytes` (server) — so parity is structural. The AC-11 golden
  test verifies `from_path` vs. `from_bytes` parity on one fixture. Reflected in FR-17, AC-11,
  Dependencies.
- **OQ-2 (assumption A1) — resolved by ADR-002**: order-independence is exact below the cap;
  once ring-tail elision advances the buffer floor, deltas below the elided floor are defined
  no-ops (clipped, counted, `high_water` monotonic), and cap-crossing sequences converge on the
  final tail window (**tail-window equivalence**). AC-02 and AC-07 no longer conflict.
  Reflected in FR-02, FR-11, AC-02, AC-07, Domain Models.
- **OQ-3 — resolved by ADR-002**: elision is metadata, never bytes spliced into content
  (spliced markers would corrupt offset math and JSONL parsing). `contiguous_tail` is the sole
  content output, so the PreCompact tail block carries **no visible elision marker** —
  preserving byte parity with local hook output, which has no elision concept. Reflected in
  FR-11, FR-17, AC-07, Domain Models.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned #4721 (vnc-024 ADR-005 retention/secrets
  posture, already cited in scope) and otherwise tangential entries; no new constraints surfaced.
