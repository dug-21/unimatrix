# Test Plan: C5′ — `scripts/cloud-cycle-lib.sh` (`cloud_cycle_gates`, extended)

Covers the HTTPS-leg bundle-emit half of **R-09 (High)**, the HTTPS-leg WAL barrier of **R-04
(Critical)**, the exit-code/skip contract of **R-10/AC-08**, and the diff-confinement guard of
**R-16/AC-11**. `cloud_cycle_gates` is EXTENDED to write `{run_token, dimension_bundle:{...}}`
to `$HTTPS_VECTOR_OUT` instead of `{run_token, metric_vector}`, riding the EXISTING
`run_smoke_gate` discriminator. NO change to the exit-code truth table contract.

Surface under test (extended):
- `cloud_cycle_gates` (bash fn) — reads `MANIFEST_PATH`/`RUN_TOKEN`/`HTTPS_VECTOR_OUT`/`SANDBOX`;
  now writes the dimension-keyed bundle
- consumed verbatim: `run_smoke_gate` exit-code truth table (0 pass / 3 skip→HARD-FAIL / 4 unacq
  / 1 broke)

Tier: **C (live, `@pytest.mark.parity`)** — exercised through the Docker smoke in the matrix
orchestrator; the exit-code/skip behavior is asserted there + via the off-Docker `rollup` truth
table (`parity_outcome.md`). Shell-level behavior is proven by the live run + diff review.

## Test Expectations

### Bundle emit (R-09 emit half, AC-01/AC-08)
- `cloud_cycle_gates` writes `{run_token, dimension_bundle:{...}}` to `$HTTPS_VECTOR_OUT` with all
  six capture_keys populated (matching the cross-language schema in `parity_bundle_contract.md`).
  The Python ingest (`load_https_bundle`) round-trips it; a missing/null key → INFRA-ERROR
  (proven off-Docker in `parity_workload.md`, satisfied live here). R-09 scenario 4.
- The emitted `run_token` matches this run's correlation token (R-12 — the live half of the
  stale-token guard; the anchored run-marker is asserted present this run).

### HTTPS-leg WAL barrier (R-04 scenario 3 — symmetry)
- DB-reading captures on the HTTPS leg (behavioral observations, isolation landing, analytics
  cycle-events) are gated behind the SAME durability discipline as the UDS leg before the bundle
  is emitted — the barrier is symmetric (the same helper class on both legs; R-04 scenario 3).
  Asserted live in the matrix orchestrator.

### Exit-code truth table + skip HARD-fail preserved (R-10, AC-08)
- The existing `run_smoke_gate` verify-by-name + exit-code truth table is UNCHANGED: 0 pass /
  3 skip→HARD-FAIL / 4 unacquirable / 1 broke. The skip-when-Docker-absent path HARD-fails by the
  DISTINCT exit code (false-green-proof, AC-08) — asserted in the matrix orchestrator + the
  off-Docker `rollup` truth table (`parity_outcome.md`).

### Diff-confinement / fork-smell (R-16 scenarios 1–2, AC-11)
- `test_git_diff_confined_to_infra_001` (shell, Stage 3c): `git diff` is confined to
  `product/test/infra-001/`; NO `crates/**`, shipped `lib/`, or production-script change
  (SCOPE-FAIL guard). Review-flag any net-new transport/cert/spawn/framing code (bridge-in-path
  reuse verified) — pairs with `bridge-cycle-driver.md`.

## Coverage Requirement
`cloud_cycle_gates` emits the full six-key dimension bundle matching the cross-language schema
with this run's run_token, the HTTPS-leg DB reads are barrier-gated symmetrically with the UDS
leg (R-04), the exit-code truth table + skip-HARD-fail contract is preserved (AC-08), and the
diff is confined to test infra with no re-implemented transport code (R-16/AC-11).
