# ADR-001: Clone-and-Truncate for Evidence Limiting

**Feature**: col-010b
**Status**: Accepted
**Date**: 2026-03-02

## Context

The `evidence_limit` parameter truncates `HotspotFinding.evidence` arrays before serialization. The full evidence is needed by Component 2 (narrative synthesis) and Component 3 (lesson-learned content generation), both of which operate on the in-memory report before it is returned to the caller.

Two approaches:
1. **Mutate in place**: truncate evidence arrays on the report, then serialize. Simpler but destroys data needed by Components 2 and 3.
2. **Clone and truncate**: build the full report, use it for synthesis and lesson-learned, then clone and truncate for serialization.

## Decision

Clone-and-truncate. The full report is built first, used for narrative synthesis and lesson-learned content, then cloned with evidence truncated for the serialized response.

## Rationale

- Components 2 and 3 require full evidence arrays to produce accurate narratives and content
- The clone cost is negligible — `RetrospectiveReport` is small (hotspot count typically < 20, evidence arrays < 50 items each)
- The `hotspots: Vec<HotspotFinding>` type constraint means we cannot use a separate summary type for the response

## Consequences

- Memory: brief duplication of the report during serialization. Negligible at expected sizes.
- Code clarity: the truncation step is explicit and isolated, not interleaved with synthesis logic.
- Testing: `evidence_limit = 0` path skips the clone entirely (no-op).
