# Agent Report: nan-001-agent-0-scope-risk

## Task
Scope-level risk assessment for nan-001 (Knowledge Export).

## Artifacts Produced
- `/workspaces/unimatrix-nan-001/product/features/nan-001/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary
- **High severity**: 2 (SR-01 format contract lock-in, SR-07 concurrent read consistency)
- **Medium severity**: 3 (SR-02 SQL/Store drift, SR-04 schema coupling, SR-09 CLI integration)
- **Low severity**: 4 (SR-03 float precision, SR-05 JSON key order, SR-06 incremental pressure, SR-08 migration race)
- **Total**: 9 risks

## Top 3 Risks for Architect/Spec Writer
1. **SR-07** — Export without a read transaction sees inconsistent state across tables if the server writes mid-export. Wrap in BEGIN DEFERRED.
2. **SR-01** — format_version 1 becomes the nan-002 contract. Any field naming or type encoding mistake is a breaking change. Define the contract explicitly.
3. **SR-05** — Deterministic output (AC-14) requires ordered JSON key serialization. Default serde_json HashMap is non-deterministic.

## Decisions Made
- Focused on product/scope risks only — no architecture-level design alternatives assessed.
- Flagged read-transaction isolation as highest priority since the SCOPE.md assumes WAL mode is sufficient but does not mention transaction wrapping.

## Knowledge Stewardship
- Queried: not applicable (no Unimatrix MCP connection in this context)
- Stored: nothing novel to store -- first nan-phase feature, no cross-feature pattern yet
