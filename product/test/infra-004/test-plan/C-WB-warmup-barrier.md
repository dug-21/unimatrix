# Test Plan — C-WB: Warmup/Readiness Barrier

> File under test: `product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh`
> Test file: **EXTEND** `release-gate-isolation-logic-test.sh` (sources the real gate, drives
> via `SMOKE_WRITE_CMD`/`SMOKE_READ_MARKER_CMD` + `fixtures/stub-read-marker.sh`).
> Critical risk: **R-01** (ceremonial barrier). Also R-02, R-03, R-04. ACs: AC-01..AC-05.

## What the barrier is

One throwaway `write_then_barrier observe <slug> <store_dir> infra003-warmup-${RUN}` call on
`WARMUP_DEADLINE_SECS` (default 180, #767-derived), inserted after `assert_routes_live`, before
`run_isolation_matrix`. Sets `WTB`; PRESENT → proceed to matrix; INFRA → `infra_fail` (exit 2).
PRESENT requires `read_marker` count ≥ 1 (a durable own-store round-trip), exactly as
`write_then_barrier` already does (line 271-294) — the load-bearing property.

## Unit / stub-seam test expectations

### R-01 — barrier is load-bearing, not ceremonial (CRITICAL)
- `test_warmup_present_proceeds_to_matrix`: stub seam with the warmup marker in `STUB_PRESENT`
  → `warmup_barrier` sets `WTB=PRESENT` and **returns/proceeds** (rc 0, no `infra_fail`).
- `test_warmup_timeout_is_infra_not_pass` (AC-03): warmup marker **omitted** from `STUB_PRESENT`
  + `READ_DEADLINE_SECS`/`WARMUP_DEADLINE_SECS=0` → assert **exit 2 (INFRA)**, assert NOT 0 and
  NOT 1 (never converts not-ready to a pass, never RED).
- `test_warmup_present_requires_durable_read_roundtrip`: assert PRESENT is set only when
  `read_marker` returns ≥ 1 through the SAME `SMOKE_READ_MARKER_CMD` a real write uses — drive
  `STUB_INFRA=<warmup cell>` (read fails) → `infra_fail` INFRA, proving PRESENT is not a
  liveness shortcut. (Mirrors existing `test_c6_missing_db_is_infra`.)
- `test_warmup_result_is_consumed` (funnel, static): grep the barrier site — the `WTB` result
  binding is **consumed to gate proceed-to-matrix** (e.g. `infra_fail` on INFRA), not
  computed-and-discarded. Fail loud on an unused result binding.
- `test_warmup_uses_write_then_barrier_not_store_size` (static): the warmup cell calls
  `write_then_barrier` (→ `read_marker`), NOT a liveness-only `store_size`/`wait_for_http_active`
  poll (AC-02 — no new readiness mechanism).

### AC-05 — stub-seam compatibility preserved post-barrier
- `test_post_barrier_full_verdict_truth_table_still_drives`: after the barrier is added, the
  EXISTING `run_isolation_matrix` truth table (GREEN 0 / RED 1 / INFRA 2 / SKIP 3) still drives
  through the stub seam without Docker — re-run the existing isolation-logic cases unchanged
  (no regression to the seam).

### R-02 — warmup-marker collision → false RED / masked leak
- `test_warmup_marker_non_substring_asserted`: `infra003-warmup-${RUN}` is asserted pairwise
  non-substring of the four cell markers; force a colliding warmup marker → fail loud (INFRA),
  mirroring existing `test_c7_substring_markers_fail_infra`. The assertion lives **inside
  `warmup_barrier`**, which calls the idempotent `derive_markers()` first (so the four cell
  markers exist before the check); the test targets that internal
  `derive_markers()`→non-substring-assertion sequence directly (OQ-A RESOLVED, Gate 3a).
- `test_warmup_row_inert_to_negatives`: the negative-cell foreign-marker greps query specific
  cell markers; confirm none match the warmup marker (warmup row inert to the matrix).

### R-03 — bound provenance (static + operational)
- `test_assert_routes_live_precedes_barrier` (static, AC-01): `git diff` / grep shows the
  barrier between `assert_routes_live` (line 429) and `run_isolation_matrix` (line 430), so the
  barrier's delta over #767 is **model-load only** (store liveness pre-established). If only
  routes-non-404 is established (not first-durable-write), FLAG the bound as under-scoped.
- `test_warmup_bound_default_documented` (static, AC-01): `WARMUP_DEADLINE_SECS` default 180 is
  documented in the diff as a #767 `READY_TIMEOUT_SECS` derivation with headroom — not guessed.
- Cold headroom is **operational** (AC-11 / §5) — not a pre-merge unit test.

### R-04 — cold-HF variance
- Covered by `test_warmup_timeout_is_infra_not_pass` (timeout → INFRA-visible). Also assert the
  barrier **logs diagnostic last-state on timeout** (`log "...timed out...INFRA"`, line 289).

## Edge cases
- Warmup timeout must `return`/`infra_fail` immediately, never fall through into the matrix
  (RED>INFRA>GREEN integrity — a warmup INFRA must not later be masked).
- `RUN` nonce non-`[a-z0-9-]` → `infra_fail` (reuse existing `derive_markers` guard, line 348).

## Coverage requirement
The barrier's PRESENT outcome is proven to require a durable own-store `read_marker` round-trip
(not route-liveness) and to gate proceed-to-matrix; timeout → INFRA (exit 2, never RED/GREEN);
non-substring invariant enforced. Deterministic cold GREEN (AC-04) is proven by AC-11 (§5),
recorded operationally.
