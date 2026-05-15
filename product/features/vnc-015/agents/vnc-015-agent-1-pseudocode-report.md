# Agent Report: vnc-015-agent-1-pseudocode

## Task

Produce per-component pseudocode files for all 8 components of vnc-015 (Typed Edge Write Path
+ context_edge Tool).

## Output Files Produced

- `product/features/vnc-015/pseudocode/OVERVIEW.md`
- `product/features/vnc-015/pseudocode/edge-input-params.md` (Component 1)
- `product/features/vnc-015/pseudocode/edge-write.md` (Component 2)
- `product/features/vnc-015/pseudocode/relation-type.md` (Component 3)
- `product/features/vnc-015/pseudocode/ppr-expand.md` (Component 4)
- `product/features/vnc-015/pseudocode/stale-dependency.md` (Component 5)
- `product/features/vnc-015/pseudocode/detection-rule.md` (Component 6)
- `product/features/vnc-015/pseudocode/contradicts-fix.md` (Component 7)
- `product/features/vnc-015/pseudocode/context-edge-handler.md` (Component 8)

## Components Covered

1. EdgeInput / StoreParams / CorrectParams extension (tools.rs)
2. edge_write.rs helper module (new file)
3. RelationType enum extension (graph.rs) — 10 variants × 3 required sites
4. PPR and graph_expand expansion (graph_ppr.rs, graph_expand.rs) — RelatedTo only
5. stale_dependency_edges field + per-cycle query (read.rs)
6. DependencyOnDeprecated detection rule (detection/scope.rs, detection/mod.rs)
7. query_contradicts_edges_for_entry bidirectional fix (read.rs)
8. context_edge handler — 13th MCP tool (tools.rs)

## Open Questions / Gaps Found

### OQ-1 (carried from architecture): validate_and_write_edges two-phase split

The IMPLEMENTATION-BRIEF.md describes a two-phase pipeline (Phase A: pre-insert validation;
Phase B: post-insert self-ref + write). ADR-001 clarifies that for `context_store`, the
`source_id` is not known pre-insert (auto-increment). The pseudocode in `edge-write.md`
represents `validate_and_write_edges` as a single function called post-insert (Phase B only),
with the Phase A inline loop in the handler. Delivery must decide:

  Option A (adopted in pseudocode): Phase A is inlined in the handler; `validate_and_write_edges`
  is called once post-insert with the actual source_id.

  Option B: `validate_and_write_edges` is called twice — once pre-insert (source_id=0, skips
  self-ref) and once post-insert (writes only). The sentinel source_id=0 approach is fragile.

Recommendation: Option A. The handler performs Phase A type + target validation inline, collects
`resolved_edges`, then calls a write-only path post-insert. The function signature in the
Implementation Brief still makes sense as the post-insert write entry point.

### OQ-2: Per-cycle stale edge query join table

`stale-dependency.md` uses `JOIN feature_entries fe ON fe.entry_id = ge.source_id` for the
per-cycle query in `context_cycle_review`. The exact table name and column names must be
verified against the current schema before implementation. If the join table linking entries to
feature cycles differs, the query must be adjusted.

### OQ-3: Tool count test location (OQ-4 from architecture)

The pseudocode for Component 8 notes that any test asserting "12 tools" must be updated to 13.
The architecture OQ-4 identified this as unresolved. Implementation agent must grep for the
specific test before implementation begins.

### OQ-4: write_graph_edge function signature confirmation

The pseudocode calls `write_graph_edge(store, source_id, target_id, relation_type, weight,
created_at, source, metadata)`. This matches the brief's description at `nli_detection.rs:78`.
Implementation agent must verify the exact parameter order and types before writing any call sites
(per Unimatrix procedure #4059: grep the actual function before writing callers).

### OQ-5: error conversion for EdgeValidationError → MCP error

The pseudocode uses `.await?` which propagates `EdgeValidationError` to the MCP handler.
The handler needs a `From<EdgeValidationError>` impl for the ServerError type, or explicit
`.map_err(...)` conversions. The exact conversion pattern should follow whichever approach
the existing 12 tools use for custom error types. Implementation agent must verify before
writing handler code.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 14 entries. Key hits: #4041
  (write_graph_edge bool contract — critical for pseudocode), #3950 (four-site RelationType
  extension requirement), #4042 (three-case contract table required in pseudocode itself),
  #2269 (implicit — confirmed via brief that RAII transaction is mandatory), #4417 (edge
  placement in MCP handler), #3650 (bidirectional Contradicts traversal pattern).
- Queried: `mcp__unimatrix__context_search` (pattern: "graph edge write patterns conventions")
  — returned #4056 (write_graph_edge source parameter pattern), #4041 (bool contract again),
  #3889 (back-fill reverse GRAPH_EDGES for symmetric relations — confirms Contradicts pattern),
  #4417 (agent-declared edge writes placement).
- Queried: `mcp__unimatrix__context_search` (decision: "vnc-015 architectural decisions")
  — returned #4421, #4426, #4422 (vnc-015 ADRs already read from files).
- Deviations from established patterns: none. All pseudocode follows patterns #4041, #3950,
  #4417, and the RAII transaction pattern established via lesson #2269. The DependencyOnDeprecated
  constructor injection pattern matches the PhaseDurationOutlierRule precedent per ADR-004.
