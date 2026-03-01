# Alignment Report: col-002b

> Reviewed: 2026-03-01
> Artifacts reviewed:
>   - product/features/col-002b/architecture/ARCHITECTURE.md
>   - product/features/col-002b/specification/SPECIFICATION.md
>   - product/features/col-002b/RISK-TEST-STRATEGY.md
>   - product/features/col-002b/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly supports M5 Retrospective Pipeline — observation-driven improvement |
| Milestone Fit | PASS | col-002b is explicit M5 scope, completing col-002's detection library |
| Scope Gaps | PASS | All 20 acceptance criteria from SCOPE.md traced through specification |
| Scope Additions | PASS | No additions beyond scope — architecture and specification stay within bounds |
| Architecture Consistency | PASS | Extends col-002 patterns without modification; 3 ADRs address scope risks |
| Risk Completeness | PASS | 12 risks covering all scope risk traceability items; 44 test scenarios |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| — | — | No gaps, additions, or simplifications identified |

All 18 detection rules from SCOPE.md are specified in FR-01 through FR-04. Baseline computation (FR-06, FR-07) covers SCOPE.md Goals section 2. Phase duration outlier (FR-04.5) covers SCOPE.md Goals section 3. Server integration (FR-09) covers SCOPE.md Proposed Approach section 4. All non-goals from SCOPE.md are reflected in the specification's NOT in Scope section.

## Variances Requiring Approval

None.

## Detailed Findings

### Vision Alignment

col-002b directly supports the product vision's M5 goal: "System observes agent behavior, identifies process hotspots from evidence, and proposes improvements." The 18 additional rules expand detection coverage from 3 proof-of-concept rules to the full 21-rule library, enabling meaningful retrospective reports. The baseline comparison adds statistical context that moves beyond raw hotspot detection toward "what is normal for this project" — a step toward the vision's "gets better with every feature delivered" principle.

The feature maintains the core value proposition of trustworthy, auditable knowledge. Detection rules are rule-based (no model required), thresholds are transparent (bootstrapped constants), and baseline computation uses standard statistics. The LLM reasons about findings in conversation — the analysis engine itself is deterministic.

### Milestone Fit

col-002b is explicitly part of M5 (Collective Phase — Orchestration Engine). The product roadmap describes col-002 as shipping with a minimal rule set and col-002b completing the full detection library. This feature fits the roadmap exactly.

No M6+ capabilities are introduced. Baseline comparison is explicitly scoped as display-only (no threshold convergence, no compound signal detection). These are future work correctly deferred.

### Architecture Review

The architecture extends col-002 cleanly:

1. **DetectionRule trait unchanged** — ADR-001 solves the phase duration outlier's need for historical data via constructor injection rather than trait extension. This preserves col-002's interface.

2. **Detection submodule organization** (ADR-002) — a pragmatic refactor that organizes 21 rules into 4 category files. The refactor is backwards-compatible (re-exports from mod.rs).

3. **Baseline arithmetic guards** (ADR-003) — addresses SR-01 with explicit handling for zero-stddev, zero-mean, and NaN prevention. The four-status model (Normal/Outlier/NoVariance/NewSignal) is well-defined.

4. **Crate independence maintained** — baseline module lives in `unimatrix-observe`, takes `&[MetricVector]` as input. No new dependencies on store or server.

5. **Minimal server changes** — only the `context_retrospective` handler is modified, adding 3 steps (load history, compute baselines, pass to report).

### Specification Review

The specification provides:

- 9 functional requirements (FR-01 through FR-09) covering all detection rules, registration, baseline computation, comparison, report extension, and server integration
- Complete record access pattern table mapping each rule to specific ObservationRecord fields
- Domain models for BaselineSet, BaselineEntry, BaselineComparison, and BaselineStatus
- Non-functional requirements for performance (2s for rules, 100ms for baselines) and extensibility
- 20 acceptance criteria traced from SCOPE.md with verification methods

FR-09.3 (exclude current feature from baseline) is a thoughtful addition that prevents self-comparison bias — this is within scope as it is necessary for correct baseline behavior.

### Risk Strategy Review

12 risks identified with 44 test scenarios. Coverage:

- R-01, R-12 (High priority) focus on the most critical risk: rules silently failing due to incorrect record field access. 14 scenarios for these two risks alone.
- R-02, R-03, R-04 (Medium) address baseline arithmetic and regex pattern reliability.
- R-05 (Medium, High severity) addresses the detection.rs refactor risk with explicit regression tests.
- R-10 (Medium) addresses baseline self-comparison.

All 9 scope risks (SR-01 through SR-09) are traced in the Scope Risk Traceability table. Each has a corresponding architecture risk or explicit "accepted" disposition.

Security review is appropriate — col-002b adds no new security surface. The only flag (regex DoS from untrusted Bash command strings) is correctly identified and mitigated.
