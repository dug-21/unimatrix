# Agent Report: crt-058-agent-2-spec

## Deliverable
`product/features/crt-058/specification/SPECIFICATION.md`

## Summary
Turned SCOPE.md (Goals / AC-01..AC-09 / Constraints) plus SCOPE-RISK-ASSESSMENT (SR-01..SR-06) into 9 functional requirements, 6 non-functional requirements, 11 acceptance criteria with behavioral verification methods, domain model, constraints (C-01..C-11), dependencies, and NOT-in-scope. Scope was settled — no design choices re-opened.

## Scope-risk folding
- SR-02 → AC-10: eager ⊆ tick as a behavioral subset test (S_eager ⊆ S_tick state comparison + chokepoint-exclusion of the repoint scenario). Fails if eager removes an edge the tick keeps/repoints.
- SR-04 → AC-04: two behavioral matrices — per-source removal (agent removed; nli/co_access/cosine_supports/S1/S2/S8 retained) and per-format count (Summary/Markdown/Json parse/value assertions, plus audit content). Rejects call-count/substring tests.
- SR-01 → AC-11: removed-edge tuples in audit — FIRM required (ADR-002, resolved at design gate).
- SR-03 → AC-04 per-source matrix + NFR-06 (provenance-enumeration-bound completeness, subset-safe).
- SR-05 → C-11 + NFR-05 (compaction-as-backstop standing invariant; warn-log not expected-suppressed).
- SR-06 → C-02/C-03 + AC-07/AC-09 (placement/ordering).
- Non-fatal / idempotency / synchronous → AC-06 / AC-07 / AC-09, FR-05/FR-06/FR-07.

## Resolved at design-review gate
- ADR-002: tuple-level auditing accepted → AC-11 firm.
- ADR-004: zero case renders `Some(0)` (literal 0 in all 3 formats); `None` = delete failed/omit → AC-05/NFR-04/FR-03/domain model updated.

## Open questions for architect
1. New `edge_write.rs` by-endpoint function vs inline; confirm no step-7 `confidence.recompute` ordering interaction.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — #3910 (identical status filters across same-table cleanup passes = eager⊆tick basis), #5431 (retired DependencyOnDeprecatedRule stale-Prerequisite condition), #4425 (EDGE_SOURCE_AGENT), #3883 (write_pool_server for graph_edges writes). No novel pattern stored (read-only tier).
