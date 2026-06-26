# K5 — Transport-health preflight (`harness/transport_health.py`)

**New**, pure-Python, stdlib-only, off-Docker unit-testable. ADR-002 (#5313) defense-in-depth.

## Purpose

Per-leg connect/idle bounded-deadline guards. A half-open-socket hang (the #839 class — #839
itself CLOSED via commit 5b6badad / PR #842, retained as DEFENSE-IN-DEPTH, NOT a gating
dependency) or any transport unavailability raises `InfraError`, never a dimension verdict and
never an unbounded hang (SR-02 / R-02). Also hosts the generalized token-guarded bundle ingest
(`load_https_bundle`) — see note below on ownership.

## Type

```
class InfraError(Exception):
    """Distinct exit-class exception: half-open hang / unreachable socket / bounded-deadline
    expiry / missing-stale-empty capture. Caught by K4 classify_dimension / the orchestrator
    and converted to Outcome.INFRA_ERROR — NEVER read as PARITY_PASS or PARITY_FAIL."""
    def __init__(self, leg: str, reason: str, *, detail: str = ""):
        self.leg = leg; self.reason = reason; self.detail = detail
        super().__init__(f"[{leg}] transport-health INFRA: {reason}" + (f" — {detail}" if detail else ""))
```

## Constants

```
DEFAULT_CONNECT_DEADLINE_S = <bounded, e.g. 5.0>   # max wait to ESTABLISH a connection
DEFAULT_IDLE_DEADLINE_S    = <bounded, e.g. 10.0>  # max wait for a reply AFTER connect (half-open)
```

Both must be tuned with explicit HEAD-ROOM over a healthy-leg latency (R-02 boundary scenario 4)
so a slow-but-healthy leg is not misread as INFRA. The HTTPS smoke shell-out keeps its existing
outer ceiling `HTTPS_SMOKE_TIMEOUT_S` (NFR-5); K5 adds the PRE-DRIVE reachability + idle probe so
a HANG (not a failure) is NAMED as INFRA rather than read as RED or hanging the suite.

## `preflight_leg`

```
def preflight_leg(leg, *, connect_deadline_s=DEFAULT_CONNECT_DEADLINE_S,
                  idle_deadline_s=DEFAULT_IDLE_DEADLINE_S) -> None:
    """Probe a leg's reachability with bounded connect + idle deadlines BEFORE driving it.
    Raises InfraError on: unreachable socket, connect-deadline expiry, or a socket that
    accepts then never replies (half-open) within the idle deadline. Returns None on a healthy,
    responsive leg.

    `leg` carries what to probe:
      - UDS leg: the MCP UDS socket path + the hook IPC socket path (both must accept + reply).
      - HTTPS leg: a cheap pinned reachability probe to the container (reuse the shipped
        cert-pin/curl posture; NO net-new transport code — C-2 fork-smell guard). A connect that
        succeeds but never returns a response within idle_deadline_s is the #839 half-open class.
    """
```

### Algorithm

```
1. CONNECT phase (bounded by connect_deadline_s):
     attempt to open the transport (UDS: socket.connect with a timeout; HTTPS: a bounded pinned
     probe). If the connect does not complete within connect_deadline_s -> InfraError(leg,
     "connect deadline expired"). If connect fails outright (refused/absent) -> InfraError(leg,
     "transport unreachable").
2. IDLE / liveness phase (bounded by idle_deadline_s):
     send a cheap liveness request and wait for ANY reply, bounded by idle_deadline_s. If the
     socket accepted but no reply arrives within the deadline (half-open hang) -> InfraError(leg,
     "idle deadline expired (half-open hang — #839 class, defense-in-depth)").
3. On a reply (healthy): return None.
Never block unbounded; every wait carries a deadline (NFR-5).
```

The probe is CHEAP and READ-ONLY — it must not mutate state or count toward observes (it runs
BEFORE the drive). It reuses the existing client/cert posture; it introduces NO new
transport/cert/spawn path (C-2; a net-new path is a fork smell to FLAG).

## `load_https_bundle` (generalized token-guarded ingest)

Ownership note: the brief lists `load_https_bundle` under `parity_workload.py (or K5)`. It
depends on `InfraError` (K5). Recommendation: DEFINE it in K5 `transport_health.py` (so it can
raise `InfraError` without a circular import) and RE-EXPORT it from C4' `parity_workload.py`
(generalizing the existing `load_https_vector` import surface). The parity_workload.md component
documents the re-export; the LOGIC lives here.

```
def load_https_bundle(out_path, expected_run_token) -> dict[str, Any]:
    """Read the HTTPS-leg dimension bundle the smoke wrote to a fresh $SANDBOX file and validate
    it. Generalizes nan-021 load_https_vector from {run_token, metric_vector} to
    {run_token, dimension_bundle:{...}}. Raises InfraError (NOT FileNotFoundError/ValueError) so
    every ingest failure folds into the INFRA-ERROR class (R-09/R-12).

    Validation:
      1. missing/absent out-file              -> InfraError("https", "bundle out-file absent")
      2. unparseable / truncated JSON         -> InfraError("https", "bundle JSON malformed")  (R-09 deserialization)
      3. run_token != expected_run_token      -> InfraError("https", "stale bundle rejected")   (R-12)
      4. no dimension_bundle dict             -> InfraError("https", "no dimension_bundle")
      5. ANY Dimension.capture_key absent     -> InfraError("https", "missing capture <key>")   (never empty-pass)
      6. a null capture for a NON-precompact key, or a precompact null with measurable!=False
                                              -> InfraError("https", "illegal null capture <key>")
    Returns the dimension_bundle dict on success.
    """
    # required keys come from parity_dimensions.capture_keys() — single source, no hand-list.
```

Returns the inner `dimension_bundle` (the orchestrator indexes it by `dim.capture_key`).

## Data flow

- INPUT (preflight): leg descriptors (socket paths / pinned probe target).
- INPUT (load_https_bundle): `$HTTPS_VECTOR_OUT` path + expected run token + `capture_keys()`.
- OUTPUT: None (preflight, on health) / `dimension_bundle` dict (ingest); `InfraError` on any
  unhealthy/missing/stale condition, caught upstream and mapped to `Outcome.INFRA_ERROR`.

## Error handling

- ALL failure modes raise `InfraError` (single exit class). Callers (preflight: the orchestrator
  before driving; ingest: the orchestrator after the HTTPS leg) catch it and emit
  `Outcome.INFRA_ERROR` with the transport-health detail, distinct ERROR exit code, never a hang.

## Key test scenarios (hints)

- `preflight_leg` against an unreachable/never-responding socket -> `InfraError` within the
  bounded connect deadline; never blocks unbounded (R-02 sc.1).
- A socket that accepts then never replies (half-open simulation) -> idle deadline expires ->
  `InfraError` (R-02 sc.2).
- A slow-but-healthy leg completing just under the idle deadline -> returns None (NOT misread as
  INFRA — guards an over-tight deadline manufacturing false INFRA; R-02 sc.4 boundary).
- `load_https_bundle` rejects: missing file, malformed JSON, stale run_token, missing capture
  key, illegal null capture (R-09 sc.1/2, R-12 sc.1) — each an `InfraError`, never a partial pass.
- A bundle with EVERY required capture_key present + non-empty round-trips successfully (R-09 sc.3).
- An `InfraError` from preflight or ingest rolls up to the distinct ERROR exit code, not a
  parity RED (R-02 sc.3 — at the orchestrator/K4 level).
