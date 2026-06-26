"""ORCH support — evidence-table emit + roll-up assertion + the off-Docker
fixture dimension-bundle (nan-022 / #837 / ADR-001 / ADR-002 / AC-08).

Split out of `suites/test_https_uds_parity.py` (≤500-line / single-responsibility
rule, the nan-021 `metric_comparator` lib-split precedent): the orchestrator owns
SEQUENCING (preflight → both legs → token-guarded ingest → classify → roll up →
emit), this module owns the table SHAPE, the roll-up assertion, the distinct
INFRA exit, and the contract-shaped fixture bundle the off-Docker seam test round-
trips (R-09 sc.3 / R-10 sc.1) — so the live suite file stays focused on the drive.

Pure-Python, stdlib-only, OFF-Docker importable (no live daemon). TEST-ONLY; no
production-code diff (NFR-1/NFR-2/AC-11).

The evidence table IS the C0 proof artifact (AC-12). This feature does NOT flip C0
— an authorized session reads the table and performs the flip.
"""

from __future__ import annotations

from typing import Any

from harness.parity_dimensions import DIMENSIONS, dimension_by_id
from harness.parity_outcome import DimensionResult, Outcome, rollup
from harness.transport_health import EXIT_INFRA, InfraError


# ===========================================================================
# Evidence table — the per-dimension PASS/FAIL artifact keyed by run_token (AC-08).
# ===========================================================================


def evidence_table(results: list[DimensionResult], run_token: str) -> dict:
    """Build the per-dimension evidence table keyed by the run-correlation token.

    The table is the C0 proof artifact (AC-12): one row per dimension carrying its
    outcome, the `blocks_c0_proof` flag (from the single-source registry — never a
    hand-list), the human-readable detail, and any diffs. INTRA-NONDET dims are
    additionally surfaced under `intra_nondeterminism` (routed to GH#746, does NOT
    redden); D5 measurability call-outs under `documented_exceptions` (honest, never
    a vacuous pass). The roll-up verdict + exit code are embedded so the table alone
    tells the flip session GREEN / RED / ERROR.
    """
    verdict, exit_code = rollup(results)
    rows: list[dict] = []
    intra: list[str] = []
    documented: list[dict] = []
    for r in results:
        # The registry is the single source of `blocks_c0_proof` (no hand-list).
        try:
            blocks = dimension_by_id(r.dimension).blocks_c0_proof
        except KeyError:
            blocks = True  # an orphan id is conservatively treated as blocking.
        rows.append(
            {
                "dimension": r.dimension,
                "outcome": r.outcome.value,
                "blocks_c0_proof": blocks,
                "detail": r.detail,
                "diffs": [list(d) for d in r.diffs],
            }
        )
        if r.outcome == Outcome.INTRA_TRANSPORT_NONDETERMINISM:
            intra.append(r.dimension)
        # A D5 documented measurability limitation surfaces as INFRA-ERROR with a
        # MEASURABILITY-flagged detail — record it honestly (never rounded up).
        if r.outcome == Outcome.INFRA_ERROR and "MEASURABILITY" in r.detail.upper():
            documented.append({"dimension": r.dimension, "detail": r.detail})
    return {
        "run_token": run_token,
        "dimensions": rows,
        "verdict": verdict,
        "exit_code": exit_code,
        "intra_nondeterminism": intra,  # routed to GH#746; does NOT redden the gate
        "documented_exceptions": documented,  # D5 host-side gap call-outs (honest)
    }


# ===========================================================================
# Roll-up assertion — fail the live test LOUD on RED / ERROR with the table.
# ===========================================================================


def assert_rollup(
    verdict: str, exit_code: int, results: list[DimensionResult], table: dict
) -> None:
    """Assert the matrix roll-up is GREEN; fail LOUD with the evidence table on
    RED (PARITY-FAIL) or ERROR (INFRA-ERROR), per ADR-002 §4 disposition (C-4/C-8/
    AC-10).

      * GREEN  → every blocks_c0_proof dimension is PARITY-PASS; pass.
      * RED    → a real cross-transport divergence; the test FAILS RED, the diffs
                 name the divergent field, disposition is "file a NEW GH bug, fix
                 NOT absorbed" — the implementer/tester never widens an exclusion.
      * ERROR  → INFRA-ERROR (transport-health / ingest / a D5 documented host-side
                 gap); a DISTINCT failure, never read as a parity RED. A D5
                 documented-exception is reported HONESTLY (never fully-measured).

    INTRA-NONDET rows are recorded in the table and do NOT redden — they are routed
    to a separately-filed GH#746 bug.
    """
    if verdict == "GREEN" and exit_code == 0:
        return

    fails = [r for r in results if r.outcome == Outcome.PARITY_FAIL]
    infra = [r for r in results if r.outcome == Outcome.INFRA_ERROR]

    import json as _json

    table_str = _json.dumps(table, indent=2, default=str)

    if infra:
        details = "; ".join(f"{r.dimension}: {r.detail}" for r in infra)
        raise AssertionError(
            f"parity matrix INFRA-ERROR (distinct exit {exit_code}={EXIT_INFRA}) — "
            f"NOT a parity RED; diagnose transport / honour the D5 documented gap. "
            f"{details}\n--- evidence table (run_token-keyed) ---\n{table_str}"
        )
    if fails:
        details = "; ".join(
            f"{r.dimension}: {r.detail} diffs={r.diffs}" for r in fails
        )
        raise AssertionError(
            f"parity matrix RED (exit {exit_code}) — REAL cross-transport divergence; "
            f"file a NEW GH bug, fix NOT absorbed (AC-10). {details}\n"
            f"--- evidence table (run_token-keyed) ---\n{table_str}"
        )
    # Defensive: a non-GREEN verdict with neither class present is itself a fault.
    raise AssertionError(
        f"parity matrix non-GREEN verdict {verdict!r} (exit {exit_code}) without a "
        f"PARITY-FAIL/INFRA-ERROR row — roll-up inconsistency.\n{table_str}"
    )


# ===========================================================================
# INFRA short-circuit — a preflight / ingest InfraError ends the run distinctly.
# ===========================================================================


def emit_infra_and_fail(exc: InfraError, run_token: str) -> None:
    """A preflight / ingest `InfraError` is a DISTINCT ERROR (never a parity RED,
    never a hang). Surface it loud with the run token so the lane discriminates a
    transport ERROR from a parity verdict. Raised as an AssertionError carrying the
    `EXIT_INFRA` class so the disposition is unambiguous (R-02 / C-8)."""
    raise AssertionError(
        f"parity matrix INFRA-ERROR (distinct class {EXIT_INFRA}) on run {run_token!r}: "
        f"[{exc.leg}] {exc.reason}"
        + (f" — {exc.detail}" if exc.detail else "")
        + " (transport-health / ingest; re-run / diagnose transport, never a parity RED)"
    )


# ===========================================================================
# Contract-shaped fixture dimension-bundle (off-Docker seam — R-09 sc.3 / R-10 sc.1).
# ===========================================================================
#
# A golden bundle that satisfies the on-disk cross-language contract (architecture §7.3
# / parity_bundle_contract.md) and clears every comparator's non-degenerate floor
# (STABLE_PREFIX_FLOOR=3) so the orchestrator's ingest → classify → table → roll-up
# round-trips to GREEN WITHOUT Docker — the seam Stage 3c plugs the live HTTPS leg into.


def _ranked_ids(n: int) -> list[int]:
    return list(range(1, n + 1))


def _ranked_scores(n: int) -> list[float]:
    return [round(1.0 - i * 0.01, 4) for i in range(n)]


def fixture_metric_vector() -> dict:
    """A contract-shaped MetricVector (the analytics comparator's input surface). All
    21 UniversalMetrics fields present; structural fields non-zero so the non-empty
    precondition holds (mirrors the committed nan-021 `_fixture_metric_vector`)."""
    from harness.metric_comparator import UNIVERSAL_FIELDS

    universal = {f: 0 for f in UNIVERSAL_FIELDS}
    universal.update(total_tool_calls=3, total_duration_secs=5, session_count=1)
    return {
        "computed_at": 1700000000,
        "universal": universal,
        "phases": {"delivery": {"duration_secs": 5, "tool_call_count": 3}},
        "domain_metrics": {},
    }


def fixture_dimension_bundle(*, feature_cycle: str = "nan-021") -> dict:
    """A contract-shaped dimension bundle keyed by every registry `capture_key`,
    each clearing its comparator's non-degenerate floor — round-trips to GREEN when
    BOTH legs supply it (the off-Docker seam proof). Keys iterate `DIMENSIONS`, so a
    registry change cannot leave the fixture stale (no hand-list)."""
    n = 5  # > STABLE_PREFIX_FLOOR (3) so retrieval/briefing are non-degenerate
    bundle: dict[str, Any] = {}
    for dim in DIMENSIONS:
        bundle[dim.capture_key] = _fixture_capture(dim.id, feature_cycle, n)
    return bundle


def _fixture_capture(dim_id: str, feature_cycle: str, n: int) -> dict:
    if dim_id == "retrieval":
        query = {
            "tool": "context_search",
            "args": {"query": "parity"},
            "result_ids": _ranked_ids(n),
            "scores": _ranked_scores(n),
        }
        return {"queries": [dict(query)], "capture_2": [dict(query)]}
    if dim_id == "behavioral":
        return {"topic_signals": [feature_cycle]}
    if dim_id == "analytics":
        return {
            "metric_vector": fixture_metric_vector(),
            "informs_edges": [10, 11],
            "phase_signal": {"delivery": {"tool_call_count": 3}},
        }
    if dim_id == "proactive":
        return {
            "briefing_ids": _ranked_ids(n),
            "briefing_scores": _ranked_scores(n),
            "injection_set": [1, 2, 3],
            "capture_2": {
                "briefing_ids": _ranked_ids(n),
                "briefing_scores": _ranked_scores(n),
            },
        }
    if dim_id == "precompact":
        # The honest first-live-run shape (ADR-006 / OQ-2): a documented host-side gap.
        # measurable=False → K4 records a DOCUMENTED-EXCEPTION (INFRA-ERROR call-out),
        # never a vacuous pass. The seam test asserts this is NOT counted GREEN.
        return {
            "restored_payload": None,
            "measurable": False,
            "host_side_gap": "documented host-side gap (fixture seam)",
        }
    raise InfraError("fixture", f"no fixture capture for dimension {dim_id!r}")


def fixture_measurable_precompact() -> dict:
    """A precompact capture that IS measurable (both legs) — used by the seam test to
    drive the D5 PARITY-PASS branch (when a future live drive proves measurability)."""
    payload = {"restored_entries": [1, 2, 3], "restoration_timestamp": 1700000000}
    return {
        "restored_payload": dict(payload),
        "measurable": True,
        "host_side_gap": None,
    }
