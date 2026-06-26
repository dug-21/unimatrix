# Test Plan: K5 — `harness/transport_health.py`

Covers **R-02 (Critical defense-in-depth; #839 CLOSED via PR #842)**. The per-leg connect/idle
bounded-deadline guards so a half-open-socket hang surfaces as INFRA-ERROR, never a parity
verdict and never an unbounded suite hang. #839 itself is closed, but the half-open-hang CLASS
remains a real hazard — defense-in-depth. The slow-but-healthy boundary test is load-bearing:
an over-tight deadline manufactures FALSE INFRA (false-RED).

Surface under test:
- `InfraError(Exception)` — distinct exit-class exception
- `preflight_leg(leg, *, connect_deadline_s, idle_deadline_s) -> None`  (raises `InfraError`)
- `load_https_bundle(out_path, expected_run_token) -> dict[str, Any]`  (may live in K5 or
  `parity_workload.py` — its ingestion tests are in `parity_workload.md`/`parity_bundle_contract`)

Tier: **A (off-Docker unit)** — uses an in-process stub socket / loopback listener, no Docker,
no daemon. File: `suites/test_transport_health.py`.

## Unit Test Expectations

### Connect deadline — unreachable leg (R-02 scenario 1)
- `test_preflight_leg_unreachable_socket_raises_infra_within_connect_deadline`: a socket path /
  endpoint that never accepts → `preflight_leg` raises `InfraError` within the bounded
  `connect_deadline_s`; assert it NEVER blocks unbounded (test asserts elapsed ≤ deadline + slack).
- `test_preflight_leg_connection_refused_raises_infra`: an actively-refused connection → `InfraError`
  (not a silent pass, not a parity verdict).

### Idle deadline — half-open hang (R-02 scenario 2 — the #839 CLASS, load-bearing)
- `test_preflight_leg_half_open_accepts_never_replies_raises_infra`: a loopback listener that
  ACCEPTS the connection then never replies (half-open simulation) → the idle deadline expires →
  `InfraError`; assert classified as INFRA-ERROR material, NOT a timeout-as-PARITY-FAIL and NOT a
  hang. This is the precise #839-class scenario the suite must defend even though #839 is closed.

### Slow-but-healthy boundary (R-02 scenario 4 — guards against false INFRA)
- `test_preflight_leg_slow_but_healthy_under_idle_deadline_passes`: a leg that replies just UNDER
  the idle deadline → `preflight_leg` returns normally (no `InfraError`). **Load-bearing:** proves
  the deadline has explicit head-room over a healthy-leg latency and does not manufacture a false
  INFRA-ERROR (a misclassified slow-healthy leg would false-RED the whole matrix).
- `test_preflight_leg_idle_deadline_head_room_documented`: assert the configured idle/connect
  deadlines carry explicit head-room over the measured healthy-leg latency (a documented constant,
  not an arbitrary tight bound).

### InfraError is a distinct class (R-02 — never a parity verdict)
- `test_infra_error_is_distinct_exception_type`: assert `InfraError` is its own type (not an
  `AssertionError`/`ParityMismatch` subclass) so the orchestrator's classifier routes it to the
  INFRA-ERROR class with a distinct exit code, never into the parity-RED path.

### Both legs (R-02 coverage requirement — symmetry)
- `test_preflight_leg_applies_to_both_legs`: parametrized over the UDS leg and the HTTPS leg
  descriptor → each leg has a bounded connect + idle deadline (no leg is exempt).

## Integration expectations (referenced; live tier in test_https_uds_parity.md)
- Live: the HTTPS leg's existing `run_smoke_gate` exit-code truth table (0 pass / 3 skip→HARD-FAIL
  / 4 unacquirable / 1 broke) is PRESERVED; K5 adds the idle-deadline classification that NAMES a
  hung (not failed) socket as INFRA. The existing outer `HTTPS_SMOKE_TIMEOUT_S` is unchanged; K5
  adds the pre-drive reachability probe + idle-deadline classification on top.

## Coverage Requirement (from R-02)
Every leg has a bounded connect + idle deadline; a hang is provably classified INFRA-ERROR
off-Docker (unreachable + half-open simulation); the roll-up never converts INFRA-ERROR into a
parity RED (see `parity_outcome.md`); the deadline is tuned with explicit head-room over a
healthy-leg latency, proven by the slow-but-healthy boundary test.
