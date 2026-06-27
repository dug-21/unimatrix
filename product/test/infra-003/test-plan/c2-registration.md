# Test Plan — C2: Two-slug registration + single restart + route-liveness precondition

> Pseudocode: `pseudocode/c2-registration.md`. Risks: **R-07** (liveness-as-verdict
> / missing-B INFRA), R-11 (slug-B collision), R-08 (stale store), R-13.
> ACs: **AC-01**, AC-13 (slug literals).

C2 boots the shipped image, registers **both** A (`arch-research`) and B
(`isolation-b`) before a **single** restart (#5079), then asserts all four routes
respond non-404 as a **precondition only**. The test of C2 proves liveness is
never reported as the isolation verdict and that a missing/unregistered B store is
INFRA, not a phantom 0-row cell.

## What C2 must do (behavior under test)

- Boot image (HTTP-on by default); wait for `HTTP transport active`.
- `project register arch-research` **and** `project register isolation-b` before
  the one `docker restart`; wait for `HTTP transport active` again.
- Probe all four routes (`/v1/A/observe`, `/v1/B/observe`, `/v1/A/mcp`,
  `/v1/B/mcp`) respond non-404 — **before any marked write**, and the liveness
  probe itself writes **no marker** (would pollute the verdict stores).
- Record non-404 as a **precondition**, never an isolation pass (AC-01, C-06).

## Verification tier 1 — off-Docker gate-logic test (stub-driven)

- `test_c2_liveness_is_precondition_not_verdict` — inspection + stub: a run where
  all four routes are non-404 but the (later) content matrix is not yet evaluated
  must **not** emit GREEN. Assert no `ALL GATES PASSED` path is reachable from
  liveness alone. (R-07a)
- `test_c2_missing_b_store_is_infra` — stub the B `unimatrix.db` as absent at read
  time → **INFRA**, never a 0-row cell that would corrupt B's positive into a
  false-RED or mask a registration fault (R-07b, FR-06.4).
- `test_c2_route_absent_post_restart_is_infra` — any of the four routes 404 after
  restart → INFRA (unregistered-B trap), distinct from RED.
- `test_c2_liveness_writes_no_marker` — grep/inspection: the liveness probe uses a
  non-write method or a body carrying **no** infra003 marker; assert the four
  verdict-store reads are unaffected by the liveness probe.

## Verification tier 2 — live run

- `test_c2_both_registered_before_single_restart` — exactly one `docker restart`
  between registration and liveness; both `arch-research` and `isolation-b` store
  dirs exist on the volume before any cell is trusted (R-07c).
- `test_c2_fresh_isolation_b` — confirm no pre-existing `isolation-b` store/marker
  before the run (fresh-volume assertion, R-11). Per-run nonce (C5/R-08) means
  even a pre-populated B cannot carry this run's marker.

## Slug-literal discipline (AC-13 / SR-08)

- `test_c2_slug_literals_not_retyped` — A reuses the existing `arch-research`
  constant; B is the single literal `isolation-b`; the ADR-004 allowlist regex
  `^[a-z0-9][a-z0-9-]{0,62}$` is **referenced**, never re-typed into the harness.
  Assert both literals validate under that regex.

## Coverage requirement

Non-404 is never an isolation verdict; the 2×2 matrix runs only against
provably-existing per-slug stores; missing/unregistered B is INFRA (R-07). B is a
fresh, non-colliding `isolation-b` (R-11).
