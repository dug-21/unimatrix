# crt-058 Architect Report

## Deliverables
- `product/features/crt-058/architecture/ARCHITECTURE.md`
- `product/features/crt-058/architecture/ADR-001-eager-delete-at-deprecation-source.md` — Unimatrix #5458
- `product/features/crt-058/architecture/ADR-002-audit-removed-edge-tuples.md` — Unimatrix #5459
- `product/features/crt-058/architecture/ADR-003-eager-subset-tick-invariant.md` — Unimatrix #5460
- `product/features/crt-058/architecture/ADR-004-edges-removed-response-plumbing.md` — Unimatrix #5461

## Key Decisions
- **Insertion point:** new step 6.5 in `context_deprecate` (`tools.rs:1413`), after the step-6 flip and before step-8 format; past the step-5 idempotency guard. Flip → delete → count → audit → format.
- **Eager delete:** new helper `delete_agent_edges_for_entry(store, entry_id) -> Result<Vec<RemovedEdge>, EdgeDeleteError>` in `mcp/edge_write.rs` beside `delete_graph_edge`; one `DELETE ... WHERE (source_id=?1 OR target_id=?1) AND source=?2 RETURNING source_id, target_id, relation_type` on `write_pool_server()`. Predicate LOCKED.
- **SR-01:** audit records removed edge TUPLES via `RETURNING`, in `AuditEvent.metadata` JSON (op `context_deprecate.edge_cleanup`), not just a count.
- **SR-02:** eager ⊆ tick enforced by a behavioral test running BOTH the real eager helper and real `run_orphaned_edge_compaction` over parallel fixtures; assert R ⊆ T and R = exactly the 2 agent edges. Catches eager-predicate widening AND tick narrowing. `superseded_by IS NULL` guaranteed structurally by chokepoint (no runtime SQL clause added).
- **SR-04:** `edges_removed: Option<u64>` threaded through `format_status_change`; `Some(n)` renders in all 3 formats (incl. `Some(0)`), `None` omits; quarantine/restore pass `None`. Behavioral per-format matrix test mandatory.
- **SR-05:** compaction-as-backstop recorded as a standing coupling invariant in ADR-001; `warn!` on failure kept at warn (not debug).
- **SR-06:** ordering pinned — delete after flip, past step-5 guard; step-7 recompute independent.

## Open Questions
None blocking. Forward flags: SR-05 (future compaction change must re-verify backstop), SR-03 (a new `EDGE_SOURCE_*` for human-authored edges would be subset-safe-missed by the `='agent'` filter; the per-source test surfaces it). No ADR supersession. No typed edges asserted — the relationship to pattern #3910 / lesson #5417 is carried in ADR prose; it does not clear the traversal-necessity bar for a graph edge.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search (compaction/eager-tick divergence, graph_edges provenance, deprecate) — applied #3910 (multi-pass identical status-filter discipline), #4167 (inclusive single-source undercount → SR-03), #3883 (GRAPH_EDGES tick writes use write_pool_server() directly), #4425 (EDGE_SOURCE_AGENT = "agent", filter on source column not relation-type).
- Stored: entry #5458 ADR-001, #5459 ADR-002, #5460 ADR-003, #5461 ADR-004 via context_store (category decision, topic crt-058).
