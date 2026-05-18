# Agent Report: vnc-015-agent-2-spec

**Agent**: Specification Writer
**Feature**: vnc-015 — Typed Edge Write Path
**Status**: Complete

## Output

- `product/features/vnc-015/specification/SPECIFICATION.md`

## Key Decisions Made

1. **Confidence floor OQ raised**: SCOPE.md states the floor applies to "source entry" but
   context_store has no source entry pre-insert. Flagged as OQ-01 for architect to close before
   pseudocode. Four resolution options provided.

2. **Validation ordering specified**: Unknown-type check → self-reference check → confidence
   floor. This order allows cheap checks (string parse) to fail fast before DB reads.

3. **write_graph_edge three-case contract documented in FR-07** per SR-02 risk, before any
   implementation detail. The bool return is load-bearing and must be checked, not wrapped.

4. **FR-10 compliance matrix**: Made the 10×4 SR-01 checklist a first-class functional
   requirement with an explicit table, not just a note in constraints. Gate-3a can grep-verify.

5. **RelatedTo bidirectionality deferred to OQ-05**: SCOPE.md mandates bidirectionality only
   for Contradicts; ASS-057 suggests RelatedTo is symmetric. Left to architect rather than
   mandating or excluding.

6. **Partial-write blast radius as NFR-06**: Accepted per SCOPE.md but raised as OQ-02 for
   architect to document the blast radius explicitly.

7. **Source ownership validation is NOT in scope**: Closed decision from SCOPE.md. The
   security mechanism is the confidence floor, not ownership checks.

## Open Questions for Architect

- OQ-01 (High): Confidence floor application in context_store — source entry doesn't exist
  pre-insert; which confidence value to check?
- OQ-02 (Med): Partial-write blast radius documentation
- OQ-03 (Low): Typed injection interface for DependencyOnDeprecated constructor
- OQ-04 (Med): Weight of Advances/Motivates in positive_out_degree_weight
- OQ-05 (Low): RelatedTo bidirectionality
- OQ-06 (Med): Current tools.rs line count — architect must verify before design

## Self-Check

- [x] All 18 ACs from SCOPE.md present (AC-01 through AC-18)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets (500-line limit, 0.1 default floor)
- [x] Domain Models section defines EdgeInput, full RelationType enum, validation failure modes
- [x] NOT in scope section is explicit
- [x] Output file in product/features/vnc-015/specification/ only
- [x] No placeholder or TBD sections — all unknowns raised as open questions
- [x] Knowledge Stewardship block included

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — 17 entries returned. Pattern #3950
  (4-site RelationType change), pattern #4041 (write_graph_edge bool contract), pattern #3650
  (TypedRelationGraph module), ADR col-029 (EDGE_SOURCE named constant) all directly applicable.
  No conflicting decisions found.
