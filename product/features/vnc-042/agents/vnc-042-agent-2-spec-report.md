# Agent Report — vnc-042-agent-2-spec

**Role:** Specification writer
**Deliverable:** `product/features/vnc-042/specification/SPECIFICATION.md`

## Summary
Authored SPECIFICATION.md from SCOPE.md + SCOPE-RISK-ASSESSMENT.md. AC-1..AC-7 carried verbatim from GH #843 with the `version` enum replaced by `follow_supersessions: bool` (default `true`); each AC has a concrete verification method. Test surface (SR-02) captured as tracked requirements TS-01..TS-09, not delivery-time surprises.

## Key requirements
- FR-01..FR-14 (all testable); NFR-01..NFR-08 (measurable, incl. byte-identity canary preservation).
- §6 test surface: byte-identity canary (TS-01), ~15 format_single_entry shape tests (TS-02), ~18 include_edges/param-additivity tests (TS-03), plus 6 new tests (TS-04..TS-09).
- Store-layer read-back tests explicitly excluded as false positives (#5383).

## ADR-owned (referenced as "per ADR", not decided here)
- ADR-D1 (SR-03): which entry's edge list a resolved get returns → FR-11, AC-07, TS-03, OQ-A.
- ADR-D2 (OQ-3): `format="json"` notice/flag shape → FR-12, AC-02/03/04, OQ-B.
- SR-06 naming/default divergence ruling → OQ-C.

## Open questions
- OQ-A/B/C above (architect/ADR).
- OQ-D: RESOLVED by ADR-002 (dead-end returns originally-requested id + loud flag).

## Coordinator course-corrections applied (from RISK-TEST-STRATEGY.md)
- **R-02:** param is `Option<bool>` (None ⇒ follow), handler-owned default per ADR-001 — reworded FR-01, NFR-01, NFR-02, AC-06 (now behavioral: field-absent ⇒ resolves), TS-09.
- **R-08:** added AC-08 (spec-derived) + FR-07 orphaned/`superseded_by IS NULL` footer edge case (no panic / no malformed `#{}`, per ADR-003); folded into TS-06.
- **AC-04** aligned to ADR-002 (returned id == originally-requested).
- Changed FR/AC ids: FR-01, FR-07, NFR-01, NFR-02, AC-04, AC-06, AC-08 (new), TS-06, TS-07, TS-09.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — #5383 (blast-radius layer partitioning), #4460 (terminal-active resolution), #3728 (id serialization → plain bool), #4468/#4538 (CTE + status=0 guard), #4303 (tool-desc accuracy). No storage (read-only tier).
