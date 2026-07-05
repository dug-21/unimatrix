# Agent Report — vnc-043-agent-1-architect

## Task
Architecture + ADRs for vnc-043: subgraph live depth-1 read (reuse subgraph_via_db) + mirror-const doc fix. GH #903.

## Produced
- `product/features/vnc-043/architecture/ARCHITECTURE.md`
- `product/features/vnc-043/architecture/ADR-001-depth1-live-dispatch-reuse.md` (Unimatrix #5448)
- `product/features/vnc-043/architecture/ADR-002-description-source-of-truth.md` (Unimatrix #5449)
- `product/features/vnc-043/architecture/ADR-003-depth1-response-ordering-truncation.md` (Unimatrix #5450)

## Key decisions
- **ADR-001** — Route `max_depth == 1` to the existing `subgraph_via_db` unconditionally; insert the
  exact-match guard after `resolve_supersessions` (`graph_read_subgraph.rs:162`), before the lock block
  (`:164`). No dedicated helper (Open Q2). Depth-1 path takes no lock (A3/AC-10). depth>1 unchanged
  (AC-02, SR-07). Requires load-bearing-path regression coverage + dual-path SET parity (SR-02, SR-06).
- **ADR-002** — Keep the twin-literal + byte-equality-guard pattern (rmcp 1.7.0 blocks a const in
  `#[tool(description)]`; guard `test_graph_tool_attr_description_matches_const` #869 already exists).
  Edit both description literals identically; schemars field docs are single-source. (SR-01)
- **ADR-003** — Uniform stable ordering across all subgraph paths: nodes by `id`, edges by
  `(source_id, target_id, relation_type)`. Truncation: default `max_nodes=200` covers realistic
  fan-in; AC-15 fixture = ≥30 incoming Advances caps, assert `truncated==false`; keep `truncated`
  surfaced. (SR-03/AC-14, SR-05/Open Q5)
- **Open Q4/SR-04 resolved negative:** no `.snap`/insta/`schema_for` snapshot pins the description or
  `GraphParams` schema; only in-crate substring + byte-equality tests, both handled in-scope.

## Typed edges asserted
- ADR-001 (#5448) `Prerequisite` → #4479 (ADR-005 vnc-018): the depth-1-live/depth>1-cache asymmetry
  invariant a future dispatch-editor must read first to avoid silently reversing the split. Single
  edge; ADR-002/003 zero-edge.

## Open questions (for tester / spec)
- Tester: confirm no existing depth>1 subgraph test asserts a *fixed* order (vs set) that ADR-003's
  uniform ordering would change; update as presentation-only if so.
- Spec/synthesizer: AC-13 substring assertions + AC-14 ordering assertion are new test contracts —
  carry into the acceptance map explicitly.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- retrieved vnc-018/vnc-019 graph ADRs (#4479 ADR-005
  neighbors depth split, #4493 ADR-004 staleness-text-only, #4490 ADR-001 max_depth, #4562 lock-guard
  lesson); applied the depth-1/depth>1 asymmetry precedent and the staleness-text-only constraint.
- Stored: entry #5448 "ADR-001 vnc-043 depth-1 live dispatch" (+Prerequisite→#4479), #5449 "ADR-002
  description source-of-truth", #5450 "ADR-003 depth-1 response ordering+truncation" via context_store
  (category decision, tags [adr, vnc-043]).
