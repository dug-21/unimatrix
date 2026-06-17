# Agent Report: vnc-038-agent-2-spec

## Deliverable
`product/features/vnc-038/specification/SPECIFICATION.md`

## Summary
Authored SPECIFICATION.md from SCOPE.md (Goals 1-6, RD-1..6, AC-01..AC-10) and SCOPE-RISK-ASSESSMENT.md (SR-01..SR-07). All ten acceptance criteria carried verbatim with verification methods pinned to existing test surfaces (bundle codec parity corpus, seam/funnel tests, project-lifecycle fixtures, hook-client transport tests) — no new scaffolding. 14 functional + 8 non-functional requirements, all testable. Domain models + ubiquitous language define slug, bundle v:2, server-composed URL, dumb-client invariant, per-request funnel, cross-pollination, N=2 proof. The N=2 isolation proof (SR-03 / #4974 precedent) is pinned as a hard constraint (C-11, AC-06). The #766 repro is AC-07's concrete test.

## Key Decisions / Interpretations
- Treated RD-1..6 as resolved; phrased requirements around them rather than re-opening.
- Made the dumb-client invariant cross-cutting (NFR-01 + C-01) and gave SR-01 a concrete testable form: post-feature set of client-side path-composition sites is empty (FR-08 invariant test).
- Pinned AC-06 verification explicitly to N=2 with a counting/recording resolver, per #4974.
- Scoped the cutover to served-project only; local UDS reconciliation under the unified resolver flagged as architect open question (OQ-2) because SR-04 tensions with RD-5's "no special case."

## Open Questions (relayed to leader/architect/human)
- OQ-1 architect: exact v:2 field layout for MCP + observe URLs.
- OQ-2 architect: local UDS single-store addressing under unified resolver without manual slug (SR-04 vs RD-5).
- OQ-3 architect: final reserved-slug set under new grammar (SR-05).
- OQ-4 human/leader: #735 sequencing on shared router/boot surface (SR-06).
- OQ-5 human: validate zero existing served stores before hard cut (RD-1 assumption).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced vnc-034 ADRs #4954, #4951, #4949, #4963 and lesson #4974; applied to ubiquitous language, domain models, N=2 constraint. Read-only tier; no storage (spec decisions are feature-specific).
