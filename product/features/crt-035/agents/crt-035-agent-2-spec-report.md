# Agent Report: crt-035-agent-2-spec

## Output

- `/workspaces/unimatrix/product/features/crt-035/specification/SPECIFICATION.md`

## Key Decisions

**D-SPEC-01** — Separated the "tests that must change" from "tests that are safe"
explicitly rather than listing only the obvious T-BLR-01/02 cases. Reading the
full test file revealed T-BLR-03 through T-BLR-08 as additional blast-radius
tests not enumerated in SCOPE.md. All 8 are now specified with before/after
assertions.

**D-SPEC-02** — AC-12 specifies the `typed_graph.rs` test extension concretely:
uses `SqlxStore` (real SQLite), inserts a `(B → A)` CoAccess edge directly into
`GRAPH_EDGES`, calls `TypedGraphState::rebuild()`, then calls
`personalized_pagerank` seeded at B, asserts A has non-zero score. This directly
satisfies SR-06 by mandating the real SQLite path.

**D-SPEC-03** — `test_existing_edge_stale_weight_updated` (Group C) was not listed
in SCOPE.md's blast-radius examples but contains `count_co_access_edges == 1` as
T-BLR-08. Added as a required update to prevent a stale assertion from passing
silently.

**D-SPEC-04** — NFR-07 addresses SR-07 (near-threshold oscillation pattern #3822):
eventual consistency per direction on next tick is the accepted behavior; no
atomic-pair transaction is required.

**D-SPEC-05** — OQ-02 (weight floor in back-fill) and OQ-03 (index coverage) are
flagged as open questions for the architect rather than resolved in spec, as
SCOPE.md does not address them and they have implementation-time verification
requirements.

## Open Questions Surfaced

- OQ-01: confirm `test_existing_edge_stale_weight_updated` count changes to 2 (T-BLR-08).
- OQ-02: should back-fill SQL apply a weight floor for `weight = 0.0` forward edges?
- OQ-03: confirm existing separate indexes on `GRAPH_EDGES` columns are sufficient for NOT EXISTS sub-join.

## Knowledge Stewardship

Queried: `mcp__unimatrix__context_briefing` — entries #3889 (back-fill pattern),
#3827 (tick ordering), #3830 (ADR-006 directionality), #3822 (oscillation pattern)
were all directly applicable and incorporated. No results were missing.
