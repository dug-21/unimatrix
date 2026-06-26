# Test Plan: K3 — `harness/ranking_tolerance.py`

Covers **R-01 (Critical)** + R-06 (corpus depth), R-07 (single-source tolerance). The ONE
ranking tolerance policy single-sourced across retrieval (D1) + briefing (D4). This is THE
dimension where "measured parity" is most tempted to soften into "tolerant parity" — the
**negative tests are load-bearing**: the policy must NOT swallow a real cross-transport
divergence (false-GREEN), and must NOT flake on tolerated HNSW tail churn (false-RED).

Surface under test:
- `ranking_parity(https_ids: list, uds_ids: list, *, scores=None) -> RankingVerdict`
- `RankingVerdict(matched: bool, stable_prefix_len: int, tail_churn: list, tie_classes: list)`

Tier: **A (off-Docker unit)** — pure-Python over synthetic id-lists, no Docker/daemon/fixture
(the `test_parity_workload.py` precedent). File: `suites/test_ranking_tolerance.py`.

## Unit Test Expectations

### Stable-prefix signal (R-01 positive — tail churn tolerated)
- `test_ranking_parity_deep_prefix_match_tail_churn_matched`: lists identical in a deep leading
  prefix but churned in the tail (membership AND order differ below the prefix) → assert
  `verdict.matched is True`, `verdict.stable_prefix_len >= N` (the NFR-7 floor), `verdict.tail_churn`
  records the churned ids (recorded, NOT failed). This is the HNSW-approximate-tail tolerance.
- `test_ranking_parity_identical_lists_full_prefix`: byte-identical lists → `matched True`,
  `stable_prefix_len == len(list)`, empty `tail_churn`.

### In-prefix divergence (R-01 negative — the divergence must NOT be swallowed)
- `test_ranking_parity_in_prefix_divergence_not_matched`: two lists that diverge WITHIN the
  stable prefix (e.g. position 2 differs while N>2) → assert `verdict.matched is False`. This
  MUST surface as a real PARITY-FAIL candidate, never tolerated. **Load-bearing false-GREEN guard.**
- `test_ranking_parity_reordered_within_prefix_not_matched`: same members, reordered inside the
  prefix → `matched is False` (the prefix is ORDER-identical, not just membership-identical).

### Tie-class handling (R-01 — equal-score / #2610 / sort_unstable)
- `test_ranking_parity_tie_class_permuted_matched`: a run of equal-score ids permuted between
  legs → tie-class membership equal, position ignored → `matched is True`; `verdict.tie_classes`
  records the class boundaries derived from `scores`.
- `test_ranking_parity_tie_class_missing_member_not_matched`: a tie-class with a MISSING member
  on one leg (different set, not just different order) → `matched is False`. **Negative guard:**
  a member-loss inside a tie-class is a real divergence, not a tolerated permutation.
- `test_ranking_parity_tie_straddles_prefix_boundary`: edge case — the last in-prefix position is
  part of a tie-class straddling the boundary; assert the policy classifies the in-prefix members
  as a tie-class and the over-boundary members as tail, deterministically (no positional flake).

### Scores-absent fallback (R-01 scenario 4 — documented, not silent loosening)
- `test_ranking_parity_scores_absent_membership_only_fallback`: `scores=None` → policy degrades
  to membership-only on the prefix; assert `matched` reflects prefix-membership equality and the
  verdict marks the fallback explicitly (e.g. empty `tie_classes` + documented path). Assert this
  is the JUSTIFIED documented fallback, NOT a silent loosening of the order signal.

### Prefix-floor boundary (R-01 scenario 5 + R-06)
- `test_ranking_parity_prefix_floor_exactly_N_passes`: prefix length exactly N → eligible to pass.
- `test_ranking_parity_prefix_floor_N_minus_1_errors`: stable prefix shorter than the NFR-7 floor
  N → the policy does NOT pass on a sub-floor prefix (`matched is False` OR a documented
  degenerate signal the orchestrator converts to INFRA-ERROR per R-06). Assert the floor N is
  asserted > 1 (non-degenerate) — a single-hit ranking cannot vacuously pass.
- `test_ranking_parity_floor_constant_gt_one`: assert the configured floor N constant is > 1.

### Edge cases (from Risk Strategy Edge Cases)
- `test_ranking_parity_empty_one_leg_not_matched`: empty list on one leg, non-empty other →
  `matched is False` (degenerate → not a vacuous empty-equals pass).
- `test_ranking_parity_both_empty`: both empty → assert this is NOT a silent pass; the verdict
  flags a degenerate/zero-length ranking the orchestrator routes to INFRA-ERROR (R-06), not PASS.

## Single-source assertion (R-07 scenario 4 / SR-03)
- `test_ranking_parity_single_sourced_no_second_policy`: assert `RetrievalComparator` and
  `BriefingComparator` (in `parity_comparator.py`) both import THIS `ranking_parity` — no second
  tie policy exists. (Cross-module import assertion; pairs with `parity_comparator.md`.) A change
  to the policy changes both consumers atomically (#5302 at architecture level).

## Coverage Requirement (from R-01)
The single `ranking_parity` policy is exercised off-Docker across deep-prefix-match,
in-prefix-divergence (NEG), tie-class permutation, tie-class-member-loss (NEG), scores-absent
fallback, and the prefix-floor boundary BEFORE any tag round. Tolerance contents map to an
enumerated justified `EXCLUDED` entry on the consuming comparators (AC-09, see
`parity_comparator.md`). Floor N asserted > 1. At first live run (Tier C) the tolerance is
scrutinized against a real cross-transport divergence to prove it cannot swallow one; an
unreachable exact-order requirement is a FILED BUG + documented C0 exception, never a silent
widening (NFR-8, product/human-signed only).
