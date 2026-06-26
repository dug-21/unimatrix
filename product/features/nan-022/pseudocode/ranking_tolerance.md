# K3 — Ranking tolerance policy (`harness/ranking_tolerance.py`)

**New**, pure-Python, stdlib-only, off-Docker unit-testable. ADR-004 (#5315).

## Purpose

The ONE embedding/ranking tolerance policy, single-sourced across the two embedding-ranked
dimensions (retrieval D1 + briefing D4). One place defines "what counts as a ranking match"
(SR-03/NFR-4). No second tie policy may exist.

The parity signal is the STABLE RANKED PREFIX: the longest leading run of result ids that is
order-identical across legs. Churn BELOW the prefix (the HNSW-approximate tail, #4990/GH#746) is
tolerated per the closed policy — not a parity defect. Ties (equal score) compare as an
UNORDERED tie-class, not positionally (#2610 / `sort_unstable`).

## Type

```
@dataclass
class RankingVerdict:
    matched: bool            # True iff the stable prefix is order-identical (tie-classes unordered)
    stable_prefix_len: int   # length of the order-identical leading run (tie-classes count as one block)
    tail_churn: list         # ids that differ below the prefix (recorded, not failed)
    tie_classes: list        # the tie-class boundaries derived from scores (for evidence)
```

## Constant

```
# NFR-7 non-degenerate floor: the stable prefix must be at least this long for a NON-vacuous
# parity signal. Below this the corpus is degenerate and the dimension is INFRA-ERROR (R-06),
# NOT a vacuous pass. The concrete value is an OQ-3/OQ-C test-design call (see open questions);
# pseudocode fixes the SHAPE: STABLE_PREFIX_FLOOR > 1.
STABLE_PREFIX_FLOOR = <N, N > 1>   # resolved in Stage 3a test design; default proposal: 3
```

NOTE: `ranking_parity` itself does NOT enforce the floor — it reports `stable_prefix_len`. The
floor is asserted by the CALLER (the leg-driver degenerate-corpus guard + the orchestrator),
so the policy stays a pure comparison and the floor stays one assertion point (R-06). This file
exposes `STABLE_PREFIX_FLOOR` for those callers to reference (single source for N).

## Function

```
def ranking_parity(https_ids: list, uds_ids: list, *, scores=None) -> RankingVerdict:
    """Compare two ranked id-lists by stable-prefix equality with unordered tie-classes.

    scores: optional tuple (https_scores, uds_scores) aligned to the id-lists. When present,
            equal-adjacent-score runs form tie-classes that compare as unordered sets. When
            absent (or None), the policy degrades to MEMBERSHIP-ONLY on the prefix — a
            DOCUMENTED justified fallback (R-01 scenario 4), not a silent loosening.
    """
```

### Algorithm

```
1. If scores is None or either score list is missing/empty:
     -> membership-only fallback path (documented):
        walk positions i = 0,1,2,...; at each i require https_ids[i] == uds_ids[i] by IDENTITY.
        stable_prefix_len = first i where they differ (or min length if all equal).
        matched = (stable_prefix_len == len of the compared prefix region we required).
        Record the absence-of-scores fallback in tie_classes = [] and proceed.
   Else: scores present -> tie-class path:
2. Derive tie-classes per leg: group adjacent positions with EQUAL score into a class.
   A tie-class is an UNORDERED set of ids sharing one score at one rank band.
3. Walk the ranked lists class-by-class from the top:
     - For each aligned tie-class band, compare the two legs' class MEMBERSHIP as sets.
     - If the membership sets are EQUAL: the band is order-identical-modulo-ties; advance;
       add its size to stable_prefix_len; record the class in tie_classes.
     - If a band's membership differs (a tie-class with a MISSING/EXTRA member): the stable
       prefix ENDS here. This is a real prefix divergence -> matched=False candidate.
     - If a SINGLETON position differs by identity: the stable prefix ends; matched=False.
4. Everything below the first divergence band is tail_churn (recorded, tolerated).
5. matched = True iff the divergence (step 3) occurred ONLY at/below the prefix boundary AND
   the boundary is not WITHIN a band that both legs agree on. Concretely:
     - matched=True  when the two lists agree (modulo tie-class membership) for the entire
       leading region they share, OR they diverge only in the tail beyond a non-trivial prefix.
     - matched=False when they diverge WITHIN the stable prefix (a real cross-transport
       ranking difference — R-01 scenario 2) or a tie-class loses/gains a member (scenario 3).
6. Return RankingVerdict(matched, stable_prefix_len, tail_churn, tie_classes).
```

### Critical disposition (R-01, load-bearing)

`matched=False` on an in-prefix divergence MUST surface as a real PARITY-FAIL candidate — the
tolerance can NEVER be set so loose (prefix trivially short) that a genuine cross-leg prefix
difference greens. The tolerance lives ONLY in: (a) tail churn below the prefix, (b) unordered
tie-class membership at equal scores. It does NOT tolerate an in-prefix identity difference.

The same `ranking_parity` is used by K4 `intra_transport_stable` (a leg's capture vs its
`capture_2`) so intra and cross use ONE tolerance (R-07 scenario 4 — no second tolerance).

## Data flow

- INPUT: two ranked id lists (+ optional aligned scores) from a dimension capture.
- OUTPUT: `RankingVerdict` consumed by `RetrievalComparator`/`BriefingComparator` (cross-leg) and
  by `intra_transport_stable` (intra-leg).

## Error handling

- Empty/zero-length list on EITHER side: `ranking_parity` returns `matched=True` only if BOTH
  are empty (degenerate equality) — but the caller's degenerate-corpus guard (R-06) rejects a
  result set shorter than `STABLE_PREFIX_FLOOR` as INFRA-ERROR before this is read as a pass.
  The policy itself does not raise; it reports.
- Misaligned score/id lengths: treat as scores-absent fallback for the misaligned region;
  record the fallback (never silently loosen).

## Key test scenarios (hints)

- Deep prefix match, churned tail -> `matched=True`, `stable_prefix_len >= N`, tail in
  `tail_churn` (R-01 scenario 1).
- In-prefix divergence -> `matched=False` (R-01 scenario 2 — a real PARITY-FAIL candidate).
- Equal-score tie-class permuted between legs -> tie-class membership equal -> `matched=True`;
  tie-class with a MISSING member -> `matched=False` (R-01 scenario 3).
- Scores absent -> membership-only fallback; assert it is the documented path, not a silent
  loosening (R-01 scenario 4).
- Boundary: prefix length exactly N vs N-1 (couples to R-06 floor; tested at the caller).
- `ranking_parity` is the SAME callable used by intra-stability (single-sourced — R-07 sc.4).
