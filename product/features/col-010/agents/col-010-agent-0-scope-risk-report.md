# Agent Report: col-010-agent-0-scope-risk

Agent ID: col-010-agent-0-scope-risk
Mode: scope-risk
Date: 2026-03-02

## Task

Scope-level risk assessment for col-010: Session Lifecycle Persistence & Structured Retrospective.

## Artifacts Produced

- `/workspaces/unimatrix/product/features/col-010/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary

| Severity | Count |
|----------|-------|
| Critical | 1     |
| High     | 2     |
| Medium   | 6     |
| Low      | 5     |
| **Total**| **14**|

## Top 3 Risks for Architect Attention

**SR-01 (Critical) — col-009 Hard Dependency: Unmerged Prerequisite**
The SessionClose handler design depends on `SignalOutput.final_outcome` from col-009. The GH Issue is "TBD (to be created after col-009 merge)" — col-009 is not yet confirmed merged. col-010 implementation cannot begin until col-009 ACs are all passing. Hard gate.

**SR-02 (High) — Feature Bundle Delivery Risk**
7 components bundled under one feature (schema v5, UDS integration, GC, auto-outcomes, structured retrospective, tiered output, lesson-learned auto-persist). Components 6–7 (tiered output, lesson-learned) are independent from the session persistence core that col-011 depends on. A blocker in evidence synthesis or ONNX embedding could delay the col-011 critical path. Recommend explicit P0/P1 split in the implementation brief.

**SR-04 (Medium) — INJECTION_LOG Orphan Records on Session GC**
`gc_sessions()` deletes SESSIONS records but does not cascade to INJECTION_LOG. INJECTION_LOG uses a monotonic key with `session_id` as a field, not a foreign key. After 30 days of steady use, orphaned records accumulate (~150K+), degrading `from_structured_events()` full-scan performance. This is a data integrity gap that must be addressed in the specification before storage implementation begins.

## Inputs Read

- `/workspaces/unimatrix/product/features/col-010/SCOPE.md`
- `/workspaces/unimatrix/product/PRODUCT-VISION.md`

## Status

COMPLETE
