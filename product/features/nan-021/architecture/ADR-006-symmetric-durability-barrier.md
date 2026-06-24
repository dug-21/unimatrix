## ADR-006 nan-021: Symmetric Observe-Durability Barrier Before Both context_cycle_review Calls

### Context
R-06 (Critical, High/High). The server `/observe` path writes observations via `tokio::spawn`
fire-and-forget to the WAL and ACKs `204` BEFORE the write is synced/visible to a later aggregation read
(#5265, #5191 `pool_config`). `context_cycle_review` reads aggregates from the durable streams; if it runs
immediately after `context_cycle(stop)`, the observes may not have landed yet → `total_tool_calls` /
`phases` short or empty.

This breaks AC-04 in two compounding ways:
1. **Non-empty precondition is inherently flaky.** ADR-003 requires `total_tool_calls > 0`,
   `session_count > 0`, `phases` populated on BOTH vectors. An un-landed observe stream makes that a race
   the gate loses intermittently — a timing artifact masquerading as a real empty-metrics failure.
2. **Parity mismatches become timing artifacts.** If the two legs read at different points in their
   respective async-flush windows, the `MetricVector`s diverge on COUNT fields (which ADR-003 compares
   exactly, and rightly so) — a false RED that proves nothing about transport parity. Worse, an
   **asymmetric** barrier (one leg waits, the other reads immediately) SELF-INDUCES divergence: the UDS
   observe path is also async, so a barrier on only the HTTPS leg compares a settled vector against an
   un-settled one.

Neither the original ARCHITECTURE.md sequence nor ADR-002/ADR-003 named a durability barrier. The
existing infra-001 store-delta gates already deadline-poll the per-slug store at DIR granularity (Gate 7,
~10s deadline) — that precedent is the barrier shape to reuse, not reinvent (#5265 takeaway 3: sample the
store DIR including `-wal`, never just `unimatrix.db`).

### Decision
**A bounded deadline-poll observe-durability barrier gates BOTH `context_cycle_review` calls, applied
IDENTICALLY (symmetrically) on both legs.**

1. **Barrier definition.** After `context_cycle(stop)` and BEFORE `context_cycle_review(feature)` on a
   leg, poll until the driven observations are durable/visible to the aggregation read: bounded deadline
   (cap ~10s), `sleep 1` between polls — NOT a flat sleep, NOT an immediate single read (#5265). The poll
   predicate asserts the EXPECTED observe count is present (the count the workload manifest declares — the
   number of tool calls that fire PostToolUse observes), read at DIR granularity (store dir incl. `-wal`,
   or the review's own count once non-zero and stable), never just `unimatrix.db`.

2. **Symmetry is load-bearing (R-06 scenario 2).** The SAME barrier — same predicate, same deadline, same
   poll cadence — runs on BOTH the HTTPS leg and the UDS leg. Both `context_cycle_review` calls execute
   only after THEIR leg's observes are provably durable. The barrier is a single shared helper (owned by
   the C4 driver, D-1) parameterized by leg, not two hand-written waits — an asymmetric barrier is itself
   a parity-divergence source and is forbidden by construction (single source of truth, mirrors ADR-001's
   one-workload rule).

3. **Timeout is a HARD failure, never an empty compare.** If the barrier deadline expires before the
   expected observe count is durable, the test FAILS LOUD ("observes not durable within deadline" + the
   observed-vs-expected count + captured child stderr) — it NEVER proceeds to `context_cycle_review`
   against a short/empty stream and NEVER compares an empty vector (that would re-create the R-06
   false-RED / vacuous-non-empty hazard).

4. **Non-empty is asserted AFTER the barrier.** ADR-003's non-empty precondition (`total_tool_calls > 0`,
   etc.) is checked only once the barrier has confirmed durability — so a believable `0` from a race can
   never satisfy parity, and a real empty-metrics defect (if it existed) still surfaces after the deadline
   as a genuine failure.

### Consequences
- **Easier:** the AC-04 non-empty precondition stops being a race — both vectors are read from settled,
  durable state, so a parity mismatch now genuinely means a transport-dependent difference (the thing the
  fixture exists to catch), not a flush-timing artifact. Symmetry removes the self-induced-divergence
  failure mode entirely. Reuses the shipped Gate-7 deadline-poll shape (#5265) — no new mechanism.
- **Harder:** the barrier adds up to ~10s per leg to the critical path (bounded; only spends time when the
  flush is slow). The poll predicate must know the EXPECTED observe count from the manifest (couples the
  barrier to the workload's declared tool-call/observe count — a real but intended dependency). DIR-
  granularity sampling (incl. `-wal`) must be honored or the predicate under-counts and the barrier
  releases early (#5265 takeaway 3).

Related: R-06; AC-04; D-5/D-6. Reuses #5265 (fire-and-forget WAL not synced before 204; store-delta
gaze-width / DIR sampling) and the shipped Gate-7 deadline-poll precedent. Pairs with ADR-003 (the
non-empty + field-for-field compare this barrier makes deterministic) and ADR-001 (the manifest that
declares the expected observe count and owns the shared barrier helper). Complemented by ADR-002's
idle-window minimization (R-05) — together they make the spawn→drive→durable→review chain race-free.
