"""K4 unit tests — outcome-class model (classifier order + roll-up truth table).

Tier A (off-Docker unit): synthetic capture dicts + stub Dimension/comparator, NO
Docker, NO daemon. Maps 1:1 to test-plan/parity_outcome.md and covers R-07 (High)
+ the roll-up half of R-02 (Critical defense-in-depth) + AC-08.

The most insidious false-GREEN is a real cross-leg divergence being reclassified
as INTRA-NONDET and silently dropped — that negative test
(`test_classify_dimension_two_intra_stable_legs_cross_divergent_is_parity_fail`)
is the load-bearing one.

Pure-Python, stdlib-only. ZERO production-code diff.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

import pytest

from harness.parity_comparator import ParityMismatch
from harness.parity_outcome import (
    EXIT_INFRA,
    EXIT_OK,
    EXIT_PARITY_FAIL,
    DimensionResult,
    Outcome,
    classify_dimension,
    intra_transport_stable,
    rollup,
)
from harness.ranking_tolerance import STABLE_PREFIX_FLOOR


# ---------------------------------------------------------------------------
# Stub Dimension + stub comparators (no real registry / no Docker)
# ---------------------------------------------------------------------------
@dataclass
class _StubDim:
    """A minimal stand-in for parity_dimensions.Dimension. `comparator` is a
    zero-arg callable returning a comparator instance (matching the real
    `dim.comparator()` call)."""

    id: str
    intra_transport_check: bool
    comparator: Any


class _CleanComparator:
    """compare() returns an empty diff list (parity clean)."""

    def compare(self, https: Any, uds: Any) -> list:
        return []


class _MismatchComparator:
    """compare() raises ParityMismatch with a single non-excluded diff."""

    DIFFS = [("field_x", "https_val", "uds_val")]

    def compare(self, https: Any, uds: Any) -> list:
        raise ParityMismatch(list(self.DIFFS))


class _RaisingComparator:
    """compare() raises a NON-ParityMismatch error if ever invoked — used to PROVE
    the comparator was not reached (INFRA/INTRA short-circuit before PARITY)."""

    def compare(self, https: Any, uds: Any) -> list:
        raise AssertionError("comparator was invoked but should have been skipped")


def _ranked(ids: list, scores: list | None = None) -> dict:
    return {"ids": ids, "scores": scores}


def _prefix_ids(n: int) -> list:
    """An ids list of length n >= floor (stable, non-degenerate)."""
    return [f"id{i}" for i in range(n)]


# ---------------------------------------------------------------------------
# Classifier order — INFRA -> INTRA -> PARITY (R-07 scenario 3)
# ---------------------------------------------------------------------------
def test_classify_dimension_infra_first_before_any_compare():
    """A missing (null) non-PreCompact capture -> INFRA_ERROR BEFORE the comparator
    or intra-check runs (the raising comparator never raises)."""
    dim = _StubDim("retrieval", intra_transport_check=True, comparator=_RaisingComparator)
    res = classify_dimension(dim, cap_uds=None, cap_https={"queries": [{"result_ids": _prefix_ids(3)}]})
    assert res.outcome == Outcome.INFRA_ERROR
    assert res.diffs == []


def test_classify_dimension_empty_capture_is_infra():
    """An empty (no queries) capture -> INFRA_ERROR, never empty-pass (C-9/R-03)."""
    dim = _StubDim("retrieval", intra_transport_check=True, comparator=_RaisingComparator)
    res = classify_dimension(dim, cap_uds={"queries": []}, cap_https={"queries": []})
    assert res.outcome == Outcome.INFRA_ERROR


def test_classify_dimension_intra_before_cross_compare():
    """An intra-check dimension whose one leg's two captures diverge within the
    stable prefix -> INTRA_TRANSPORT_NONDETERMINISM; the cross-leg comparator does
    NOT run (the raising comparator is never invoked)."""
    floor = STABLE_PREFIX_FLOOR
    stable_q = [{"result_ids": _prefix_ids(floor), "scores": None}]
    # capture_2 diverges at position 0 -> in-prefix divergence -> intra-unstable.
    divergent_q = [{"result_ids": ["X"] + _prefix_ids(floor)[1:], "scores": None}]
    cap_https = {"queries": stable_q, "capture_2": divergent_q}
    cap_uds = {"queries": stable_q, "capture_2": stable_q}
    dim = _StubDim("retrieval", intra_transport_check=True, comparator=_RaisingComparator)
    res = classify_dimension(dim, cap_uds=cap_uds, cap_https=cap_https)
    assert res.outcome == Outcome.INTRA_TRANSPORT_NONDETERMINISM


def test_classify_dimension_order_proven_explicitly():
    """A capture simultaneously infra-bad (null) AND would be intra-unstable AND
    cross-divergent -> INFRA_ERROR (the earliest applicable class wins). Proves the
    order is INFRA->INTRA->PARITY."""
    dim = _StubDim("retrieval", intra_transport_check=True, comparator=_MismatchComparator)
    # uds is null (infra-bad); https is intra-unstable AND would cross-diverge.
    res = classify_dimension(dim, cap_uds=None, cap_https={"queries": [{"result_ids": ["X"]}]})
    assert res.outcome == Outcome.INFRA_ERROR


# ---------------------------------------------------------------------------
# Cross-divergence must NEVER escape to INTRA (R-07 sc.3 — load-bearing)
# ---------------------------------------------------------------------------
def test_classify_dimension_two_intra_stable_legs_cross_divergent_is_parity_fail():
    """BOTH legs intra-stable (each leg's two captures agree) but the cross-leg
    comparator finds a non-excluded diff -> PARITY_FAIL, NEVER INTRA. THE single
    most important negative test in K4."""
    floor = STABLE_PREFIX_FLOOR
    q = [{"result_ids": _prefix_ids(floor), "scores": None}]
    # both legs self-stable: capture == capture_2 on each leg.
    cap_https = {"queries": q, "capture_2": q}
    cap_uds = {"queries": q, "capture_2": q}
    dim = _StubDim("retrieval", intra_transport_check=True, comparator=_MismatchComparator)
    res = classify_dimension(dim, cap_uds=cap_uds, cap_https=cap_https)
    assert res.outcome == Outcome.PARITY_FAIL
    assert res.outcome != Outcome.INTRA_TRANSPORT_NONDETERMINISM
    assert res.diffs == _MismatchComparator.DIFFS


def test_classify_dimension_one_leg_intra_unstable_classed_intra():
    """One leg intra-stable, the other intra-unstable -> INTRA (not a half-compare)."""
    floor = STABLE_PREFIX_FLOOR
    stable_q = [{"result_ids": _prefix_ids(floor), "scores": None}]
    unstable_q2 = [{"result_ids": ["X"] + _prefix_ids(floor)[1:], "scores": None}]
    cap_https = {"queries": stable_q, "capture_2": stable_q}       # stable
    cap_uds = {"queries": stable_q, "capture_2": unstable_q2}       # unstable
    dim = _StubDim("retrieval", intra_transport_check=True, comparator=_RaisingComparator)
    res = classify_dimension(dim, cap_uds=cap_uds, cap_https=cap_https)
    assert res.outcome == Outcome.INTRA_TRANSPORT_NONDETERMINISM


# ---------------------------------------------------------------------------
# intra_transport_stable (R-07 scenarios 1,2,4)
# ---------------------------------------------------------------------------
def test_intra_transport_stable_tail_churn_only_is_stable():
    """Two captures differing only in the tolerated tail (beyond the stable prefix)
    -> True (the leg is self-stable, proceeds to cross-compare)."""
    floor = STABLE_PREFIX_FLOOR
    a = _ranked(_prefix_ids(floor) + ["tailA"])
    b = _ranked(_prefix_ids(floor) + ["tailB"])
    assert intra_transport_stable(a, b, tolerance="ranked") is True


def test_intra_transport_stable_in_prefix_divergence_is_unstable():
    """Two captures differing WITHIN the stable prefix -> False (intra-unstable)."""
    a = _ranked(["a", "b", "c", "d"])
    b = _ranked(["a", "X", "c", "d"])  # divergence at position 1, prefix len 1 < floor
    assert intra_transport_stable(a, b, tolerance="ranked") is False


def test_intra_transport_stable_uses_k3_tolerance_single_sourced(monkeypatch):
    """The intra-diff uses the SAME ranking_tolerance.ranking_parity as the cross-leg
    compare — no second tolerance (SR-03/R-07 sc.4). Proven by patching the ONE
    callable and asserting it is the function the intra path calls."""
    import harness.parity_outcome as po

    called = {"hit": False}
    real = po.ranking_parity

    def _spy(*args, **kwargs):
        called["hit"] = True
        return real(*args, **kwargs)

    monkeypatch.setattr(po, "ranking_parity", _spy)
    intra_transport_stable(_ranked(["a", "b", "c"]), _ranked(["a", "b", "c"]), tolerance="ranked")
    assert called["hit"], "intra path must route through K3 ranking_parity (single source)"


# ---------------------------------------------------------------------------
# PARITY-PASS / PARITY-FAIL (happy / defect)
# ---------------------------------------------------------------------------
def test_classify_dimension_clean_modulo_excluded_is_parity_pass():
    """Two intra-stable captures, comparator clean -> PARITY_PASS, empty diffs."""
    floor = STABLE_PREFIX_FLOOR
    q = [{"result_ids": _prefix_ids(floor), "scores": None}]
    dim = _StubDim("retrieval", intra_transport_check=True, comparator=_CleanComparator)
    res = classify_dimension(dim, cap_uds={"queries": q, "capture_2": q}, cap_https={"queries": q, "capture_2": q})
    assert res.outcome == Outcome.PARITY_PASS
    assert res.diffs == []


def test_classify_dimension_non_excluded_diff_is_parity_fail():
    """A non-excluded diff -> PARITY_FAIL; diffs carries field + both values."""
    dim = _StubDim("behavioral", intra_transport_check=False, comparator=_MismatchComparator)
    cap = {"topic_signals": ["s1"]}
    res = classify_dimension(dim, cap_uds=cap, cap_https=cap)
    assert res.outcome == Outcome.PARITY_FAIL
    assert ("field_x", "https_val", "uds_val") in res.diffs
    assert "RED" in res.detail and "AC-10" in res.detail


# ---------------------------------------------------------------------------
# PreCompact (D5) measurability branch (ADR-006 / R-08)
# ---------------------------------------------------------------------------
def test_classify_precompact_measurable_false_is_infra_documented_exception():
    """D5 measurable=False + host_side_gap -> INFRA_ERROR DOCUMENTED-EXCEPTION,
    NOT a pass, NOT silently green (R-08 sc.1)."""
    dim = _StubDim("precompact", intra_transport_check=False, comparator=_RaisingComparator)
    cap = {"restored_payload": None, "measurable": False, "host_side_gap": "CC host not drivable"}
    res = classify_dimension(dim, cap_uds=cap, cap_https=cap)
    assert res.outcome == Outcome.INFRA_ERROR
    assert "DOCUMENTED MEASURABILITY LIMITATION" in res.detail
    assert "CC host not drivable" in res.detail


def test_classify_precompact_measurable_true_runs_parity():
    """D5 measurable=True + two payloads -> normal PARITY_PASS (R-08 sc.2)."""
    dim = _StubDim("precompact", intra_transport_check=False, comparator=_CleanComparator)
    cap = {"restored_payload": {"k": "v"}, "measurable": True, "host_side_gap": None}
    res = classify_dimension(dim, cap_uds=cap, cap_https=cap)
    assert res.outcome == Outcome.PARITY_PASS


def test_classify_precompact_null_capture_both_legs_measurable_true_is_infra():
    """measurable=True but null restored_payload -> empty capture -> INFRA (never pass)."""
    dim = _StubDim("precompact", intra_transport_check=False, comparator=_RaisingComparator)
    cap = {"restored_payload": None, "measurable": True, "host_side_gap": None}
    res = classify_dimension(dim, cap_uds=cap, cap_https=cap)
    assert res.outcome == Outcome.INFRA_ERROR


# ---------------------------------------------------------------------------
# Missing capture -> INFRA (R-09)
# ---------------------------------------------------------------------------
def test_classify_dimension_null_capture_non_d5_is_infra():
    """A null capture for a non-D5 dimension -> INFRA_ERROR (R-09 sc.2)."""
    dim = _StubDim("behavioral", intra_transport_check=False, comparator=_RaisingComparator)
    cap = {"topic_signals": ["s1", "s2"]}
    res = classify_dimension(dim, cap_uds=None, cap_https=cap)
    assert res.outcome == Outcome.INFRA_ERROR


def test_classify_intra_dimension_missing_capture_2_is_infra():
    """An intra-check dimension whose capture lacks the mandatory capture_2 ->
    INFRA_ERROR (the second capture is mandatory; its absence is a misroute, R-03)."""
    floor = STABLE_PREFIX_FLOOR
    q = [{"result_ids": _prefix_ids(floor), "scores": None}]
    cap = {"queries": q}  # no capture_2
    dim = _StubDim("retrieval", intra_transport_check=True, comparator=_RaisingComparator)
    res = classify_dimension(dim, cap_uds=cap, cap_https=cap)
    assert res.outcome == Outcome.INFRA_ERROR


# ---------------------------------------------------------------------------
# Determinism floor (NFR-6) — exact dims never invoke ranking_parity
# ---------------------------------------------------------------------------
def test_classify_dimension_exact_compare_for_non_ranking_dims(monkeypatch):
    """For a non-ranking dim (behavioral) the comparison is EXACT — the
    intra_transport_check=False dims never invoke ranking_parity (NFR-6)."""
    import harness.parity_outcome as po

    def _boom(*a, **k):
        raise AssertionError("ranking_parity must NOT be called for exact dims (NFR-6)")

    monkeypatch.setattr(po, "ranking_parity", _boom)
    dim = _StubDim("behavioral", intra_transport_check=False, comparator=_CleanComparator)
    cap = {"topic_signals": ["s1", "s2"]}
    res = classify_dimension(dim, cap_uds=cap, cap_https=cap)
    assert res.outcome == Outcome.PARITY_PASS


# ---------------------------------------------------------------------------
# Roll-up exit-code truth table (R-02 sc.3, AC-08)
# ---------------------------------------------------------------------------
def _r(outcome: Outcome) -> DimensionResult:
    return DimensionResult("d", outcome, [], "")


def test_rollup_green_iff_all_parity_pass():
    results = [_r(Outcome.PARITY_PASS) for _ in range(6)]
    assert rollup(results) == ("GREEN", EXIT_OK)


def test_rollup_any_parity_fail_is_red():
    results = [_r(Outcome.PARITY_PASS), _r(Outcome.PARITY_FAIL), _r(Outcome.PARITY_PASS)]
    verdict, code = rollup(results)
    assert verdict == "RED"
    assert code == EXIT_PARITY_FAIL
    assert code != 0


def test_rollup_infra_error_distinct_exit_not_parity_red():
    """One INFRA among passes -> ERROR exit (distinct code), not green, not parity
    RED (R-02: INFRA never counted as RED, never green)."""
    results = [_r(Outcome.PARITY_PASS), _r(Outcome.INFRA_ERROR), _r(Outcome.PARITY_PASS)]
    verdict, code = rollup(results)
    assert verdict == "ERROR"
    assert code == EXIT_INFRA
    assert code != EXIT_PARITY_FAIL
    assert code != EXIT_OK


def test_rollup_infra_takes_precedence_over_parity_fail():
    """INFRA_ERROR + PARITY_FAIL together -> ERROR (ERROR > RED precedence, §4)."""
    results = [_r(Outcome.PARITY_FAIL), _r(Outcome.INFRA_ERROR)]
    verdict, code = rollup(results)
    assert verdict == "ERROR"
    assert code == EXIT_INFRA


def test_rollup_intra_nondet_does_not_redden():
    """A lone INTRA-NONDET among passes -> GREEN (does NOT redden)."""
    results = [_r(Outcome.PARITY_PASS), _r(Outcome.INTRA_TRANSPORT_NONDETERMINISM)]
    assert rollup(results) == ("GREEN", EXIT_OK)


def test_rollup_intra_nondet_does_not_mask_real_red():
    """INTRA-NONDET alongside a PARITY-FAIL -> RED (intra never masks a real RED)."""
    results = [_r(Outcome.INTRA_TRANSPORT_NONDETERMINISM), _r(Outcome.PARITY_FAIL)]
    verdict, code = rollup(results)
    assert verdict == "RED"
    assert code == EXIT_PARITY_FAIL


@pytest.mark.parametrize(
    "outcomes,expected_verdict,expected_code",
    [
        ([Outcome.PARITY_PASS, Outcome.PARITY_PASS], "GREEN", EXIT_OK),
        ([Outcome.PARITY_PASS, Outcome.PARITY_FAIL], "RED", EXIT_PARITY_FAIL),
        ([Outcome.PARITY_PASS, Outcome.INFRA_ERROR], "ERROR", EXIT_INFRA),
        ([Outcome.INFRA_ERROR, Outcome.PARITY_FAIL], "ERROR", EXIT_INFRA),
        ([Outcome.PARITY_PASS, Outcome.INTRA_TRANSPORT_NONDETERMINISM], "GREEN", EXIT_OK),
        ([Outcome.INTRA_TRANSPORT_NONDETERMINISM, Outcome.PARITY_FAIL], "RED", EXIT_PARITY_FAIL),
    ],
)
def test_rollup_truth_table(outcomes, expected_verdict, expected_code):
    results = [_r(o) for o in outcomes]
    assert rollup(results) == (expected_verdict, expected_code)
