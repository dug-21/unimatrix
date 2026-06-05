# Risk-Based Test Strategy: vnc-025

Inputs: SCOPE.md, ARCHITECTURE.md + ADR-001..008, SPECIFICATION.md (FR-01..21, NFR-01..08,
AC-01..13), SCOPE-RISK-ASSESSMENT.md (SR-01..09). Historical evidence: Unimatrix #2299, #2249,
#2266, #4379 (audit-write pool starvation cluster), #3902 (dispatch-arm registry-call audit
interaction), #4140 (silent-eviction attribution loss), #3426 (golden-output pattern), #2984
(test expectations copied from wrong inputs), #735 (spawn_blocking saturation).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Hole-tracking merge (`apply_delta`) is incorrect under reorder/duplicate/overlap — holes mis-split, span mis-extended, below-base clipping wrong | High | High | Critical |
| R-02 | Offset arithmetic unsoundness: `offset + bytes.len()` overflow, far-future offset jump, u64→usize conversions — panic (poisons buffer mutex) or unbounded allocation | High | Medium | High |
| R-03 | Ring-tail overflow × out-of-order interaction violates tail-window equivalence; `high_water` regresses; elided-byte accounting drifts | High | Medium | High |
| R-04 | Delta bytes reach a durable row — batch-filter regression or new path from tee loop to `insert_observations_batch` | High | Low | High |
| R-05 | Transcript content leaks to logs/audit/Debug output — derived `Debug` on `SessionState` reaches a tracing line, parse-failure path logs payload, audit `detail` interpolates content | High | Medium | High |
| R-06 | Lock-ordering violation or buffer-mutex poisoning: a panic inside `apply_delta`/`contiguous_tail` poisons the per-session mutex, bricking merges and PreCompact for that session | High | Low | High |
| R-07 | Audit emission failure modes: `log_event_async` misuse in the `handle_session_close` sync/async boundary, spawn burst when sweep purges many sessions at once, silent audit loss (#2299/#2249/#2266/#4379 cluster) | Medium | Medium | Medium |
| R-08 | Drain/sweep signature changes regress call sites; silently-evicted sessions (empty `injection_history`, no `SweepResult`) miss the purge audit — AC-08 hole named in ADR-004 | Medium | Medium | Medium |
| R-09 | PreCompact parity drift or double-prepend: `from_bytes` diverges from `from_path` (mid-line tail start, hole-truncated window shorter than 12 KB), or empty-buffer path is no longer byte-identical | High | Medium | High |
| R-10 | Cycle-review clear semantics wrong: feature matching misses/over-matches sessions (`state.feature == None`), post-clear merges misbehave (cleared buffer + resumed high-offset stream creates a giant hole), retention enum gate hardcoded instead of matched | Medium | Medium | Medium |
| R-11 | Config plumbing gaps: `with_transcript_cap` not wired at all three production sites, `validate()` floor missing, project-wins merge arm absent — silent fallback to default | Low | Medium | Low |
| R-12 | HTTP convergence regression: `prefix_session_id` drops/alters `event_type` for a batch shape; `SessionWrite` not enforced for delta-only requests | Medium | Low | Low |
| R-13 | Prompt-injection surface: streamed transcript bytes (remote-writable with a bearer) are prepended verbatim into `BriefingContent` at compaction — adversarial content steers the post-compaction agent | Medium | Low | Medium |
| R-14 | `hook.rs` extraction move regresses the local hook — the only live-in-production path today; an import/constant slip changes behavior for every existing session | High | Low | Medium |
| R-15 | Pathological sparse deltas exhaust hole metadata; collapse-to-newest at the 64-range cap loses more than intended or miscounts elision | Medium | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Merge correctness under reorder/duplicate/overlap (Critical)
**Severity**: High. **Likelihood**: High — ADR-002 itself calls hole bookkeeping "the most
intricate code in the feature."
**Impact**: Corrupted buffer content silently feeds a wrong PreCompact block; AC-01/AC-02 fail.

**Test Scenarios**:
1. Property-style permutation test: a fixed set of covering deltas applied in shuffled orders
   (plus duplicates and partial overlaps) converges to identical full content and identical
   `high_water` — below the cap (AC-02 exact-equality arm). Reuse ass-069 PoC fixtures.
2. Hole-surgery unit tests: a write that (a) fully fills a hole, (b) shrinks it from each end,
   (c) splits it in two, (d) spans multiple holes; assert hole list and content after each.
3. Delta entirely below `base_offset` after ring-tail advance: defined no-op, clipped bytes
   counted in `elided_bytes`, `high_water` still updated (FR-02).
4. Delta starting beyond span end creates a hole; `contiguous_tail` never returns bytes
   crossing it (FR-19) — assert no zero-fill ever appears in any returned tail.
5. Beware #2984: derive expected content programmatically from the covered-range set, never
   hand-copy expected bytes between scenarios.

**Coverage Requirement**: Permutation convergence + every hole-mutation class + clipping +
tail-contiguity. This is the densest test surface in the feature; treat `apply_delta` as a
pure state machine and test it exhaustively at unit level.

### R-02: Offset arithmetic unsoundness (High)
**Severity**: High — a panic under the buffer mutex poisons it (see R-06); `offset` is
attacker-controlled u64 on an authenticated but untrusted wire.
**Likelihood**: Medium.
**Impact**: Per-session DoS (poisoned mutex) or memory blow-up; release-mode wrap corrupts
offset math silently.

**Test Scenarios**:
1. `apply_delta(u64::MAX - 10, &[0u8; 100])` — no panic, no wrap: per ADR-008 Layer 1, the
   whole delta is silently dropped (`checked_add` → `None`): no state change, no `high_water`
   update, no `elided_bytes` accounting.
2. Offset jump far ahead (e.g. `1 << 40`) with a small payload: allocation stays ≤ cap
   (ADR-002 ring-tail claim — verify, don't trust), `base_offset` advances, prior content
   counted as elided.
3. Single delta of exactly 1 MiB (frame ceiling, FR-05) into a 4 MiB buffer and into a
   64 KiB-cap buffer (the `validate()` floor): both merge without panic; the small-cap case
   ring-tails correctly.
4. u64→usize boundary: ADR-008 pins conversions to span-relative values already proven
   ≤ `max_bytes` (no raw `offset as usize`) — review-verify the invariant comment at each
   conversion site; randomized test covers the dynamic side.

**Coverage Requirement**: ADR-008 Layer 1's contract — no input reachable from the wire (any
`offset: u64`, any `bytes` ≤ 1 MiB) can panic inside `TranscriptBuffer`. The fuzz-ish
randomized test over (offset, len) pairs is the named verification for that contract.

### R-03: Overflow × reorder — tail-window equivalence (High)
**Severity**: High. **Likelihood**: Medium — A1 explicitly flags that the PoC never combined
overflow with reorder.
**Impact**: AC-02/AC-07 conflict resurfaces; PreCompact serves order-dependent content.

**Test Scenarios**:
1. Cap-crossing delta sequence applied in multiple arrival orders: final
   `contiguous_tail(window)` identical across orders (tail-window equivalence per FR-02);
   full content explicitly NOT asserted equal.
2. Late head-fill after ring-tail advanced `base_offset`: clipped, counted, no content change.
3. `high_water` monotonicity across overflow events, including when the clipping delta itself
   carries the new maximum.
4. `elided_bytes` accounting: sum of (clipped + ring-dropped) bytes matches a hand-computable
   fixture; never double-counted when a hole is dropped below base.

**Coverage Requirement**: The AC-02-under-overflow arm as specified (tail-window equivalence),
plus accounting invariants. Combine with R-01's permutation harness — same fixture machinery,
cap set low to force overflow.

### R-04: Delta bytes → durable row (High)
**Severity**: High — reopens the exact channel vnc-024 ADR-004 R-04 closed; secrets to disk.
**Likelihood**: Low — ADR-003 keeps the filter untouched; the risk is a future refactor slip.
**Impact**: Raw conversation bytes persisted in observation rows.

**Test Scenarios**:
1. vnc-024's zero-rows test (its AC-12) runs **unmodified** with the buffer active (AC-05) —
   the test must not be edited to pass.
2. Mixed batch (deltas + normal events, UDS and HTTP): normal events persist with exact row
   counts; deltas merge into the buffer; zero delta-derived rows (assert on row content too,
   not just count — a delta's bytes must not appear in any persisted column).
3. Review-diff gate: the filter line at `listener.rs:1009` is byte-identical pre/post (ADR-003
   reduces SR-07 to a one-line diff check — make it explicit in review).

**Coverage Requirement**: Zero-rows preserved + mixed-batch row-content assertion on both
transports' arms.

### R-05: Content leak to logs/audit/Debug (High)
**Severity**: High — NFR-01: in-memory + purge IS the secrets guarantee; no redactor behind it.
**Likelihood**: Medium — `SessionState` keeps `derive(Debug)`; one `tracing::debug!("{:?}",
state)` anywhere prints the buffer's Debug; parse-failure and audit paths are classic leak
points.
**Impact**: Raw conversation bytes in logs or audit rows; unrecoverable secret exposure.

**Test Scenarios**:
1. `TranscriptBuffer` Debug output test: format a populated buffer with `{:?}`; assert output
   contains metadata fields and none of the payload bytes (use a sentinel string).
2. Log-capture integration test (extends AC-04): dispatch malformed AND well-formed deltas with
   a sentinel payload; assert the sentinel never appears in captured tracing output across the
   single arm, batch tee, merge, overflow, and purge paths.
3. Audit-row assertion (extends AC-08/AC-09): purge a buffer whose content holds the sentinel;
   assert the `transcript_session_purged` row (all columns, incl. `detail`) is sentinel-free.
4. Static gate: grep for `tracing` calls in `infra/session_transcript.rs` and
   `uds/transcript_block.rs` touching content-bearing values; verify no `Display` impl and no
   content-bearing `Result` in the new modules' public API (AC-12 hard gate).

**Coverage Requirement**: Sentinel-based dynamic checks on every new code path + the static
grep/review gate. Both, not either.

### R-06: Lock ordering / mutex poisoning (High)
**Severity**: High. **Likelihood**: Low (discipline is documented), but consequence is a
bricked session.
**Impact**: Poisoned buffer mutex → every later `lock()` on that session panics or errors;
merges and PreCompact dead for the session; possibly a crashed dispatch task.

**Test Scenarios**:
1. Concurrency smoke test: N tasks streaming deltas to one session while M tasks run
   `get_state()` + PreCompact reads — completes without deadlock (registry→buffer order only).
2. Poisoned-mutex test (verifies ADR-008 Layer 2): poison the buffer mutex via a deliberately
   panicking closure in a test helper; assert every lock site recovers via
   `into_inner()` + `clear()` — merge resumes against an empty buffer with `Ack` (always-Ack
   preserved), PreCompact degrades to the empty-buffer path, purge reports best-effort
   `bytes_purged`. Review gate: no bare `unwrap()` on the buffer mutex anywhere.
3. `clear_transcripts_for_feature` under concurrent delta stream: no deadlock (Arcs cloned
   under registry lock, cleared after release per ADR-004); post-clear merges still apply.
4. Orphaned-Arc merge (delta racing drain/sweep key removal): merge lands in the orphaned
   buffer, frees on drop, no panic, no effect on a re-registered session of the same id.

**Coverage Requirement**: One loom-style or stress concurrency test + the explicit
poisoned-mutex test against ADR-008's pinned policy (the poison path is near-dead code by
Layer 1 design, yet must still be tested — ADR-008 names this consequence).

### R-07: Audit emission failure modes (Medium)
**Severity**: Medium. **Likelihood**: Medium — this exact failure class recurred across
nxs-011 and two bugfixes (#2299, #2249, #2266, #4379: write-pool starvation/deadlock from
audit writes in the wrong execution context).
**Impact**: Lost purge audits (compliance hole) or, worse, pool starvation regressions in
unrelated paths.

**Test Scenarios**:
1. `handle_session_close` path: verify emission uses `log_event_async` + `tokio::spawn`
   fire-and-forget (no `log_event`/`block_in_place` in async context — #4379 pattern);
   assert via test that the close path completes even when the audit store is unavailable.
2. Audit-failure independence (FR-14): inject an audit-write failure; assert the purge
   completed (buffer gone/cleared), a content-free `tracing::warn!` fired, and no retry loop.
3. Sweep burst: sweep 20+ sessions with non-empty buffers in one pass; all audit rows
   eventually land; write pool does not starve (single-connection pool precedent #2266).
4. Zero-byte suppression: empty-buffer purges emit nothing (ADR-004) — assert absence.

**Coverage Requirement**: All three purge points exercised against a failing and a slow audit
sink; emission-context review against pattern #4379.

### R-08: Drain/sweep signature changes + silently-evicted audit gap (Medium)
**Severity**: Medium. **Likelihood**: Medium — ADR-004 names it: "needs an explicit test or it
will regress unnoticed"; precedent #4140 shows silent-eviction paths hide attribution loss.
**Impact**: AC-08 hole — sessions evicted with empty `injection_history` purge a non-empty
buffer with no audit trail; or call sites at `listener.rs:1796/:1814` mis-handle new tuples.

**Test Scenarios**:
1. The named case: register a session, stream deltas, never inject (empty
   `injection_history`), idle past 4 h (mocked clock) → swept with NO `SweepResult` but WITH
   a `TranscriptPurgeRecord` and an audit row.
2. Drain returning `Some((SignalOutput, Some(record)))` vs `Some((_, None))` (empty buffer)
   vs `None` (unknown session) — all three shapes handled at the listener call site.
3. `SignalOutput` shape regression guard: serialized signal-queue output unchanged (it feeds
   the persisted queue — ADR-004 firm constraint).

**Coverage Requirement**: The silently-evicted case is mandatory and named; all return-shape
variants exercised at real call sites, not just unit level.

### R-09: PreCompact parity drift / double-prepend (High)
**Severity**: High — this is the feature's like-for-like centerpiece (goal 5), and it ships
dark: drift is invisible until F3 (SR-05).
**Likelihood**: Medium.
**Impact**: Remote PreCompact block diverges from local hook output; or legacy local-hook
sessions get a double-prepend.

**Test Scenarios**:
1. Golden parity test per ADR-005/#3426: fixture JSONL transcript; expected =
   `extract_transcript_block(path)`; actual = stream the same file bytes as shuffled +
   duplicated deltas → `extract_transcript_block_from_bytes(contiguous_tail(...))`;
   byte-for-byte equality. No hand-written expectation.
2. Mid-line tail start: force `contiguous_tail` to begin mid-JSONL-line; assert the partial
   line is filtered identically to the path variant's mid-line seek behavior.
3. Hole-truncated window: a hole inside the last 12 KB → served window is shorter; block still
   well-formed; never includes pre-hole bytes.
4. Empty-buffer byte-identity (AC-11/FR-18): snapshot the full `CompactPayload` response
   pre-change vs post-change for a session that never streamed — byte-identical (the
   no-double-prepend guard).
5. Prepend-before-token-count ordering: `token_count` reflects the prepended block (parity
   with how the local hook's client-side prepend affects what the agent sees).
6. Live-buffer read consistency: deltas arriving concurrently with the compact read produce a
   point-in-time tail (no torn read) — covered by the buffer lock; assert block parses.

**Coverage Requirement**: Golden test + empty-buffer byte-identity are both hard gates; the
local hook's existing test suite passes unmodified after the ADR-005 move (see R-14).

### R-10: Cycle-review clear semantics (Medium)
**Severity**: Medium. **Likelihood**: Medium.
**Impact**: Wrong sessions cleared (or missed), post-clear merge weirdness, enterprise seam
hardcoded.

**Test Scenarios**:
1. Mixed registry: sessions with `feature == Some(cycle)`, `Some(other)`, and `None` — only
   the first group clears; all stay registered; counts returned match.
2. Post-clear resumed stream: after `clear()`, deltas continue at high file offsets — define
   and assert the behavior (likely: large hole → ring-tail/collapse → tail still serveable;
   whatever the choice, it must be tested, not emergent).
3. `clear()` return value = bytes purged; `high_water`/`elided_bytes` post-clear semantics
   pinned by test (crt-052 will build on them).
4. Retention gate: exhaustive `match` on `TranscriptRetention` (FR-16) — compile-level
   guarantee; test that review output is otherwise unchanged (snapshot pre/post, AC-09).
5. Review runs for a feature with zero attributed sessions: no-op, no audit, no error.

**Coverage Requirement**: Attribution-matching matrix + post-clear merge behavior pinned.

### R-11: Config plumbing (Low)
**Scenarios**: (1) serde default = 4 MiB when field absent; (2) `validate()` rejects
`< 65_536` with a clear error; (3) project-wins merge arm overrides global; (4) all three
production construction sites use `with_transcript_cap` (grep/review gate); (5) cap actually
reaches a freshly registered session's buffer (integration: set 128 KiB cap, overflow at
128 KiB not 4 MiB).
**Coverage Requirement**: Scenario 5 is the one that catches wiring gaps the unit tests miss.

### R-12: HTTP transport convergence (Low)
**Scenarios**: per pattern #4725 — (1) `prefix_session_id` preserves `event_type` for the
single shape and for every element of a batch (incl. mixed batches); (2) delta lands in the
`http-{id}` buffer, not the bare-id buffer; (3) missing/insufficient bearer or absent
`SessionWrite` capability → rejected before dispatch (no merge occurs).
**Coverage Requirement**: Pre-dispatch transform tests per transport + shared-arm proof once
via direct dispatch.

### R-13: Prompt injection via streamed transcript (Medium, security)
**Severity**: Medium. **Likelihood**: Low (requires a valid bearer + SessionWrite).
**Impact**: An attacker (or compromised client) streams crafted "transcript" bytes that the
server later prepends into `BriefingContent` — instructions injected into the post-compaction
agent context. Blast radius: that session's agent only; no persistence, no cross-session reach.
**Test Scenarios**:
1. Assert the served block is bounded (≤ `MAX_PRECOMPACT_BYTES` budget) and structurally
   wrapped by the same header/footer as the local hook — an attacker cannot inflate the block
   past the budget regardless of buffer size.
2. Document-and-accept test: the content itself is untrusted by design (identical exposure to
   today's local hook reading a local file); record the equivalence explicitly so the
   acceptance is deliberate, not overlooked.
**Coverage Requirement**: Budget-bound assertion; no content sanitization is in scope
(like-for-like), and that acceptance is written down.

### R-14: hook.rs extraction move regresses the live local path (Medium)
**Severity**: High — the local hook is the only production-live transcript behavior today.
**Likelihood**: Low — move is "verbatim where possible."
**Test Scenarios**:
1. Entire existing `hook.rs` test module passes unmodified (imports aside) after the move —
   per pattern #3253, verify the test *names* in the moved/retained suites match the
   pre-move inventory (no silently dropped tests).
2. Constants pinned: `MAX_PRECOMPACT_BYTES == 3000`, `TAIL_MULTIPLIER == 4` asserted in the
   new module (a transposed constant changes both hook and server silently).
**Coverage Requirement**: Pre/post test-name inventory + constant pins.

### R-15: Hole-metadata exhaustion / collapse (Low)
**Scenarios**: (1) drive 64 disjoint holes, then the 65th → collapse to newest contiguous
segment, old span counted as elided, no panic, memory bounded; (2) sustained pathological
sparse stream (alternating far offsets) → throughput remains sane (no O(n²) blow-up in hole
list maintenance); (3) post-collapse merges and `contiguous_tail` remain correct.
**Coverage Requirement**: Collapse correctness + a coarse perf sanity bound. The 64 constant
is tunable (ARCHITECTURE OQ-3); the bounded-metadata property is what tests must pin.

## Integration Risks

- **Dispatch arms × registry** (R-04, R-05, #3902): merge calls sit inside existing arms after
  `sanitize_session_id`/`SessionWrite` — assert no new audit events fire on a normal delta
  dispatch (the #3902 regression signature).
- **Drain/sweep call sites** (R-08): two signature changes ripple to `listener.rs:1796/:1814`
  and the test suite; the silently-evicted case is the hidden seam.
- **Cycle-review handler × registry** (R-10): first-ever registry mutation from `tools.rs`;
  no feature→session index exists — the linear scan and the `None`-feature case are new ground.
- **Snapshot-shares-live-buffer** (R-09.6): `get_state()` snapshots alias the live buffer via
  Arc — any future reader copying "snapshot" semantics will be surprised; pinned by the
  concurrent-read test.
- **Config → registry → buffer cap chain** (R-11.5): three layers; integration test through
  all of them once.

## Edge Cases

- Zero-length delta (`bytes: ""`) at any offset — no-op or defined behavior, never a panic;
  `high_water` semantics for len 0 defined.
- Delta at offset 0 into an empty buffer (the trivial first write) and repeated verbatim
  (pure duplicate).
- Non-UTF8 bytes inside `bytes: String` — impossible by type, but the buffer API takes
  `&[u8]`; direct-call tests must cover invalid-UTF8 since crt-052 will read these bytes.
- Cap exactly equal to one delta's size; delta exactly at the cap boundary (off-by-one).
- `contiguous_tail(window)` with window > buffer len, window 0, and window landing exactly on
  a hole boundary.
- Session re-registration after drain: fresh empty buffer, no ghost content from the orphaned
  Arc.
- Sweep and cycle-review racing: both purge the same session's buffer — at most one non-zero
  audit event (no double-count of the same bytes).

## Security Risks

**Untrusted input surface**: `TranscriptDeltaPayload { offset: u64, bytes: String ≤ ~1 MiB }`
over UDS (local trust) and HTTP `/observe` (bearer + `SessionWrite` — authenticated, NOT
trusted; F1 already established the server must not trust client caps, Constraint 9).

- **Malformed/adversarial input damage**: integer overflow panic → poisoned mutex →
  per-session DoS (R-02/R-06); far-offset jumps probing for unbounded allocation (R-02.2);
  sparse-delta hole-metadata exhaustion (R-15). All bounded by design — every bound gets an
  adversarial test, not just a happy-path one.
- **Blast radius if compromised**: the buffer holds raw conversation bytes for ONE session;
  never persisted, no cross-session reads, freed on purge. Compromise of the component leaks
  at most the in-memory tails of currently-live sessions. The prompt-injection path into
  `BriefingContent` (R-13) is the only outbound influence channel.
- **No path traversal / deserialization risk**: `from_bytes` takes memory, not paths; serde
  JSON parse of the payload is the existing F1 surface, unchanged (wire frozen, NFR-08).
- **Memory DoS**: per-session cap enforced; aggregate deliberately uncapped (SR-06,
  human-accepted) — unregistered-session deltas are free no-ops (good: no allocation before
  the registry check, worth asserting), and session registration itself is the gate.

## Failure Modes

| Failure | Required behavior | Verified by |
|---------|-------------------|-------------|
| Malformed delta payload | `Ack`, content-free debug log, no state change | AC-04, R-05.2 |
| Unregistered session | `Ack`, no slot, no allocation | AC-03, Security note |
| Over-cap merge | Ring-tail, metadata elision, `Ack` | AC-07, R-03 |
| Audit write fails | Purge stands; content-free warn; no retry | FR-14, R-07.2 |
| Buffer mutex poisoned | ADR-008 Layer 2: `into_inner()` + `clear()` treat-as-empty; still `Ack`; PreCompact degrades to empty-buffer path | R-06.2, ADR-008 |
| Server crash | In-flight transcript lost by design (NFR-05); no recovery machinery | Posture review only |
| Sweep before cycle review | Silent transcript loss, accepted; clear is a no-op for swept sessions | Spec W-hazard; R-08.1 adjacent |
| PreCompact with hole in tail | Shorter contiguous block or `None`; never gap filler | FR-19, R-09.3 |
| `contiguous_tail` → `None` | `CompactPayload` response identical to empty-buffer path | FR-18, R-09.4 |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (clone-cost) | — (closed structurally) | ADR-001 `Arc<Mutex<_>>`: `get_state()` clone is 8 bytes + refcount. Residual verification: AC-10 structural review or clone-cost guard test; concurrency behavior covered by R-06 |
| SR-02 (content leak) | R-05 | ADR-002 content-opacity by construction + AC-12 grep gate; R-05 adds sentinel-based dynamic leak tests on every new path |
| SR-03 (audit under lock) | R-07 | ADR-004 collect-under-lock/emit-after-release; R-07 tests failure independence, emission context (#4379), and sweep bursts |
| SR-04 (crt-052 seam shape) | R-10 | ADR-004 names `clear_transcripts_for_feature` as the seam (counts-only, take-shaped later); R-10.3 pins post-clear semantics crt-052 inherits |
| SR-05 (ships dark, parity unproven) | R-09 | ADR-005 shared extraction core makes parity structural; R-09 golden test + empty-buffer byte-identity are hard gates |
| SR-06 (aggregate memory) | — | Accepted at scope review; evidence trigger documented (ARCHITECTURE: >32 sessions or >256 MiB). No test; ops-review posture |
| SR-07 (batch filter) | R-04 | ADR-003 tee-before-untouched-filter; R-04 preserves vnc-024 zero-rows test unmodified + mixed-batch row-content assertions |
| SR-08 (sanitize interaction) | R-12, Integration | ADR-003 reuses sanitized entry, no parallel path; integration test asserts no #3902-signature audit events on delta dispatch |
| SR-09 (lifecycle ordering / double-prepend) | R-08, R-09 | Sweep-before-review loss accepted (spec W-hazard); empty-buffer invariant is an F3 contract (A2) — R-09.4 byte-identity test guards the OSS side |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 5 (permutation harness + hole surgery + clipping + contiguity) |
| High | 5 (R-02, R-03, R-04, R-05, R-09) | 19 |
| Medium | 5 (R-06, R-07, R-08, R-10, R-13) | 16 |
| Low | 4 (R-11, R-12, R-14, R-15) | 12 |

Concentration of effort: `TranscriptBuffer` internals (R-01/R-02/R-03/R-15 share one
property-test harness with a tunable cap), the secrets gates (R-04/R-05 — both are hard
review gates, not advisory), and the golden-parity pair (R-09.1 + R-09.4). The poisoned-mutex
policy (R-06.2) is pinned by ADR-008 (drop-whole checked arithmetic + `into_inner()`+`clear()`
treat-as-empty, always-Ack preserved) — tests verify the pinned policy, no decision remains open.
