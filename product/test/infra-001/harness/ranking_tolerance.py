"""K3 — Ranking tolerance policy (ADR-004 / #5315 / SR-01 / SR-03 / R-01) — nan-022.

The ONE embedding/ranking tolerance policy, single-sourced across the two
embedding-ranked dimensions (retrieval D1 + briefing D4). One place defines
"what counts as a ranking match" (SR-03 / NFR-4). No second tie policy may exist
(C-5 single-source the full CONTRACT).

The parity signal is the STABLE RANKED PREFIX: the longest leading run of result
ids that is order-identical across legs (tie-classes count as one unordered
block). Churn BELOW the prefix (the HNSW-approximate tail, #4990 / GH#746) is
tolerated per the closed policy — not a parity defect. Ties (equal score) compare
as an UNORDERED tie-class, not positionally (#2610 / `sort_unstable`).

DISPOSITION (R-01, load-bearing): the tolerance lives ONLY in (a) tail churn below
the prefix and (b) unordered tie-class membership at equal scores. It NEVER
tolerates an in-prefix identity difference or a tie-class member loss/gain — those
surface as `matched=False`, a real PARITY-FAIL candidate. The tolerance can never
be set so loose (prefix trivially short) that a genuine cross-leg prefix difference
greens; the caller's degenerate-corpus guard (R-06) rejects a sub-floor prefix as
INFRA-ERROR before the verdict is read as a pass. An unachievable exact-order
requirement is a FILED BUG + documented C0 exception, never a quiet widening
(NFR-8, product/human-signed only).

Pure-Python, stdlib-only, runnable OFF Docker so its TEETH (R-01) are unit-tested
before any release-gate tag round (the nan-019 #5258 seam). ZERO new runtime deps.
ZERO production-code diff (NFR-1).

The SAME `ranking_parity` callable is used by K4 `intra_transport_stable` (a leg's
capture vs its `capture_2`) so intra and cross use ONE tolerance (R-07 scenario 4).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Optional

# NFR-7 non-degenerate floor: the stable prefix must be at least this long for a
# NON-vacuous parity signal. Below this the corpus is degenerate and the dimension
# is INFRA-ERROR (R-06), NOT a vacuous pass. The concrete value is an OQ-3/OQ-C
# test-design call; the pseudocode fixes the SHAPE: STABLE_PREFIX_FLOOR > 1. The
# delivery-time default proposed by ADR-004 / OQ-C is 3 (non-degenerate; a single
# or two-hit ranking cannot vacuously pass).
#
# NOTE: `ranking_parity` itself does NOT enforce this floor — it reports
# `stable_prefix_len`. The floor is asserted by the CALLER (the leg-driver
# degenerate-corpus guard + the orchestrator) so the policy stays a pure comparison
# and the floor stays ONE assertion point (R-06). This module exposes the constant
# as the SINGLE SOURCE of N for those callers to reference.
STABLE_PREFIX_FLOOR: int = 3


@dataclass
class RankingVerdict:
    """The result of one `ranking_parity` comparison.

    matched           True iff the stable prefix is order-identical across legs
                      (tie-classes compared as unordered membership). In-prefix
                      identity divergence or tie-class member loss => False.
    stable_prefix_len Length of the order-identical leading run, in IDS (tie-class
                      members each count; a matched tie-class band contributes its
                      size). The caller compares this to STABLE_PREFIX_FLOOR.
    tail_churn        Ids that differ at/below the prefix boundary (recorded for
                      evidence, NOT failed) — the HNSW-approximate tail.
    tie_classes       The tie-class boundaries derived from `scores`, for evidence.
                      EMPTY list marks the documented scores-absent membership-only
                      fallback path (NOT a silent loosening).
    scores_absent     True when the policy took the documented membership-only
                      fallback (scores None/empty/misaligned) — surfaced explicitly
                      so a reviewer can see the fallback was justified, not silent.
    degenerate        True when neither leg yields a non-trivial ranking (both empty
                      or a one-sided empty) — the caller routes this to INFRA-ERROR
                      (R-06), never reads it as a parity PASS.
    """

    matched: bool
    stable_prefix_len: int
    tail_churn: list = field(default_factory=list)
    tie_classes: list = field(default_factory=list)
    scores_absent: bool = False
    degenerate: bool = False


def _tie_classes(ids: list, scores: Optional[list]) -> list[list]:
    """Group adjacent positions sharing one EQUAL score into a tie-class band.

    Returns a list of bands, each a list of ids (in their ranked order, but
    compared as an UNORDERED set by `ranking_parity`). When scores are absent or
    misaligned, every id is its own singleton band (membership-only degrades to
    positional identity, which is the documented fallback).
    """
    if not scores or len(scores) != len(ids):
        return [[i] for i in ids]
    bands: list[list] = []
    current: list = []
    current_score: Any = object()  # sentinel that never == a real score
    for idx, score in zip(ids, scores):
        if current and score == current_score:
            current.append(idx)
        else:
            if current:
                bands.append(current)
            current = [idx]
            current_score = score
    if current:
        bands.append(current)
    return bands


def _membership_only(https_ids: list, uds_ids: list) -> RankingVerdict:
    """Documented scores-absent fallback (R-01 scenario 4): walk positions and
    require IDENTITY at each rank. Both lists are non-empty here (the empty/one-empty
    cases are handled by `ranking_parity` before this is reached).

    The longest leading run that is position-identical is the STABLE PREFIX; the
    first positional identity difference ENDS it and everything below is tolerated
    tail churn (the HNSW-approximate tail — no scores to band, so position IS the
    only signal). `matched` is True iff a real leading agreement exists (prefix > 0);
    the caller's R-06 floor guard rejects a sub-`STABLE_PREFIX_FLOOR` prefix as
    INFRA-ERROR, so a trivially short prefix can never vacuously pass.

    This is the JUSTIFIED documented fallback — `scores_absent=True`,
    `tie_classes=[]` — NOT a silent loosening: a head (in-prefix) identity
    difference yields prefix 0 => `matched=False`.
    """
    shared = min(len(https_ids), len(uds_ids))
    prefix = 0
    for i in range(shared):
        if https_ids[i] == uds_ids[i]:
            prefix += 1
        else:
            break
    # tail churn = everything at/below the first divergence on either leg.
    h_tail = list(https_ids[prefix:])
    tail = h_tail + [u for u in uds_ids[prefix:] if u not in h_tail]
    fully_identical = prefix == len(https_ids) == len(uds_ids)
    # Same disposition as the tie-class path (ONE policy): fully-identical lists
    # match; otherwise a divergence is tolerated TAIL churn only beyond a non-trivial
    # (>= floor) prefix, and an in-prefix (short-prefix) divergence is NOT swallowed.
    matched = fully_identical or prefix >= STABLE_PREFIX_FLOOR
    return RankingVerdict(
        matched=matched,
        stable_prefix_len=prefix,
        tail_churn=tail,
        tie_classes=[],
        scores_absent=True,
        degenerate=False,
    )


def ranking_parity(
    https_ids: list,
    uds_ids: list,
    *,
    scores: Optional[tuple] = None,
) -> RankingVerdict:
    """Compare two ranked id-lists by stable-prefix equality with unordered tie-classes.

    The SINGLE ranking tolerance policy (SR-03 / C-5) shared by `RetrievalComparator`,
    `BriefingComparator` (cross-leg) and `intra_transport_stable` (intra-leg).

    Args:
        https_ids: ranked result ids from the HTTPS (or first) leg.
        uds_ids:   ranked result ids from the UDS (or second) leg.
        scores:    optional tuple ``(https_scores, uds_scores)`` aligned 1:1 to the
                   id-lists. When present, equal-adjacent-score runs form tie-classes
                   that compare as unordered sets. When absent/None, or when a score
                   list is missing/empty/misaligned, the policy degrades to the
                   DOCUMENTED membership-only fallback (R-01 scenario 4) — never a
                   silent loosening.

    Returns:
        RankingVerdict — the policy REPORTS; it does NOT raise and does NOT enforce
        the STABLE_PREFIX_FLOOR (that is the caller's one assertion point, R-06).
    """
    # ---- degenerate / empty-list handling (does not raise; reports) -----------
    if not https_ids and not uds_ids:
        # Both empty: degenerate equality. NOT a silent pass — the caller's R-06
        # floor guard rejects a zero-length prefix as INFRA-ERROR. matched reports
        # "no divergence found" but degenerate=True flags it.
        return RankingVerdict(
            matched=True,
            stable_prefix_len=0,
            tail_churn=[],
            tie_classes=[],
            scores_absent=scores is None,
            degenerate=True,
        )
    if not https_ids or not uds_ids:
        # One leg empty, the other not — a real divergence, never a vacuous pass.
        return RankingVerdict(
            matched=False,
            stable_prefix_len=0,
            tail_churn=list(https_ids) + list(uds_ids),
            tie_classes=[],
            scores_absent=scores is None,
            degenerate=True,
        )

    # ---- scores-absent / misaligned -> documented membership-only fallback ----
    https_scores: Optional[list] = None
    uds_scores: Optional[list] = None
    if scores is not None:
        try:
            https_scores, uds_scores = scores
        except (TypeError, ValueError):
            https_scores, uds_scores = None, None
    aligned = (
        https_scores is not None
        and uds_scores is not None
        and len(https_scores) == len(https_ids)
        and len(uds_scores) == len(uds_ids)
        and len(https_scores) > 0
        and len(uds_scores) > 0
    )
    if not aligned:
        return _membership_only(https_ids, uds_ids)

    # ---- tie-class path -------------------------------------------------------
    https_bands = _tie_classes(https_ids, https_scores)
    uds_bands = _tie_classes(uds_ids, uds_scores)

    stable_prefix_len = 0
    tie_classes: list = []
    band_count = min(len(https_bands), len(uds_bands))
    divergence_idx: Optional[int] = None

    for b in range(band_count):
        h_band = https_bands[b]
        u_band = uds_bands[b]
        if set(h_band) == set(u_band):
            # Order-identical modulo ties: advance, record the class boundary.
            stable_prefix_len += len(h_band)
            tie_classes.append(sorted(set(h_band)))
        else:
            # The bands stop agreeing here. Whether this is an IN-PREFIX divergence
            # (matched=False, a real PARITY-FAIL candidate) or tolerated TAIL churn
            # (matched stays True) is decided below by the stable-prefix length
            # against the single-sourced STABLE_PREFIX_FLOOR.
            divergence_idx = b
            break

    if divergence_idx is None:
        # Every shared band agreed. Any surplus bands on the longer leg are below
        # the stable prefix => tolerated tail churn, NOT a divergence.
        tail = []
        for extra in https_bands[band_count:] + uds_bands[band_count:]:
            tail.extend(extra)
        return RankingVerdict(
            matched=True,
            stable_prefix_len=stable_prefix_len,
            tail_churn=tail,
            tie_classes=tie_classes,
            scores_absent=False,
            degenerate=False,
        )

    # Bands diverged at `divergence_idx`. Everything from that band down (on either
    # leg) is recorded as tail churn. The DISPOSITION (R-01, load-bearing):
    #   * divergence WITHIN the stable prefix (the agreed leading run is shorter than
    #     STABLE_PREFIX_FLOOR) is a REAL cross-leg ranking difference -> matched=False
    #     (a PARITY-FAIL candidate the tolerance must NEVER swallow);
    #   * divergence only in the TAIL beyond a non-trivial (>= floor) prefix is the
    #     tolerated HNSW-approximate-tail churn (#4990 / GH#746) -> matched stays True.
    # The boundary between prefix and tail is the ONE single-sourced floor constant;
    # the caller separately rejects a sub-floor corpus as INFRA-ERROR (R-06) using the
    # SAME constant, so a trivially short prefix can never vacuously green.
    tail = []
    for extra in https_bands[divergence_idx:] + uds_bands[divergence_idx:]:
        tail.extend(extra)
    matched = stable_prefix_len >= STABLE_PREFIX_FLOOR
    return RankingVerdict(
        matched=matched,
        stable_prefix_len=stable_prefix_len,
        tail_churn=tail,
        tie_classes=tie_classes,
        scores_absent=False,
        degenerate=False,
    )
