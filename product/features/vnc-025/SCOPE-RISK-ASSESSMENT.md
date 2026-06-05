# Scope Risk Assessment: vnc-025

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Transcript buffer rides `SessionState: Clone` — `get_state()` deep-clones on every context_search hot path (session.rs:223, tools.rs:747/:1404); a multi-MB Vec cloned per search is a silent latency regression (Unimatrix #4737) | High | High | Architect must fix field shape FIRST (Arc handle / sibling map / non-cloning accessor) before any other design decision; spec must carry AC-10's structural-or-test proof |
| SR-02 | Secrets posture is purely architectural — no redactor exists (#4721); one stray `tracing` line, debug Display impl, or error path interpolating buffer bytes leaks raw conversation content with no safety net | High | Medium | Architect: design the buffer type to NOT impl Debug/Display over content; spec: require grep/test gate on tracing in new paths (AC-12) as a hard criterion, not advisory |
| SR-03 | Audit emission at purge points: existing purge code runs under registry mutex with strict no-I/O discipline (Constraint 3), but `transcript_session_purged` is a SQL write — risk of awaiting under lock or fire-and-forget audit loss (#735 spawn_blocking saturation precedent) | Medium | Medium | Architect: collect (session_id, byte_count) under lock, emit audit after release; define behavior when audit write fails (purge must still succeed) |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | crt-052 split leaves forward references live (distill-before-purge, reconstruction fallback, range tracking); cycle-review clear method shaped wrong = crt-052 retrofit becomes a rewrite (#3158 deferred-scope pattern) | Medium | Medium | Spec: the new registry clear-by-feature method must return/expose snapshot-able bytes (or be trivially extendable); name the seam explicitly so crt-052 inserts, not rewires |
| SR-05 | Ships dark until F3 — PreCompact "byte-identical" parity and merge semantics are only test-proven; no production traffic validates ordering/drop behavior before F3 builds on it | Medium | Medium | Spec: parity AC (AC-11) needs a golden-output comparison against the local hook's `extract_transcript_block`, not a hand-written expectation (#3426 golden-output pattern) |
| SR-06 | Aggregate memory deliberately unbounded (Constraint 11: per-session 4 MiB × N sessions, no global cap); HTTP `/observe` with a valid bearer can register many sessions and fill each | Low | Low | Accepted at scope review (human-approved). Architect: document the evidence trigger for a global cap; rely on 4 h sweep as backstop |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Batch-arm rewiring: routing deltas to the merge while keeping them out of `obs_batch` touches the load-bearing non-persistence filter (vnc-024 ADR-004 R-04); a refactor slip reopens delta-bytes → durable-row | High | Medium | Architect: merge-then-filter ordering must keep the existing filter untouched; spec: preserve vnc-024's AC-12 zero-rows test with buffer active (AC-05) |
| SR-08 | Adding a registry call inside a UDS dispatch arm has previously triggered unexpected `sanitize_session_id` audit interactions (Unimatrix #3902, crt-026) | Medium | Medium | Architect: check whether `apply_transcript_delta` enters via a path that re-sanitizes session_id; reuse the existing record-path entry pattern, don't add a parallel one |
| SR-09 | Lifecycle ordering: 4 h `sweep_stale_sessions` purges buffers BEFORE cycle review runs for long-lived features — silent transcript loss mirrors the eviction-induced no-op failure mode (#4134, col-022); also, double-prepend guard rests on "local hook never streams deltas" staying true through F3 | Medium | Medium | Spec: state explicitly that sweep-before-review transcript loss is accepted (degrades to crt-052 reconstruction); record the empty-buffer/no-streaming invariant as an F3 contract |

## Assumptions

- **A1** (Why now / Goals 1): ass-069 Q1 PoC merge semantics (0 mis-attribution, ≤50% drop/reorder/dup) transfer intact to the production `TranscriptBuffer` — the PoC did not exercise the 4 MiB ring-tail overflow combined with out-of-order arrival; if overflow+reorder interact badly, AC-02/AC-07 conflict.
- **A2** (Goal 5 / Non-goals): the local Rust hook never streams deltas, so empty-buffer is a reliable no-double-prepend guard. If F3 ships a client that both streams and runs the local hook, parity breaks — invariant must be owned by F3's scope.
- **A3** (Proposed Approach 5): the 12 KB tail-window constant family in `hook.rs` is reusable server-side without divergence; if hook-side extraction logic is client-private, parity becomes a maintained duplicate.
- **A4** (Constraint 11): handful-of-sessions personal-cloud posture holds; if deployment patterns change (many concurrent HTTP sessions), the no-global-cap decision needs revisiting.

## Design Recommendations

1. **Decide buffer field shape first** (SR-01): the architect's opening ADR should be the `SessionState` integration shape — every other decision (merge, purge, PreCompact read) depends on it. Pattern #4737 already records the trap.
2. **Make the buffer content-opaque by construction** (SR-02): no `Debug`/`Display` over bytes, no content in any `Result`/error type; review gate greps new modules for `tracing` + buffer access.
3. **Audit outside the lock** (SR-03): purge points snapshot metadata under lock, emit audit after; purge success must not depend on audit success.
4. **Shape the cycle-review clear for crt-052** (SR-04): one registry method, `clear_transcripts_for_feature`, returning purged byte counts now and extendable to return snapshots later.
5. **Golden-parity test for PreCompact** (SR-05): compare server-built block byte-for-byte against the local hook's output on the same fixture transcript.
6. **Touch the batch filter minimally** (SR-07): tee deltas to the merge before the existing filter line; keep vnc-024's zero-rows test running unmodified.

## Historical Evidence

Unimatrix entries informing this assessment: #4737 (SessionState clone-cost, vnc-025), #3902 (UDS dispatch registry-call audit, crt-026), #4134 (eviction silent no-op, col-022), #735 (fire-and-forget blocking-pool saturation, vnc-010), #3158 (deferred-scope AC ambiguity, crt-026), #3426 (golden-output regression pattern, col-026), #4721 (vnc-024 ADR-005 retention/secrets posture).
