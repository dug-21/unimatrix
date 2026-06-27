# Test Plan — C5: Per-cell write + read-as-barrier positive control

> Pseudocode: `pseudocode/c5-read-as-barrier.md`. Risks: **R-05** (durability
> barrier soundness), **R-04** (WAL false-empty), R-06, R-08. ACs: **AC-03**,
> **AC-07**, **AC-10**, **AC-11**.

C5 is where the durability model lives: there is **no aggregate `store_size`
barrier** (unsound — satisfied by the first of a store's two writes). Each write is
issued strictly sequentially per store and immediately followed by its **own**
marker-keyed **bounded retry-until-present** read; that read **is** the barrier.
The test of C5 proves the loop is bounded, polls the WAL-complete copy, and that an
own-store timeout classifies **INFRA (never RED, never vacuous pass)**.

## What C5 must do (behavior under test)

- Writes strictly sequential per store; each followed by its own positive read.
- Positive read: `vol cat` the store (db **+ `-wal` + `-shm`**) → host `sqlite3`
  query for **this cell's** marker, retrying on a deadline-poll until ≥1 row.
  - Observe: `SELECT topic_signal FROM observations WHERE topic_signal='<marker>'`
  - MCP: `SELECT id FROM entries WHERE content LIKE '%<marker>%' OR topic='<marker>'`
- Not-yet-present before deadline → keep polling (INFRA/retry).
- Marker still absent **at** deadline → **INFRA** (own-store durability not
  established), **never RED** (AC-03/AC-07/AC-10).
- `store_size` is retained for C2 liveness/boot waits **only**, never the barrier.

## Verification tier 1 — off-Docker gate-logic test (stub-driven, primary teeth)

The C5 read primitive must be routed through a stub seam (mirroring
`SMOKE_SHELL_CAPTURES` / `gate7_store_size` in infra-001) so the retry/timeout
disposition is driven without Docker:

- `test_c5_no_aggregate_store_size_barrier` (R-05/AC-10) — grep/inspection: there
  is **no** "A grew AND B grew" `du` gate before any content read; `store_size`
  appears only in C2 liveness waits.
- `test_c5_positive_is_retry_until_present` (AC-10) — stub returns "absent" for N
  polls then "present"; assert the loop retries and then passes (not a fixed
  `sleep`, not a single read).
- `test_c5_deadline_bounded` (R-05 residual) — stub always returns "absent";
  assert the loop terminates at the bounded deadline (no unbounded hang) and
  classifies the result **INFRA**, not RED, not GREEN.
- `test_c5_own_timeout_is_infra_not_red` (AC-03/AC-07/AC-10) — own marker never
  appears → that direction is INFRA; assert the gate does **not** emit RED for the
  own-store timeout and does **not** vacuously pass.
- `test_c5_sequential_per_store` (R-05) — inspection: write→read ordering is
  per-cell sequential; A-obs's read completes before A-mcp is issued (no shared
  aggregate barrier across a store's two writes).

## Verification tier 2 — live run

- `test_c5_wal_sidecars_copied` (R-04/AC-11) — `vol cat` copies db + `-wal` +
  `-shm` for the store before each query; a missing main db = INFRA, an absent
  already-checkpointed `-wal` is acceptable only with a present durable main db.
- `test_c5_deadline_value_arm64_headroom` (R-05 residual) — the deadline mirrors
  the existing ~10s store-grow wait in `docker-http-posture-smoke.sh` (arm64-CI
  headroom); confirm it is a deadline-poll, not a fixed sleep.
- `test_c5_read_back_consistency` (R-04 sc.3) — a query against the
  copied-with-WAL snapshot agrees with a post-explicit-checkpoint snapshot (no
  pre-checkpoint blind spot), per store.

## Coverage requirement

The aggregate `store_size` barrier is gone; every positive control is a bounded
marker-keyed read-as-barrier over a WAL-complete copy with
own-store-timeout = **INFRA** (never RED, never vacuous). This pins the
`tokio::spawn` fire-and-forget + `synchronous=NORMAL` race so the positive does not
false-RED (R-05/#5321).
