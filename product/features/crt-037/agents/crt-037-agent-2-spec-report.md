# Agent Report: crt-037-agent-2-spec

## Output

`product/features/crt-037/specification/SPECIFICATION.md`

## Key Decisions Made

**FR interpretation: composite guard as typed struct (SR-01 + SR-08)**
The discriminator tag in the merged Phase 7 batch is specified as a Rust tagged union
(`NliCandidatePair`) with compile-time variant matching, not parallel lists indexed by
position. This closes SR-08's failure mode. The `InformsCandidate` record carries all Phase
4b guard metadata so Phase 8b can re-verify without a second DB read. This closes SR-01's
risk of neutral-only false positives by requiring all five guards to hold simultaneously.

**NF-01: tick p95 budget set at +50 ms**
SCOPE.md did not specify the latency bound. The specification derives +50 ms from the crt-029
baseline target of ~200 ms p95, permitting a 25% addition for a second-priority scan. If the
Phase 4b candidate slice is empty, overhead target is < 5 ms. The architect must confirm or
revise this bound.

**FR-11: SR-01 entailment/contradiction exclusion from Informs path**
The spec adds one guard not explicitly in SCOPE.md: if `nli.entailment` exceeds
`supports_edge_threshold` or `nli.contradiction` exceeds `contradicts_edge_threshold`, the
pair must not additionally receive an `Informs` edge. A pair strong enough for
`Supports`/`Contradicts` should be handled by that path exclusively. This closes the
SR-01-adjacent case where a pair might receive both a `Supports` edge (from Phase 8) and an
`Informs` edge (from Phase 8b) if scores straddle both thresholds.

**C-14: Direction::Outgoing contract made explicit**
Entry #3744 confirms that `personalized_pagerank` uses `Direction::Outgoing` for the reverse
walk — not `Direction::Incoming` as pseudocode suggested. This is codified in C-14 as a named
constraint to prevent a future implementer from using `Incoming` and silently getting zero
mass flow from lesson nodes.

**AC-05 direction assertion**
Per SR-07's recommendation, AC-05 is specified to assert that the lesson node **specifically**
receives non-zero PPR mass — not merely that some non-zero score exists anywhere. The test
shape is explicit: two-node graph, `Informs` A→B, seed at B, assert `scores[A] > 0.0`.

## Open Questions for Architect

**OQ-S1 (blocking)**: Confirm `GRAPH_EDGES.relation_type` has no CHECK constraint in current
DDL. If a constraint exists, C-01 is violated and a schema migration is needed.

**OQ-S2 (precision risk)**: Confirm whether `NliScores.neutral` is a direct model logit or
residual (`1 - entailment - contradiction`). If residual, the `neutral > 0.5` threshold may
have lower precision than assumed.

**OQ-S3 (implementation shape)**: Confirm whether `select_source_candidates` returns category
metadata alongside IDs for Phase 4b filtering, or whether a join/secondary lookup is required.

**OQ-S4 (cap split point)**: Specify the exact point in the tick where the
`Supports`/`Contradicts` vs `Informs` cap split is computed to prevent a tick where 100
Supports candidates fully exhaust the budget with no log signal for Informs starvation.

## Constraint Gaps Identified

**Gap 1**: SCOPE.md states the `Informs` write path uses `write_nli_edge` directly "under its
own cap tracking within `max_graph_inference_per_tick`" but does not specify where the cap
accounting register lives. The spec adopts the interpretation that the split happens at batch
merge time (Phase 5), but this needs architectural confirmation to avoid two independent
tallies getting out of sync.

**Gap 2**: The spec adds FR-11 (SR-01 entailment/contradiction exclusion from `Informs` write)
which is not present as an explicit AC in SCOPE.md AC-01..AC-24. This is a derived requirement
from the SR-01 invariant. If the architect disagrees with this interpretation, FR-11 can be
relaxed; however, without it a pair could simultaneously produce both a `Supports` and an
`Informs` edge, which contradicts the intent of separate write paths.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — retrieved 17 entries; entries #2417, #3656,
  #3658, #3826 directly incorporated into constraints and domain model.
- Queried: `mcp__unimatrix__context_search` (3 queries) — additionally retrieved entry #3744
  (PPR Direction::Outgoing reverse-walk pattern) which is load-bearing for AC-05 and C-14.
