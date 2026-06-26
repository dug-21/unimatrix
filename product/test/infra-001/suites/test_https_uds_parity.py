"""C3 — the pytest-as-orchestrator HTTPS-vs-UDS parity entrypoint (nan-021).

Drives the SINGLE C4 parity workload manifest over the local UDS transport
(`UnimatrixUdsClient` + `UnimatrixHookClient`) against a live foreground daemon,
producing `MetricVector(UDS)` IN-PROCESS. The same pytest invocation shells out to
the smoke's C2 `cloud_cycle_gates` for the HTTPS leg, ingests `MetricVector(HTTPS)`
from a fresh `$SANDBOX` out-file carrying this run's correlation token, then runs
the C4 comparator — live-vs-live (D-6), one execution (R-03), one manifest, one
stable session identity (ADR-001).

The leg drivers live in `harness/parity_legs.py` (drive_uds_leg / run_https_leg /
assert_derived_attribution); this file is the orchestrator + the C3 contract tests.
EXTENDS the existing harness (uds_client.py / hook_client.py / conftest.py
`daemon_server`); CONSUMES the C4 contract (parity_workload.py + the comparator)
verbatim (AC-07 / SR-04). NO seed site is reachable from this path (AC-03 / FR-6).

Markers:
  * `@pytest.mark.integration` — needs the live `daemon_server` (UDS leg).
  * `@pytest.mark.parity` (the orchestrator) — additionally shells out to the
    Docker smoke's HTTPS leg. The TRUE cross-leg live run is owned by Stage 3c;
    absent UNIMATRIX_HTTPS_SMOKE the orchestrator SKIPs, and the off-Docker seam
    proof (`test_c3_orchestrator_seam_with_fixture_https_vector`) covers the wiring
    (the nan-019 #5258 stub-drive precedent).
"""

from __future__ import annotations

import json

import pytest

from harness import parity_workload as pw
from harness.parity_workload import (
    compare_metric_vectors,
    default_workload,
    durability_barrier,
    field_by_field_record,
    load_https_vector,
    write_field_record,
)
from harness.uds_client import UnimatrixUdsClient
from harness.hook_client import UnimatrixHookClient
from harness import parity_legs
from harness.parity_legs import (
    PARITY_PHASE,
    assert_derived_attribution,
    drive_uds_bundle,
    drive_uds_leg,
    run_https_leg,
)

# nan-022 matrix path (ADR-001/ADR-002) — the single authoritative enumeration, the
# drift guard, the classifier/roll-up, the transport-health preflight + token-guarded
# bundle ingest, and the evidence-table / roll-up-assertion support.
#
# IMPORT ORDER IS LOAD-BEARING (Stage-3c fix; see product/features/nan-022/testing/RISK-COVERAGE-REPORT.md): `parity_comparator` MUST be imported BEFORE
# `from harness.parity_dimensions import DIMENSIONS`. K2's import triggers
# `bind_comparators`, which REBINDS the module-global `DIMENSIONS` (a new tuple, not an
# in-place mutation). A `from ... import DIMENSIONS` that runs first captures the stale
# string-bound tuple, and the drift guard then sees `comparator='RetrievalComparator'`
# (a str, not a class). Mirrors the safe ordering in test_https_uds_parity_matrix.py.
from harness.parity_comparator import assert_comparator_contract
from harness.parity_dimensions import DIMENSIONS
from harness.parity_outcome import classify_dimension, rollup
from harness.transport_health import (
    DEFAULT_CONNECT_DEADLINE_S,
    DEFAULT_IDLE_DEADLINE_S,
    InfraError,
    load_https_bundle,
    preflight_leg,
)
from harness.parity_matrix_support import (
    assert_rollup,
    emit_infra_and_fail,
    evidence_table,
)


# ===========================================================================
# The single orchestrator entrypoint (ADR-001 pytest-as-orchestrator).
# ===========================================================================


@pytest.mark.integration
@pytest.mark.parity
def test_https_uds_parity(daemon_server, tmp_path):
    """Drive BOTH legs in ONE execution and assert MetricVector parity (AC-04).

    UDS leg in-process; HTTPS leg via the smoke shell-out; ingest the HTTPS vector
    via the stale-token-guarded `load_https_vector`; run the C4 comparator
    field-for-field MODULO the closed D-5 set; emit BOTH raw vectors + the per-field
    table (the first-live-run evidence record, ADR-003 #5293). A missing leg ERRORS —
    never a vacuous pass.
    """
    workload = default_workload()
    workload.validate()
    store_dir = daemon_server["store_dir"]
    run_token = workload.session_id  # the single stable correlation token (R-03)

    sandbox = tmp_path / "sandbox"
    sandbox.mkdir(parents=True, exist_ok=True)
    https_out = sandbox / "https_metric_vector.json"
    assert not https_out.exists(), "stale HTTPS out-file present at start (R-03 guard)"

    manifest_path = workload.write_manifest(sandbox / "parity_workload.json")

    # ---- UDS leg (this process) ----
    uds = UnimatrixUdsClient(daemon_server["mcp_socket_path"], timeout=30.0)
    uds.connect()
    try:
        mv_uds = drive_uds_leg(uds, daemon_server["socket_path"], workload, store_dir)
    finally:
        uds.disconnect()

    # ---- derived-attribution assertion on the UDS leg (AC-03) ----
    assert_derived_attribution(workload.feature_cycle, store_dir)

    # ---- HTTPS leg (shell-out to C1+C2) ----
    run_https_leg(
        manifest_path=manifest_path,
        run_token=run_token,
        https_out=https_out,
        sandbox=sandbox,
    )
    # Ingest with the stale-token guard (missing/stale → ERROR, never empty — R-03).
    mv_https = load_https_vector(https_out, run_token)

    # ---- first-live-run field-by-field evidence (ADR-003 #5293) ----
    record = field_by_field_record(mv_https, mv_uds, run_token=run_token)
    write_field_record(record, sandbox / f"field_record_{run_token}.json")

    # ---- comparator (C4) — field-for-field modulo D-5; non-empty on both ----
    diffs = compare_metric_vectors(mv_https, mv_uds)
    assert diffs == [], f"unexpected parity diffs survived comparison: {diffs}"


@pytest.mark.integration
def test_c3_uds_leg_live_review_non_empty_and_attributed(daemon_server):
    """FR-5 / R-06 / AC-03 (live): drive the UDS leg against a real daemon; the
    review MetricVector is NON-EMPTY after the barrier and every observation is
    attributed `topic_signal == feature` — no HTTPS leg needed. Also exercises the
    live `_extract_metric_vector` + comparator non-empty precondition on real daemon
    output (self-parity holds by construction for the identical vector)."""
    from harness.metric_comparator import assert_non_empty

    workload = default_workload()
    store_dir = daemon_server["store_dir"]

    uds = UnimatrixUdsClient(daemon_server["mcp_socket_path"], timeout=30.0)
    uds.connect()
    try:
        mv_uds = drive_uds_leg(uds, daemon_server["socket_path"], workload, store_dir)
    finally:
        uds.disconnect()

    # Non-empty AFTER the barrier (R-06): real counts, not a believable 0.
    assert_non_empty(mv_uds, "UDS")
    assert mv_uds["universal"]["total_tool_calls"] > 0
    assert mv_uds["phases"], "phases must be populated"

    # Derived attribution (AC-03): topic_signal == feature EXACTLY, never seeded.
    assert_derived_attribution(workload.feature_cycle, store_dir)

    # The live vector compares to ITSELF with zero diffs — proving the comparator
    # runs cleanly over real daemon output (the HTTPS leg substitutes its own vector
    # for mv_https in the full orchestrator; here we self-compare).
    assert compare_metric_vectors(mv_uds, mv_uds) == []


# ===========================================================================
# The nan-022 parity-MATRIX orchestrator (ADR-001 — drive both legs once, classify
# every dimension, roll up, emit the evidence table). Cumulative sibling of the
# nan-021 MetricVector test above (which stays UNCHANGED — AC-11).
# ===========================================================================


@pytest.mark.integration
@pytest.mark.parity
def test_https_uds_parity_matrix(daemon_server, tmp_path):
    """Drive BOTH legs in ONE execution and assert the six-dimension parity matrix
    (AC-01..08). UDS leg in-process via `drive_uds_bundle` → dimension bundle; HTTPS
    leg via the smoke shell-out → token-guarded `load_https_bundle`; classify each
    dimension (INFRA→INTRA→PARITY); roll up; emit the evidence table keyed by the run
    token. A missing leg / capture / stale bundle ERRORS (distinct INFRA exit) — never
    a vacuous pass.
    """
    # ---- 0. drift guard BEFORE any drive (off-Docker discipline, fails fast) ----
    assert_comparator_contract(DIMENSIONS)

    # ---- 1. ONE workload / ONE identity / ONE token ----
    workload = default_workload()
    workload.validate()
    run_token = workload.session_id  # the SINGLE correlation token (R-03/R-13)
    store_dir = daemon_server["store_dir"]
    sandbox = tmp_path / "sandbox"
    sandbox.mkdir(parents=True, exist_ok=True)
    https_out = sandbox / "https_dimension_bundle.json"
    assert not https_out.exists(), "stale HTTPS out-file present at start (R-12 guard)"
    manifest_path = workload.write_manifest(sandbox / "parity_workload.json")

    # ---- 2. UDS leg (in-process), preflight first (defense-in-depth) ----
    try:
        _preflight_uds(daemon_server)  # K5; InfraError → distinct INFRA, never RED
    except InfraError as e:
        emit_infra_and_fail(e, run_token)

    uds = UnimatrixUdsClient(daemon_server["mcp_socket_path"], timeout=30.0)
    uds.connect()
    try:
        bundle_uds = drive_uds_bundle(
            uds, daemon_server["socket_path"], workload, store_dir
        )
    finally:
        uds.disconnect()
    assert_derived_attribution(workload.feature_cycle, store_dir)  # AC-03 (UDS leg)

    # ---- 3. HTTPS leg (shell-out), preflight first; token-guarded never-empty ingest ----
    try:
        _preflight_https(daemon_server)
        run_https_leg(
            manifest_path=manifest_path,
            run_token=run_token,
            https_out=https_out,
            sandbox=sandbox,
        )
        # token-guarded; missing/stale/null/empty → InfraError, never empty-pass.
        bundle_https = load_https_bundle(https_out, run_token)
    except InfraError as e:
        emit_infra_and_fail(e, run_token)

    # ---- 4. classify every dimension (INFRA → INTRA → PARITY) ----
    results = _classify_matrix(bundle_uds, bundle_https)

    # ---- 5. evidence table keyed by run_token + per-dimension evidence records ----
    table = evidence_table(results, run_token)
    write_field_record(table, sandbox / f"parity_matrix_{run_token}.json")
    _emit_evidence_records(bundle_uds, bundle_https, run_token, sandbox)

    # ---- 6. roll up + assert (§4) — GREEN iff every dimension PARITY-PASS ----
    verdict, exit_code = rollup(results)
    assert_rollup(verdict, exit_code, results, table)


# ---------------------------------------------------------------------------
# Matrix orchestrator helpers (sequencing only — no per-leg parity assertion).
# ---------------------------------------------------------------------------


def _classify_matrix(bundle_uds: dict, bundle_https: dict) -> list:
    """Classify EVERY dimension by iterating `DIMENSIONS` (the single enumeration —
    no hand-list). A missing capture_key in either bundle surfaces as INFRA via the
    classifier's emptiness guard (never an empty-pass)."""
    results = []
    for dim in DIMENSIONS:
        cap_uds = bundle_uds.get(dim.capture_key)
        cap_https = bundle_https.get(dim.capture_key)
        results.append(classify_dimension(dim, cap_uds, cap_https))
    return results


def _emit_evidence_records(
    bundle_uds: dict, bundle_https: dict, run_token: str, sandbox
) -> None:
    """Emit the per-dimension first-live-run evidence record (ADR-003 discipline,
    generalized per dimension) for each dimension whose captures are present and
    non-null on both legs. A null/absent capture is left to the classifier's INFRA
    path (the table records it) — the evidence record is a best-effort artifact, not
    a gate, so it never raises on a missing/null capture."""
    for dim in DIMENSIONS:
        h = bundle_https.get(dim.capture_key)
        u = bundle_uds.get(dim.capture_key)
        if h is None or u is None:
            continue
        try:
            rec = dim.comparator().evidence_record(h, u, run_token=run_token)
        except Exception:  # noqa: BLE001 — evidence is non-gating; INFRA is in the table
            continue
        write_field_record(rec, sandbox / f"evidence_{dim.id}_{run_token}.json")


def _preflight_uds(daemon_server: dict) -> None:
    """Bounded connect + liveness reachability probe of the UDS legs (the MCP + hook
    sockets) BEFORE driving — defense-in-depth so a half-open hang surfaces as INFRA,
    never an unbounded hang / parity RED (R-02). Reuses the SHIPPED clients for the
    liveness proof; introduces NO new transport path (C-2).

    LIVENESS MODEL (Stage-3c first-live-run fix — Stage-3c fix; see product/features/nan-022/testing/RISK-COVERAGE-REPORT.md): both UDS sockets are
    REQUEST-DRIVEN — the MCP server replies only to a valid JSON-RPC request and the
    hook server only to a length-prefixed hook op; NEITHER echoes an unsolicited byte.
    The generic `uds_socket_leg` nudge (`sendall(b"\\n"); recv(1)`) therefore BLOCKS on
    a perfectly healthy daemon and false-classifies it as a #839 half-open hang. The
    correct, C-2-honouring liveness proof is the shipped client's own handshake:
      * MCP socket  — `UnimatrixUdsClient.connect()` completes the real MCP `initialize`
        handshake; a genuine reply proves liveness, a half-open server never completes
        it and the client's bounded socket timeout trips → InfraError (true half-open).
      * hook socket — request-driven, no handshake-on-connect; a clean bounded
        `connect()` proves the accept loop is alive (the #839 half-open class the guard
        targets is the HTTPS leg, deferred to `_preflight_https` / the smoke ceiling).
    """
    from harness.transport_health import uds_socket_leg

    # MCP leg: liveness = the shipped client's real initialize handshake (true reply).
    def _mcp_connect(deadline_s: float) -> None:
        c = UnimatrixUdsClient(daemon_server["mcp_socket_path"], timeout=deadline_s)
        c.connect()  # raises on connect/handshake failure within the bounded timeout
        c.disconnect()

    def _mcp_liveness(deadline_s: float) -> bool:
        # connect() above already completed the initialize handshake (a real reply);
        # re-proving here would double-handshake. A completed handshake IS liveness.
        return True

    preflight_leg(
        "uds-mcp",
        connect_deadline_s=DEFAULT_CONNECT_DEADLINE_S,
        idle_deadline_s=DEFAULT_IDLE_DEADLINE_S,
        connect_probe=_mcp_connect,
        liveness_probe=_mcp_liveness,
    )

    # Hook leg: request-driven; a clean bounded connect proves the listener is alive.
    def _hook_connect(deadline_s: float) -> None:
        import socket as _socket

        s = _socket.socket(_socket.AF_UNIX, _socket.SOCK_STREAM)
        s.settimeout(deadline_s)
        try:
            s.connect(str(daemon_server["socket_path"]))
        finally:
            s.close()

    preflight_leg(
        "uds-hook",
        connect_deadline_s=DEFAULT_CONNECT_DEADLINE_S,
        idle_deadline_s=DEFAULT_IDLE_DEADLINE_S,
        connect_probe=_hook_connect,
        liveness_probe=lambda _d: True,
    )


def _preflight_https(daemon_server: dict) -> None:
    """Bounded reachability probe of the HTTPS leg BEFORE the smoke shell-out — a
    defense-in-depth name for a half-open hang (R-02). The live smoke owns the TLS /
    cert posture and its own outer ceiling (`HTTPS_SMOKE_TIMEOUT_S`); K5 here only adds
    a bounded PRE-DRIVE reachability classification so a HANG is NAMED as INFRA, not a
    timeout-as-RED. When no HTTPS endpoint is exposed to the orchestrator (the smoke
    runs the container internally), this preflight is a no-op — the smoke's own
    run_smoke_gate exit-code truth table is the binding HTTPS health gate."""
    # The orchestrator does not own the container's TLS endpoint (the smoke does); the
    # binding HTTPS reachability gate is run_smoke_gate inside run_https_leg. No new
    # transport/cert path is introduced here (C-2) — see the report's flag.
    return None


# ===========================================================================
# C3 structural / contract tests (no live HTTPS leg required).
# ===========================================================================


def test_c3_drives_same_manifest_object():
    """FR-5 / R-09: the UDS leg drives the SAME C4 manifest object — not a hand-
    written parallel script. drive_uds_leg takes the manifest; it does not author
    its own tool-call list."""
    import inspect

    src = inspect.getsource(drive_uds_leg)
    # It iterates the passed workload's tool_calls; it never hard-codes a list.
    assert "workload.tool_calls" in src
    assert "workload.session_id" in src
    # The single source of truth is C4's default_workload — used by the orchestrator.
    wl = default_workload()
    assert wl.tool_calls, "manifest must declare tool calls"


def test_c3_same_session_identity_as_https():
    """FR-4 / SR-05 / R-09: the UDS-leg session identity IS the manifest's stable
    session_id, which is ALSO the run-correlation token the HTTPS leg receives — so
    both legs use the SAME value by construction (one manifest)."""
    import inspect

    # The orchestrator threads wl.session_id as both the UDS identity and run_token.
    orch = inspect.getsource(test_https_uds_parity)
    assert "run_token = workload.session_id" in orch
    # drive_uds_leg derives the hook session id from the SAME field.
    drv = inspect.getsource(drive_uds_leg)
    assert "sid = workload.session_id" in drv


def test_c3_uses_existing_clients_not_fork():
    """AC-07: the leg uses the EXISTING UnimatrixUdsClient / UnimatrixHookClient — no
    net-new UDS spawn or hook-IPC path. Verified by the imported symbols."""
    from harness import uds_client, hook_client

    assert UnimatrixUdsClient is uds_client.UnimatrixUdsClient
    assert UnimatrixHookClient is hook_client.UnimatrixHookClient
    # The required tool/hook surfaces exist on the existing clients (not re-authored).
    for m in ("connect", "disconnect", "context_cycle", "context_cycle_review"):
        assert hasattr(UnimatrixUdsClient, m)
    # The C3 leg drives observes through the live-wire HookRequest methods (the
    # nan-021 extension of the existing client — same framing/transport, no fork).
    for m in (
        "session_register",
        "session_close",
        "record_event",
        "record_cycle_start",
        "record_cycle_stop",
        "record_pre_tool_use",
        "record_post_tool_use",
    ):
        assert hasattr(UnimatrixHookClient, m)


def test_c3_no_seed_site_reachable():
    """AC-03 (static, no-seed): NO forbidden seed site is reachable from THIS
    orchestrator path or the leg drivers (the UDS leg lives where the SQL seed
    helpers sit in a sibling suite, so this audit is load-bearing here). Reuses C4's
    audit over this module + the leg module + the comparator."""
    from harness import metric_comparator

    pw.assert_no_seed_reachable(
        __file__, parity_legs.__file__, metric_comparator.__file__
    )


def test_c3_uds_barrier_before_review_symmetry():
    """R-06: the UDS leg runs the SAME shared C4 barrier helper (parameterized by leg)
    AFTER cycle_stop and BEFORE context_cycle_review — it is NOT a hand-written
    UDS-only wait. An asymmetric barrier self-induces divergence."""
    import inspect

    src = inspect.getsource(drive_uds_leg)
    assert "durability_barrier(" in src
    assert 'leg="UDS"' in src
    stop_at = src.index("record_cycle_stop(")
    barrier_at = src.index("durability_barrier(")
    review_at = src.index("context_cycle_review(")
    assert stop_at < barrier_at < review_at, (
        "barrier must run AFTER cycle_stop and BEFORE cycle_review (R-06 symmetry)"
    )
    # It is the SAME callable C4 exposes (no duplicate).
    assert durability_barrier is pw.durability_barrier


def test_c3_runs_in_same_pytest_invocation():
    """R-03 / D-6: the orchestrator owns BOTH legs in ONE invocation — the UDS
    MetricVector is produced in-process and the HTTPS vector is ingested via the
    token-guarded out-file from THIS run (not a captured golden / prior run)."""
    import inspect

    src = inspect.getsource(test_https_uds_parity)
    assert "drive_uds_leg(" in src  # UDS leg in-process
    assert "run_https_leg(" in src  # HTTPS leg shelled out in the same call
    assert "load_https_vector(https_out, run_token)" in src  # token-guarded ingest
    assert "compare_metric_vectors(" in src  # both compared in one execution


def test_c3_missing_https_leg_errors_never_empty(tmp_path):
    """R-03: a missing HTTPS out-file is an ERROR (load_https_vector raises), never an
    empty/vacuous compare — proven against the C4 ingestion contract."""
    with pytest.raises(FileNotFoundError):
        load_https_vector(tmp_path / "absent.json", "run-x")


@pytest.mark.parity
def test_c3_orchestrator_seam_with_fixture_https_vector(tmp_path):
    """Off-Docker WIRING PROOF (nan-019 #5258 stub-drive precedent): the orchestration
    + token-guarded ingestion + first-run record + comparator wire together end-to-end
    against a contract-shaped fixture HTTPS vector. Proves the seam Stage 3c plugs the
    LIVE HTTPS leg into — without needing Docker here."""
    workload = default_workload()
    run_token = workload.session_id

    mv = _fixture_metric_vector()
    https_out = tmp_path / "https_metric_vector.json"
    https_out.write_text(
        json.dumps({"run_token": run_token, "metric_vector": mv}), encoding="utf-8"
    )

    mv_https = load_https_vector(https_out, run_token)
    record = field_by_field_record(mv_https, mv, run_token=run_token)
    p = write_field_record(record, tmp_path / f"field_record_{run_token}.json")
    assert json.loads(p.read_text())["run_token"] == run_token

    diffs = compare_metric_vectors(mv_https, mv)
    assert diffs == [], f"fixture-seam parity must hold by construction: {diffs}"


def test_c3_seam_rejects_stale_token(tmp_path):
    """R-03: a prior-tag HTTPS out-file (token mismatch) is REJECTED by the seam,
    never ingested — the orchestrator cannot compare a stale vector."""
    https_out = tmp_path / "https_metric_vector.json"
    https_out.write_text(
        json.dumps({"run_token": "OLD-tag", "metric_vector": _fixture_metric_vector()}),
        encoding="utf-8",
    )
    with pytest.raises(ValueError):
        load_https_vector(https_out, default_workload().session_id)


def test_c3_shared_phase_constant():
    """The phase BOTH legs declare is a single shared constant (symmetry) — an
    asymmetric phase would bucket observations differently and self-induce a phases
    divergence."""
    assert PARITY_PHASE
    src_uds = __import__("inspect").getsource(drive_uds_leg)
    assert "PARITY_PHASE" in src_uds  # the UDS leg uses the shared constant


# ===========================================================================
# Helpers (test-only)
# ===========================================================================


def _fixture_metric_vector() -> dict:
    """A contract-shaped MetricVector (the comparator's input surface) for the
    off-Docker wiring proof. All 21 UniversalMetrics fields present; structural fields
    non-zero so the non-empty precondition holds."""
    from harness.metric_comparator import UNIVERSAL_FIELDS

    universal = {f: 0 for f in UNIVERSAL_FIELDS}
    universal.update(total_tool_calls=3, total_duration_secs=5, session_count=1)
    return {
        "computed_at": 1700000000,
        "universal": universal,
        "phases": {"delivery": {"duration_secs": 5, "tool_call_count": 3}},
        "domain_metrics": {},
    }
