# ADR-006: P0/P1 Component Split

**Feature**: col-010
**Status**: Accepted
**Date**: 2026-03-02

## Context

SR-02 flagged delivery risk from bundling 7 components + 24 acceptance criteria in one feature. The scope risk assessment noted that components 1–5 are core session lifecycle persistence required by col-011, while components 6–7 (tiered retrospective output + lesson-learned) resolve issue #65 but do not block col-011.

A delivery blocker in the evidence synthesis path or lesson-learned ONNX embedding flow would delay the entire feature, including the SESSIONS and INJECTION_LOG tables that col-011 depends on.

## Decision

The 7 components are split into two explicit priority tiers:

**P0 — required for col-011 (must ship together)**:

| # | Component | Rationale |
|---|-----------|-----------|
| 1 | Storage Layer: SESSIONS + INJECTION_LOG + schema v5 | Foundation; col-011 reads both tables |
| 2 | UDS Listener Integration | Populates both tables from hook events |
| 3 | Session GC with INJECTION_LOG cascade | Data integrity; bounded growth |
| 4 | Auto-Generated Session Outcomes | col-001 integration; closes the outcome loop |

**P1 — independent (resolves issue #65, can slip to col-010b)**:

| # | Component | Rationale |
|---|-----------|-----------|
| 5 | Structured Retrospective (`from_structured_events()`) | New retro entry point; uses P0 tables |
| 6 | Tiered Retrospective Output + Evidence Synthesis | Resolves issue #65; no col-011 dependency |
| 7 | Lesson-Learned Auto-Persistence + Provenance Boost | Knowledge quality; no col-011 dependency |

**If implementation timeline is constrained**: ship P0 components first, gate col-011 on P0 acceptance criteria only (AC-01 through AC-11), then follow with P1 as a continuation.

**If shipping together**: P1 work begins only after P0 acceptance criteria pass. All 24 ACs apply to the combined delivery.

## Rationale

This split has zero architecture impact — P0 and P1 components touch different files and are independently testable. The split is purely a delivery sequencing decision.

col-011 explicitly depends on SESSIONS + INJECTION_LOG (P0). It has no dependency on `from_structured_events()`, tiered output, or lesson-learned entries (P1).

Components 5–7 are more novel (new `from_structured_events()` entry point, evidence synthesis heuristics, fire-and-forget embedding path) and carry higher implementation risk. Isolating them from the P0 critical path protects col-011's timeline.

## Consequences

- Implementation brief must label each component with P0/P1.
- P0 acceptance criteria: AC-01 through AC-11 + AC-24 (regression).
- P1 acceptance criteria: AC-12 through AC-23 + AC-24 (regression).
- If P0 ships first (without P1), the `context_retrospective` tool defaults to the JSONL path until P1 adds `from_structured_events()`. No user-visible regression — existing behavior is preserved.
- P1 components can ship in a follow-on without a schema migration (they touch only application logic, not the database).
