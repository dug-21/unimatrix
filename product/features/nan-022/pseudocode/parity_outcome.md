# K4 — Outcome-class model (`harness/parity_outcome.py`)

**New**, pure-Python, stdlib-only, off-Docker unit-testable. ADR-002 (#5313).

## Purpose

The four-valued per-dimension verdict, the classifier with the FIXED order INFRA -> INTRA ->
PARITY, the double-capture-and-diff intra-transport stability check, and the matrix-level
roll-up rules (§4). Structurally separates cross-transport divergence from intra-transport
nondeterminism from transport-infra errors — none can masquerade as another (SR-01/02/04).

## Imports

```
from harness.parity_comparator import ParityMismatch
from harness.ranking_tolerance import ranking_parity   # the ONE tolerance (intra reuse)
from harness.transport_health import InfraError        # K5
```

## Types

```
class Outcome(Enum):
    PARITY_PASS = "PARITY_PASS"
    PARITY_FAIL = "PARITY_FAIL"
    INFRA_ERROR = "INFRA_ERROR"
    INTRA_TRANSPORT_NONDETERMINISM = "INTRA_TRANSPORT_NONDETERMINISM"

@dataclass
class DimensionResult:
    dimension: str        # Dimension.id
    outcome: Outcome
    diffs: list           # the comparator diff list (empty unless PARITY_FAIL)
    detail: str           # human-readable: which guard tripped / which leg intra-unstable /
                          #   D5 host_side_gap call-out / ParityMismatch summary
```

## `intra_transport_stable`

```
def intra_transport_stable(cap_a, cap_b, *, tolerance) -> bool:
    """Double-capture-and-diff: a single leg captured its dimension output TWICE in the same
    drive; is the leg self-stable (modulo the SAME tolerance the cross-leg compare uses)?

    `tolerance` selects the comparison mode for ranked dimensions vs exact dimensions:
      - ranked (retrieval/proactive): cap_a/cap_b are the two ranked id captures; stability is
        ranking_parity(cap_a_ids, cap_b_ids, scores=...).matched  (the SAME K3 policy — R-07 sc.4).
      - exact (non-intra dimensions): not called (intra_transport_check=False).
    Returns True if the leg is self-stable; False if it self-diverges WITHIN the stable prefix.
    """
```

Single-source rule (R-07 scenario 4): intra-diff uses the SAME `ranking_parity` as the cross-leg
compare. There is NO second tolerance. A leg that self-diverges in the tail only is intra-STABLE
(tolerated); a leg that self-diverges WITHIN the stable prefix is intra-UNSTABLE.

## `classify_dimension` — the FIXED-ORDER classifier

```
def classify_dimension(dim, cap_uds, cap_https) -> DimensionResult:
    """Produce exactly ONE Outcome for a dimension. Order is FIXED: INFRA -> INTRA -> PARITY.
    cap_uds / cap_https are the per-dimension capture dicts (dimension_bundle[dim.capture_key]).
    """

    # ---- 1. INFRA: missing / empty / null / un-ingestable capture ----
    for leg, cap in (("uds", cap_uds), ("https", cap_https)):
        if cap is None:
            # null is permitted ONLY for D5 with measurable=False (handled below); any other
            # null capture is INFRA-ERROR (R-09, never empty-pass).
            if dim.id == "precompact":
                continue   # defer to the measurability branch
            return DimensionResult(dim.id, Outcome.INFRA_ERROR, [], f"{leg} capture missing/null")
        if _capture_is_empty(dim, cap):
            return DimensionResult(dim.id, Outcome.INFRA_ERROR, [], f"{leg} capture empty (R-03/R-04 — never empty-pass)")

    # ---- 1b. D5 measurability branch (ADR-006) ----
    if dim.id == "precompact":
        m_uds  = cap_uds.get("measurable", False)
        m_https = cap_https.get("measurable", False)
        if not (m_uds and m_https):
            gap = cap_uds.get("host_side_gap") or cap_https.get("host_side_gap") or "host-side component not test-only-drivable"
            # DOCUMENTED-EXCEPTION call-out — NOT a pass, NOT a parity RED. Recorded distinctly.
            return DimensionResult(dim.id, Outcome.INFRA_ERROR, [],
                                   f"DOCUMENTED MEASURABILITY LIMITATION: {gap} (D5 measured-where-drivable; NEVER rounded up to fully-measured)")
        # both measurable -> fall through to PARITY compare with non-null payloads.

    # ---- 2. INTRA: double-capture-and-diff for intra-check dimensions ----
    if dim.intra_transport_check:
        for leg, cap in (("uds", cap_uds), ("https", cap_https)):
            cap_1, cap_2 = _intra_pair(dim, cap)   # the capture + its capture_2
            if not intra_transport_stable(cap_1, cap_2, tolerance="ranked"):
                # this leg is intra-unstable (HNSW/tie flip #4990) — routed OUT of the red gate.
                return DimensionResult(dim.id, Outcome.INTRA_TRANSPORT_NONDETERMINISM, [],
                                       f"{leg} leg self-diverged within the stable prefix (file GH#746; NOT a cross-transport defect)")

    # ---- 3. PARITY: both legs captured + intra-stable -> cross-leg compare ----
    comparator = dim.comparator()
    try:
        diffs = comparator.compare(cap_https, cap_uds)   # raises ParityMismatch on non-excluded diff
    except ParityMismatch as e:
        return DimensionResult(dim.id, Outcome.PARITY_FAIL, e.diffs,
                               "cross-transport divergence — REAL C0 defect; file GH bug, gate stays RED (AC-10)")
    return DimensionResult(dim.id, Outcome.PARITY_PASS, diffs, "parity clean modulo closed exclusion set")
```

The order is LOAD-BEARING (R-07 scenario 3): a cross-leg divergence on two intra-STABLE legs can
NEVER be reclassified as INTRA — INTRA only fires when a SINGLE leg self-diverges; if both legs
are intra-stable the classifier always proceeds to the cross-compare. A too-loose intra detector
that lets a cross-divergent-but-self-stable leg be called intra-unstable would silently drop the
defect — forbidden; intra-stability is decided per leg against ITS OWN second capture, never
against the other leg.

## Helpers

```
def _capture_is_empty(dim, cap) -> bool:
    # per-dimension emptiness predicate against the capture shape:
    #   retrieval: no queries, or any query result_ids shorter than STABLE_PREFIX_FLOOR (R-06 degenerate)
    #   behavioral: empty topic_signals
    #   analytics: missing/empty metric_vector (the nan-021 assert_non_empty precondition)
    #   proactive: empty briefing_ids, or shorter than STABLE_PREFIX_FLOOR (R-06)
    #   precompact: (only reached when measurable=True) null restored_payload
    #   isolation: missing either boolean key
    # Imports STABLE_PREFIX_FLOOR from ranking_tolerance (single source for N).

def _intra_pair(dim, cap) -> tuple:
    # extract (capture, capture_2) for an intra-check dimension:
    #   retrieval: (cap["queries"], cap["capture_2"])   # both are query-result lists
    #   proactive: ({"ids":cap["briefing_ids"],"scores":cap["briefing_scores"]}, cap["capture_2"])
    # raises InfraError-equivalent (handled as INFRA upstream) if capture_2 absent for an
    # intra-check dimension (the second capture is mandatory — its absence is a misroute, R-03).
```

## `rollup` — matrix roll-up (§4)

```
def rollup(results: list[DimensionResult]) -> tuple[str, int]:
    """Reduce per-dimension results to a (verdict, exit_code). §4 rules:
       GREEN iff every dimension is PARITY_PASS.
       Any PARITY_FAIL  -> RED.
       Any INFRA_ERROR  -> ERROR (distinct exit code), even if no PARITY_FAIL.
       INTRA_TRANSPORT_NONDETERMINISM -> recorded, does NOT redden, does NOT error.
    Precedence for the exit code when multiple classes present: ERROR > RED > GREEN
    (an INFRA-ERROR means the gate could not measure; it must not be reported green or red-only).
    """
    has_fail  = any(r.outcome == Outcome.PARITY_FAIL for r in results)
    has_infra = any(r.outcome == Outcome.INFRA_ERROR for r in results)
    if has_infra:
        return ("ERROR", EXIT_INFRA)        # distinct exit code, never green/red parity
    if has_fail:
        return ("RED", EXIT_PARITY_FAIL)
    return ("GREEN", EXIT_OK)
```

Exit codes are distinct constants (`EXIT_OK=0`, `EXIT_PARITY_FAIL=1`, `EXIT_INFRA=<distinct, e.g.
5>`) so the release-gate lane can discriminate a parity RED from a transport ERROR (C-8/FR-18).
Aligns with the existing `run_smoke_gate` exit-code truth table (0/1/3/4); EXIT_INFRA must not
collide with the skip(3)/unacq(4) codes.

## Data flow

- INPUT: per-dimension capture dicts from both legs' `dimension_bundle`; `Dimension` records.
- OUTPUT: `DimensionResult` per dimension; the `(verdict, exit_code)` roll-up consumed by ORCH.

## Error handling

- A `ParityMismatch` from `compare()` is CAUGHT and converted to `PARITY_FAIL` (loud detail).
- A missing/null/empty capture is converted to `INFRA_ERROR` (never propagated as a pass).
- `InfraError` from a capture-extraction helper is converted to `INFRA_ERROR`.
- D5 `measurable=False` -> `INFRA_ERROR` with a DOCUMENTED-EXCEPTION detail string (the roll-up
  surfaces it as an honest call-out, never rounded up to measured — R-08).

## Key test scenarios (hints)

- Classifier order proven INFRA -> INTRA -> PARITY: a cross-divergent pair of two intra-STABLE
  legs is PARITY_FAIL, NEVER reclassified INTRA (R-07 scenario 3 — the most insidious false-GREEN).
- A leg whose two captures differ only in the tolerated tail -> intra-STABLE -> proceeds to
  cross-compare (R-07 sc.1).
- A leg whose two captures differ within the stable prefix -> INTRA_TRANSPORT_NONDETERMINISM,
  routed out of the red gate (R-07 sc.2).
- A null/empty capture for a non-D5 dimension -> INFRA_ERROR (R-09 sc.2).
- D5 measurable=False + non-null host_side_gap -> INFRA_ERROR DOCUMENTED-EXCEPTION, NOT pass,
  NOT silently green (R-08 sc.1).
- D5 measurable=True + two payloads -> normal PARITY_PASS/FAIL (R-08 sc.2).
- `rollup`: one INFRA among passes -> ERROR exit (distinct code), not green, not parity RED
  (R-02 sc.3); one PARITY_FAIL -> RED; an INTRA among passes -> GREEN-with-record (does not redden).
- `intra_transport_stable` uses the SAME `ranking_parity` callable as the cross-compare
  (single-sourced — R-07 sc.4).
