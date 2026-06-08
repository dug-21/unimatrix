# Risk-Based Test Strategy: crt-052

GH Issue: #689. Mode: architecture-risk. Inputs: SCOPE.md, ARCHITECTURE.md, ADR-001..009,
SPECIFICATION.md (FR-1..15, NFR-1..9, AC-01..13 + AC-V-SEAM/AC-V-FUZZ), SCOPE-RISK-ASSESSMENT.md
(SR-01..09). Risks below are specific to the **designed** system — the snapshot seam (ADR-001/002),
the pure distill module (ADR-003), the four-return helper (ADR-005), the Option B held-buffer store
(ADR-008), and the Wave A/B staging boundary (ADR-009) — not generic feature risks. The Option B
held-buffer state machine is the dominant risk surface and carries the most entries.

Historical grounding: pattern #4750 (four success returns), #3753 (use the pre-cloned snapshot, never
re-acquire a lock in a new pipeline step), #4764 (checked-offset / treat-as-empty poison recovery),
#3793 (cycle-review memoization is a synchronous SQL persist — the secrets trap), #3800 (memoization
hit deserializes the stored report).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Held buffer re-adopted under the WRONG `feature_cycle` (re-adopt on re-register, ADR-008 §3) silently mis-scopes candidates into another cycle's review | High | Med | **Critical** |
| R-02 | Held-buffer memory grows unbounded — cap-hit eviction or TTL sweep not exercising, or one reclamation mechanism silently inert (ADR-008 §1/§2) | High | Med | **Critical** |
| R-03 | `transcript_session_purged` audit fires more/less than exactly-once per held session across drain→hold→re-adopt→review/sweep/evict (ADR-008/009, AC-11) | High | Med | **Critical** |
| R-04 | Candidate/buffer content leaks into the memoized `cycle_review_index` row, a log line, or the audit detail — secrets-posture breach (ADR-004/007, SR-07, #3793) | High | Med | **Critical** |
| R-05 | AC-11 simulation is not faithful to the real per-turn-drain lifecycle (fewer than 3 drains, or no deltas-between-drains) → primary path "proven" but actually broken post-merge | High | Med | **Critical** |
| R-06 | A second/third buffer content reader sneaks in (#700 retrofit, or a Wave A helper bypassing the seam), breaking the single-reader invariant (ADR-002, Constraint 4) | High | Low | High |
| R-07 | Distillation wired at fewer than all four `result.is_ok()` returns → cache-hit / purged-signals / degraded paths silently skipped (ADR-005, #4750, SR-05) | High | Med | **Critical** |
| R-08 | Parse/marker-match executed while a registry or buffer lock is held → lock-discipline / latency regression; or deltas streaming during snapshot cause torn read/deadlock (ADR-001, AC-01) | High | Low | High |
| R-09 | Reconstruction-fallback trigger mis-calibrated against tail-window-equivalence → over-fires (discards good buffer) or under-fires (misses real loss) (ADR-006, SR-08, #3359) | Med | Med | High |
| R-10 | JSONL parser panics or errors on malformed/adversarial client-disk bytes instead of skip-with-count → cycle-review handler crashes (ADR-003, FR-14, SR-09) | High | Med | **Critical** |
| R-11 | Wave B contaminates Wave A — Wave A code references `transcript_hold.rs` or assumes non-empty buffers, so reverting Wave B breaks Wave A (ADR-009 rollback boundary) | Med | Med | High |
| R-12 | `byte_offset` reported array-relative not logical (`base_offset`-adjusted) under ring-tail overflow → provenance meaningless across elision (ADR-002, ARCH OQ-3) | Med | Med | High |
| R-13 | Two attributed-session scans (snapshot then purge, ADR-001) diverge — a session snapshotted but not purged (leak) or purged but not snapshotted (loss); held + registered double-counted | Med | Med | High |
| R-14 | `topic_source` soft-preference implemented as a filter (sort that drops) → legitimately-attributed sessions dropped from reconstruction (ADR-006, SR-06) | Low | Low | Low |
| R-15 | Per-cycle aggregate cap truncation non-deterministic or order-dependent → flaky tests, unpredictable candidate set (ADR-005 §4, FR-4) | Med | Low | Med |
| R-16 | Cap-hit eviction or poison-recovery (treat-as-empty, #4764) silently drops a buffer with no loss surfaced → silent loss reintroduced (ADR-008 §1, ADR-001 phase 2) | Med | Med | High |
| R-17 | Held buffer keeps merging deltas after drain but delta routing adds a lookup on the hot path that regresses microsecond lock discipline (ADR-008 §3, NFR-1) | Med | Low | Med |
| R-18 | `RetainDays(_)` arm reachable or accidentally distills/purges in some config → enterprise seam violated (ADR-005 §1, AC-10) | Low | Low | Low |
| R-19 | Manual metadata-only `Debug` on `TranscriptSnapshot` regresses to `derive(Debug)` (or a new content-bearing Debug elsewhere) → bytes leak via logs (ADR-002) | Med | Low | Med |
| R-20 | AC-03 fixture authored from the same regex set it validates → ≥0.90 recall self-fulfilling and meaningless (ADR-003, AC-03, OQ-6) | Med | Med | High |

## Risk-to-Scenario Mapping

### R-01: Held buffer re-adopted under the wrong `feature_cycle`
**Severity**: High **Likelihood**: Med
**Impact**: Candidates from session A's narrative attach to cycle B's review. Mis-attributed
decisions/lessons enter the knowledge base under the wrong feature — the #981 silent-mis-scope failure
mode, now in a held-buffer guise. Worse than data loss: actively poisons the KB.
**Test Scenarios**:
1. Hold a buffer for session S under `feature_cycle=X`; re-register S with `feature_cycle=X` → assert
   `readopt` rebinds and the snapshot carries S's bytes (happy re-adopt).
2. Hold S under `X`; re-register S with `feature_cycle=Y` (mismatch) → assert **fail-loud**: held buffer
   dropped (treated as fresh), a metadata-only diagnostic emitted, NO content re-adopted under Y.
3. Re-register S with `feature_cycle=None`/NULL → assert no silent re-adoption (cite #981).
4. Concurrent: re-register arrives during a cycle review scanning the hold → assert no rebind to an
   in-flight reviewed cycle produces a torn/double snapshot.
**Coverage Requirement**: re-adoption key-match path and the mismatch fail-loud path both have explicit
tests; mismatch never silently re-adopts; diagnostic is metadata-only (R-04). This is part of AC-11(b).

### R-02: Held-buffer memory unbounded
**Severity**: High **Likelihood**: Med
**Impact**: A never-reviewed/never-swept session (drift, crash, mis-attribution) leaks one buffer
(up to 4 MiB) each; held-count climbs to OOM. The two reclamation mechanisms (count cap, TTL) must be
independent — if either is inert, the other must still bound memory.
**Test Scenarios**:
1. Hold `transcript_hold_max_sessions + 1` sessions → assert cap-hit eviction fires, oldest-
   `last_activity_at`-first, held-count never exceeds cap.
2. Hold N sessions, advance clock past `transcript_hold_ttl_secs`, run `sweep_stale_sessions` → assert
   TTL sweep reclaims them **without any cycle review firing** (reclamation independent of review).
3. Disable cycle review entirely in a test; assert memory still bounded by cap × buffer_cap via TTL +
   cap eviction alone.
4. Memory-bound assertion: held bytes ≤ `buffer_cap × max_sessions` under adversarial hold churn.
**Coverage Requirement**: both cap eviction and TTL sweep independently tested to bound memory with no
review; this is AC-11(c)/(d). NFR-4 satisfied.

### R-03: Audit fires not-exactly-once per held session
**Severity**: High **Likelihood**: Med
**Impact**: Over-count corrupts retention/GC accounting; under-count means a purge is invisible (silent
loss, violates "loss never silent"). The audit moves off per-close onto review/sweep/evict (ADR-009) —
the cadence change is exactly where double/zero-emit bugs live.
**Test Scenarios**:
1. drain→hold→re-adopt→cycle review → assert `transcript_session_purged` fires **once** (at review),
   not at the per-turn drains.
2. drain→hold→TTL sweep (no re-adopt) → assert one audit at sweep, `trigger=stale_sweep`.
3. drain→hold→cap eviction → assert one audit at eviction (eviction never silent, ADR-008 §1).
4. drain→hold→re-adopt→drain→hold→review → assert still exactly one audit at the terminal purge, not one
   per hold cycle.
5. Audit `detail` content-free: `bytes=<n> trigger=<…>`, no transcript bytes (R-04).
**Coverage Requirement**: AC-11(e) — exactly-once across all three terminal paths (review/sweep/evict)
and across multiple hold/re-adopt rounds. Plus the ADR-009 named no-consumer survey (see Coverage Gaps).

### R-04: Candidate/buffer content leaks to persisted store, log, or audit
**Severity**: High **Likelihood**: Med
**Impact**: Direct breach of the #4721 secrets guarantee. crt-033 memoizes `RetrospectiveReport`
synchronously to `cycle_review_index` (#3793); any candidate field folded onto that struct persists raw
transcript excerpts to SQL. Logs and audit `detail` are secondary leak surfaces.
**Test Scenarios**:
1. Structural: assert `RetrospectiveReport` has **no** candidate field (compile-level — the leak is
   structurally impossible per ADR-004).
2. Re-review of a stored `cycle_review_index` record → assert returned report carries **no** candidates
   (cache-hit path, #3800).
3. Content-leak grep/log gate (extend vnc-025 AC-12): run a full cycle review with candidates present;
   assert no candidate/buffer bytes appear in any SQL write, file write, or log line in the new paths.
4. Audit-detail assertion: `transcript_session_purged.detail` is content-free.
5. `TranscriptSnapshot`/`HeldBuffer` Debug output: assert metadata-only, never bytes (R-19).
**Coverage Requirement**: AC-06 — structural absence + re-review test + extended content-leak gate over
ALL new code paths (Wave A and Wave B). Merge gate.

### R-05: AC-11 simulation not faithful to per-turn-drain reality
**Severity**: High **Likelihood**: Med
**Impact**: AC-11 is the **only** pre-merge proof the primary (non-fallback) path works before the
dogfooding switchover. A weak simulation (single drain, or no deltas between drains, or re-adopt without
a real `SessionClose`-mapped drain) lets a re-adoption/merge gap ship and surface only in production,
where it silently degrades every multi-turn review to the 0.81 fallback.
**Test Scenarios**:
1. Named test `continuity_simulated_lifecycle` executes the real sequence: register → deltas → drain
   (Stop→SessionClose) → deltas → drain → deltas → drain → re-register → cycle review (**≥3 drain
   cycles, deltas applied between each drain**).
2. Assert the review snapshot contains content streamed across **all** turns, not just the last
   (proves merge-while-held, not just last-turn survival).
3. Assert deltas applied to a held (drained, not re-registered) buffer are merged (held buffers keep
   accepting deltas).
4. Negative: a single-turn-only test must NOT be accepted as AC-11 evidence.
**Coverage Requirement**: AC-11 verification is a **hard merge gate** — the test must be the faithful
≥3-drain-cycle simulation with inter-drain deltas, asserting cross-turn content presence, loud
re-adopt (R-01), bounded held-count (R-02), TTL reclaim (R-02), and exactly-once audit (R-03). A
happy single-turn path does not satisfy it. See Merge Gates.

### R-06: A second/third buffer content reader sneaks in
**Severity**: High **Likelihood**: Low
**Impact**: The single-reader invariant (Constraint 4, vnc-030 ADR-007 §4) is load-bearing for #700. A
parallel `contiguous_tail`-style reader fragments the seam, breaks #700's reuse contract, and forces an
expensive retrofit. PreCompact (`listener.rs:1834-1838`) is the existing reader; the seam is the second
and last.
**Test Scenarios**:
1. AC-V-SEAM structural check: only two production callers of buffer content extraction exist
   (PreCompact `contiguous_tail`, and `snapshot()` via the seam); a grep/source-assertion test fails if
   a third content-reading call site is added.
2. Reuse proof: a #700-style marker-recovery caller parses `TranscriptSnapshot.bytes` with its own
   patterns and **without** calling `contiguous_tail` or any buffer accessor.
3. Assert `snapshot()` returns owned bytes + all four metadata fields (`base_offset`, `high_water`,
   `elided_bytes`, `holes`) so #700 needs no re-read.
**Coverage Requirement**: AC-V-SEAM — single-reader source assertion + #700-shaped reuse test over the
returned snapshot. SR-04 closed.

### R-07: Distillation not wired at all four success returns
**Severity**: High **Likelihood**: Med
**Impact**: The handler has four `result.is_ok()` purge sites (`tools.rs:2110/2236/2925/3027`). Wiring
distillation at only the tail return silently skips purged-signals, cached-MetricVector, and
memoization-hit paths — invisible in happy-path tests (#4750, SR-05).
**Test Scenarios**:
1. Per-path test exercising each of the four returns, asserting (distill → purge) order at each.
2. **Exhaustiveness regression test**: fails if a fifth success return is added without wiring the
   shared helper (modeled on vnc-025's purge exhaustiveness test).
3. Memoization-hit path (return #2925, #3800): assert candidates are distilled fresh from call-time
   buffer content and may differ from the cached report (AC-05, OQ-4).
4. Error-path test: transcripts retained, no candidates produced.
**Coverage Requirement**: AC-05 — all four returns tested + exhaustiveness guard against a fifth. Merge
gate (one shared helper, no per-site copy).

### R-08: Parse/match under lock; or torn read under concurrent deltas
**Severity**: High **Likelihood**: Low
**Impact**: Parsing under the registry or buffer lock regresses microsecond lock holds (NFR-1) and risks
deadlock against delta-merge writers. A torn read during snapshot yields corrupt candidate bytes.
**Test Scenarios**:
1. AC-01(a) structural/source assertion: no JSONL parse or marker-match call occurs inside a lock-guard
   scope; the byte copy is the only content work under the buffer lock (per pattern #3753 — use the
   snapshot, never re-acquire).
2. AC-01(b) concurrency test: stream transcript deltas concurrently during a cycle review → assert no
   deadlock, no torn read, consistent snapshot (run under loom or a stress loop).
3. Assert two-phase discipline: Arc-clone under registry lock (phase 1), byte copy under per-buffer lock
   (phase 2), all parsing after both released.
**Coverage Requirement**: AC-01 — source assertion + concurrency/stress test. NFR-1 lock-hold latency.

### R-09: Reconstruction-fallback trigger mis-calibrated
**Severity**: Med **Likelihood**: Med
**Impact**: Threshold assuming a lossless buffer over- or under-fires (#3359 class). Over-fire discards
a good buffer for the 0.81 reconstruction floor; under-fire ships a clipped tail as if full, losing
early DEC items silently (ass-070 Q5: elision clips the head, the highest-value family).
**Test Scenarios**:
1. Empty snapshot (no user/assistant blocks after filtering) → fallback fires whole-session.
2. `elided_bytes > 0` (ring-tail clipping at/above threshold) → fallback fires; assert against ADR-002
   tail-window-equivalence semantics, not assumed losslessness.
3. `holes` covering more than the configured fraction → fallback fires; below fraction → primary path.
4. **Boundary tests at the 4 MiB cap edge and under ring-tail overflow** (cite #4764) — the exact
   calibration points SR-08 flags.
5. Whole-session either/or: assert no byte-level primary/reconstructed mix within one session (OQ-2).
**Coverage Requirement**: AC-07 — threshold tested at cap boundary and under overflow; trigger is
whole-session; provenance label matches the path taken.

### R-10: JSONL parser panics on adversarial input
**Severity**: High **Likelihood**: Med
**Impact**: Buffer content is untrusted client-disk JSONL. A panic in the parser crashes the
cycle-review handler — denial of service on every review, and the buffer is the attack surface
(Constraint 7, SR-09). The blast radius is the whole `context_cycle_review` MCP call.
**Test Scenarios**:
1. AC-V-FUZZ malformed-line corpus: truncated JSON, non-UTF-8 bytes, oversized single line, unknown
   record type, embedded NUL → assert **skip-with-count**, no `Err`, no panic.
2. Truncated final line (ring-tail/hole boundary, common in real snapshots) → assert tolerated.
3. Adversarial: deeply nested / billion-laughs-style JSON, gigantic field → assert bounded handling, no
   resource exhaustion.
4. Handler-level: feed a fully-corrupt snapshot through `distill_before_purge` → assert the handler
   returns a normal review response (candidates absent), never panics.
**Coverage Requirement**: AC-V-FUZZ — fuzz/malformed corpus asserting skip-with-count and no panic at
both module and handler level. Security merge gate (untrusted input).

### R-11: Wave B contaminates Wave A
**Severity**: Med **Likelihood**: Med
**Impact**: The rollback boundary (ADR-009) requires Wave A to be independently correct (degrades to
fallback with empty buffers). If Wave A code references `transcript_hold.rs` or assumes non-empty
buffers, reverting Wave B breaks Wave A — the safe rollback target is lost.
**Test Scenarios**:
1. Build/dependency assertion: Wave A modules (`distill/`, seam, helper, response types) have **zero**
   compile-time reference to `transcript_hold.rs`.
2. Wave-A-only run: with no held-buffer machinery and every buffer empty at call time, assert clean
   degrade to the reconstruction fallback (AC-07) — a real tested mode, not degenerate.
3. Simulated Wave B revert: assert Wave A still compiles, tests pass, ships degraded.
**Coverage Requirement**: Wave A test suite passes with Wave B absent; dependency-direction assertion in
CI/review. Rollback-boundary integrity.

### R-12: `byte_offset` array-relative not logical under overflow
**Severity**: Med **Likelihood**: Med
**Impact**: Under ring-tail overflow `base_offset > 0`; if `byte_offset` is reported array-relative,
provenance ("where in the session stream") is wrong, and #700 marker recovery mislocates anchors.
**Test Scenarios**:
1. Overflowed buffer (`base_offset > 0`): assert each candidate `byte_offset == base_offset +
   in_snapshot_offset` (logical stream position).
2. Non-overflowed buffer (`base_offset == 0`): assert `byte_offset` equals the in-snapshot offset.
3. Consumer-contract assertion: the candidate ordering key `(ts, session_id, byte_offset)` is stable
   and meaningful across an elision event.
**Coverage Requirement**: candidate `byte_offset` is logical (base_offset-adjusted) and tested across an
elision boundary. (ARCH OQ-3 — spec must pin this.)

### R-13: Snapshot scan and purge scan diverge
**Severity**: Med **Likelihood**: Med
**Impact**: ADR-001 introduces two attributed-session scans (snapshot via `take_…`, then purge via
`clear_…`/`purge_held_for_feature`). A session matched by one scan but not the other leaks (snapshotted,
not purged) or loses data (purged, not snapshotted). Under Option B the seam must scan **both**
registered and held buffers (ADR-008 §4).
**Test Scenarios**:
1. Session held + a session registered, same `feature_cycle` → assert both snapshotted AND both purged.
2. Assert no double-snapshot of a buffer that is both registered and held (Arc identity).
3. A session registered between the snapshot scan and the purge scan → assert no leak (it is either in
   both or neither for this review).
4. Post-review: assert no held buffer for the reviewed cycle survives (purge_held_for_feature fired).
**Coverage Requirement**: snapshot-set and purge-set congruence over registered ∪ held for the reviewed
cycle; no leak, no double-count.

### R-14: `topic_source` becomes a hard filter
**Severity**: Low **Likelihood**: Low
**Impact**: Hardening the soft preference drops legitimately-attributed sessions from reconstruction
(SR-06, out of scope).
**Test Scenarios**:
1. Observations with mixed `topic_source` → assert reconstruction **reorders** (declared/registry-fill
   first) but drops **no** observation and excludes **no** feature-matched session.
2. All-`vote` observations → assert they still reconstruct (not filtered out).
**Coverage Requirement**: AC-07(iv) — topic_source only reorders, never drops; stable-sort, not filter.

### R-15: Per-cycle aggregate-cap truncation non-deterministic
**Severity**: Med **Likelihood**: Low
**Impact**: Flaky tests and an unpredictable candidate set if truncation order is not pinned.
**Test Scenarios**:
1. Candidates exceeding `transcript_candidate_cycle_cap_bytes` across sessions → assert deterministic
   truncation (chronological keep-earliest or family-priority — whichever the spec pins) is repeatable.
2. Per-session cap and per-cycle cap exercised independently (two knobs, FR-4).
**Coverage Requirement**: AC-02 — both caps independently tested; aggregate-cap truncation deterministic
and order-defined.

### R-16: Cap eviction / poison recovery silently drops a buffer
**Severity**: Med **Likelihood**: Med
**Impact**: Cap-hit eviction (ADR-008 §1) and treat-as-empty poison recovery (#4764, ADR-001 phase 2)
both discard buffer content; if neither surfaces loss, "loss never silent" is violated.
**Test Scenarios**:
1. Cap-hit eviction → assert the evicted session emits the purge audit (eviction never silent).
2. Poisoned buffer lock during snapshot → assert treat-as-empty recovery, `clear_poison`, and the
   session surfaces as empty/lossy in `SessionLossInfo` (not silently absent).
**Coverage Requirement**: eviction audit asserted (R-03 overlap); poison recovery surfaces loss in the
section (AC-08).

### R-17: Held-buffer delta routing regresses hot-path lock discipline
**Severity**: Med **Likelihood**: Low
**Impact**: Routing deltas to a held buffer adds a lookup on the drain→re-register window; if done under
the wrong lock or with a scan, it regresses NFR-1 microsecond holds on the delta-apply hot path.
**Test Scenarios**:
1. Assert held-buffer lookup on delta apply is O(1) keyed (no linear scan) and merges under the buffer
   lock only (vnc-025 ADR-001 discipline).
2. Latency/structural assertion that `apply_transcript_delta` lock holds are unchanged in class with the
   hold active.
**Coverage Requirement**: NFR-1 — delta-apply lock holds unchanged in class with Option B active.

### R-18: `RetainDays` arm distills or purges
**Severity**: Low **Likelihood**: Low
**Impact**: Enterprise-seam violation (AC-10); in OSS `RetainDays` is rejected at `validate()` but the
match must remain exhaustive and the arm must neither distill nor purge.
**Test Scenarios**:
1. Compile-level: the `TranscriptRetention` match has no wildcard arm (exhaustive).
2. Assert `RetainDays` config is rejected at `validate()` (OSS unreachable).
3. Construct `RetainDays` in a test (bypassing validate) → assert the helper returns `None` (no distill,
   no purge).
**Coverage Requirement**: AC-10 — exhaustive match + validate-rejection + neither-distill-nor-purge.

### R-19: `TranscriptSnapshot` Debug regresses to content-bearing
**Severity**: Med **Likelihood**: Low
**Impact**: `derive(Debug)` or a new content-printing Debug leaks raw bytes via logs/panics (SR-02/SR-07).
**Test Scenarios**:
1. Assert `TranscriptSnapshot`/`HeldBuffer` Debug output is metadata-only (`{len, base_offset,
   high_water, holes:n, elided_bytes}`), never bytes.
2. Grep gate: no `derive(Debug)` on content-bearing snapshot types (extend AC-12 leak gate).
**Coverage Requirement**: AC-06 leak gate covers Debug impls; manual metadata-only Debug asserted.

### R-20: AC-03 fixture self-fulfilling
**Severity**: Med **Likelihood**: Med
**Impact**: If the labeled corpus is authored from the regex set it validates, ≥0.90 recall is
circular and proves nothing about real-session recall.
**Test Scenarios**:
1. Committed fixture carries a provenance header asserting independence mode (anchors-before-port OR
   different-author) and authoring order/author.
2. Review check: independence header present and asserts one of the two modes.
3. Recall test: ≥0.90 block-level recall against the independent labels; selected volume ≤10% of raw
   bytes.
**Coverage Requirement**: AC-03 — committed independence-provenance header is a review gate; recall and
volume thresholds tested. (OQ-6 / ARCH OQ-5.)

## Integration Risks

- **Seam ↔ held store ↔ purge (R-01, R-03, R-13)**: the seam (ADR-001) must scan registered ∪ held;
  purge must clear the same set; audit must fire once per terminal purge. The three scans/operations
  share the `feature_cycle` key — divergence is where leaks and mis-scopes live. This trio is the
  highest-density integration risk.
- **Four-return helper ↔ memoization (R-04, R-07)**: the helper attaches candidates at assembly level
  AFTER `store_cycle_review()` (synchronous persist, #3793). The memoization-hit return (#2925)
  deserializes the stored report (#3800) then distills fresh call-time candidates — the cache-hit
  candidate-vs-report divergence must be tested, not assumed.
- **Snapshot ↔ delta merge (R-08, R-17)**: deltas stream concurrently with snapshot and with the held
  buffer's continued merge. Lock discipline (registry lock → Arc clone; buffer lock → byte copy/merge;
  all parsing off-lock, pattern #3753) is the boundary; loom/stress coverage required.
- **Snapshot metadata ↔ fallback trigger ↔ loss visibility (R-09, R-12, R-16)**: `elided_bytes`/`holes`/
  `base_offset` from one `snapshot()` feed the fallback predicate (ADR-006) AND the loss section
  (ADR-007). The same predicate must drive both (ADR-007 warns against re-computation).
- **Wave A ↔ Wave B boundary (R-11)**: one PR, two waves; Wave A must not depend on `transcript_hold.rs`.
  Dependency-direction is an integration invariant policed in CI/review.

## Edge Cases

- Empty buffer / no user-assistant blocks after filtering → fallback (R-09).
- Buffer at exactly the 4 MiB cap; one byte over (ring-tail just engaged, `base_offset` just advanced) (R-09, R-12).
- Truncated final JSONL line at a snapshot boundary (R-10).
- Session both registered and held for the same cycle (Arc identity, R-13).
- Held-count at exactly the cap; cap+1 (eviction, R-02).
- TTL boundary: `last_activity_at` exactly at TTL; just under/over (R-02).
- Re-register with same cycle / different cycle / NULL cycle (R-01).
- Multiple hold→re-adopt rounds before a terminal purge (audit once, R-03).
- Per-cycle aggregate cap hit by a single oversized session vs many small (R-15).
- All four success returns reached in distinct review invocations (R-07).
- Poisoned buffer lock during snapshot (R-16).
- Zero attributed sessions → section absent, not empty (AC-04).

## Security Risks

**Untrusted input (R-10, primary attack surface)**: buffer content is Claude Code JSONL copied from the
client's disk — fully attacker-controllable if the client host is compromised. It enters the pure
selection module (`distill/jsonl.rs`). Damage from malformed input: handler panic (DoS on every cycle
review), resource exhaustion (oversized/nested lines), or non-UTF-8 corruption. Mitigation: skip-with-
count, never error/panic (FR-14, AC-V-FUZZ); operate on `&[u8]`; tolerate truncated final line. Blast
radius is contained to one `context_cycle_review` call's candidate section — but a panic there is a hard
failure of the review. **Merge gate.**

**Content-leak / secrets (R-04, R-19)**: transcript bytes may contain secrets (the in-memory + purge
posture #4721 IS the guarantee — there is no redactor). Persistence to `cycle_review_index` (#3793),
logs, or audit `detail`/`Debug` are the leak surfaces. Blast radius if breached: raw conversation
(possibly secrets) written to durable SQL/logs, surviving the purge that is the entire security model.
Mitigation: structural absence from the memoized type (ADR-004), metadata-only Debug (ADR-002),
content-free audit (ADR-009), extended grep/log leak gate (AC-06). **Merge gate.**

**Mis-attribution as integrity attack (R-01)**: a held buffer re-adopted under the wrong cycle injects
one session's narrative into another feature's KB entries — an integrity (not confidentiality) breach.
Mitigation: fail-loud re-adoption key match (ADR-008 §3). **Merge gate (part of AC-11).**

No path traversal / deserialization-of-untrusted-type / injection vectors: the JSONL parser produces
only `TranscriptCandidate` value types (no code/path/SQL constructed from content); candidates flow to
the agent as data, never executed server-side.

## Failure Modes

| Failure | Expected behavior |
|---------|-------------------|
| Malformed/adversarial JSONL line | Skip-with-count; parser returns the parseable candidates; never `Err`/panic (R-10) |
| Poisoned buffer lock | Treat-as-empty recovery + `clear_poison` (#4764); session surfaces as lossy in `SessionLossInfo`, not silently dropped (R-16) |
| Empty / hole-ridden buffer | Reconstruction fallback from observations, labeled `Reconstructed`; loss surfaced (R-09) |
| Held-count cap exceeded | Oldest-`last_activity_at`-first eviction, purge audit emitted (never silent) (R-02, R-16) |
| Session never reviewed/re-registered | TTL stale sweep reclaims, audit at sweep (R-02, R-03) |
| Re-adopt cycle mismatch | Fail loud: drop held buffer, metadata-only diagnostic, treat as fresh (R-01) |
| `RetainDays` config (OSS) | Rejected at `validate()`; helper neither distills nor purges (R-18) |
| Wave B reverted | Wave A ships degraded to reconstruction fallback; still correct (R-11) |
| Zero attributed sessions | `transcript_candidates` absent (not null/empty) (AC-04) |
| Memoization cache hit | Fresh call-time candidates attached; cached report unchanged; divergence documented (R-07, OQ-4) |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (held-buffer memory bound) | R-02, R-16 | ADR-008: held-count cap + independent TTL sweep, both bound memory alone; cap-eviction emits audit. Tested independent of cycle review (R-02). |
| SR-02 (re-adoption correctness) | R-01 | ADR-008 §3: re-adopt only on `feature_cycle` match; fail loud on mismatch (cite #981). AC-11(b). |
| SR-03 (audit-shape change) | R-03 | ADR-009: shape unchanged, fires exactly-once at review/sweep/evict; named no-consumer survey is a gate (see Coverage Gaps). AC-11(e). |
| SR-04 (#700 seam coupling) | R-06, R-12 | ADR-002: seam returns owned bytes + 4 metadata fields; single-reader source assertion + #700-reuse test (AC-V-SEAM). |
| SR-05 (four-return drift) | R-07 | ADR-005: one shared helper at all four returns + exhaustiveness regression test (AC-05). |
| SR-06 (`topic_source` scope creep) | R-14 | ADR-006: soft ordering preference only, never a filter; stable-sort tested to drop nothing (AC-07iv). |
| SR-07 (memoization secrets breach) | R-04, R-19 | ADR-004: candidates outside the memoized struct (structural impossibility) + extended content-leak gate (AC-06). |
| SR-08 (hole/elision threshold mis-calibration) | R-09, R-12 | ADR-006: trigger defined against tail-window-equivalence (#4764); boundary tests at cap edge + overflow (AC-07). |
| SR-09 (untrusted input) | R-10 | ADR-003: pure parser, skip-with-count, never panic; fuzz/malformed corpus (AC-V-FUZZ). |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 6 (R-01, R-02, R-03, R-04, R-05, R-07, R-10) | ~30 scenarios — all merge gates |
| High | 7 (R-06, R-08, R-09, R-11, R-12, R-13, R-16, R-20) | ~26 scenarios |
| Medium | 4 (R-15, R-17, R-19) | ~8 scenarios |
| Low | 3 (R-14, R-18) | ~5 scenarios |

(Critical count lists 7 IDs; R-16 and R-20 are High — table groups by the band each scenario set lands in.)

### Merge Gates (must pass before PR merge)

1. **AC-11 `continuity_simulated_lifecycle`** (R-05, R-01, R-02, R-03) — the faithful ≥3-drain-cycle
   simulation with inter-drain deltas is the **only pre-merge proof of the primary path**. A single-turn
   happy path does NOT satisfy it. Coverage requirement: cross-turn content presence, loud re-adopt,
   bounded held-count + eviction, TTL reclaim, exactly-once audit — all asserted in one named test.
2. **Content-leak gate** (R-04, R-19) — extended grep/log/SQL gate over all new paths + structural
   absence of candidates from `RetrospectiveReport` + re-review-of-stored-record test (AC-06).
3. **Four-return exhaustiveness** (R-07) — all four returns wired + a test that fails on a fifth (AC-05).
4. **AC-V-FUZZ no-panic** (R-10) — malformed/adversarial corpus skip-with-count, handler never panics.
5. **AC-V-SEAM single-reader** (R-06) — source assertion that no third buffer content reader exists +
   #700-reuse test over the snapshot.
6. **AC-01 snapshot-and-release** (R-08) — no-parse-under-lock source assertion + concurrency/stress test.

## Coverage Gaps Requiring Architecture or Spec Attention

1. **ADR-009 no-consumer audit survey is named but not yet recorded (SR-03 / R-03).** The architecture
   makes "no downstream consumer keys on per-close `transcript_session_purged` cadence" an explicit gate
   condition (survey `gc_audit_log`/crt-036, retention/analytics readers, per-close-emission tests). This
   survey is a **prerequisite** to moving the audit points and is not yet performed. The spec must record
   its result as a gate before Wave B lands. **Risk-strategy flag: the audit move must not merge until
   this survey is clean.**
2. **Held-buffer cap/TTL defaults unset (R-02 / ARCH OQ-1).** `transcript_hold_max_sessions` and
   `transcript_hold_ttl_secs` defaults are a spec/tuning decision (architecture suggests ~64 sessions /
   24 h). The cap-hit eviction test (R-02) and TTL sweep test cannot be pinned to concrete boundary values
   until the spec fixes the numbers.
3. **Per-cycle aggregate-cap default + truncation order undefined (R-15 / ARCH OQ-2).** FR-4 mandates the
   cap and deterministic truncation but the default (~256 KB suggested) and the truncation rule
   (chronological keep-earliest vs family-priority) are unpinned. R-15's determinism test needs the rule
   chosen first.
4. **`byte_offset` logical-vs-array semantics not pinned in the spec (R-12 / ARCH OQ-3).** ADR-002 makes
   `byte_offset` logical (`base_offset`-adjusted); the spec lists this as an open question. Pin it so the
   consumer (#700 and the agent) treats offsets as stream positions across elision.
5. **AC-11 simulation faithfulness is an authoring obligation, not a structural guarantee (R-05).** The
   only enforcement is reviewer discipline that the named test executes the real ≥3-drain lifecycle with
   inter-drain deltas. Recommend the spec/test plan make the drain count, the inter-drain delta
   application, and the cross-turn content assertion explicit, non-negotiable test requirements — because
   this is the sole pre-merge proof of the primary path.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for held-buffer/state-machine failures and risk patterns — surfaced
  #4750 (four success returns), #3753 (use pre-cloned snapshot, never re-acquire a lock), #4764
  (checked-offset / treat-as-empty poison recovery), #3793 (cycle-review synchronous SQL persist — the
  secrets trap), #3800 (memoization hit deserializes the stored report), #3479 (two-site coupled-test
  pattern). All folded into R-04/R-07/R-08/R-16 and the integration-risk analysis.
- Stored: nothing novel to store — the recurring patterns (four-return gating, snapshot-not-relock,
  poison-recovery, memoization-persist secrets trap) are already captured as #4750/#3753/#4764/#3793;
  crt-052-specific risks live in this document, not Unimatrix (per stewardship rules). No cross-feature
  pattern visible across 2+ features that is not already stored.
