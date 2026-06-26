"""K5 unit tests — transport-health preflight + token-guarded bundle ingest.

Tier A (off-Docker unit): in-process stub sockets / loopback AF_UNIX listeners, NO
Docker, NO daemon. Maps 1:1 to test-plan/transport_health.md and covers R-02
(critical defense-in-depth; #839 CLOSED via PR #842 but the half-open-hang CLASS
remains a real hazard). The slow-but-healthy boundary test is load-bearing: an
over-tight deadline manufactures FALSE INFRA (false-RED).

Pure-Python, stdlib-only (socket / threading / tempfile / json). No fixtures beyond
what each test sets up in-process.
"""

import json
import socket
import threading
import time
from pathlib import Path

import pytest

from harness import transport_health as th
from harness.transport_health import (
    DEFAULT_CONNECT_DEADLINE_S,
    DEFAULT_IDLE_DEADLINE_S,
    EXIT_INFRA,
    HEALTHY_LEG_LATENCY_REF_S,
    InfraError,
    LegProbe,
    load_https_bundle,
    preflight_leg,
    uds_socket_leg,
)


# ---------------------------------------------------------------------------
# Loopback AF_UNIX listener helpers (in-process, bounded, no Docker)
# ---------------------------------------------------------------------------


class _UnixListener:
    """A bounded in-process AF_UNIX listener with a configurable accept-handler.

    behavior:
      - "reply_after": accept, sleep `delay_s`, send one byte (slow-but-healthy).
      - "never_reply": accept, then sleep effectively forever (half-open hang).
      - if never started, the socket path does not exist (unreachable).
    """

    def __init__(self, sock_dir: Path, *, behavior: str, delay_s: float = 0.0):
        self.path = str(sock_dir / "probe.sock")
        self.behavior = behavior
        self.delay_s = delay_s
        self._srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self._srv.bind(self.path)
        self._srv.listen(8)
        self._stop = threading.Event()
        self._conns: list[socket.socket] = []
        self._thread = threading.Thread(target=self._serve, daemon=True)

    def start(self) -> "_UnixListener":
        self._thread.start()
        return self

    def _serve(self) -> None:
        self._srv.settimeout(0.2)
        while not self._stop.is_set():
            try:
                conn, _ = self._srv.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            self._conns.append(conn)
            try:
                if self.behavior == "reply_after":
                    if self.delay_s:
                        time.sleep(self.delay_s)
                    conn.sendall(b"x")
                elif self.behavior == "never_reply":
                    # Accept the connection but NEVER reply (half-open). Hold it open
                    # until the test tears the listener down.
                    while not self._stop.is_set():
                        time.sleep(0.05)
            except OSError:
                pass

    def close(self) -> None:
        self._stop.set()
        try:
            self._srv.close()
        except OSError:
            pass
        for c in self._conns:
            try:
                c.close()
            except OSError:
                pass
        self._thread.join(timeout=2.0)


# ---------------------------------------------------------------------------
# Connect deadline — unreachable leg (R-02 scenario 1)
# ---------------------------------------------------------------------------


def test_preflight_leg_unreachable_socket_raises_infra_within_connect_deadline(
    tmp_path,
):
    """A UDS path that never accepts (no listener bound) → InfraError within the
    bounded connect deadline; NEVER blocks unbounded."""
    missing = tmp_path / "does-not-exist.sock"
    leg = uds_socket_leg("uds", missing)
    start = time.monotonic()
    with pytest.raises(InfraError) as exc:
        preflight_leg(leg, connect_deadline_s=1.0, idle_deadline_s=1.0)
    elapsed = time.monotonic() - start
    # Bounded: never blocks unbounded (deadline + generous slack).
    assert elapsed <= 1.0 + 2.0
    assert exc.value.leg == "uds"
    assert "unreachable" in exc.value.reason or "connect deadline" in exc.value.reason


def test_preflight_leg_connection_refused_raises_infra(tmp_path):
    """An absent/refused UDS endpoint → InfraError (not a silent pass, not a parity
    verdict)."""
    refused = tmp_path / "refused.sock"  # no listener bound → connect refused/absent
    leg = uds_socket_leg("uds", refused)
    with pytest.raises(InfraError) as exc:
        preflight_leg(leg, connect_deadline_s=1.0, idle_deadline_s=1.0)
    assert "unreachable" in exc.value.reason or "connect deadline" in exc.value.reason


# ---------------------------------------------------------------------------
# Idle deadline — half-open hang (R-02 scenario 2 — the #839 CLASS, load-bearing)
# ---------------------------------------------------------------------------


def test_preflight_leg_half_open_accepts_never_replies_raises_infra(tmp_path):
    """A loopback listener that ACCEPTS then never replies (half-open simulation) →
    the idle deadline expires → InfraError, classified as INFRA-ERROR material, NOT a
    timeout-as-PARITY-FAIL and NOT a hang. The precise #839-class scenario."""
    listener = _UnixListener(tmp_path, behavior="never_reply").start()
    try:
        leg = uds_socket_leg("uds", listener.path)
        start = time.monotonic()
        with pytest.raises(InfraError) as exc:
            # Connect succeeds (listener accepts); idle deadline must trip.
            preflight_leg(leg, connect_deadline_s=2.0, idle_deadline_s=0.75)
        elapsed = time.monotonic() - start
        # Bounded by the idle deadline (+ slack); never an unbounded hang.
        assert elapsed <= 0.75 + 2.0
        assert "idle deadline expired" in exc.value.reason
        assert "half-open" in exc.value.reason
        # INFRA-ERROR material — its own class, not a parity verdict.
        assert isinstance(exc.value, InfraError)
        assert not isinstance(exc.value, AssertionError)
    finally:
        listener.close()


# ---------------------------------------------------------------------------
# Slow-but-healthy boundary (R-02 scenario 4 — guards against false INFRA)
# ---------------------------------------------------------------------------


def test_preflight_leg_slow_but_healthy_under_idle_deadline_passes(tmp_path):
    """A leg that replies just UNDER the idle deadline → preflight returns normally
    (no InfraError). LOAD-BEARING: proves the deadline has explicit head-room over a
    healthy-leg latency and does not manufacture a false INFRA-ERROR."""
    # Reply after 0.4s; idle deadline 1.5s → comfortable head-room, healthy.
    listener = _UnixListener(tmp_path, behavior="reply_after", delay_s=0.4).start()
    try:
        leg = uds_socket_leg("uds", listener.path)
        result = preflight_leg(leg, connect_deadline_s=2.0, idle_deadline_s=1.5)
        assert result is None
    finally:
        listener.close()


def test_preflight_leg_idle_deadline_head_room_documented():
    """Assert the configured idle/connect deadlines carry explicit HEAD-ROOM over a
    measured healthy-leg latency — documented constants, not arbitrary tight bounds.
    Asserts head-room EXISTS, not the exact magnitude (re-tunable at first live run)."""
    # Head-room over a healthy-leg latency reference.
    assert DEFAULT_CONNECT_DEADLINE_S > HEALTHY_LEG_LATENCY_REF_S
    assert DEFAULT_IDLE_DEADLINE_S > HEALTHY_LEG_LATENCY_REF_S
    # Comfortable (not razor-thin) head-room: at least ~2x over the healthy latency.
    assert DEFAULT_CONNECT_DEADLINE_S >= 2 * HEALTHY_LEG_LATENCY_REF_S
    assert DEFAULT_IDLE_DEADLINE_S >= 2 * HEALTHY_LEG_LATENCY_REF_S
    # The idle deadline (open-ended half-open hang) is looser than connect.
    assert DEFAULT_IDLE_DEADLINE_S >= DEFAULT_CONNECT_DEADLINE_S


# ---------------------------------------------------------------------------
# InfraError is a distinct class (R-02 — never a parity verdict)
# ---------------------------------------------------------------------------


def test_infra_error_is_distinct_exception_type():
    """InfraError is its own type — NOT an AssertionError subclass — so the
    orchestrator's classifier routes it to the INFRA-ERROR class with a distinct exit
    code, never into the parity-RED path."""
    err = InfraError("https", "bundle out-file absent", detail="x")
    assert isinstance(err, Exception)
    assert not isinstance(err, AssertionError)
    assert err.leg == "https"
    assert err.reason == "bundle out-file absent"
    assert err.detail == "x"
    assert "https" in str(err)


def test_exit_infra_code_distinct_from_smoke_and_standard_codes():
    """EXIT_INFRA must NOT collide with standard 0/1 or run_smoke_gate's 3/4 codes."""
    assert EXIT_INFRA not in {0, 1, 3, 4}
    assert isinstance(EXIT_INFRA, int)


# ---------------------------------------------------------------------------
# Both legs (R-02 coverage requirement — symmetry)
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("label", ["uds", "https"])
def test_preflight_leg_applies_to_both_legs(tmp_path, label):
    """Parametrized over the UDS and HTTPS leg descriptors → each leg has a bounded
    connect + idle deadline (no leg is exempt). Both a half-open hang trip and a
    healthy pass are exercised symmetrically via injectable probes."""
    # Half-open: connect ok, liveness blocks → idle deadline trips for BOTH labels.
    half_open = LegProbe(
        label,
        connect=lambda d: None,
        liveness=lambda d: (_ for _ in ()).throw(socket.timeout()),
    )
    start = time.monotonic()
    with pytest.raises(InfraError) as exc:
        preflight_leg(half_open, connect_deadline_s=1.0, idle_deadline_s=0.5)
    assert time.monotonic() - start <= 0.5 + 2.0
    assert exc.value.leg == label
    assert "idle deadline expired" in exc.value.reason

    # Healthy: connect ok, liveness replies promptly → returns None for BOTH labels.
    healthy = LegProbe(label, connect=lambda d: None, liveness=lambda d: True)
    assert preflight_leg(healthy, connect_deadline_s=1.0, idle_deadline_s=1.0) is None


def test_preflight_leg_str_label_without_probe_raises_infra():
    """A bare str leg with no wired probe → InfraError (C-2: the orchestrator must
    wire the shipped client posture; K5 never invents a transport path)."""
    with pytest.raises(InfraError) as exc:
        preflight_leg("uds", connect_deadline_s=1.0, idle_deadline_s=1.0)
    assert "no probe wired" in exc.value.reason


# ---------------------------------------------------------------------------
# load_https_bundle — token-guarded never-empty ingest (R-09 / R-12)
# ---------------------------------------------------------------------------

_TOKEN = "nan-022-parity-session-0001"


def _full_bundle() -> dict:
    """A bundle with EVERY required capture_key present + non-empty (the happy path)."""
    return {
        "retrieval": {"queries": [{"tool": "context_search", "result_ids": [1]}],
                      "capture_2": [{"result_ids": [1]}]},
        "behavioral": {"topic_signals": ["nan-022"]},
        "analytics": {"metric_vector": {"count": 1}, "informs_edges": [], "phase_signal": {}},
        "proactive": {"briefing_ids": [1], "briefing_scores": [0.9],
                      "injection_set": [1], "capture_2": {"briefing_ids": [1]}},
        "precompact": {"restored_payload": {"x": 1}, "measurable": True, "host_side_gap": None},
    }


def _write_payload(tmp_path: Path, payload) -> Path:
    out = tmp_path / "https_bundle.json"
    out.write_text(json.dumps(payload), encoding="utf-8")
    return out


def test_load_https_bundle_full_bundle_round_trips(tmp_path):
    """A bundle with EVERY required capture_key present + non-empty round-trips
    successfully (R-09 sc.3) and returns the inner dimension_bundle dict."""
    out = _write_payload(tmp_path, {"run_token": _TOKEN, "dimension_bundle": _full_bundle()})
    bundle = load_https_bundle(out, _TOKEN)
    assert set(bundle) >= {
        "retrieval", "behavioral", "analytics", "proactive", "precompact",
    }
    assert bundle["behavioral"]["topic_signals"] == ["nan-022"]


def test_load_https_bundle_missing_file_raises_infra(tmp_path):
    """Missing out-file → InfraError, never compare against empty (R-09 sc.1)."""
    with pytest.raises(InfraError) as exc:
        load_https_bundle(tmp_path / "absent.json", _TOKEN)
    assert exc.value.leg == "https"
    assert "absent" in exc.value.reason


def test_load_https_bundle_malformed_json_raises_infra(tmp_path):
    """Truncated/unparseable JSON → InfraError (R-09 sc.2 deserialization)."""
    out = tmp_path / "bad.json"
    out.write_text('{"run_token": "x", "dimension_bundle": {', encoding="utf-8")
    with pytest.raises(InfraError) as exc:
        load_https_bundle(out, _TOKEN)
    assert "malformed" in exc.value.reason


def test_load_https_bundle_stale_run_token_raises_infra(tmp_path):
    """run_token != expected → InfraError stale bundle rejected (R-12)."""
    out = _write_payload(tmp_path, {"run_token": "STALE", "dimension_bundle": _full_bundle()})
    with pytest.raises(InfraError) as exc:
        load_https_bundle(out, _TOKEN)
    assert "stale bundle rejected" in exc.value.reason


def test_load_https_bundle_no_dimension_bundle_raises_infra(tmp_path):
    """Missing dimension_bundle dict → InfraError (never empty-pass)."""
    out = _write_payload(tmp_path, {"run_token": _TOKEN, "metric_vector": {}})
    with pytest.raises(InfraError) as exc:
        load_https_bundle(out, _TOKEN)
    assert "no dimension_bundle" in exc.value.reason


def test_load_https_bundle_missing_capture_key_raises_infra(tmp_path):
    """ANY required capture_key absent → InfraError missing capture (never empty-pass,
    C-9/FR-12)."""
    b = _full_bundle()
    del b["proactive"]
    out = _write_payload(tmp_path, {"run_token": _TOKEN, "dimension_bundle": b})
    with pytest.raises(InfraError) as exc:
        load_https_bundle(out, _TOKEN)
    assert "missing capture proactive" in exc.value.reason


def test_load_https_bundle_illegal_null_non_precompact_raises_infra(tmp_path):
    """A null capture for a NON-precompact key → InfraError illegal null capture."""
    b = _full_bundle()
    b["retrieval"] = None
    out = _write_payload(tmp_path, {"run_token": _TOKEN, "dimension_bundle": b})
    with pytest.raises(InfraError) as exc:
        load_https_bundle(out, _TOKEN)
    assert "illegal null capture retrieval" in exc.value.reason


def test_load_https_bundle_precompact_null_payload_with_measurable_false_ok(tmp_path):
    """precompact.restored_payload may be null ONLY with measurable=False (ADR-006
    documented host-side gap) → ingests successfully."""
    b = _full_bundle()
    b["precompact"] = {"restored_payload": None, "measurable": False,
                       "host_side_gap": "harness cannot drive a live CC host"}
    out = _write_payload(tmp_path, {"run_token": _TOKEN, "dimension_bundle": b})
    bundle = load_https_bundle(out, _TOKEN)
    assert bundle["precompact"]["restored_payload"] is None


def test_load_https_bundle_precompact_null_payload_measurable_true_raises_infra(tmp_path):
    """precompact null payload with measurable!=False → InfraError (never a vacuous
    pass — ADR-006)."""
    b = _full_bundle()
    b["precompact"] = {"restored_payload": None, "measurable": True, "host_side_gap": None}
    out = _write_payload(tmp_path, {"run_token": _TOKEN, "dimension_bundle": b})
    with pytest.raises(InfraError) as exc:
        load_https_bundle(out, _TOKEN)
    assert "illegal null capture precompact" in exc.value.reason
