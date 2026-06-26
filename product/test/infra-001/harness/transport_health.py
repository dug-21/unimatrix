"""K5 — Transport-health preflight + token-guarded bundle ingest (nan-022).

New, pure-Python, stdlib-only, OFF-Docker unit-testable. Implements ADR-002 (#5313)
defense-in-depth: a bounded per-leg connect + idle deadline so a half-open-socket
hang (the #839 class — #839 itself CLOSED via commit 5b6badad / PR #842, retained
here as DEFENSE-IN-DEPTH, NOT a gating dependency) or any transport unreachability
surfaces as `InfraError`, NEVER a dimension parity verdict and NEVER an unbounded
suite hang (SR-02 / R-02).

Also hosts the generalized token-guarded bundle ingest `load_https_bundle` — its
LOGIC lives here so it can raise `InfraError` without a circular import (it depends
on `InfraError`), and it is RE-EXPORTED from C4' `parity_workload.py` (generalizing
the existing `load_https_vector` import surface). See pseudocode/transport_health.md.

ZERO new runtime deps (stdlib only). ZERO production-code diff (NFR-1). Reuses the
existing client/cert posture; introduces NO new transport/cert/spawn path (C-2 — a
net-new path is a fork smell to FLAG, never to add here).
"""

from __future__ import annotations

import json
import logging
import socket
import time
from pathlib import Path
from typing import Any, Callable

logger = logging.getLogger(__name__)


# =============================================================================
# Distinct INFRA exit code (Gate-3a advisory)
# =============================================================================
#
# Pin the INFRA exit code to a value NOT in {0, 1, 3, 4} so it cannot collide with
# the standard 0 (success) / 1 (generic failure) shell codes, nor with
# `run_smoke_gate`'s truth table (3 = Docker-absent skip → HARD-FAIL, 4 =
# credentials unacquirable). 2 is conventionally "usage error" (argparse / our own
# CLI usage shim), so we pin 7 — a distinct, unambiguous INFRA-ERROR class code that
# the orchestrator's roll-up maps `Outcome.INFRA_ERROR` onto. Named so the
# orchestrator/K4 import the constant rather than hand-coding the integer.
EXIT_INFRA: int = 7


# =============================================================================
# InfraError — the single distinct exit-class exception (ADR-002 / R-02)
# =============================================================================


class InfraError(Exception):
    """Distinct exit-class exception: half-open hang / unreachable socket /
    bounded-deadline expiry / missing-stale-empty capture.

    Caught by K4 `classify_dimension` / the orchestrator and converted to
    `Outcome.INFRA_ERROR` with the distinct `EXIT_INFRA` code — NEVER read as
    `PARITY_PASS` or `PARITY_FAIL`. It is deliberately a direct `Exception`
    subclass (NOT an `AssertionError` / `ParityMismatch` subclass) so the
    classifier routes it into the INFRA-ERROR class, never into the parity-RED path.
    """

    def __init__(self, leg: str, reason: str, *, detail: str = ""):
        self.leg = leg
        self.reason = reason
        self.detail = detail
        super().__init__(
            f"[{leg}] transport-health INFRA: {reason}"
            + (f" — {detail}" if detail else "")
        )


# =============================================================================
# Bounded deadlines (first-live-run tuning values — HEAD-ROOM rationale)
# =============================================================================
#
# These are FIRST-LIVE-RUN tuning values. They MUST carry explicit HEAD-ROOM over a
# healthy-leg latency so a slow-but-healthy runner is NOT misread as INFRA (R-02
# boundary scenario 4 — an over-tight deadline manufactures a FALSE INFRA, which
# false-REDs the whole matrix). The head-room is the gap between the deadline and a
# typical healthy-leg latency:
#
#   * Healthy UDS connect / first-byte is sub-millisecond; healthy HTTPS pinned-TLS
#     connect + first byte to a warm local container is well under ~1s. We pin the
#     CONNECT deadline at 5.0s — roughly 5x head-room over a warm-but-busy connect.
#   * A healthy liveness reply (a cheap read-only probe) returns in well under ~1s on
#     both legs. We pin the IDLE deadline at 10.0s — ~10x head-room over a healthy
#     reply — so cold caches / a momentarily busy event loop never trip a false
#     half-open. The idle deadline is intentionally LOOSER than the connect deadline
#     because the half-open hang it guards is open-ended (a hung socket never
#     replies), so erring loose costs only a bounded extra wait on a genuine hang
#     while protecting the slow-but-healthy boundary.
#
# Tests assert that this HEAD-ROOM EXISTS (idle > connect, both comfortably above a
# measured healthy-leg latency), NOT the exact magnitude — the magnitudes are
# re-tunable at first live run without breaking the contract. The HTTPS smoke
# shell-out keeps its own outer ceiling `HTTPS_SMOKE_TIMEOUT_S` (NFR-5); K5 adds the
# PRE-DRIVE reachability + idle probe so a HANG (not a failure) is NAMED as INFRA.
DEFAULT_CONNECT_DEADLINE_S: float = 5.0
DEFAULT_IDLE_DEADLINE_S: float = 10.0

# A reference healthy-leg latency used purely to document/justify the head-room (the
# slow-but-healthy boundary test asserts the deadlines sit comfortably above it).
HEALTHY_LEG_LATENCY_REF_S: float = 1.0


# =============================================================================
# Leg descriptor — what `preflight_leg` probes (no net-new transport code, C-2)
# =============================================================================
#
# A `leg` carries WHAT to probe. To keep K5 stdlib-only and off-Docker testable, the
# probe is expressed as an injectable, CHEAP, READ-ONLY callable plus the leg label.
# Production callers (the orchestrator) pass a `probe` that reuses the SHIPPED
# client/cert posture (UnimatrixUdsClient / the pinned cert-pin probe) — K5 itself
# introduces no new transport/cert/spawn path. The default UDS-socket probe below is
# a thin stdlib `socket.connect` used only when a leg is described by a UDS path.
#
# `leg` may be:
#   * a str label ("uds" | "https") — then `probe`/`connect_probe` are supplied by
#     the caller (the orchestrator wires the shipped client), OR
#   * a LegProbe carrying a label + a connect callable + a liveness callable.


class LegProbe:
    """A leg descriptor for `preflight_leg`: a label plus a bounded connect probe
    and a bounded liveness probe. Both probes are CHEAP and READ-ONLY (they run
    BEFORE the drive and must not mutate state or count toward observes).

    `connect(deadline_s)` must establish reachability within the deadline; it raises
    on refused/absent and may raise/return-late on a connect-deadline expiry.
    `liveness(deadline_s)` must send a cheap request and wait for ANY reply within
    the deadline; on a socket that accepted-but-never-replies (half-open) it must
    NOT return within the deadline (the caller times it and names the hang).
    """

    def __init__(
        self,
        label: str,
        *,
        connect: Callable[[float], None],
        liveness: Callable[[float], bool],
    ):
        self.label = label
        self._connect = connect
        self._liveness = liveness

    def connect(self, deadline_s: float) -> None:
        self._connect(deadline_s)

    def liveness(self, deadline_s: float) -> bool:
        return self._liveness(deadline_s)


def uds_socket_leg(label: str, socket_path: str | Path) -> LegProbe:
    """Build a LegProbe for a UDS leg from a socket path, using only stdlib sockets.

    The connect phase opens an `AF_UNIX` stream socket with the connect deadline as
    its timeout; the liveness phase sends a cheap byte and waits (bounded) for ANY
    reply. A socket that accepts then never replies (the #839 half-open class) leaves
    `recv` blocked until the idle deadline — the caller times it out and names it.
    No net-new transport path: this is the stdlib socket the shipped UDS client
    already speaks over.
    """
    sp = str(socket_path)

    def _connect(deadline_s: float) -> None:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(deadline_s)
        try:
            s.connect(sp)
        finally:
            s.close()

    def _liveness(deadline_s: float) -> bool:
        s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        s.settimeout(deadline_s)
        try:
            s.connect(sp)
            # Cheap read-only liveness nudge; any reply (even an error frame) proves
            # the socket is live, not half-open. A half-open peer never replies and
            # `recv` blocks until the deadline → socket.timeout (named as half-open).
            try:
                s.sendall(b"\n")
            except OSError:
                pass
            data = s.recv(1)
            return bool(data)
        finally:
            s.close()

    return LegProbe(label, connect=_connect, liveness=_liveness)


def _coerce_leg(
    leg: Any,
    connect_probe: Callable[[float], None] | None,
    liveness_probe: Callable[[float], bool] | None,
) -> LegProbe:
    """Resolve the `leg` arg into a LegProbe. Accepts a LegProbe directly, or a str
    label together with caller-supplied `connect_probe`/`liveness_probe` callables
    (the orchestrator wires the shipped client posture there)."""
    if isinstance(leg, LegProbe):
        return leg
    label = str(leg)
    if connect_probe is None or liveness_probe is None:
        raise InfraError(
            label,
            "no probe wired",
            detail=(
                "preflight_leg needs a LegProbe or a (connect_probe, liveness_probe) "
                "pair reusing the shipped client/cert posture (C-2: no net-new path)"
            ),
        )
    return LegProbe(label, connect=connect_probe, liveness=liveness_probe)


# =============================================================================
# preflight_leg — bounded connect + idle reachability probe (ADR-002 / R-02)
# =============================================================================


def preflight_leg(
    leg: Any,
    *,
    connect_deadline_s: float = DEFAULT_CONNECT_DEADLINE_S,
    idle_deadline_s: float = DEFAULT_IDLE_DEADLINE_S,
    connect_probe: Callable[[float], None] | None = None,
    liveness_probe: Callable[[float], bool] | None = None,
) -> None:
    """Probe a leg's reachability with bounded connect + idle deadlines BEFORE
    driving it. Raises `InfraError` on: unreachable socket, connect-deadline expiry,
    or a socket that accepts then never replies (half-open) within the idle deadline.
    Returns None on a healthy, responsive leg.

    The probe is CHEAP and READ-ONLY — it runs BEFORE the drive, must not mutate
    state or count toward observes, and reuses the shipped client/cert posture (NO
    new transport/cert/spawn path — C-2). Every wait carries a deadline; this NEVER
    blocks unbounded (NFR-5).

    `leg` is a `LegProbe`, or a str label plus caller-supplied
    `connect_probe`/`liveness_probe` callables (the orchestrator wires the shipped
    UDS client / pinned HTTPS probe). Applies symmetrically to BOTH legs — no leg is
    exempt from a bounded connect + idle deadline.
    """
    probe = _coerce_leg(leg, connect_probe, liveness_probe)
    label = probe.label

    # 1. CONNECT phase — bounded by connect_deadline_s.
    start = time.monotonic()
    try:
        probe.connect(connect_deadline_s)
    except (socket.timeout, TimeoutError):
        raise InfraError(
            label,
            "connect deadline expired",
            detail=f"no connection established within {connect_deadline_s}s",
        )
    except (ConnectionRefusedError, FileNotFoundError) as exc:
        raise InfraError(
            label,
            "transport unreachable",
            detail=f"{type(exc).__name__}: {exc}",
        )
    except OSError as exc:
        raise InfraError(
            label,
            "transport unreachable",
            detail=f"{type(exc).__name__}: {exc}",
        )
    connect_elapsed = time.monotonic() - start
    if connect_elapsed > connect_deadline_s:
        # A probe that returned but only after overshooting the deadline (e.g. a probe
        # that swallows its own timeout) is still a connect-deadline expiry.
        raise InfraError(
            label,
            "connect deadline expired",
            detail=f"connect took {connect_elapsed:.3f}s > {connect_deadline_s}s",
        )

    # 2. IDLE / liveness phase — bounded by idle_deadline_s. A socket that accepted
    #    but never replies within the deadline is the #839 half-open class.
    idle_start = time.monotonic()
    try:
        replied = probe.liveness(idle_deadline_s)
    except (socket.timeout, TimeoutError):
        raise InfraError(
            label,
            "idle deadline expired (half-open hang — #839 class, defense-in-depth)",
            detail=f"no reply within {idle_deadline_s}s after a successful connect",
        )
    except OSError as exc:
        raise InfraError(
            label,
            "transport unreachable",
            detail=f"liveness probe failed: {type(exc).__name__}: {exc}",
        )
    idle_elapsed = time.monotonic() - idle_start
    if not replied or idle_elapsed > idle_deadline_s:
        raise InfraError(
            label,
            "idle deadline expired (half-open hang — #839 class, defense-in-depth)",
            detail=(
                f"no reply within {idle_deadline_s}s after a successful connect "
                f"(elapsed {idle_elapsed:.3f}s, replied={replied})"
            ),
        )

    # 3. Healthy, responsive leg.
    logger.debug(
        "[%s] preflight healthy: connect=%.3fs idle=%.3fs",
        label,
        connect_elapsed,
        idle_elapsed,
    )
    return None


# =============================================================================
# load_https_bundle — generalized token-guarded never-empty ingest (R-09 / R-12)
# =============================================================================
#
# The required capture keys are SINGLE-SOURCED from K1 `parity_dimensions.capture_keys()`
# (C-5 — no hand-list). K5 is in the same off-Docker wave as K1, so we import it
# LAZILY at call time: when K1 is present we use its tuple; before K1 lands (or in an
# isolated off-Docker unit run) we fall back to the canonical five. The fallback equals
# K1's keys by construction (asserted by K1's own registry tests) — it is a bootstrap
# for the off-Docker seam, never a second authoritative source.
_CANONICAL_CAPTURE_KEYS: tuple[str, ...] = (
    "retrieval",
    "behavioral",
    "analytics",
    "proactive",
    "precompact",
)
# The ONLY capture whose value may legally be `null`, and ONLY with measurable=False
# (ADR-006 / OVERVIEW). Single source for the null-capture exception.
PRECOMPACT_KEY: str = "precompact"


def _required_capture_keys() -> tuple[str, ...]:
    """Return the required capture keys, single-sourced from K1 when importable."""
    try:
        from harness.parity_dimensions import capture_keys  # noqa: WPS433 (lazy)

        keys = tuple(capture_keys())
        if keys:
            return keys
    except Exception:  # noqa: BLE001 — K1 not yet present in this off-Docker wave
        logger.debug(
            "parity_dimensions.capture_keys() unavailable; using canonical fallback"
        )
    return _CANONICAL_CAPTURE_KEYS


def load_https_bundle(
    out_path: str | Path, expected_run_token: str
) -> dict[str, Any]:
    """Read the HTTPS-leg dimension bundle the smoke wrote to a fresh $SANDBOX file
    and validate it. Generalizes nan-021's `load_https_vector` from
    `{run_token, metric_vector}` to `{run_token, dimension_bundle:{...}}`.

    Raises `InfraError` (NOT `FileNotFoundError`/`ValueError`) on EVERY ingest failure
    so every failure folds into the single INFRA-ERROR class (R-09/R-12), never an
    empty-equals-empty pass. Returns the inner `dimension_bundle` dict on success
    (the orchestrator indexes it by `dim.capture_key`).

    Validation order:
      1. missing/absent out-file              → InfraError("https", "bundle out-file absent")
      2. unparseable / truncated JSON         → InfraError("https", "bundle JSON malformed")
      3. run_token != expected_run_token      → InfraError("https", "stale bundle rejected")
      4. no dimension_bundle dict             → InfraError("https", "no dimension_bundle")
      5. ANY required capture_key absent      → InfraError("https", "missing capture <key>")
      6. illegal null capture (non-precompact null, or precompact null w/ measurable!=False)
                                              → InfraError("https", "illegal null capture <key>")
    """
    leg = "https"
    p = Path(out_path)

    # 1. missing / absent out-file.
    if not p.is_file():
        raise InfraError(
            leg,
            "bundle out-file absent",
            detail=f"{p} (smoke leg failed to emit — never compare against empty)",
        )

    # 2. unparseable / truncated JSON.
    try:
        raw = p.read_text(encoding="utf-8")
    except OSError as exc:
        raise InfraError(
            leg, "bundle out-file unreadable", detail=f"{p}: {exc}"
        )
    try:
        payload = json.loads(raw)
    except (json.JSONDecodeError, ValueError) as exc:
        raise InfraError(
            leg,
            "bundle JSON malformed",
            detail=f"{p}: {exc} (R-09 deserialization)",
        )
    if not isinstance(payload, dict):
        raise InfraError(
            leg,
            "bundle JSON malformed",
            detail=f"{p}: top-level JSON is {type(payload).__name__}, expected object",
        )

    # 3. stale run-token guard (R-12) — a prior-tag file CANNOT be ingested.
    token = payload.get("run_token")
    if token != expected_run_token:
        raise InfraError(
            leg,
            "stale bundle rejected",
            detail=(
                f"run_token {token!r} != this run {expected_run_token!r} "
                f"(R-12 — a prior-tag file cannot be ingested)"
            ),
        )

    # 4. dimension_bundle dict present.
    bundle = payload.get("dimension_bundle")
    if not isinstance(bundle, dict):
        raise InfraError(
            leg,
            "no dimension_bundle",
            detail=f"{p}: 'dimension_bundle' is {type(bundle).__name__}, expected object",
        )

    # 5. every required capture key present (never empty-pass — C-9 / FR-12).
    for key in _required_capture_keys():
        if key not in bundle:
            raise InfraError(
                leg,
                f"missing capture {key}",
                detail=(
                    f"{p}: dimension_bundle lacks required capture_key {key!r} "
                    f"(routed to the wrong wire surface records nothing → INFRA, "
                    f"never empty-pass)"
                ),
            )

    # 6. illegal null captures. Only precompact.restored_payload may be null, and
    #    ONLY with measurable=False (ADR-006). Any other null → INFRA.
    for key in _required_capture_keys():
        cap = bundle[key]
        if key == PRECOMPACT_KEY:
            _validate_precompact_capture(leg, p, cap)
            continue
        if cap is None:
            raise InfraError(
                leg,
                f"illegal null capture {key}",
                detail=(
                    f"{p}: capture {key!r} is null; only precompact may be null "
                    f"(and only with measurable=False)"
                ),
            )

    return bundle


def _validate_precompact_capture(leg: str, p: Path, cap: Any) -> None:
    """The precompact capture is the ONLY one that may carry a null payload, and
    ONLY when `measurable=False` (ADR-006 documented host-side gap). A bare null
    precompact capture, or a null `restored_payload` with `measurable!=False`, is an
    illegal null capture → InfraError."""
    if cap is None:
        raise InfraError(
            leg,
            f"illegal null capture {PRECOMPACT_KEY}",
            detail=(
                f"{p}: precompact capture itself is null; expected an object with "
                f"restored_payload/measurable/host_side_gap"
            ),
        )
    if not isinstance(cap, dict):
        raise InfraError(
            leg,
            f"illegal null capture {PRECOMPACT_KEY}",
            detail=f"{p}: precompact capture is {type(cap).__name__}, expected object",
        )
    if cap.get("restored_payload") is None and cap.get("measurable") is not False:
        raise InfraError(
            leg,
            f"illegal null capture {PRECOMPACT_KEY}",
            detail=(
                f"{p}: precompact.restored_payload is null but measurable is "
                f"{cap.get('measurable')!r}; a null payload is legal ONLY with "
                f"measurable=False (ADR-006 documented host-side gap)"
            ),
        )
