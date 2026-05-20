# Agent Report: vnc-020-agent-2-spec

**Agent**: vnc-020-agent-2-spec (Specification Writer)
**Feature**: vnc-020
**Output**: `product/features/vnc-020/specification/SPECIFICATION.md`

## Completion Status

COMPLETE.

## Key Decisions Made

1. **Staleness disclosure text** (SR-01): Produced exact text for `path` mode tool description
   modeled verbatim on ADR-004 vnc-019 (#4493). Text specifies tick-window lag (30-60s),
   names `inverse` and `filter` as the freshness alternatives, and documents the `found: false`
   no-error contract for missing graph nodes.

2. **Param/mode rejection matrix** (SR-08): Full 11-row x 7-column table produced covering
   all 8 new fields plus `depth`, `from_id`, `to_id`. Cells distinguish accept/reject/n/a.
   Footnotes document the `depth` silent-ignore correction (FR-17) as a deliberate behavior
   change.

3. **depth acceptance clarification** (SR-04): `depth` is accepted by `neighbors` and `path`
   only. All other 5 modes reject with validation error. The silent-ignore correction is
   explicit in FR-17, AC-25, and the rejection matrix footnote.

4. **Wire format for path mode**: `from_id` is a top-level field, never in `hops`. Empty
   `hops` + `found: false` for both no-path and missing-node cases. `length = hops.len()`.

5. **filter mode SQL security** (C9/NFR-06): `where_clause: String` is explicitly excluded.
   All property filters are typed params. Documented in both FR-05 and NFR-06.

6. **AND semantics for inverse mode** (SR-06, OQ-03): Multiple LEFT JOINs, all null checks
   ANDed. Documented in FR-03, ubiquitous language, and tool description model.

## Open Questions for Architect

- **OQ-A1**: Does `TypedRelationGraph` already perform per-hop intermediate node resolution
  during BFS traversal, or does `path` mode need to implement this as new infrastructure?
  (SR-05: intermediate resolution is mentioned in SCOPE.md but not in neighbors/subgraph ADRs)

- **OQ-A2**: Confirm module boundary: `validate_no_unsupported_params` stays in
  `graph_read.rs`; per-mode parameter validation lives in each sibling module.

## Self-Check Results

- [x] SPECIFICATION.md covers all 31 ACs from SCOPE.md (every AC-ID present)
- [x] Every functional requirement is testable (18 FRs, each with explicit verification method)
- [x] Non-functional requirements include measurable targets (sub-ms, <10ms, tick interval)
- [x] Domain Models section defines all key structs and fields with types and constraints
- [x] NOT in scope section is explicit (12 items)
- [x] Output file is in `product/features/vnc-020/specification/` only
- [x] No placeholder or TBD sections — unknowns raised as open questions OQ-A1, OQ-A2
- [x] Knowledge Stewardship report block included in SPECIFICATION.md

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — 10 results returned
- Retrieved in full: #4493 (ADR-004 vnc-019: staleness disclosure model), #4490 (ADR-001
  vnc-019: GraphParams locked fields), #4477 (ADR-003 vnc-018: forward-compat validation)
- No new patterns identified for storage (specification decisions are feature-specific)
