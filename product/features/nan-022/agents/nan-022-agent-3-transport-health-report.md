# nan-022-agent-3-transport-health — K5 Transport-health preflight

## Files created
- `product/test/infra-001/harness/transport_health.py` (492 lines)
- `product/test/infra-001/suites/test_transport_health.py` (365 lines)

## Surface delivered (per brief Function Signatures / pseudocode/transport_health.md)
- `InfraError(Exception)` — distinct exit-class exception (NOT an AssertionError subclass).
- `EXIT_INFRA = 7` — INFRA exit code pinned outside {0,1,3,4} (Gate-3a advisory; avoids run_smoke_gate 3/4 + standard 0/1; 2 reserved for usage).
- `preflight_leg(leg, *, connect_deadline_s, idle_deadline_s, connect_probe=None, liveness_probe=None) -> None`.
- `load_https_bundle(out_path, expected_run_token) -> dict` (LOGIC owned here; re-export from C4' later).
- Helpers: `LegProbe`, `uds_socket_leg`, deadline constants with head-room rationale.

## Tests: 19 passed / 0 failed (off-Docker, ~1.8s)
- unreachable + connection-refused → InfraError within bounded connect deadline (never unbounded).
- half-open accept-never-reply (real loopback AF_UNIX listener) → idle-deadline InfraError (#839 class).
- slow-but-healthy under idle deadline → returns None (false-INFRA guard, load-bearing).
- head-room-documented: idle>connect, both ≥2x healthy-leg latency ref (asserts head-room exists, not magnitude).
- InfraError distinct type; EXIT_INFRA ∉ {0,1,3,4}.
- both legs (parametrized uds/https) bounded; bare-str-no-probe → InfraError.
- load_https_bundle: full round-trip; missing file / malformed JSON / stale token / no dimension_bundle / missing capture / illegal null (non-precompact) → InfraError; precompact null+measurable=False ok; precompact null+measurable=True → InfraError.

## Design notes
- Required capture keys single-sourced from K1 `parity_dimensions.capture_keys()` via a LAZY import at call time, with a canonical fallback to the six keys. Rationale: K1 is in the same Wave A and may not exist at K5 import time / in isolated off-Docker runs. The fallback equals K1's keys by construction (asserted by K1's own registry tests) — a bootstrap for the off-Docker seam, NOT a second authoritative source (C-5 honored once K1 lands).
- No net-new transport/cert/spawn path (C-2): `preflight_leg` takes injectable connect/liveness probes; production callers wire the shipped UnimatrixUdsClient / pinned cert-pin posture. The stdlib `uds_socket_leg` probe speaks the same AF_UNIX socket the shipped UDS client uses.
- Deadlines (connect 5.0s / idle 10.0s) are first-live-run tuning values with explicit head-room comments; idle is intentionally looser than connect (open-ended half-open hang).

## Issues / adjacent breakage flagged
- NONE blocking. K1 `harness/parity_dimensions.py` does not yet exist (parallel Wave A). When K1 lands, `_required_capture_keys()` will transparently pick up `capture_keys()`; no change needed in K5. Recommend Gate-3a confirm K1's `capture_key`s equal the canonical fallback tuple (they do per parity_dimensions.md: retrieval/behavioral/analytics/proactive/precompact/isolation).
- C4' (`parity_workload.py`) must RE-EXPORT `load_https_bundle` from K5 and generalize its existing `load_https_vector` import surface — flagged for the C4' agent (Wave C). I did NOT modify parity_workload.py (out of my file scope).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (decision/nan-022) + context_get #5313 — surfaced ADR-002 four-valued outcome model: INFRA first in the ordered classifier, bounded connect+idle deadlines, half-open #839 class as defense-in-depth, distinct exit code never read as parity RED. Applied directly.
- Stored: nothing novel to store — the lazy-single-source-with-canonical-fallback and injectable-probe-to-avoid-fork patterns are scope-mandated by the brief/pseudocode (C-2/C-5) and ADR-002, not newly discovered gotchas; storing them would duplicate the ADR.
