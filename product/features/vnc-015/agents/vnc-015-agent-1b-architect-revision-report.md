# Agent Report: vnc-015-agent-1b-architect-revision

## Task
Revise vnc-015 architecture to incorporate Phase 2a scope changes: drop confidence floor,
add target validation, add context_edge as 13th MCP tool, supersede ADR-002.

## Files Updated

### ARCHITECTURE.md
`/workspaces/unimatrix/product/features/vnc-015/architecture/ARCHITECTURE.md`

Changes:
- System Overview: replaced confidence-floor bullet with target validation + context_edge tool
- Component 2 (edge_write.rs): removed BelowConfidenceFloor, added TargetNotFound/TargetQuarantined
  to EdgeValidationError; added delete_graph_edge and redirect_graph_edge to public surface;
  updated validate_and_write_edges signature (confidence_floor param removed)
- Component 8 (confidence_floor Config): REMOVED entirely
- Component 9 (context_edge handler): NEW — full spec of EdgeParams struct, 7-step validation
  pipeline, mode dispatch, tool count note
- Component Interactions: removed confidence floor step, added context_edge flow diagram
- Validation Pipeline: replaced 3-phase (A/B/C with confidence floor) with 2-phase (A: pre-insert
  validation including target DB lookup; B: write). Documented context_edge pipeline separately.
- Technology Decisions: updated ADR list (ADR-002 superseded, ADR-009 and ADR-010 added)
- New Interfaces table: removed StoreConfig.edge_confidence_floor, added EdgeParams,
  delete_graph_edge, redirect_graph_edge, EdgeDeleteError, EdgeRedirectError
- Integration Surface table: same removals/additions
- Open Questions: removed resolved confidence floor timing question; added 4 new questions
  (default_rules callers, EntryStatus enum access, tool count test location, redirect
  transaction API)

### New ADR Files
- `/workspaces/unimatrix/product/features/vnc-015/architecture/ADR-002-edge-write-failure-posture.md`
  (replaces old ADR-002-confidence-floor-failure-posture.md — old file remains for reference)
- `/workspaces/unimatrix/product/features/vnc-015/architecture/ADR-009-context-edge-tool-design.md`
- `/workspaces/unimatrix/product/features/vnc-015/architecture/ADR-010-target-validation-query-pattern.md`

## Unimatrix Operations

| ADR | Operation | Old ID | New ID |
|-----|-----------|--------|--------|
| ADR-002 | context_correct (supersede) | #4419 | #4426 |
| ADR-009 | context_store (new) | — | #4427 |
| ADR-010 | context_store (new) | — | #4428 |

## Key Design Decisions

1. **Confidence floor removed entirely.** No StoreConfig.edge_confidence_floor. No
   BelowConfidenceFloor error variant. validate_and_write_edges drops the confidence_floor
   parameter. Component 8 (config.rs change) is removed from architecture.

2. **Target validation is pre-write.** store.get_entry_by_id(target_id) runs in the same
   validation pass as type resolution and self-ref check — all before StoreService.insert().
   Missing target or quarantined target → fail entire call, no entry written. Deprecated target
   → allowed.

3. **context_edge is the 13th MCP tool.** Handler in tools.rs. 7-step validation pipeline.
   Three modes: add (idempotent INSERT OR IGNORE), remove (idempotent DELETE), redirect (atomic
   transaction). Ownership check is non-vacuous — source is an existing entry owned by any agent.
   Source must be Active (not Quarantined, not Deprecated).

4. **redirect is atomic (exception to ADR-003).** DELETE old + INSERT new in one SQLite
   transaction. For Contradicts: all 4 direction rows managed atomically. This is the only
   transactional operation in edge_write.rs. Non-atomicity would leave a half-retargeted edge
   (data loss). remove remains non-transactional (idempotent DELETE, recoverable).

5. **EdgeValidationError updated.** Variants: UnknownType, SelfReferential, TargetNotFound,
   TargetQuarantined. BelowConfidenceFloor removed.

6. **edge_write.rs gains two new functions.** delete_graph_edge and redirect_graph_edge join
   validate_and_write_edges as the pub(crate) surface.

## Open Questions for Spec

1. default_rules() signature change: audit all callers, update test to assert 23 rules
2. EntryStatus enum vs integer literals for status comparison in edge_write.rs
3. Tool count test: identify which test asserts exactly 12 MCP tools → update to 13
4. redirect_graph_edge transaction API: confirm write_pool_server() exposes begin() or
   equivalent; this is the first transactional write in edge_write.rs
