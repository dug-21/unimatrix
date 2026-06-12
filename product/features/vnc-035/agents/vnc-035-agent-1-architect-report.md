# Agent Report: vnc-035-agent-1-architect

## Task
Design the architecture for vnc-035 — `context_correct` outgoing-edge carry-forward by default.

## Artifacts Produced
- `product/features/vnc-035/architecture/ARCHITECTURE.md`
- `product/features/vnc-035/architecture/ADR-001-carry-forward-step-placement.md` (Unimatrix #4983)
- `product/features/vnc-035/architecture/ADR-002-outgoing-query-eligibility-and-posture.md` (Unimatrix #4984)
- `product/features/vnc-035/architecture/ADR-003-edges-carried-count-contract.md` (Unimatrix #4985)
- `product/features/vnc-035/architecture/ADR-004-additive-on-triple-upsert-composition.md` (Unimatrix #4986)
- `product/features/vnc-035/architecture/ADR-005-contradicts-bidirectional-carry.md` (Unimatrix #4987)

## Key Decisions
1. **Step 8b′** carry-forward runs between Phase B (`params.edges`, 8b) and incoming redirect (8c). Carry writes second; composition via DB UNIQUE constraint.
2. New `query_outgoing_edges` in `unimatrix-store`; eligibility predicate (`NOT IN (Supersedes, CoAccess, Informs)`) expressed once at SQL level — superset of incoming exclusion, documented as intentional (SR-03).
3. `edges_carried` counts only `true` (new-insert) returns; carry owns its write loop because `validate_and_write_edges` discards the bool (SR-02).
4. Additive-on-triple is `INSERT OR IGNORE` on `UNIQUE(source_id, target_id, relation_type)` — no diff logic (AC-08).
5. Warn-and-continue parity; `failed` counter + `CarrySummary` make the failure path observable/testable (SR-01).
6. `Contradicts` reuses bidirectional handling; carry (outgoing) and redirect (incoming) act on disjoint row sets (SR-06).

## Open Questions (for delivery, non-blocking)
- O-1: Confirm an index on `graph_edges.source_id` exists; if not, assess whether one is warranted.
- O-2: `query_outgoing_edges` goes in `read.rs` or a new `read_outgoing.rs` depending on the live 500-line count.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_get for #4460/#4463/#4439/#4426/#4420/#4472/#4473/#4459/#4041 — applied vnc-017 redirect-loop pattern, vnc-015 Phase A/B + partial-write posture, #4041 rows-affected contract (load-bearing for the count contract), #4473 missing-failure-test lesson (drove SR-01 observability design).
- Stored: ADRs #4983, #4984, #4985 (with Prerequisite edges -> #4041 and #4983), #4986, #4987 via context_store (category decision, topic vnc-035).
- No prior ADR required deprecation: the vnc-015 "no auto-transfer" stance was a Non-Goal/AC posture (consciously superseded for outgoing edges per settled SCOPE), not a standalone active ADR; vnc-015 ADR-002/003 and vnc-017 ADRs remain valid and are extended, not replaced.
