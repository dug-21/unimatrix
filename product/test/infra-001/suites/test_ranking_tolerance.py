"""K3 unit tests — ranking_tolerance: the ONE ranking/embedding tolerance policy.

Pure-Python over synthetic id-lists; NO Docker, NO daemon, NO fixtures (Tier A,
the `test_parity_workload.py` off-Docker precedent / #5258 seam). Maps 1:1 to
test-plan/ranking_tolerance.md. Covers R-01 (Critical) + R-06 (corpus depth) +
R-07 (single-source tolerance).

The NEGATIVE tests are LOAD-BEARING: the policy must NOT swallow a real
cross-transport divergence (false-GREEN), and must NOT flake on tolerated HNSW
tail churn (false-RED). The tolerance lives ONLY in (a) tail churn below the
stable prefix and (b) unordered tie-class membership at equal scores.
"""

from harness.ranking_tolerance import (
    STABLE_PREFIX_FLOOR,
    RankingVerdict,
    ranking_parity,
)


# ---------------------------------------------------------------------------
# Floor constant (R-06 / NFR-7) — non-degenerate, > 1
# ---------------------------------------------------------------------------


def test_ranking_parity_floor_constant_gt_one():
    """The configured stable-prefix floor N MUST be > 1 (non-degenerate): a single
    or two-hit ranking cannot vacuously pass (R-06 / NFR-7)."""
    assert isinstance(STABLE_PREFIX_FLOOR, int)
    assert STABLE_PREFIX_FLOOR > 1


# ---------------------------------------------------------------------------
# Stable-prefix signal (R-01 positive — tail churn tolerated)
# ---------------------------------------------------------------------------


def test_ranking_parity_deep_prefix_match_tail_churn_matched():
    """Identical in a deep leading prefix, churned in the tail (membership AND order
    differ below the prefix) -> matched True, stable_prefix_len >= floor, tail churn
    recorded (NOT failed). The HNSW-approximate-tail tolerance."""
    https = ["a", "b", "c", "d", "x", "y", "z"]
    uds = ["a", "b", "c", "d", "q", "r"]
    https_scores = [0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3]
    uds_scores = [0.9, 0.8, 0.7, 0.6, 0.45, 0.35]
    v = ranking_parity(https, uds, scores=(https_scores, uds_scores))
    assert v.matched is True
    assert v.stable_prefix_len >= STABLE_PREFIX_FLOOR
    assert v.stable_prefix_len == 4
    # The churned tail ids are recorded for evidence, not failed.
    assert set(v.tail_churn) >= {"x", "y", "z", "q", "r"}


def test_ranking_parity_identical_lists_full_prefix():
    """Byte-identical lists -> matched True, prefix == len(list), empty tail."""
    ids = ["a", "b", "c", "d"]
    scores = [0.9, 0.8, 0.7, 0.6]
    v = ranking_parity(ids, list(ids), scores=(list(scores), list(scores)))
    assert v.matched is True
    assert v.stable_prefix_len == len(ids)
    assert v.tail_churn == []


# ---------------------------------------------------------------------------
# In-prefix divergence (R-01 negative — the divergence must NOT be swallowed)
# ---------------------------------------------------------------------------


def test_ranking_parity_in_prefix_divergence_not_matched():
    """Two lists that diverge WITHIN the stable prefix (position 2 differs, N>2) ->
    matched False. A real PARITY-FAIL candidate, never tolerated. LOAD-BEARING
    false-GREEN guard."""
    https = ["a", "b", "c", "d", "e"]
    uds = ["a", "b", "ZZ", "d", "e"]
    https_scores = [0.9, 0.8, 0.7, 0.6, 0.5]
    uds_scores = [0.9, 0.8, 0.7, 0.6, 0.5]
    v = ranking_parity(https, uds, scores=(https_scores, uds_scores))
    assert v.matched is False
    # The stable prefix ends at the divergence band (the first two agreed).
    assert v.stable_prefix_len == 2


def test_ranking_parity_reordered_within_prefix_not_matched():
    """Same members reordered inside the prefix (DISTINCT scores -> singleton bands)
    -> matched False. The prefix is ORDER-identical, not merely membership-identical."""
    https = ["a", "b", "c", "d"]
    uds = ["a", "c", "b", "d"]
    https_scores = [0.9, 0.8, 0.7, 0.6]
    uds_scores = [0.9, 0.8, 0.7, 0.6]
    v = ranking_parity(https, uds, scores=(https_scores, uds_scores))
    assert v.matched is False


# ---------------------------------------------------------------------------
# Tie-class handling (R-01 — equal-score / #2610 / sort_unstable)
# ---------------------------------------------------------------------------


def test_ranking_parity_tie_class_permuted_matched():
    """A run of equal-score ids permuted between legs -> tie-class membership equal,
    position ignored -> matched True; tie_classes records the class boundaries."""
    https = ["a", "b", "c", "d"]
    uds = ["a", "c", "b", "d"]  # b/c permuted within an equal-score tie-class
    https_scores = [0.9, 0.8, 0.8, 0.6]
    uds_scores = [0.9, 0.8, 0.8, 0.6]
    v = ranking_parity(https, uds, scores=(https_scores, uds_scores))
    assert v.matched is True
    assert v.stable_prefix_len == 4
    # The {b, c} equal-score tie-class is recorded by membership, not position.
    assert ["b", "c"] in v.tie_classes


def test_ranking_parity_tie_class_missing_member_not_matched():
    """A tie-class with a MISSING member on one leg (different set, not just order)
    -> matched False. A member-loss inside a tie-class is a REAL divergence, not a
    tolerated permutation. NEGATIVE guard."""
    https = ["a", "b", "c", "d"]
    uds = ["a", "b", "QQ", "d"]  # c -> QQ inside the {b,c}/{b,QQ} tie-class band
    https_scores = [0.9, 0.8, 0.8, 0.6]
    uds_scores = [0.9, 0.8, 0.8, 0.6]
    v = ranking_parity(https, uds, scores=(https_scores, uds_scores))
    assert v.matched is False


def test_ranking_parity_tie_straddles_prefix_boundary():
    """Edge: a tie-class straddling the boundary classifies its in-prefix members as
    a tie-class deterministically (no positional flake). The shared bands agree by
    membership; surplus below is tail."""
    https = ["a", "b", "c", "d", "e"]
    uds = ["a", "c", "b", "d", "f"]  # {b,c} permuted tie; tail e vs f differs
    https_scores = [0.9, 0.8, 0.8, 0.7, 0.6]
    uds_scores = [0.9, 0.8, 0.8, 0.7, 0.55]
    v = ranking_parity(https, uds, scores=(https_scores, uds_scores))
    # Bands {a}, {b,c}, {d} agree by membership (prefix 4 >= floor); the singleton
    # tail {e}/{f} differs and is tolerated HNSW-tail churn beyond a non-trivial
    # prefix -> matched True; the in-prefix {b,c} tie-class is classified by
    # membership (no positional flake), the over-boundary {e}/{f} as tail.
    assert v.matched is True
    assert v.stable_prefix_len == 4  # a + {b,c} + d
    assert ["b", "c"] in v.tie_classes
    assert set(v.tail_churn) >= {"e", "f"}
    # Determinism: re-running yields the identical verdict (no iteration-order flake).
    v2 = ranking_parity(https, uds, scores=(https_scores, uds_scores))
    assert (v2.matched, v2.stable_prefix_len, v2.tie_classes) == (
        v.matched,
        v.stable_prefix_len,
        v.tie_classes,
    )


# ---------------------------------------------------------------------------
# Scores-absent fallback (R-01 scenario 4 — documented, not silent loosening)
# ---------------------------------------------------------------------------


def test_ranking_parity_scores_absent_membership_only_fallback():
    """scores=None -> membership-only fallback on the prefix; matched reflects
    prefix-membership equality and the verdict MARKS the fallback (scores_absent,
    empty tie_classes). The JUSTIFIED documented path, NOT a silent loosening."""
    https = ["a", "b", "c", "d", "x"]
    uds = ["a", "b", "c", "d", "y"]
    v = ranking_parity(https, uds, scores=None)
    assert v.matched is True
    assert v.stable_prefix_len == 4
    assert v.scores_absent is True
    assert v.tie_classes == []
    assert set(v.tail_churn) >= {"x", "y"}


def test_ranking_parity_scores_absent_head_divergence_not_matched():
    """scores=None and a HEAD (in-prefix) identity difference -> prefix 0 ->
    matched False. The fallback still fails a real order divergence (not a silent
    loosening)."""
    https = ["a", "b", "c"]
    uds = ["ZZ", "b", "c"]
    v = ranking_parity(https, uds, scores=None)
    assert v.matched is False
    assert v.stable_prefix_len == 0
    assert v.scores_absent is True


def test_ranking_parity_misaligned_scores_fall_back():
    """Misaligned score/id lengths -> treated as scores-absent fallback for the
    region (recorded via scores_absent), never a silent loosen."""
    https = ["a", "b", "c", "d"]
    uds = ["a", "b", "c", "d"]
    v = ranking_parity(https, uds, scores=([0.9, 0.8], [0.9, 0.8, 0.7, 0.6]))
    assert v.scores_absent is True
    assert v.matched is True
    assert v.stable_prefix_len == 4


# ---------------------------------------------------------------------------
# Prefix-floor boundary (R-01 scenario 5 + R-06) — caller asserts the floor
# ---------------------------------------------------------------------------


def test_ranking_parity_prefix_floor_exactly_N_passes():
    """Prefix length exactly N (the floor) -> eligible to pass: matched True AND
    stable_prefix_len >= STABLE_PREFIX_FLOOR (the caller's floor guard admits it)."""
    n = STABLE_PREFIX_FLOOR
    head = [f"id{i}" for i in range(n)]
    https = head + ["tail_h"]
    uds = head + ["tail_u"]
    https_scores = [1.0 - i * 0.1 for i in range(len(https))]
    uds_scores = [1.0 - i * 0.1 for i in range(len(uds))]
    v = ranking_parity(https, uds, scores=(https_scores, uds_scores))
    assert v.matched is True
    assert v.stable_prefix_len == n
    assert v.stable_prefix_len >= STABLE_PREFIX_FLOOR


def test_ranking_parity_prefix_floor_N_minus_1_errors():
    """Stable prefix shorter than the floor N: the policy reports a sub-floor prefix
    the orchestrator converts to INFRA-ERROR (R-06). Assert stable_prefix_len <
    STABLE_PREFIX_FLOOR so a single-hit/short ranking cannot vacuously pass — and the
    floor itself is > 1 (non-degenerate)."""
    n = STABLE_PREFIX_FLOOR
    head = [f"id{i}" for i in range(n - 1)]
    https = head + ["DIV_H", "z"]
    uds = head + ["DIV_U", "z"]  # diverge right after the (N-1)-long agreed head
    https_scores = [1.0 - i * 0.1 for i in range(len(https))]
    uds_scores = [1.0 - i * 0.1 for i in range(len(uds))]
    v = ranking_parity(https, uds, scores=(https_scores, uds_scores))
    # The agreed leading run is only N-1 -> below the floor; the caller rejects it.
    assert v.stable_prefix_len == n - 1
    assert v.stable_prefix_len < STABLE_PREFIX_FLOOR
    # In-prefix-region divergence is NOT a pass either way.
    assert v.matched is False


# ---------------------------------------------------------------------------
# Edge cases (from Risk Strategy Edge Cases)
# ---------------------------------------------------------------------------


def test_ranking_parity_empty_one_leg_not_matched():
    """Empty list on one leg, non-empty other -> matched False (degenerate, NOT a
    vacuous empty-equals pass)."""
    v = ranking_parity([], ["a", "b", "c"], scores=None)
    assert v.matched is False
    assert v.degenerate is True
    v2 = ranking_parity(["a", "b", "c"], [], scores=None)
    assert v2.matched is False
    assert v2.degenerate is True


def test_ranking_parity_both_empty():
    """Both empty -> NOT a silent pass: the verdict FLAGS a degenerate/zero-length
    ranking (stable_prefix_len 0, degenerate True) the orchestrator routes to
    INFRA-ERROR (R-06), not PASS. A zero-length prefix is below the floor."""
    v = ranking_parity([], [], scores=None)
    assert v.degenerate is True
    assert v.stable_prefix_len == 0
    assert v.stable_prefix_len < STABLE_PREFIX_FLOOR


# ---------------------------------------------------------------------------
# Single-source assertion (R-07 scenario 4 / SR-03)
# ---------------------------------------------------------------------------


def test_ranking_parity_single_sourced_no_second_policy():
    """`ranking_parity` is THE one callable both embedding-ranked comparators
    (RetrievalComparator, BriefingComparator) import — no second tie policy exists.
    Asserted here against the policy module itself; the cross-module import
    assertion lives with parity_comparator (Wave B). A change to this policy changes
    both consumers atomically (#5302 at architecture level)."""
    import harness.ranking_tolerance as rt

    assert callable(rt.ranking_parity)
    # The SAME callable used cross-leg is reused intra-leg (K4 intra_transport_stable)
    # so intra and cross share ONE tolerance — identity, not a copy.
    assert ranking_parity is rt.ranking_parity


def test_ranking_verdict_shape():
    """RankingVerdict carries the four ADR-004 evidence fields."""
    v = RankingVerdict(matched=True, stable_prefix_len=3, tail_churn=["x"], tie_classes=[["a"]])
    assert v.matched is True
    assert v.stable_prefix_len == 3
    assert v.tail_churn == ["x"]
    assert v.tie_classes == [["a"]]
