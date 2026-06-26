"""K4 — Outcome-class model (nan-022 / #837 / ADR-002 / #5313).

The four-valued per-dimension verdict, the classifier with the FIXED order
INFRA -> INTRA -> PARITY, the double-capture-and-diff intra-transport stability
check, and the matrix-level roll-up. Structurally separates cross-transport
divergence (a real C0 parity defect, RED) from intra-transport nondeterminism
(a SEPARATELY filed bug — GH#746, does NOT redden) from transport-infra error
(distinct ERROR exit) — none can masquerade as another (SR-01/SR-02/SR-04).

THE LOAD-BEARING INVARIANT (R-07 scenario 3): a cross-leg divergence where BOTH
legs are individually intra-STABLE MUST classify PARITY_FAIL, never INTRA. The
fixed order plus the per-leg both-legs-stable check guarantees this — INTRA only
fires when a SINGLE leg self-diverges against ITS OWN second capture; if both
legs are intra-stable the classifier always proceeds to the cross-compare. A
too-loose intra detector that let a cross-divergent-but-self-stable leg be called
intra-unstable would silently DROP the defect (the most insidious false-GREEN) —
forbidden by construction here.

Pure-Python, stdlib-only, OFF-Docker unit-testable (the #5258 seam). ZERO new
runtime deps. TEST-ONLY; no production-code diff (NFR-1/NFR-2/AC-11).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any

from harness.parity_comparator import ParityMismatch
from harness.ranking_tolerance import (
    STABLE_PREFIX_FLOOR,  # single source for the non-degenerate floor (R-06)
    ranking_parity,  # the ONE tolerance — intra reuses the cross-leg policy (R-07 sc.4)
)
from harness.transport_health import (
    EXIT_INFRA,  # single-sourced distinct INFRA exit code (K5; do NOT redefine 7)
    InfraError,
)

# =============================================================================
# Exit codes — distinct constants so the release-gate lane discriminates a
# parity RED from a transport ERROR (C-8 / FR-18). EXIT_INFRA is IMPORTED from
# K5 (single source, =7) and must not be redefined here. EXIT_OK / EXIT_PARITY
# pin 0 / 1; EXIT_INFRA (7) cannot collide with run_smoke_gate's 0/1/3/4 table.
# =============================================================================
EXIT_OK: int = 0
EXIT_PARITY_FAIL: int = 1


# =============================================================================
# 1. The four-valued outcome enum (ADR-002)
# =============================================================================
class Outcome(Enum):
    """Exactly one of these is assigned to each dimension by `classify_dimension`."""

    PARITY_PASS = "PARITY_PASS"
    PARITY_FAIL = "PARITY_FAIL"
    INFRA_ERROR = "INFRA_ERROR"
    INTRA_TRANSPORT_NONDETERMINISM = "INTRA_TRANSPORT_NONDETERMINISM"


@dataclass
class DimensionResult:
    """The per-dimension verdict + evidence.

    dimension  Dimension.id.
    outcome    the assigned `Outcome`.
    diffs      the comparator diff list (empty unless PARITY_FAIL).
    detail     human-readable: which guard tripped / which leg was intra-unstable /
               the D5 host_side_gap call-out / the ParityMismatch summary.
    """

    dimension: str
    outcome: Outcome
    diffs: list = field(default_factory=list)
    detail: str = ""


# =============================================================================
# 2. intra_transport_stable — double-capture-and-diff (R-07 sc.1,2,4)
# =============================================================================
def intra_transport_stable(cap_a: Any, cap_b: Any, *, tolerance: str) -> bool:
    """Is a single leg self-stable across its two same-drive captures?

    A leg captures its dimension output TWICE in the same drive; this diffs the
    two captures modulo the SAME tolerance the cross-leg compare uses (there is NO
    second tolerance — R-07 scenario 4). It answers "is THIS leg self-stable",
    decided against ITS OWN second capture, NEVER against the other leg.

    `tolerance`:
      - "ranked" (retrieval/proactive): cap_a/cap_b are ``{"ids": [...], "scores":
        [...]}`` shaped; stability is ``ranking_parity(a_ids, b_ids,
        scores=(a_scores, b_scores)).matched`` — the SAME K3 policy as the cross
        compare. A leg that self-diverges only in the tolerated tail is
        intra-STABLE (True); a leg that self-diverges WITHIN the stable prefix is
        intra-UNSTABLE (False).
      - "exact": byte-equality (used only if an exact-dim ever opts into the
        intra check; the registered exact dims set intra_transport_check=False and
        never reach this path — NFR-6).

    Returns True if the leg is self-stable, False if it self-diverges within the
    stable prefix.
    """
    if tolerance == "ranked":
        a_ids = _ids_of(cap_a)
        b_ids = _ids_of(cap_b)
        a_scores = _scores_of(cap_a)
        b_scores = _scores_of(cap_b)
        verdict = ranking_parity(a_ids, b_ids, scores=(a_scores, b_scores))
        return bool(verdict.matched)
    # Exact stability: byte-equality, no tolerance (NFR-6 determinism floor).
    return cap_a == cap_b


def _ids_of(cap: Any) -> list:
    """Extract the ranked id-list from an intra capture in either accepted shape.

    Accepts ``{"ids": [...]}`` (the normalized intra-pair shape) or a bare list of
    ids (already extracted). Raises InfraError on an un-extractable capture so the
    classifier folds it into INFRA-ERROR (never a silent empty compare)."""
    if isinstance(cap, dict):
        if "ids" in cap:
            return list(cap["ids"])
        raise InfraError(
            "intra", "un-extractable intra capture", detail=f"no 'ids' key in {cap!r}"
        )
    if isinstance(cap, (list, tuple)):
        return list(cap)
    raise InfraError(
        "intra", "un-extractable intra capture", detail=f"unexpected shape {type(cap).__name__}"
    )


def _scores_of(cap: Any) -> Any:
    """Extract the aligned scores list from an intra capture, or None if absent
    (ranking_parity degrades to the documented membership-only fallback)."""
    if isinstance(cap, dict):
        return cap.get("scores")
    return None


# =============================================================================
# 3. classify_dimension — the FIXED-ORDER classifier (INFRA -> INTRA -> PARITY)
# =============================================================================
def classify_dimension(dim: Any, cap_uds: Any, cap_https: Any) -> DimensionResult:
    """Produce exactly ONE `Outcome` for a dimension. Order is FIXED:
    INFRA -> INTRA -> PARITY. `cap_uds`/`cap_https` are the per-dimension capture
    dicts (``dimension_bundle[dim.capture_key]``).

    The order is LOAD-BEARING (R-07 scenario 3): a cross-leg divergence on two
    intra-STABLE legs can NEVER be reclassified INTRA — INTRA fires only when a
    SINGLE leg self-diverges against its OWN second capture; if both legs are
    intra-stable the classifier always proceeds to the cross-compare. The earliest
    applicable class wins (INFRA before INTRA before PARITY)."""
    try:
        return _classify(dim, cap_uds, cap_https)
    except InfraError as exc:
        # Any capture-extraction InfraError folds into the INFRA-ERROR class
        # (never propagated as a pass, never read as a parity verdict).
        return DimensionResult(
            dim.id, Outcome.INFRA_ERROR, [], f"{exc.leg} capture INFRA: {exc.reason}"
        )


def _classify(dim: Any, cap_uds: Any, cap_https: Any) -> DimensionResult:
    # ---- 1. INFRA: missing / empty / null / un-ingestable capture -------------
    for leg, cap in (("uds", cap_uds), ("https", cap_https)):
        if cap is None:
            # null is permitted ONLY for D5 with measurable=False (handled below);
            # any other null capture is INFRA-ERROR (R-09, never empty-pass).
            if dim.id == "precompact":
                continue
            return DimensionResult(
                dim.id, Outcome.INFRA_ERROR, [], f"{leg} capture missing/null"
            )
        if _capture_is_empty(dim, cap):
            return DimensionResult(
                dim.id,
                Outcome.INFRA_ERROR,
                [],
                f"{leg} capture empty (R-03/R-04 — never empty-pass)",
            )

    # ---- 1b. D5 measurability branch (ADR-006) --------------------------------
    if dim.id == "precompact":
        m_uds = cap_uds.get("measurable", False)
        m_https = cap_https.get("measurable", False)
        if not (m_uds and m_https):
            gap = (
                cap_uds.get("host_side_gap")
                or cap_https.get("host_side_gap")
                or "host-side component not test-only-drivable"
            )
            # DOCUMENTED-EXCEPTION call-out — NOT a pass, NOT a parity RED. Recorded
            # distinctly as INFRA-ERROR; NEVER rounded up to "fully measured" (R-08).
            return DimensionResult(
                dim.id,
                Outcome.INFRA_ERROR,
                [],
                f"DOCUMENTED MEASURABILITY LIMITATION: {gap} "
                f"(D5 measured-where-drivable; NEVER rounded up to fully-measured)",
            )
        # both measurable -> fall through to PARITY with non-null payloads.

    # ---- 2. INTRA: double-capture-and-diff for intra-check dimensions ---------
    if dim.intra_transport_check:
        for leg, cap in (("uds", cap_uds), ("https", cap_https)):
            cap_1, cap_2 = _intra_pair(dim, cap)
            if not intra_transport_stable(cap_1, cap_2, tolerance="ranked"):
                # this leg is intra-unstable (HNSW/tie flip #4990) — routed OUT of
                # the red gate to a SEPARATELY filed bug (GH#746).
                return DimensionResult(
                    dim.id,
                    Outcome.INTRA_TRANSPORT_NONDETERMINISM,
                    [],
                    f"{leg} leg self-diverged within the stable prefix "
                    f"(file GH#746; NOT a cross-transport defect)",
                )

    # ---- 3. PARITY: both legs captured + intra-stable -> cross-leg compare ----
    comparator = dim.comparator()
    try:
        diffs = comparator.compare(cap_https, cap_uds)  # raises ParityMismatch
    except ParityMismatch as exc:
        return DimensionResult(
            dim.id,
            Outcome.PARITY_FAIL,
            list(exc.diffs),
            "cross-transport divergence — REAL C0 defect; file GH bug, "
            "gate stays RED (AC-10)",
        )
    return DimensionResult(
        dim.id, Outcome.PARITY_PASS, diffs, "parity clean modulo closed exclusion set"
    )


# =============================================================================
# 4. Per-dimension capture helpers (the emptiness predicate + intra-pair extract)
# =============================================================================
def _capture_is_empty(dim: Any, cap: Any) -> bool:
    """Per-dimension emptiness predicate against the capture shape (R-03/R-04).

    A routed-to-the-wrong-surface dimension records NOTHING; that MUST surface as
    INFRA-ERROR via this predicate, never a vacuous empty-equals-empty pass (C-9).
    Imports STABLE_PREFIX_FLOOR from K3 (single source for N — R-06)."""
    if not isinstance(cap, dict):
        # A non-dict capture (e.g. a bare null slipped past) is treated as empty.
        return True
    dim_id = dim.id
    if dim_id == "retrieval":
        queries = cap.get("queries")
        if not queries:
            return True
        # Any query whose result_ids is shorter than the non-degenerate floor is a
        # degenerate (R-06) capture — the ranking signal would be vacuous.
        for q in queries:
            if len(q.get("result_ids") or []) < STABLE_PREFIX_FLOOR:
                return True
        return False
    if dim_id == "behavioral":
        return not cap.get("topic_signals")
    if dim_id == "analytics":
        mv = cap.get("metric_vector")
        return not mv
    if dim_id == "proactive":
        ids = cap.get("briefing_ids")
        if not ids:
            return True
        return len(ids) < STABLE_PREFIX_FLOOR
    if dim_id == "precompact":
        # The measurability branch (1b) handles the measurable=False / documented
        # host-side-gap case. Here we only call a MEASURABLE precompact with a null
        # restored_payload empty; a measurable=False capture is NOT empty (it is a
        # documented exception routed by branch 1b), and an un-measured null payload
        # is left for branch 1b to surface, never as a bare empty here.
        if cap.get("measurable") is False:
            return False
        return cap.get("restored_payload") is None
    if dim_id == "isolation":
        return (
            "slug_a_writes_visible_to_b" not in cap or "landed_only_in_a" not in cap
        )
    # Unknown dimension: be conservative — an unrecognized non-empty dict is not
    # treated as empty (the registry is the single source; an orphan id is caught
    # by the drift guard upstream).
    return False


def _intra_pair(dim: Any, cap: Any) -> tuple:
    """Extract the normalized ``({"ids","scores"}, {"ids","scores"})`` intra pair
    for an intra-check dimension (capture + its mandatory `capture_2`).

    Raises InfraError (folded into INFRA upstream) if `capture_2` is absent for an
    intra-check dimension — the second capture is mandatory; its absence is a
    misroute (R-03), never a half-compare against nothing."""
    dim_id = dim.id
    if dim_id == "retrieval":
        c1 = cap.get("queries")
        c2 = cap.get("capture_2")
        if c2 is None:
            raise InfraError(
                "intra",
                "missing capture_2",
                detail="retrieval intra-check requires a second capture",
            )
        return (_retrieval_rank(c1), _retrieval_rank(c2))
    if dim_id == "proactive":
        c2 = cap.get("capture_2")
        if c2 is None:
            raise InfraError(
                "intra",
                "missing capture_2",
                detail="proactive intra-check requires a second capture",
            )
        c1 = {"ids": cap.get("briefing_ids"), "scores": cap.get("briefing_scores")}
        c2_norm = {
            "ids": c2.get("briefing_ids") if isinstance(c2, dict) else None,
            "scores": c2.get("briefing_scores") if isinstance(c2, dict) else None,
        }
        return (c1, c2_norm)
    raise InfraError(
        "intra",
        "no intra-pair for dimension",
        detail=f"{dim_id} is not an intra-check dimension",
    )


def _retrieval_rank(queries: Any) -> dict:
    """Flatten a retrieval queries-list into one ranked ``{"ids","scores"}`` view.

    Retrieval may carry multiple queries; the intra-stability check concatenates
    their result_ids (and scores) into one ranked sequence so a self-divergence in
    ANY query surfaces. Score absence degrades to membership-only in K3."""
    ids: list = []
    scores: list = []
    have_scores = True
    for q in queries or []:
        q_ids = q.get("result_ids") or []
        ids.extend(q_ids)
        q_scores = q.get("scores")
        if q_scores is None:
            have_scores = False
        else:
            scores.extend(q_scores)
    return {"ids": ids, "scores": scores if have_scores and scores else None}


# =============================================================================
# 5. rollup — matrix roll-up (ADR-002 §4): ERROR > RED > GREEN precedence
# =============================================================================
def rollup(results: list[DimensionResult]) -> tuple[str, int]:
    """Reduce per-dimension results to a ``(verdict, exit_code)``.

    Rules (ADR-002 §4):
      GREEN iff every blocks_c0_proof dimension is PARITY_PASS.
      Any PARITY_FAIL                       -> RED.
      Any INFRA_ERROR (even amid passes)    -> ERROR (distinct exit code).
      INTRA_TRANSPORT_NONDETERMINISM        -> recorded; does NOT redden, does NOT
                                               error (a separately-filed GH#746 bug).

    Precedence when multiple classes are present: ERROR > RED > GREEN — an
    INFRA-ERROR means the gate could not MEASURE and must not be reported green or
    red-only; a PARITY-FAIL reddens even alongside an INTRA record (intra never
    masks a real RED)."""
    has_fail = any(r.outcome == Outcome.PARITY_FAIL for r in results)
    has_infra = any(r.outcome == Outcome.INFRA_ERROR for r in results)
    if has_infra:
        return ("ERROR", EXIT_INFRA)  # distinct exit code, never green/red parity
    if has_fail:
        return ("RED", EXIT_PARITY_FAIL)
    return ("GREEN", EXIT_OK)
