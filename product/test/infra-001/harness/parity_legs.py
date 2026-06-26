"""C3' — the two transport LEG drivers for the HTTPS-vs-UDS parity matrix.

EXTENDED IN PLACE, cumulative (nan-022 / #837 / ADR-001 / ADR-005). Started life as
the nan-021 single-`MetricVector` leg driver (`drive_uds_leg`); nan-022 ADDS
`drive_uds_bundle`, which drives the AUGMENTED workload once and returns the full
dimension-keyed BUNDLE (retrieval, behavioral, analytics, proactive, precompact),
routing each dimension to its correct wire surface and double-capturing
the intra-check dimensions. The nan-021 `drive_uds_leg` is KEPT BYTE-FOR-BYTE VERBATIM
so the committed `test_https_uds_parity` MetricVector path (and its source-inspection
tests) stay GREEN (AC-11 cumulative — removing/refactoring it churns a proven path);
the Wave-E ORCH sibling matrix test consumes `drive_uds_bundle`. `drive_uds_bundle`
COMPOSES `drive_uds_leg` (seed corpus → `drive_uds_leg` drives the ONE #5298 11-frame
cycle + the ONE shared barrier + returns the analytics MetricVector) then layers the
per-dimension captures on top — there is NO second cycle and NO second barrier.

CONSUMES the C4' contract (`parity_workload`) and the K1 registry
(`parity_dimensions`) verbatim — it does NOT redefine the manifest, the durability
barrier, the comparators, the stale-token guard, or the dimension enumeration.
EXTENDS the existing harness clients (`UnimatrixUdsClient`, `UnimatrixHookClient`);
no new transport / spawn / framing path.

Contains:
  * `drive_uds_leg`        — nan-021 VERBATIM: drive the manifest over local hook IPC
                              + MCP UDS, apply the symmetric durability barrier, return
                              the parsed MetricVector (FR-5 / FR-10 / R-06).
  * `drive_uds_bundle`     — nan-022: compose `drive_uds_leg` then return the dimension
                              BUNDLE keyed by `Dimension.capture_key`; per-dimension
                              wire-surface routing + double-capture for intra dims
                              (FR-12 / R-03).
  * `assert_derived_attribution` — AC-03 (consumed verbatim).
  * `run_https_leg`        — shell out to the smoke's C1+C2 HTTPS gate (consumed verbatim).
  * `PARITY_PHASE`         — the single phase BOTH legs declare (shared contract).

Per-dimension CAPTURE logic lives in `parity_legs_capture.py` (≤500-line split); this
module owns the DRIVE (the #5298 11-frame observe cycle + the barrier in
`drive_uds_leg`, plus the per-dimension routing fan-out in `drive_uds_bundle`).

A note on the wire surface (why the live-wire hook methods exist): the daemon's
`HookRequest` enum routes `SessionRegister` / `SessionClose` / `RecordEvent` — NOT
the older `SessionStart`/`PostToolUse` `type` tags. Observations therefore land via
`record_*` (RecordEvent) frames; the cycle is declared via a `cycle_start`
RecordEvent (sets Declared attribution AND writes the cycle_events row the review's
primary col-024 path reads). A real tool call fires PreToolUse (which increments
`UniversalMetrics.total_tool_calls`) THEN PostToolUse; `MetricVector.phases` buckets
by a TaskCreate/TaskUpdate PreToolUse whose `tool_input.subject` carries a
`"{phase}: …"` prefix (metrics.rs::compute_phases).
"""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

import pytest

from harness import parity_workload as pw
from harness.parity_workload import (
    DurabilityTimeout,
    durability_barrier,
)
from harness.parity_dimensions import DIMENSIONS
from harness.uds_client import UnimatrixUdsClient
from harness.hook_client import UnimatrixHookClient
from harness import parity_legs_capture as cap


# The single phase BOTH legs declare in cycle_start so observations bucket into a
# phase (an un-phased cycle yields empty `phases`, failing the non-empty gate). Part
# of the shared driving contract — symmetric across legs (mirrors the manifest).
PARITY_PHASE = "delivery"

# Outer ceiling for the (real) HTTPS smoke shell-out. The live smoke owns its own
# internal gate timeouts; this prevents a wedged child from hanging the parity gate.
HTTPS_SMOKE_TIMEOUT_S = float(os.environ.get("UNIMATRIX_HTTPS_SMOKE_TIMEOUT_S", "600"))


# ===========================================================================
# UDS-leg driver (FR-5) — nan-021 VERBATIM. Drive the manifest, barrier, review.
# ===========================================================================


def drive_uds_leg(
    uds: UnimatrixUdsClient,
    hook_socket_path: str | Path,
    workload: pw.ParityWorkload,
    store_dir: str | Path,
    *,
    agent_id: str = "nan-021-uds-leg",
    hook_timeout: float = 30.0,
) -> dict:
    """Drive the C4 manifest over the local hook channel + MCP UDS and return the
    parsed MetricVector dict.

    Sequence (ADR-001 / FR-5 / FR-10), ALL under ONE stable session id (#832):
      SessionRegister(feature) → cycle_start(feature, phase) → phase-setting
      TaskCreate → ordered Pre+Post observes → cycle_stop → SessionClose →
      SYMMETRIC durability barrier → context_cycle_review.

    The cycle is declared over the SAME hook channel that carries the observes (one
    identity threaded through declaration + every observe — the #832 contract / R-09),
    NOT split across the MCP session. The barrier (the SHARED C4 helper, parameterized
    only by leg) gates the review so a never-landed observe race can never satisfy
    non-emptiness (R-06). The MCP UDS client is used only to READ the review
    (`RetrospectiveReport.metrics`, format=json).

    The hook IPC listener serves EXACTLY ONE framed message per connection
    (uds/listener.rs handle_connection — read header+body, respond, close), so each
    hook event opens a FRESH UnimatrixHookClient connection.

    nan-022 KEEPS this function byte-for-byte and layers the dimension BUNDLE on top via
    `drive_uds_bundle`, which COMPOSES this driver (same cycle, same barrier) so the
    committed orchestrator + its source-inspection tests stay green (AC-11).
    """
    sid = workload.session_id  # the #832 stable identity — SAME value as the HTTPS leg

    def _hook(send, label: str) -> None:
        # One short-lived connection per hook event (one-shot listener contract).
        with UnimatrixHookClient(hook_socket_path, timeout=hook_timeout) as h:
            resp = send(h)
            _assert_hook_ok(resp, label)

    # Register + declare the cycle so observations attribute to the feature (R-07):
    # SessionRegister sets the feature; cycle_start force-sets Declared + records the
    # cycle_events row. Both under the SAME stable session id.
    _hook(
        lambda h: h.session_register(
            sid, agent_role="tester", feature=workload.feature_cycle
        ),
        "SessionRegister",
    )
    # Declare the cycle WITH a phase so observations bucket into a phase. The phase is
    # part of the shared driving contract — BOTH legs declare the SAME phase.
    _hook(
        lambda h: h.record_cycle_start(sid, workload.feature_cycle, phase=PARITY_PHASE),
        "cycle_start",
    )

    # `MetricVector.phases` buckets observations by the LAST TaskCreate/TaskUpdate
    # PreToolUse whose `tool_input.subject` carries a `"{phase}: …"` prefix
    # (metrics.rs::compute_phases / extract_phase_name) — NOT by the observation
    # `phase` column. Emit one phase-setting TaskCreate so the manifest's tool calls
    # bucket into PARITY_PHASE (the SAME phase on both legs — shared driving contract).
    _hook(
        lambda h: h.record_pre_tool_use(
            sid,
            "TaskCreate",
            tool_input={"subject": f"{PARITY_PHASE}: drive the parity workload"},
        ),
        "PreToolUse(TaskCreate phase-set)",
    )

    for call in workload.tool_calls:
        if call.observe:
            # A real tool call fires PreToolUse THEN PostToolUse: the Pre increments
            # UniversalMetrics.total_tool_calls (metrics.rs), the Post carries the
            # response. The load-bearing Bash call's response_snippet carries the
            # feature-ID token; topic_signal is DERIVED over the wire (AC-03), not seeded.
            _hook(
                lambda h, c=call: h.record_pre_tool_use(sid, c.name, tool_input=c.args),
                f"PreToolUse({call.name})",
            )
            _hook(
                lambda h, c=call: h.record_post_tool_use(
                    sid,
                    c.name,
                    response_size=c.response_size,
                    response_snippet=c.response_snippet,
                    tool_input=c.args,
                ),
                f"PostToolUse({call.name})",
            )

    _hook(lambda h: h.record_cycle_stop(sid, workload.feature_cycle), "cycle_stop")
    _hook(
        lambda h: h.session_close(sid, outcome="completed", duration_secs=1),
        "SessionClose",
    )

    # SYMMETRIC durability barrier — SAME C4 helper / predicate / deadline as the
    # HTTPS leg (FR-10 / ADR-006). Timeout is a HARD failure, never an empty compare.
    try:
        durability_barrier(
            leg="UDS", expected=workload.expected_observe_count, store_dir=store_dir
        )
    except DurabilityTimeout as e:
        pytest.fail(str(e))

    review = uds.context_cycle_review(
        workload.feature_cycle, agent_id=agent_id, format="json"
    )
    return _extract_metric_vector(review, "UDS")


# ===========================================================================
# UDS-leg bundle driver (nan-022 FR-12 / ADR-001 / ADR-005) — drive once, bundle.
# ===========================================================================


def drive_uds_bundle(
    uds: UnimatrixUdsClient,
    hook_socket_path: str | Path,
    workload: pw.ParityWorkload,
    store_dir: str | Path,
    *,
    agent_id: str = "nan-022-uds-leg",
    hook_timeout: float = 30.0,
) -> dict:
    """Drive the augmented manifest over local hook IPC + MCP UDS and return the
    dimension BUNDLE keyed by `Dimension.capture_key`::

        { "retrieval": {...}, "behavioral": {...}, "analytics": {...},
          "proactive": {...}, "precompact": {...} }

    iterating `DIMENSIONS` (the single authoritative enumeration — no hand-list).
    ALL captures run under the ONE stable session id (#832). The barrier gates every
    DB-reading capture (R-04).

    Sequence (ADR-001 / ADR-005 / FR-5 / FR-10 / FR-12):
      PHASE 0  seed the corpus over MCP (`context_store`) so both legs rank the
               IDENTICAL corpus (CONTENT only — never a compared output; R-15).
      PHASE 1+2 COMPOSE `drive_uds_leg`: it drives the ONE #5298 11-frame observe
               cycle (byte-identical to the HTTPS leg — analytics cycle frames +
               behavioral attribution land here) AND applies the ONE SYMMETRIC
               durability barrier (the SHARED C4' helper, leg="UDS") AFTER cycle_stop.
               Every PHASE-3 DB-reading capture therefore runs AFTER the barrier
               (R-04) — there is NO second cycle and NO second barrier. The returned
               MetricVector is the analytics dimension's `metric_vector`.
      PHASE 3  per-dimension capture, routed by `Dimension.wire_surface`; intra-check
               dimensions (retrieval, proactive) are double-captured; dual-surface
               dimensions (analytics) fan out BOTH surfaces explicitly. A
               dimension routed to the wrong surface records nothing → empty capture →
               K4 INFRA-ERROR (C-9), never an empty-pass.
    """
    sid = workload.session_id  # the #832 stable identity — SAME value as the HTTPS leg

    # ---- PHASE 0: seed the corpus over MCP (content-only; R-15) ----
    _seed_corpus_uds(uds, workload)

    # ---- PHASE 1+2: drive the cycle + barrier via the committed driver (one cycle,
    #      one barrier). The returned MetricVector is the analytics capture; the
    #      barrier it applies gates every PHASE-3 DB read (R-04). ----
    metric_vector = drive_uds_leg(
        uds,
        hook_socket_path,
        workload,
        store_dir,
        agent_id=agent_id,
        hook_timeout=hook_timeout,
    )

    # ---- PHASE 3: per-dimension capture, routed by wire_surface (iterate DIMENSIONS) ----
    bundle: dict = {}
    for dim in DIMENSIONS:
        bundle[dim.capture_key] = _capture_dimension(
            dim,
            uds,
            hook_socket_path,
            workload,
            store_dir,
            sid,
            metric_vector=metric_vector,
            agent_id=agent_id,
            hook_timeout=hook_timeout,
        )
    return bundle


# ===========================================================================
# Bundle-phase helpers (seed + per-dimension capture routing).
# ===========================================================================


def _seed_corpus_uds(uds: UnimatrixUdsClient, workload: pw.ParityWorkload) -> None:
    """PHASE 0 — replay the workload's seed-corpus `context_store` calls over MCP UDS
    so the store is IDENTICALLY seeded on both legs (CONTENT only — never a compared
    output; R-15). Iterates `workload.seed_calls` (a VIEW over the single manifest)."""
    for call in workload.seed_calls:
        args = dict(call.args)
        content = args.pop("content", "")
        topic = args.pop("topic", "")
        category = args.pop("category", "")
        uds.context_store(content, topic, category, **cap._clean(args))


# Per-dimension capture + wire-surface routing live in `parity_legs_capture.py`
# (the routing fan-out + DRIVER_WIRE_SURFACES). `_capture_dimension` re-exported here
# under its private name so the driver and tests keep one import surface.
_capture_dimension = cap.capture_dimension
DRIVER_WIRE_SURFACES = cap.DRIVER_WIRE_SURFACES


# ===========================================================================
# Derived-attribution assertion (AC-03 / FR-6 / R-07) — derived, never seeded.
# (Consumed verbatim from nan-021 — extends to a capture both legs emit via D2.)
# ===========================================================================


def assert_derived_attribution(feature: str, store_dir: str | Path) -> None:
    """Assert every driven observation's topic_signal == feature EXACTLY (string-
    exact); `unattributed`/NULL is a HARD fail (the #832 near-miss guard, R-07).

    The column is read straight from the daemon's `observations` table in the
    per-slug store DIR — it must have been DERIVED by the server's attribution
    chain over the wire (declared branch), NOT injected by any seed site (AC-03).
    """
    import sqlite3

    db = Path(store_dir) / "unimatrix.db"
    assert db.is_file(), f"store db absent for attribution read: {db}"
    conn = sqlite3.connect(str(db))
    try:
        rows = conn.execute(
            "SELECT DISTINCT topic_signal FROM observations "
            "WHERE topic_signal IS NOT NULL"
        ).fetchall()
    finally:
        conn.close()

    signals = {r[0] for r in rows}
    assert signals, (
        f"no attributed observations found for {feature!r} — topic_signal must be "
        f"DERIVED over the wire (AC-03), not empty/unattributed"
    )
    assert "unattributed" not in signals, (
        f"observation attributed 'unattributed' — derived attribution failed "
        f"(R-07 near-miss guard); signals={sorted(signals)}"
    )
    assert signals == {feature}, (
        f"topic_signal must equal {feature!r} EXACTLY for every driven observation "
        f"(AC-03); got {sorted(signals)}"
    )


# ===========================================================================
# HTTPS leg shell-out (C1+C2) — single-execution seam (R-03). Consumed verbatim.
# ===========================================================================


def run_https_leg(
    *, manifest_path: Path, run_token: str, https_out: Path, sandbox: Path
) -> None:
    """Shell out to the smoke's C1+C2 gate to drive the HTTPS leg, passing the
    LOCKED contract env (the manifest, the run-correlation token, the out-file).
    A non-zero rc or a missing out-file is a HARD failure (ERROR, never skip / never
    compare empty — R-03). Stage 3c plugs the live smoke in via UNIMATRIX_HTTPS_SMOKE;
    absent it, the orchestrator SKIPs (the off-Docker seam proof covers wiring)."""
    smoke = os.environ.get("UNIMATRIX_HTTPS_SMOKE")
    if not smoke:
        pytest.skip(
            "live HTTPS smoke not wired (UNIMATRIX_HTTPS_SMOKE unset) — the true "
            "cross-leg live run is Stage 3c; see test_c3_orchestrator_seam_with_"
            "fixture_https_vector for the off-Docker wiring proof"
        )

    env = os.environ.copy()
    env.update(
        {
            "MANIFEST_PATH": str(manifest_path),
            "RUN_TOKEN": run_token,
            "HTTPS_VECTOR_OUT": str(https_out),
            "SANDBOX": str(sandbox),
        }
    )
    proc = subprocess.run(
        [smoke],
        env=env,
        capture_output=True,
        text=True,
        timeout=HTTPS_SMOKE_TIMEOUT_S,
    )
    if proc.returncode != 0:
        tail = "\n".join((proc.stderr or "").splitlines()[-30:])
        pytest.fail(
            f"HTTPS smoke leg failed rc={proc.returncode} "
            f"(ERROR, never skip)\n--- smoke stderr tail ---\n{tail}"
        )
    if not https_out.is_file():
        pytest.fail(
            f"HTTPS vector out-file missing after smoke success: {https_out} "
            f"(not live-vs-live — R-03)"
        )


# ===========================================================================
# Helpers (consumed verbatim from nan-021)
# ===========================================================================


def _assert_hook_ok(resp, label: str) -> None:
    """Assert a hook IPC response is not an Error frame.

    `UnimatrixHookClient._request` returns the deserialized `HookResponse` WITHOUT
    raising on `type="Error"` — so a frame the daemon rejected (e.g. a stale wire
    variant) would otherwise pass silently and record NOTHING. This fails LOUD with
    the server's error message so a wire mismatch surfaces immediately."""
    raw = getattr(resp, "raw", {}) or {}
    if raw.get("type") == "Error":
        pytest.fail(
            f"{label} rejected by hook daemon: "
            f"code={raw.get('code')} message={raw.get('message')!r}"
        )


def _extract_metric_vector(resp: dict, leg: str) -> dict:
    """Extract the MetricVector dict (RetrospectiveReport.metrics) from a UDS
    context_cycle_review result. The result text is the JSON report (format=json)."""
    text = ""
    content = resp.get("content") if isinstance(resp, dict) else None
    if isinstance(content, list) and content:
        first = content[0]
        if isinstance(first, dict) and first.get("type") == "text":
            text = first.get("text", "")
    if isinstance(resp, dict) and resp.get("isError"):
        pytest.fail(f"{leg} context_cycle_review returned error: {text[:400]}")
    try:
        report = json.loads(text)
    except (json.JSONDecodeError, TypeError) as e:
        pytest.fail(f"{leg} review JSON unparseable: {e}\ntext head: {text[:400]}")
    mv = report.get("metrics")
    if not isinstance(mv, dict):
        pytest.fail(
            f"{leg} review has no 'metrics' MetricVector dict "
            f"(keys: {sorted(report) if isinstance(report, dict) else type(report)})"
        )
    return mv
