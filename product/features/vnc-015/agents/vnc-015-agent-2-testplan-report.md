# Agent Report: vnc-015-agent-2-testplan

**Phase**: Stage 3a — Test Plan Design
**Agent ID**: vnc-015-agent-2-testplan
**Feature**: vnc-015 — Typed Edge Write Path + context_edge Tool

---

## Output Summary

9 test plan files created in `product/features/vnc-015/test-plan/`:

| File | Components Covered | ACs | Risks |
|------|--------------------|-----|-------|
| OVERVIEW.md | All 8 | All 26 (mapped) | All R-01–R-15 |
| edge-input-params.md | Component 1 | AC-01,02,07,08,09,15 | R-08,R-09,R-12 |
| edge-write.md | Component 2 | AC-05,06,10,18,25,26 | R-02,R-03,R-04,R-15 |
| relation-type.md | Component 3 | AC-03,04,14 | R-01 (Critical) |
| ppr-expand.md | Component 4 | AC-04,17 | R-11 |
| stale-dependency.md | Component 5 | AC-11 | R-14 |
| detection-rule.md | Component 6 | AC-12,13 | R-10 (High) |
| contradicts-fix.md | Component 7 | AC-16 | R-07 |
| context-edge-handler.md | Component 9 (13th tool) | AC-15,19-26 | R-05,R-06,R-13 |

---

## Risk Coverage Mapping

| Risk | Severity | Covered By | Test Count |
|------|----------|-----------|------------|
| R-01 | Critical | relation-type.md | 20 (10 round-trip + 10 Pass 2b survival — per-variant, individually named) |
| R-02 | Critical | edge-write.md | 4 (RAII code-review gate + 3 integration: non-Contradicts, Contradicts 4-row, rollback) |
| R-03 | Critical | edge-write.md | 2 (idempotent re-assert non-Contradicts + Contradicts) |
| R-04 | Critical | edge-write.md + context-edge-handler.md | 4 (Contradicts bidirectionality: edges param write, context_edge add, context_edge remove, context_edge redirect) |
| R-05 | Critical | context-edge-handler.md | 3 (rollback on bad target, quarantined new target, atomic success) |
| R-06 | High | context-edge-handler.md | 3 (quarantined source, deprecated source, active baseline all modes) |
| R-07 | High | contradicts-fix.md | 4 (source direction, target direction transition compat, bidirectional, caller regression) |
| R-08 | High | edge-input-params.md | 2 (first-error-abort 5-edge slice, latency note) |
| R-09 | High | edge-input-params.md + context-edge-handler.md | 2 (context_store post-insert actual ID, context_edge pre-operation) |
| R-10 | High | detection-rule.md | 3 (count=23, rule fires, caller audit compile gate) |
| R-11 | Medium | ppr-expand.md | 4 (RelatedTo flows, Advances absent, Motivates absent, existing types regression) |
| R-12 | Medium | edge-input-params.md | 1 (duplicate guard before edge writes) |
| R-13 | Medium | context-edge-handler.md | 2 (new_target_id rejected for add, rejected for remove) |
| R-14 | Medium | stale-dependency.md | 4 (zero-when-none, zero-when-active, count-deprecated, Prerequisite-only) |
| R-15 | Low | edge-write.md | 1 (constant usage code-review gate + distinctness unit test) |

---

## Integration Suite Plan

Suites selected for Stage 3c:

| Suite | Justification |
|-------|---------------|
| `smoke` | Mandatory minimum gate |
| `tools` | 13th tool added; context_store/context_correct param extensions; tool count 12→13 |
| `protocol` | Tool discovery reflects 13 tools |
| `lifecycle` | Edge persist across restart; stale dependency in context_status |
| `security` | SourceFrozen validation; capability gate for context_edge |
| `edge_cases` | Empty edges vec; idempotent remove of non-existent edge; redirect no-op |
| `contradiction` | Bidirectional Contradicts write/query/remove |

Suites NOT required: `confidence`, `volume`, `adaptation`.

New tests to add to infra-001 (detailed in OVERVIEW.md): approximately 30 new test functions
across `test_tools.py` and `test_lifecycle.py`.

---

## Open Questions Carried Into Stage 3b

1. **OQ-4 (Tool count test location)**: The specific test asserting `12 tools` must be located
   in Stage 3b and updated to 13. Likely in `suites/test_tools.py` or `suites/test_protocol.py`.
   Stage 3c must verify this update was made.

2. **R-07 caller audit**: `suppress_contradicts` is the named caller in ARCHITECTURE.md.
   Stage 3b must grep for all callers of `query_contradicts_edges_for_entry` before implementing
   the OR-clause fix. Any additional callers must be audited and tested.

3. **R-05 redirect rollback mechanism**: The rollback test (redirect to non-existent target)
   relies on target validation firing before the transaction opens. If the implementation
   validates after opening the transaction and the test infrastructure cannot inject mid-
   transaction failures, an alternative rollback signal (e.g., simulated disk-full) may be
   needed. This is flagged for Stage 3b review.

4. **DependencyOnDeprecated scope**: `context_cycle_review` pre-queries stale Prerequisite
   edges for the current cycle's entries. The exact scoping query (which entries are "in the
   cycle") must be confirmed in Stage 3b. Integration test setup for AC-12 depends on this.

---

## Self-Check Results

- [x] OVERVIEW.md maps all risks R-01–R-15 from RISK-TEST-STRATEGY.md to test scenarios
- [x] OVERVIEW.md includes integration harness plan with specific suite selections and new tests
- [x] Per-component test plans match architecture Component Map boundaries (8 files)
- [x] Every Critical risk (R-01–R-05) has at minimum 2 specific test expectations
- [x] Integration tests defined for all component boundaries
- [x] All output files in `product/features/vnc-015/test-plan/`
- [x] All 26 ACs mapped in OVERVIEW.md coverage table
- [x] SR-01 grep verification procedure included in relation-type.md (ADR-007 mitigation)

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 12 entries. Most directly relevant:
  #4066 (BFS direction test pairing — applied in ppr-expand.md), #4046 (EDGE_SOURCE_AGENT
  constant test pattern — applied in edge-write.md), #3631 (sibling module inline tests —
  informed edge_write.rs test placement), #4422/#4424 (ADR-005/ADR-007 — confirmed SR-01
  checklist scope and grep commands).
- Queried: ADR searches returned #4421–#4429 entries covering all 10 vnc-015 ADRs.
  Sufficient to design plans without reading ADR files individually.
- Stored: nothing novel to store — test plan patterns applied here are drawn from existing
  Unimatrix entries (#4066, #4046, #3631). The per-variant Pass 2b survival test pattern
  (10×individually-named tests) is already captured in ADR-007 (#4424). No new cross-feature
  pattern emerged.
