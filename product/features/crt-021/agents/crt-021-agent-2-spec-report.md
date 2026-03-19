# Agent Report: crt-021-agent-2-spec

## Summary

Produced SPECIFICATION.md for crt-021 (W1-1: Typed Relationship Graph).

## Deliverable

`product/features/crt-021/specification/SPECIFICATION.md`

## Key Specification Decisions

1. **AC-08 closed definitively** — No Contradicts edges at bootstrap. The SCOPE.md AC-08
   conditional language ("if the mapping can be resolved") has been replaced with a firm
   requirement: zero Contradicts edges written during v12→v13 migration. The `bootstrap_only`
   and `source` columns exist in schema for W1-2. Basis: SR-04 and confirmed by entry #2404.

2. **bootstrap_only promotion path specified (AC-21, FR-26)** — W1-2 uses DELETE+INSERT via
   `AnalyticsWrite::GraphEdge` to promote `bootstrap_only=1` to `bootstrap_only=0`. The
   `UNIQUE(source_id, target_id, relation_type)` constraint + INSERT OR IGNORE makes this
   idempotent. W1-1 does not implement the logic but the mechanism is fully specified.

3. **TypedGraphState rename enforced by compiler (C-11)** — No type aliases permitted.
   SR-06 risk addressed by making compiler enforcement explicit as a constraint.

4. **Prerequisite variant reserved (C-12, AC-20)** — No Prerequisite edges written by any
   crt-021 code path. Variant exists for W3-1 forward compatibility only.

5. **Graph rebuilt from GRAPH_EDGES, not recomputed from canonical sources (C-14, FR-22)** —
   SCOPE.md open question #4 closed as Option (a): query GRAPH_EDGES from DB each tick.
   This preserves attribution and captures runtime NLI edges written between migration and
   tick.

6. **CO_ACCESS_BOOTSTRAP_MIN_COUNT = 3 (FR-09, AC-07)** — SCOPE.md open question #2
   closed at 3. This matches the product vision "high-count pairs" framing and avoids
   flooding the graph with noise at count=1.

7. **edges_of_type filter boundary required (FR-20, AC-11)** — SR-01 risk addressed by
   specifying a single `edges_of_type` method rather than ad-hoc checks. Exact API shape
   (method vs. function, EdgeFiltered vs. iterator) is deferred to architect as OQ-05.

8. **AC-19 added** — `sqlx-data.json` regeneration is an explicit acceptance criterion.
   SR-09 risk addressed.

9. **AC-21 added** — bootstrap_only promotion mechanism formally specified as an AC even
   though W1-2 implements it, per the spawn prompt requirement.

## Open Questions Raised for Architect

- **OQ-01**: DELETE+INSERT vs. UPDATE path for bootstrap_only promotion (SR-07)
- **OQ-02**: Analytics queue shed risk for W1-2 runtime NLI edge writes (SR-02)
- **OQ-03**: GRAPH_EDGES compaction per-tick row limit vs. unbounded DELETE (SR-03, NF-09)
- **OQ-04**: `metadata TEXT` column for W3-1 GNN edge feature readiness (SR-08)
- **OQ-05**: `edges_of_type` API shape — method vs. function, EdgeFiltered vs. iterator (FR-20)

## Knowledge Stewardship

Queried: /uni-query-patterns for typed graph, supersession, migration, analytics write —
entries #2403, #1607, #1601, #1602, #1604, #2063, #2404 found and incorporated. All prior
ADRs and confirmed constraints reflected in specification constraints section.
