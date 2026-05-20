# Agent Report: vnc-019-agent-2-spec

## Output

`product/features/vnc-019/specification/SPECIFICATION.md` — written.

## Key Requirements Captured

- 23 functional requirements (FR-01 through FR-23) covering dispatch, capability gate,
  all 6 parameters (seed_ids, edge_types, direction, max_depth, max_nodes,
  resolve_supersessions), BFS algorithm contract, seed-inclusion invariant, cap enforcement,
  edge deduplication, post-BFS hydration and metadata batch, missing-seed behavior,
  depth_reached computation, SubgraphResponse wire type, file placement, tool description
  staleness disclosure, validate_no_unsupported_params extension, and no-engine-change
  mandate.
- 19 acceptance criteria (AC-01 through AC-19) — each from SCOPE.md ACs plus additions
  for: AC-16 (max_depth on non-subgraph modes, SR-07), AC-17 (missing seed empty result),
  AC-18 (EdgeRecord.metadata populated), AC-19 (no metadata query when edge list empty).
- All 4 ADR decisions incorporated: ADR-001 (max_depth field), ADR-002 (file split),
  ADR-003 (post-BFS batch), ADR-004 (no graph_rebuilt_at).
- SR-02 and SR-05 addressed explicitly in constraints C-09 and C-10 respectively.

## Gaps and Ambiguities Found

One open ambiguity: **FR-07 max_nodes above 200** — the SCOPE.md specifies a 200 hard cap
but does not specify whether a caller-supplied value above 200 is clamped silently or
returns a validation error. The spec marks this as an architect decision ("clamped to 200
or rejected — architect decision"). This should be resolved before delivery; recommend
clamping for forward compatibility consistency with the `max_nodes` stub pattern.

No other gaps. All 6 SCOPE.md OQs are resolved and incorporated. SR-01 through SR-07
dispositions are reflected in constraints or acceptance criteria.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — 12 entries returned, all consistent with
  architecture decisions. No conflicts found between indexed knowledge and the architecture
  documents.
