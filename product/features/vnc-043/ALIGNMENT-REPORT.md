# Alignment Report: vnc-043

> Reviewed: 2026-07-05
> Artifacts reviewed:
>   - product/features/vnc-043/architecture/ARCHITECTURE.md
>   - product/features/vnc-043/specification/SPECIFICATION.md
>   - product/features/vnc-043/RISK-TEST-STRATEGY.md
> Also read: SCOPE.md, SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md + strategic goal entries (#4946, #5219, #4678, #4673)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | WARN | Honestly framed as read-surface ergonomics; advances no strategic goal's success criteria directly. `goal:self-learning` label overstates strategic contribution (empty capability coverage, as scope review flagged). Artifacts do NOT overclaim frontier progress. |
| Milestone Fit | PASS | Narrow dispatch + doc + tests. No future-milestone capability pulled forward. |
| Scope Gaps | PASS | All SCOPE goals, AC-01..AC-15, constraints, and open questions Q4/Q5 are carried into the source docs. |
| Scope Additions | WARN | FR-9 applies the new uniform ordering to the depth>1 path too, in mild tension with Goal 3 / AC-02 ("depth>1 exactly as today"). Traceable to SR-03; reconciled as presentation-only (set unchanged, prior order was "arbitrary"). |
| Architecture Consistency | PASS | ARCHITECTURE / SPECIFICATION / RISK share ADR references, FR/AC numbering, insertion point, and the ≥30 fan-in threshold. No divergence. |
| Risk Completeness | PASS | R-01..R-11 map to all SR-01..SR-08; the two structural hazards (promoted load-bearing path, four-point doc drift) are the focus; security/edge/failure sections present. |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE item is addressed. Q4 (snapshot pin) resolved in ARCHITECTURE "no external snapshot exists"; Q5 (max_nodes / realistic fan-in) resolved to a concrete ≥30 test threshold, default `max_nodes` unchanged. |
| Addition | Uniform ordering applied to depth>1 (FR-9 / ADR-003 vnc-043) | SCOPE Goal 3 / AC-02 says depth>1 is "exactly as today / behaviorally unchanged." FR-9 pins a new deterministic sort on BOTH depths. Not a free-lance addition: it directly implements SCOPE-RISK SR-03's recommendation ("apply it to BOTH depth-1 and depth>1 so callers see one contract"). Reconciled as presentation-only — `fetch_nodes_batch` already documented "arbitrary order," so the returned SET is unchanged. See Variance #1. |
| Simplification | Reuse `subgraph_via_db` instead of a dedicated depth-1 helper | Rationale documented (SCOPE Open Q2 resolved, ARCHITECTURE ADR-001): the existing live-SQL path already satisfies filter/dedup/dangling/hydration/metadata/`max_nodes`. Acceptable; risk of promoting a cold-start-only branch to load-bearing is explicitly covered (SR-02 → R-04). |

## Variances Requiring Approval

No FAIL-level or blocking variances. Two items for human awareness (both WARN):

1. **Depth>1 ordering change vs "exactly as today"**
   - **What**: FR-9 / ADR-003 applies the new node/edge sort to the depth>1 path, whose SCOPE contract (Goal 3, AC-02) is "behaviorally unchanged."
   - **Why it matters**: Milestone-discipline / scope-fidelity. A strict byte-order regression on depth>1 output could red-bar, and a caller depending on the prior (arbitrary) serialization order would observe a change.
   - **Recommendation**: Accept. It is the deliberate SR-03 resolution (one ordering contract across both depths), the SET is unchanged, and ARCHITECTURE already directs the tester to sweep existing depth>1 fixed-order tests and update them as presentation-only. Confirm AC-02's "behaviorally unchanged" is read as SET-unchanged, not byte-order-unchanged — the spec should make that explicit so the two clauses do not read as contradictory.

2. **`goal:self-learning` attribution with empty capability coverage**
   - **What**: The feature carries `goal:self-learning` but closes no capability's `done_when`; it is discoverability + freshness ergonomics for the uni-zero §6 capability-board read (confirmed at scope review).
   - **Why it matters**: "User Intent is Authoritative" + honest goal attribution. If ergonomics/tooling features accumulate under a frontier goal label, the goal's delivery signal is diluted.
   - **Recommendation**: Accept and relabel as tooling/infra rather than frontier progress. This is a labeling/tracking correction, not an artifact-rework item — the artifacts themselves are honest (see Vision Alignment finding). The human decides whether to retag the GH issue.

## Detailed Findings

### Vision Alignment
The feature serves Architectural Principle #4 (typed relationship graph — "graph traversal surfaces what vector search alone cannot"): it makes `subgraph` traversal correctly discoverable (the `edge_types`/`direction` doc fix) and live at depth-1. It does not violate Principle #7 (in-memory hot path): the depth-1 live read is the established `neighbors` depth-1 = live / depth>1 = cache asymmetry (ADR-005 vnc-018), the depth-1 path takes no `TypedGraphState` lock (NFR-2 / A3), and the tick cache path is untouched. Principle #5 (graceful degradation) is preserved — the depth>1 cold-start `use_fallback` branch remains (AC-02, R-02).

On strategic-goal advancement: this feature does not move any of the four goals' success criteria. Self-learning intelligence (#5219) is about retrieval-quality-from-usage; this is a read-surface fix. The `goal:self-learning` label therefore overstates contribution — consistent with the scope review's "empty capability coverage" flag.

**Honesty check (the parent's specific ask)**: The artifacts are honest about what this is. SCOPE frames it as the uni-zero §6 capability-board / frontier *query* ergonomics and the #903 context-overflow origin. ARCHITECTURE's header self-labels it "NARROW feature: handler dispatch + doc text + tests … does not invent new contracts." SPECIFICATION's workflow section names the concrete curator read-and-hand-assembly problem. No artifact claims frontier/capability progress, claims to close a capability, or dresses the ergonomics fix as learning-quality improvement. The framing is accurate; the only mismatch is the goal *tag*, not the artifact prose. Hence WARN, not VARIANCE.

### Milestone Fit
Minimal-footprint: ~6-line dispatch insertion, doc text on four edit points, tests. No wire/struct/interface change (NFR-1, AC-10). No capability from a later milestone is built early. PASS.

### Architecture Review
Insertion point is precisely specified (`graph_read_subgraph.rs:162`, exact `max_depth == 1` before the lock block), with the load-bearing ordering rationale (SR-07 → R-01/R-02). ADRs ADR-001/002/003 vnc-043 resolve SR-01/02/03/06/07 and Open Q4/Q5. Integration surface enumerates every reused function with signatures and marks the fixed `SubgraphResponse` shape. Downstream guardrail present ("MUST NOT invent … a `subgraph_sql` helper"). Consistent with SPECIFICATION and RISK.

### Specification Review
FR-1..FR-10 and NFR-1..NFR-7 are each testable and mapped to AC-01..AC-15 with named verification methods. Dual-path SET parity (NFR-3) and load-bearing-path regression (NFR-6) correctly elevate the promoted-branch hazard. OQ-A/B/C are handed to the architect and are answered in ARCHITECTURE — closed loop. The one internal tension (FR-9 depth>1 ordering vs AC-02 "unchanged") is noted above; the spec should state the SET-vs-byte-order reading explicitly.

### Risk Strategy Review
Proportionate to a narrow feature — concentrated on the two real hazards (R-04 promoted load-bearing path, R-07 four-point doc drift), both rated Critical, plus R-03 dual-path SET divergence. Full Scope-Risk traceability table (SR-01..SR-08 → R-01..R-11). Security section correctly bounds the read-only, `require_cap(Read)`-gated, validated-input surface and confirms no new input path. Vision-relevant risks are covered: R-08 guards Principle #7 (no lock on depth-1), R-02 guards Principle #5 (cold-start fallback). PASS.

## Knowledge Stewardship
- Queried: /uni-query-patterns (context_search topic=vision) for alignment patterns -- closest hits #3742 (optional future-branch must match scope-deferral intent -> WARN) and #2298 (config semantic divergence); neither maps to this feature's ergonomics/goal-label situation. No directly reusable prior alignment pattern.
- Stored: nothing novel to store -- the two findings are single-instance. The "internal-tooling feature tagged to a frontier goal with empty capability coverage" observation could generalize into a vision pattern, but one occurrence is insufficient; flag for storage if a 2nd feature exhibits it.
