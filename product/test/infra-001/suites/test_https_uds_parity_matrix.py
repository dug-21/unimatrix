"""ORCH off-Docker unit tests for the nan-022 parity-MATRIX orchestrator (#837).

The LIVE matrix orchestrator (`test_https_uds_parity_matrix`) lives in
`suites/test_https_uds_parity.py` (the cumulative sibling of the nan-021
MetricVector test) and is marked `@pytest.mark.integration` + `@pytest.mark.parity`
so it is DESELECTED off-Docker (the live cross-leg drive is Stage 3c / the release
lane's `pytest -m parity`). THIS file is the DAEMON-FREE wiring proof: a contract-
shaped FIXTURE dimension bundle round-trips through the orchestrator's ingest →
classify → table → roll-up WITHOUT Docker (R-09 sc.3 / R-10 sc.1), plus the
classification-wiring / evidence-table / never-empty / token-guard / roll-up-mapping
unit tests. NO `@pytest.mark.integration` / `@pytest.mark.parity` here — these run in
the daemon-free K-suite (`-m "not integration and not parity"`).

Mirrors the nan-021 `test_c3_orchestrator_seam_with_fixture_https_vector` precedent
(#5258) generalized from one MetricVector to the six-dimension bundle.
"""

from __future__ import annotations

import inspect
import json

import pytest

from harness.parity_comparator import assert_comparator_contract
from harness.parity_dimensions import DIMENSIONS, dimension_by_id
from harness.parity_outcome import (
    DimensionResult,
    Outcome,
    classify_dimension,
    rollup,
)
from harness.parity_workload import default_workload
from harness.transport_health import EXIT_INFRA, InfraError, load_https_bundle
from harness import parity_matrix_support as matrix
from harness.parity_matrix_support import (
    assert_rollup,
    emit_infra_and_fail,
    evidence_table,
    fixture_dimension_bundle,
    fixture_measurable_precompact,
)

# The module under test (its sequencing helpers + the live matrix test for source
# inspection). Importing the suite module is daemon-free (the live test only runs
# under -m parity, but the module imports cleanly off-Docker).
from suites import test_https_uds_parity as orch


# ===========================================================================
# Helper: classify a full fixture bundle the way the orchestrator does.
# ===========================================================================


def _classify_all(bundle_uds: dict, bundle_https: dict) -> list[DimensionResult]:
    return [
        classify_dimension(d, bundle_uds.get(d.capture_key), bundle_https.get(d.capture_key))
        for d in DIMENSIONS
    ]


# ===========================================================================
# Off-Docker seam proof (R-10 sc.1 — orchestrator wiring without Docker).
# ===========================================================================


def test_matrix_orchestrator_seam_with_fixture_bundle(tmp_path):
    """A contract-shaped FIXTURE dimension bundle round-trips through the orchestrator
    wiring (token-guarded ingest → classify → table → roll-up) WITHOUT Docker, BEFORE
    any tag round (#5258). Both legs supply the SAME golden bundle, so every measurable
    dimension is PARITY-PASS; precompact is the honest documented host-side gap
    (INFRA-ERROR call-out — NOT a vacuous pass), so the roll-up is ERROR, not GREEN."""
    workload = default_workload()
    run_token = workload.session_id

    bundle = fixture_dimension_bundle(feature_cycle=workload.feature_cycle)
    https_out = tmp_path / "https_dimension_bundle.json"
    https_out.write_text(
        json.dumps({"run_token": run_token, "dimension_bundle": bundle}),
        encoding="utf-8",
    )

    # Ingest the HTTPS leg the way the orchestrator does (token-guarded, never-empty).
    bundle_https = load_https_bundle(https_out, run_token)
    bundle_uds = fixture_dimension_bundle(feature_cycle=workload.feature_cycle)

    results = _classify_all(bundle_uds, bundle_https)
    table = evidence_table(results, run_token)

    by_dim = {r.dimension: r for r in results}
    # The five measurable dimensions are PARITY-PASS on identical golden captures.
    for dim_id in ("retrieval", "behavioral", "analytics", "proactive", "isolation"):
        assert by_dim[dim_id].outcome == Outcome.PARITY_PASS, (
            f"{dim_id}: {by_dim[dim_id].detail}"
        )
    # precompact is the honest documented host-side gap → INFRA-ERROR call-out.
    assert by_dim["precompact"].outcome == Outcome.INFRA_ERROR
    assert "MEASURABILITY" in by_dim["precompact"].detail.upper()

    # The table records it as a documented exception — never rounded up to GREEN.
    assert table["verdict"] == "ERROR"
    assert table["exit_code"] == EXIT_INFRA
    assert table["documented_exceptions"], "D5 host-side gap must be surfaced honestly"


def test_matrix_seam_all_measurable_is_green(tmp_path):
    """When a future live drive proves D5 measurability, an all-measurable golden
    bundle (precompact measurable on BOTH legs) rolls up GREEN end-to-end — the seam
    proves the HAPPY path the live Docker drive targets (R-10 sc.1)."""
    workload = default_workload()
    run_token = workload.session_id

    def _measurable_bundle() -> dict:
        b = fixture_dimension_bundle(feature_cycle=workload.feature_cycle)
        b["precompact"] = fixture_measurable_precompact()
        return b

    bundle_uds = _measurable_bundle()
    bundle_https = _measurable_bundle()
    results = _classify_all(bundle_uds, bundle_https)
    verdict, exit_code = rollup(results)
    table = evidence_table(results, run_token)

    assert verdict == "GREEN" and exit_code == 0, [
        (r.dimension, r.outcome.value, r.detail) for r in results
    ]
    # assert_rollup passes silently on GREEN (the orchestrator's §6 assertion).
    assert_rollup(verdict, exit_code, results, table)


# ===========================================================================
# Classification wiring per dimension (each dimension classifies independently).
# ===========================================================================


def test_matrix_classifies_each_dimension_independently():
    """Each registry dimension classifies via `classify_dimension` over its own
    capture pair — iterating DIMENSIONS (no hand-list). On a golden bundle every
    measurable dimension is PARITY-PASS and precompact is the documented exception."""
    wl = default_workload()
    uds = fixture_dimension_bundle(feature_cycle=wl.feature_cycle)
    https = fixture_dimension_bundle(feature_cycle=wl.feature_cycle)
    results = _classify_all(uds, https)
    assert {r.dimension for r in results} == {d.id for d in DIMENSIONS}
    for r in results:
        assert isinstance(r.outcome, Outcome)


def test_matrix_one_dimension_parity_fail_reddens_only_that_dimension():
    """A real cross-leg divergence on ONE dimension (two intra-stable legs) →
    PARITY-FAIL on that dimension, the others unaffected; the roll-up is RED (R-07
    sc.3 — a cross-divergence on intra-stable legs is PARITY-FAIL, never INTRA)."""
    wl = default_workload()
    uds = fixture_dimension_bundle(feature_cycle=wl.feature_cycle)
    https = fixture_dimension_bundle(feature_cycle=wl.feature_cycle)
    # Make D5 measurable on both legs so the ONLY non-pass is the injected behavioral
    # divergence (else the fixture's documented D5 gap is INFRA-ERROR and ERROR > RED).
    uds["precompact"] = fixture_measurable_precompact()
    https["precompact"] = fixture_measurable_precompact()
    # Diverge behavioral cross-leg (string-exact dim, no tolerance) — both legs are
    # individually self-consistent (no intra check), so this is a true PARITY-FAIL.
    https["behavioral"] = {"topic_signals": ["a-different-signal"]}
    results = _classify_all(uds, https)
    by_dim = {r.dimension: r for r in results}
    assert by_dim["behavioral"].outcome == Outcome.PARITY_FAIL
    assert by_dim["retrieval"].outcome == Outcome.PARITY_PASS
    verdict, exit_code = rollup(results)
    assert verdict == "RED" and exit_code == 1


def test_matrix_intra_nondeterminism_does_not_redden():
    """A leg that self-diverges within the stable prefix (HNSW/tie flip) on an
    intra-check dimension is classed INTRA-NONDET, recorded, and does NOT redden the
    gate (R-07 sc.2) — only the cross-compare runs when both legs are intra-stable."""
    dim = dimension_by_id("retrieval")
    # UDS leg's two captures disagree WITHIN the stable prefix → intra-unstable.
    unstable = {
        "queries": [{"tool": "context_search", "args": {}, "result_ids": [1, 2, 3, 4, 5], "scores": None}],
        "capture_2": [{"tool": "context_search", "args": {}, "result_ids": [9, 8, 7, 6, 5], "scores": None}],
    }
    stable = {
        "queries": [{"tool": "context_search", "args": {}, "result_ids": [1, 2, 3, 4, 5], "scores": None}],
        "capture_2": [{"tool": "context_search", "args": {}, "result_ids": [1, 2, 3, 4, 5], "scores": None}],
    }
    result = classify_dimension(dim, unstable, stable)
    assert result.outcome == Outcome.INTRA_TRANSPORT_NONDETERMINISM
    # The roll-up of an otherwise-green matrix with one INTRA row is NOT reddened.
    others = [
        DimensionResult(d.id, Outcome.PARITY_PASS) for d in DIMENSIONS if d.id != "retrieval"
    ]
    verdict, exit_code = rollup([result, *others])
    assert verdict == "GREEN" and exit_code == 0


# ===========================================================================
# Missing dimension / empty capture → INFRA (never a vacuous pass) — C-9 / R-03.
# ===========================================================================


def test_matrix_missing_capture_classifies_infra_never_vacuous_pass():
    """A capture absent from a bundle (a misroute records nothing) → INFRA-ERROR via
    the classifier, NEVER PARITY-PASS (C-9 / R-03). The orchestrator's `.get()` yields
    None for a missing key; the classifier's emptiness guard names it INFRA."""
    dim = dimension_by_id("behavioral")
    result = classify_dimension(dim, None, {"topic_signals": ["nan-021"]})
    assert result.outcome == Outcome.INFRA_ERROR
    assert result.outcome != Outcome.PARITY_PASS


def test_matrix_empty_capture_classifies_infra():
    """An EMPTY capture (a wrong-surface drive records nothing) → INFRA-ERROR, never
    an empty-equals-empty pass (R-03/R-04 fault injection)."""
    dim = dimension_by_id("retrieval")
    empty = {"queries": [], "capture_2": []}
    result = classify_dimension(dim, empty, empty)
    assert result.outcome == Outcome.INFRA_ERROR


def test_matrix_one_infra_dimension_errors_not_red():
    """One INFRA-ERROR dimension amid passes → the roll-up is a DISTINCT ERROR exit
    (EXIT_INFRA), NOT counted as a parity RED (R-02 sc.3 — ERROR > RED > GREEN)."""
    results = [DimensionResult(d.id, Outcome.PARITY_PASS) for d in DIMENSIONS]
    results[0] = DimensionResult(DIMENSIONS[0].id, Outcome.INFRA_ERROR, [], "infra")
    verdict, exit_code = rollup(results)
    assert verdict == "ERROR" and exit_code == EXIT_INFRA


# ===========================================================================
# Token-guard rejects a stale bundle on ingest (R-12).
# ===========================================================================


def test_matrix_load_https_bundle_rejects_stale_token(tmp_path):
    """A prior-tag HTTPS out-file (token mismatch) is REJECTED by `load_https_bundle`
    as InfraError, never ingested — the orchestrator cannot compare a stale bundle
    (R-12). InfraError (not ValueError) so it folds into the single INFRA class."""
    wl = default_workload()
    https_out = tmp_path / "https_dimension_bundle.json"
    https_out.write_text(
        json.dumps(
            {
                "run_token": "OLD-tag",
                "dimension_bundle": fixture_dimension_bundle(feature_cycle=wl.feature_cycle),
            }
        ),
        encoding="utf-8",
    )
    with pytest.raises(InfraError):
        load_https_bundle(https_out, wl.session_id)


def test_matrix_missing_https_bundle_errors_never_empty(tmp_path):
    """A missing HTTPS out-file is an InfraError (never an empty/vacuous compare) —
    the never-empty ingest contract (R-09)."""
    with pytest.raises(InfraError):
        load_https_bundle(tmp_path / "absent.json", "run-x")


def test_matrix_missing_capture_key_in_bundle_errors(tmp_path):
    """A bundle missing a required capture_key → InfraError on ingest (a misroute
    records nothing — never an empty-pass on the absent dimension, C-9)."""
    wl = default_workload()
    bundle = fixture_dimension_bundle(feature_cycle=wl.feature_cycle)
    del bundle["isolation"]
    https_out = tmp_path / "https_dimension_bundle.json"
    https_out.write_text(
        json.dumps({"run_token": wl.session_id, "dimension_bundle": bundle}),
        encoding="utf-8",
    )
    with pytest.raises(InfraError):
        load_https_bundle(https_out, wl.session_id)


# ===========================================================================
# Roll-up verdict mapping + the §6 assertion (GREEN/RED/ERROR → exit code).
# ===========================================================================


def test_matrix_rollup_verdict_mapping():
    """The roll-up maps exactly: all-pass → GREEN/0; any PARITY-FAIL → RED/1; any
    INFRA-ERROR → ERROR/EXIT_INFRA (ERROR precedence over RED); INTRA does not redden."""
    passes = [DimensionResult(d.id, Outcome.PARITY_PASS) for d in DIMENSIONS]
    assert rollup(passes) == ("GREEN", 0)

    with_fail = list(passes)
    with_fail[1] = DimensionResult(DIMENSIONS[1].id, Outcome.PARITY_FAIL, [("f", 1, 2)])
    assert rollup(with_fail) == ("RED", 1)

    with_infra = list(with_fail)
    with_infra[2] = DimensionResult(DIMENSIONS[2].id, Outcome.INFRA_ERROR)
    assert rollup(with_infra) == ("ERROR", EXIT_INFRA)  # ERROR > RED

    with_intra = list(passes)
    with_intra[0] = DimensionResult(DIMENSIONS[0].id, Outcome.INTRA_TRANSPORT_NONDETERMINISM)
    assert rollup(with_intra) == ("GREEN", 0)  # INTRA does not redden


def test_matrix_assert_rollup_fails_loud_on_red():
    """`assert_rollup` raises with the evidence table on a RED verdict (file a GH bug,
    fix NOT absorbed — AC-10); it passes silently on GREEN."""
    fail_results = [DimensionResult(DIMENSIONS[0].id, Outcome.PARITY_FAIL, [("x", 1, 2)], "div")]
    table = evidence_table(fail_results, "tok-red")
    with pytest.raises(AssertionError) as ei:
        assert_rollup("RED", 1, fail_results, table)
    assert "RED" in str(ei.value) and "tok-red" in str(ei.value)


def test_matrix_assert_rollup_fails_distinct_on_infra():
    """`assert_rollup` raises a DISTINCT INFRA failure (not a parity RED) on an
    INFRA-ERROR verdict — the error names the transport/ingest detail (R-02/C-8)."""
    infra_results = [DimensionResult(DIMENSIONS[0].id, Outcome.INFRA_ERROR, [], "hung socket")]
    table = evidence_table(infra_results, "tok-infra")
    with pytest.raises(AssertionError) as ei:
        assert_rollup("ERROR", EXIT_INFRA, infra_results, table)
    assert "INFRA-ERROR" in str(ei.value)


def test_matrix_emit_infra_and_fail_is_distinct_class():
    """`emit_infra_and_fail` surfaces a preflight/ingest InfraError as a DISTINCT
    ERROR carrying the run token + the EXIT_INFRA class — never a parity RED."""
    with pytest.raises(AssertionError) as ei:
        emit_infra_and_fail(InfraError("https", "stale bundle rejected"), "tok-x")
    msg = str(ei.value)
    assert "INFRA-ERROR" in msg and "tok-x" in msg and str(EXIT_INFRA) in msg


# ===========================================================================
# Evidence-table emit keyed by run_token (AC-08 — the C0 proof artifact).
# ===========================================================================


def test_matrix_evidence_table_keyed_by_run_token():
    """The evidence table is keyed by the run-correlation token and carries one row
    per dimension with outcome + blocks_c0_proof (from the registry, not a hand-list)
    + detail + diffs (AC-08 / AC-12)."""
    wl = default_workload()
    uds = fixture_dimension_bundle(feature_cycle=wl.feature_cycle)
    https = fixture_dimension_bundle(feature_cycle=wl.feature_cycle)
    results = _classify_all(uds, https)
    table = evidence_table(results, wl.session_id)

    assert table["run_token"] == wl.session_id
    assert {row["dimension"] for row in table["dimensions"]} == {d.id for d in DIMENSIONS}
    for row in table["dimensions"]:
        assert "outcome" in row and "blocks_c0_proof" in row and "detail" in row
        # blocks_c0_proof is single-sourced from the registry.
        assert row["blocks_c0_proof"] == dimension_by_id(row["dimension"]).blocks_c0_proof
    assert table["verdict"] in ("GREEN", "RED", "ERROR")


def test_matrix_evidence_table_routes_intra_and_documents_d5():
    """INTRA-NONDET dims are listed under `intra_nondeterminism` (routed to GH#746,
    not reddening); a D5 measurability call-out under `documented_exceptions`
    (honest, never a vacuous pass)."""
    results = [
        DimensionResult("retrieval", Outcome.INTRA_TRANSPORT_NONDETERMINISM, [], "flip"),
        DimensionResult(
            "precompact", Outcome.INFRA_ERROR, [], "DOCUMENTED MEASURABILITY LIMITATION: gap"
        ),
        DimensionResult("behavioral", Outcome.PARITY_PASS),
    ]
    table = evidence_table(results, "tok-d5")
    assert "retrieval" in table["intra_nondeterminism"]
    assert any(d["dimension"] == "precompact" for d in table["documented_exceptions"])


# ===========================================================================
# Structural / contract tests (extend the nan-021 set — source inspection).
# ===========================================================================


def test_matrix_orchestrator_iterates_dimensions_not_handlist():
    """The live matrix orchestrator iterates `DIMENSIONS` (the single enumeration) —
    no hand-list of six — asserted by source inspection (SR-05)."""
    src = inspect.getsource(orch._classify_matrix)
    assert "for dim in DIMENSIONS" in src
    assert "dim.capture_key" in src


def test_matrix_drift_guard_runs_before_any_drive():
    """`assert_comparator_contract(DIMENSIONS)` is called BEFORE any leg drives —
    asserted by source order (off-Docker discipline, fails fast)."""
    src = inspect.getsource(orch.test_https_uds_parity_matrix)
    guard_at = src.index("assert_comparator_contract(DIMENSIONS)")
    drive_at = src.index("drive_uds_bundle(")
    assert guard_at < drive_at, "drift guard must run BEFORE any drive"


def test_matrix_drives_uds_via_bundle_and_ingests_token_guarded():
    """The live matrix owns BOTH legs in ONE invocation: the UDS leg via
    `drive_uds_bundle` (NOT the nan-021 `drive_uds_leg`), the HTTPS leg via
    `run_https_leg` + token-guarded `load_https_bundle` (R-03/D-6)."""
    src = inspect.getsource(orch.test_https_uds_parity_matrix)
    assert "drive_uds_bundle(" in src  # the matrix path uses the bundle driver
    assert "run_https_leg(" in src
    assert "load_https_bundle(https_out, run_token)" in src
    assert "run_token = workload.session_id" in src  # ONE identity == ONE token (R-13)


def test_matrix_one_workload_one_token_one_barrier():
    """ONE `ParityWorkload`; `run_token == workload.session_id`; the barrier is the
    ONE shared helper composed inside `drive_uds_bundle` (R-13). The orchestrator does
    not author a second workload/identity/token."""
    src = inspect.getsource(orch.test_https_uds_parity_matrix)
    assert src.count("default_workload()") == 1
    assert "run_token = workload.session_id" in src


def test_matrix_drift_guard_passes_on_bound_registry():
    """The drift guard passes over the bound `DIMENSIONS` (every comparator is a
    bound DimensionComparator subclass with a justified closed exclusion set) — the
    orchestrator's fail-fast precondition holds off-Docker."""
    assert_comparator_contract(DIMENSIONS)  # raises on any drift


def test_matrix_no_seed_site_reachable_from_orchestrator_and_support():
    """NO forbidden seed site is reachable from the matrix orchestrator OR its support
    module OR the fixture-bundle loader (extends the nan-021 audit to all net-new
    matrix modules — AC-03/FR-6)."""
    import harness.parity_workload as pw

    pw.assert_no_seed_reachable(orch.__file__, matrix.__file__, __file__)


def test_matrix_nan021_metricvector_test_still_present():
    """Cumulative (AC-11): the committed nan-021 MetricVector `test_https_uds_parity`
    + its `load_https_vector` ingest path are UNCHANGED alongside the matrix sibling."""
    assert hasattr(orch, "test_https_uds_parity")
    src = inspect.getsource(orch.test_https_uds_parity)
    assert "compare_metric_vectors(" in src
    assert "load_https_vector(https_out, run_token)" in src
