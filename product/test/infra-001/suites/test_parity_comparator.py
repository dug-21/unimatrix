"""K2 unit tests — parity_comparator: the comparator framework + drift guard.

Tier A (off-Docker unit): synthetic capture dicts, NO Docker, NO daemon, NO
fixtures (the #5258 seam). Maps 1:1 to test-plan/parity_comparator.md. Covers
R-05 (the structural SR-05/#5302 drift guard), R-08 (PreCompact call-out), R-14
(MetricVector adapter logic-identical), and the structural half of R-15 (ONE
forbidden-seed set).

The NEGATIVE tests are LOAD-BEARING false-GREEN guards: an unjustified exclusion,
a non-subclass comparator, an orphan capture_key, or a private forbidden-seed copy
MUST fail an off-Docker test BEFORE any tag round (#5258), not at the gate.

Importing this module triggers K2's ONE `bind_comparators` call at import, which
resolves the K1 registry comparator names -> classes (the prescribed Wave A->B hook).
"""

from __future__ import annotations

import dataclasses

import pytest

from harness import parity_comparator as pc
from harness import parity_dimensions as pd
from harness import parity_workload
from harness.metric_comparator import (
    EXCLUDED as MV_EXCLUDED,
)
from harness.metric_comparator import (
    ParityMismatch,
)
from harness.parity_comparator import (
    FORBIDDEN_SEED_SITES,
    AttributionComparator,
    BriefingComparator,
    DimensionComparator,
    MetricVectorComparator,
    PreCompactComparator,
    RetrievalComparator,
    assert_comparator_contract,
)


# ---------------------------------------------------------------------------
# Synthetic capture builders (the on-disk bundle shape, brief Data Structures)
# ---------------------------------------------------------------------------
def _metric_vector(total_tool_calls=10, search_miss_rate=0.0, computed_at="t0"):
    return {
        "computed_at": computed_at,
        "universal": {
            "total_tool_calls": total_tool_calls,
            "total_duration_secs": 1.0,  # EXCLUDED (wall-clock)
            "session_count": 1,
            "search_miss_rate": search_miss_rate,
            "edit_bloat_total_kb": 0,
            "edit_bloat_ratio": 0.0,
            "permission_friction_events": 0,
            "bash_for_search_count": 0,
            "cold_restart_events": 0,
            "coordinator_respawn_count": 0,
            "parallel_call_rate": 0.0,
            "context_load_before_first_write_kb": 0,
            "total_context_loaded_kb": 0,
            "post_completion_work_pct": 0.0,
            "follow_up_issues_created": 0,
            "knowledge_entries_stored": 0,
            "sleep_workaround_count": 0,
            "agent_hotspot_count": 0,
            "friction_hotspot_count": 0,
            "session_hotspot_count": 0,
            "scope_hotspot_count": 0,
        },
        "phases": {"design": {"tool_call_count": 3, "duration_secs": 0.5}},
        "domain_metrics": {},
    }


def _analytics_capture(mv=None, edges=("e1", "e2"), phase="settled"):
    return {
        "metric_vector": mv if mv is not None else _metric_vector(),
        "informs_edges": list(edges),
        "phase_signal": phase,
    }


def _retrieval_capture(queries):
    return {"queries": queries, "capture_2": []}


def _q(ids, scores=None):
    q = {"tool": "context_search", "args": {}, "result_ids": list(ids)}
    if scores is not None:
        q["scores"] = list(scores)
    return q


# ===========================================================================
# Drift guard — R-05 scenarios 1-4 (the structural SR-05 fix, AC-09)
# ===========================================================================
def test_assert_comparator_contract_passes_on_clean_registry():
    # The real bound DIMENSIONS passes: every comparator is a DimensionComparator
    # subclass; each EXCLUDED is non-empty-or-justified-empty; every EXCLUDED key
    # has a justification; capture_keys unique + match the bundle schema; seed set
    # single-sourced.
    assert_comparator_contract(pd.DIMENSIONS)


def _clean_registry():
    """A deep copy of the bound registry the negative tests mutate in isolation."""
    return tuple(dataclasses.replace(d) for d in pd.DIMENSIONS)


def test_assert_comparator_contract_fails_unjustified_exclusion():
    # Inject a stub comparator with an EXCLUDED key absent from JUSTIFICATIONS.
    class _Unjustified(DimensionComparator):
        EXCLUDED = frozenset({"ghost_field"})
        EXCLUSION_JUSTIFICATIONS = {}  # ghost_field NOT justified

        def compare(self, https, uds):
            return []

    reg = _clean_registry()
    drifted = (dataclasses.replace(reg[0], comparator=_Unjustified),) + reg[1:]
    with pytest.raises(AssertionError, match="unjustified exclusion"):
        assert_comparator_contract(drifted)


def test_assert_comparator_contract_fails_non_subclass_comparator():
    # A Dimension.comparator that is not a DimensionComparator subclass.
    class _NotAComparator:
        EXCLUDED = frozenset()
        EXCLUSION_JUSTIFICATIONS = {}

    reg = _clean_registry()
    drifted = (dataclasses.replace(reg[0], comparator=_NotAComparator),) + reg[1:]
    with pytest.raises(AssertionError, match="not a DimensionComparator subclass"):
        assert_comparator_contract(drifted)


def test_assert_comparator_contract_fails_capture_key_schema_mismatch():
    # An orphan capture_key with no matching on-disk bundle schema key.
    reg = _clean_registry()
    drifted = (dataclasses.replace(reg[0], capture_key="orphan_key"),) + reg[1:]
    with pytest.raises(AssertionError, match="no matching bundle schema key"):
        assert_comparator_contract(drifted)


def test_assert_comparator_contract_fails_should_exclude_empty():
    # A should-exclude dimension (retrieval) with an empty EXCLUDED set.
    class _EmptyRetrieval(DimensionComparator):
        EXCLUDED = frozenset()
        EXCLUSION_JUSTIFICATIONS = {}

        def compare(self, https, uds):
            return []

    reg = _clean_registry()
    # reg[0] is retrieval (a should-exclude dimension).
    assert reg[0].id == "retrieval"
    drifted = (dataclasses.replace(reg[0], comparator=_EmptyRetrieval),) + reg[1:]
    with pytest.raises(AssertionError, match="non-empty justified EXCLUDED"):
        assert_comparator_contract(drifted)


def test_forbidden_seed_sites_single_definition():
    # FORBIDDEN_SEED_SITES is the ONE C4' tuple object — no private copy (R-05 sc.2).
    assert FORBIDDEN_SEED_SITES is parity_workload.FORBIDDEN_SEED_SITES
    # The drift guard enforces the same identity.
    assert_comparator_contract(pd.DIMENSIONS)


# ===========================================================================
# Per-comparator EXCLUDED discipline (AC-09 + NFR-6 determinism floor)
# ===========================================================================
def test_attribution_comparator_excluded_is_empty():
    assert AttributionComparator.EXCLUDED == frozenset()
    assert AttributionComparator.EXCLUSION_JUSTIFICATIONS == {}


def test_metric_vector_comparator_excluded_matches_consumed_set():
    # IS the consumed nan-021 EXCLUDED (not a re-declared copy) — AC-04.
    assert MetricVectorComparator.EXCLUDED is MV_EXCLUDED


def test_retrieval_briefing_excluded_justified():
    for comp in (RetrievalComparator, BriefingComparator):
        assert comp.EXCLUDED  # non-empty
        for k in comp.EXCLUDED:
            assert k in comp.EXCLUSION_JUSTIFICATIONS, (comp.__name__, k)


# ===========================================================================
# compare() raises loud on a non-excluded diff (C-4 disposition authority, NFR-8)
# ===========================================================================
def test_compare_non_excluded_diff_raises_parity_mismatch():
    # AttributionComparator: differing topic_signal sets -> ParityMismatch.
    comp = AttributionComparator()
    with pytest.raises(ParityMismatch):
        comp.compare({"topic_signals": ["a", "b"]}, {"topic_signals": ["a", "c"]})


def test_compare_excluded_field_diff_tolerated():
    # MetricVector pair differing ONLY in EXCLUDED wall-clock fields -> clean.
    comp = MetricVectorComparator()
    https = _analytics_capture(mv=_metric_vector(computed_at="t0"))
    uds = _analytics_capture(mv=_metric_vector(computed_at="t9"))  # differs computed_at
    uds["metric_vector"]["universal"]["total_duration_secs"] = 99.0  # EXCLUDED
    assert comp.compare(https, uds) == []


# ===========================================================================
# Ranking comparators single-source K3 (R-07 scenario 4 / SR-03)
# ===========================================================================
def test_retrieval_and_briefing_import_same_ranking_parity():
    # Both comparators delegate to ranking_tolerance.ranking_parity — no 2nd policy.
    from harness import ranking_tolerance

    assert pc.ranking_parity is ranking_tolerance.ranking_parity
    # The module references exactly ONE ranking_parity symbol (single import surface).
    assert "ranking_parity" in pc.__dict__


def test_retrieval_comparator_ranking_divergence_raises():
    # A real in-prefix ranking divergence -> ParityMismatch (the tolerance must not
    # swallow it). Scores absent -> membership-only; a head divergence => prefix 0.
    comp = RetrievalComparator()
    https = _retrieval_capture([_q(["a", "b", "c", "d"])])
    uds = _retrieval_capture([_q(["z", "b", "c", "d"])])  # head differs
    with pytest.raises(ParityMismatch):
        comp.compare(https, uds)


def test_retrieval_comparator_tail_churn_tolerated():
    # Identical deep prefix (>= floor), churn only in the tail -> clean.
    comp = RetrievalComparator()
    https = _retrieval_capture([_q(["a", "b", "c", "x"])])
    uds = _retrieval_capture([_q(["a", "b", "c", "y"])])  # tail differs only
    assert comp.compare(https, uds) == []


def test_briefing_comparator_injection_set_divergence_raises():
    comp = BriefingComparator()
    https = {
        "briefing_ids": ["a", "b", "c"],
        "briefing_scores": None,
        "injection_set": ["i1", "i2"],
    }
    uds = {
        "briefing_ids": ["a", "b", "c"],
        "briefing_scores": None,
        "injection_set": ["i1", "i9"],  # injection set differs
    }
    with pytest.raises(ParityMismatch):
        comp.compare(https, uds)


# ===========================================================================
# Attribution unattributed -> HARD fail
# ===========================================================================
def test_attribution_unattributed_present_hard_fail():
    comp = AttributionComparator()
    with pytest.raises(ParityMismatch, match="unattributed"):
        comp.compare(
            {"topic_signals": ["unattributed", "t1"]},
            {"topic_signals": ["unattributed", "t1"]},
        )


def test_attribution_identical_signals_clean():
    comp = AttributionComparator()
    assert comp.compare({"topic_signals": ["t1", "t2"]}, {"topic_signals": ["t2", "t1"]}) == []


# ===========================================================================
# PreCompact measurability (R-08, AC-06) + field compare
# ===========================================================================
def test_precompact_comparator_measurable_true_field_compare():
    comp = PreCompactComparator()
    https = {
        "restored_payload": {"entry_ids": [1, 2], "content": "x"},
        "measurable": True,
        "host_side_gap": None,
    }
    uds = {
        "restored_payload": {"entry_ids": [1, 2], "content": "x"},
        "measurable": True,
        "host_side_gap": None,
    }
    assert comp.compare(https, uds) == []


def test_precompact_comparator_content_divergence_raises():
    comp = PreCompactComparator()
    https = {"restored_payload": {"entry_ids": [1, 2], "content": "x"}}
    uds = {"restored_payload": {"entry_ids": [1, 2], "content": "DIFFERENT"}}
    with pytest.raises(ParityMismatch):
        comp.compare(https, uds)


def test_precompact_comparator_excluded_fields_tolerated():
    # Differing ONLY in restoration_timestamp + envelope.* -> clean (modulo EXCLUDED).
    comp = PreCompactComparator()
    https = {
        "restored_payload": {
            "content": "x",
            "restoration_timestamp": "t0",
            "envelope.seq": 1,
        }
    }
    uds = {
        "restored_payload": {
            "content": "x",
            "restoration_timestamp": "t9",  # EXCLUDED
            "envelope.seq": 2,  # EXCLUDED (envelope.*)
        }
    }
    assert comp.compare(https, uds) == []


# ===========================================================================
# MetricVector adapter is logic-identical to the consumed compare (R-14)
# ===========================================================================
def test_metric_vector_comparator_clean_pair_matches_consumed():
    # Identical analytics captures -> clean diff list THROUGH the adapter (logic
    # unaltered: compare_metric_vectors returns [] and the adapter does too).
    comp = MetricVectorComparator()
    cap = _analytics_capture()
    assert comp.compare(cap, _analytics_capture()) == []


def test_metric_vector_comparator_mv_divergence_raises_through_adapter():
    # A real MetricVector field divergence -> ParityMismatch THROUGH the adapter,
    # identical to calling compare_metric_vectors directly (R-14).
    comp = MetricVectorComparator()
    https = _analytics_capture(mv=_metric_vector(total_tool_calls=10))
    uds = _analytics_capture(mv=_metric_vector(total_tool_calls=11))  # non-excluded
    with pytest.raises(ParityMismatch) as exc:
        comp.compare(https, uds)
    assert any("total_tool_calls" in d[0] for d in exc.value.diffs)


def test_metric_vector_comparator_informs_edges_divergence_raises():
    comp = MetricVectorComparator()
    https = _analytics_capture(edges=("e1", "e2"))
    uds = _analytics_capture(edges=("e1", "e3"))  # edge-ID set differs
    with pytest.raises(ParityMismatch) as exc:
        comp.compare(https, uds)
    assert any("informs_edges" in d[0] for d in exc.value.diffs)


def test_metric_vector_comparator_phase_signal_divergence_raises():
    comp = MetricVectorComparator()
    https = _analytics_capture(phase="settled")
    uds = _analytics_capture(phase="pending")
    with pytest.raises(ParityMismatch) as exc:
        comp.compare(https, uds)
    assert any("phase_signal" in d[0] for d in exc.value.diffs)


# ===========================================================================
# evidence_record (AC-10 first-live-run record)
# ===========================================================================
def test_evidence_record_default_field_by_field():
    comp = AttributionComparator()
    rec = comp.evidence_record(
        {"topic_signals": ["a"]}, {"topic_signals": ["b"]}, run_token="rt-1"
    )
    assert rec["run_token"] == "rt-1"
    assert rec["dimension"] == "AttributionComparator"
    assert rec["excluded_set"] == []
    # the non-raising shadow compare captured the divergence for disposition.
    assert any("topic_signals" in d[0] for d in rec["diffs"])


def test_metric_vector_evidence_record_delegates_to_consumed():
    comp = MetricVectorComparator()
    https = _analytics_capture()
    uds = _analytics_capture(edges=("e1", "e9"))
    rec = comp.evidence_record(https, uds, run_token="rt-2")
    # delegates to field_by_field_record (consumed) -> universal_table present,
    # then attaches the informs/phase rows.
    assert rec["run_token"] == "rt-2"
    assert "universal_table" in rec
    assert rec["informs_edges_uds"] == ["e1", "e9"]


# ===========================================================================
# bind_comparators ran at import (the Wave A->B hook)
# ===========================================================================
def test_bind_comparators_ran_registry_resolved_to_classes():
    for d in pd.DIMENSIONS:
        assert isinstance(d.comparator, type), d.id
        assert issubclass(d.comparator, DimensionComparator), d.id
