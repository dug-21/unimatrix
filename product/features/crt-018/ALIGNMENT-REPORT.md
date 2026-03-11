# Alignment Report: crt-018

> Reviewed: 2026-03-11
> Artifacts reviewed:
>   - product/features/crt-018/architecture/ARCHITECTURE.md
>   - product/features/crt-018/specification/SPECIFICATION.md
>   - product/features/crt-018/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly serves "trustworthy, correctable, auditable" knowledge lifecycle and confidence evolution pillars |
| Milestone Fit | PASS | Explicitly listed in Activity Intelligence milestone, Wave 3 |
| Scope Gaps | PASS | All 15 SCOPE acceptance criteria covered in source docs |
| Scope Additions | WARN | Spec adds AC-16 and AC-17 beyond SCOPE's 15 ACs; both trace to SCOPE-RISK-ASSESSMENT recommendations |
| Architecture Consistency | PASS | Consolidation of 4 scan methods into 1 follows SR-07 recommendation; architecture ADRs well-justified |
| Risk Completeness | PASS | 13 risks with full traceability to all 8 scope risks; edge cases thorough |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | Store scan methods consolidated | SCOPE proposed 4 independent Store methods (`scan_injection_stats_by_entry`, `scan_session_outcomes_by_entry`, `scan_topic_activity`, `scan_injection_confidence_buckets`). Architecture consolidates into single `compute_effectiveness_aggregates()` + `load_entry_classification_meta()`. Rationale: follows StatusAggregates pattern per ADR-004 (#704), addresses SR-01 and SR-07. Acceptable — reduces SQL round-trips. |
| Addition | AC-16: NULL topic handling | Not in SCOPE's 15 ACs. Added in spec based on SCOPE-RISK-ASSESSMENT SR-06 and known failure mode (#981). Adds explicit "(unattributed)" bucket for NULL/empty topic entries. Justified — prevents a known production bug pattern. |
| Addition | AC-17: Named constants for outcome weights | Not in SCOPE's 15 ACs. Added in spec based on SCOPE-RISK-ASSESSMENT SR-08. Requires OUTCOME_WEIGHT_* as named constants. Justified — prevents magic numbers. |
| Addition | DataWindow struct in output | Not in SCOPE. Architecture adds DataWindow (session_count, earliest_session_at, latest_session_at) to EffectivenessReport. Traces to SCOPE-RISK-ASSESSMENT SR-02 recommendation. Justified — consumers need GC window context. |
| Addition | noisy_trust_sources parameter | SCOPE hardcodes Noisy to trust_source="auto". Architecture makes it a configurable array constant `NOISY_TRUST_SOURCES`. Traces to SCOPE-RISK-ASSESSMENT SR-05. Justified — one-line future extensibility with no added complexity. |
| Simplification | SCOPE AC-14 boundary conditions corrected | SCOPE AC-14 references "exactly 5 injections, exactly 30-day cutoff" as boundary tests. Spec AC-14 corrects to "exactly INEFFECTIVE_MIN_INJECTIONS threshold, exactly 30% success rate" — aligning with the actual thresholds (3 injections, no hardcoded time cutoff). The SCOPE text contained stale values from an earlier draft. |

## Variances Requiring Approval

None. All deviations from SCOPE are either:
- Simplifications that improve the design (store method consolidation)
- Additions that trace directly to SCOPE-RISK-ASSESSMENT recommendations (AC-16, AC-17, DataWindow, noisy_trust_sources)
- Corrections of internal SCOPE inconsistencies (AC-14 boundary values)

No VARIANCE or FAIL items requiring human approval.

## Detailed Findings

### Vision Alignment

The product vision states the core value proposition as: "Trust + Lifecycle + Integrity + Learning + Invisible Delivery." crt-018 directly serves two of these pillars:

- **Trust**: Validates whether the confidence formula actually predicts entry utility via calibration buckets. If confidence 0.7 entries succeed only 30% of the time, the system's trust signal is broken — and crt-018 reveals that. (SCOPE Goal 3, Spec FR-04)
- **Learning**: Measures effectiveness of the self-learning pipeline's auto-extracted entries by computing per-trust-source aggregate metrics. This is the first empirical test of whether the neural extraction pipeline (crt-007/008) creates value. (SCOPE Goal 2, Spec FR-03)

The vision describes "confidence evolution from real usage signals" — crt-018 is the measurement layer that validates those signals work.

The feature is explicitly read-only and measure-only (SCOPE Non-Goals, Spec NOT in Scope). This respects the vision principle of not taking automated actions without validation — crt-018 provides the data, humans decide.

### Milestone Fit

Product vision lists crt-018 under "Activity Intelligence" milestone, Wave 3:
> "crt-018: Knowledge Effectiveness Analysis — Per-entry utility scoring from injection_log + session outcomes. Confidence calibration validation. Dead knowledge detection. Surfaces via context_status."

The source documents deliver exactly this scope. No Wave 1/Wave 2 dependencies are assumed — the SCOPE explicitly states "No topic_deliveries dependency" and uses entries.topic + sessions.feature_cycle directly. This avoids coupling to col-017 (Wave 1), which is appropriate for a Wave 3 feature that can proceed independently.

### Architecture Review

The architecture makes four ADR-backed decisions, all well-justified:

1. **ADR-001 (consolidated Store method)**: Follows the existing StatusAggregates pattern (Unimatrix #704). Addresses SR-01 (performance) and SR-07 (pattern consistency). The decision to use one `compute_effectiveness_aggregates()` call with a single `lock_conn()` scope also mitigates the GC race condition (R-07).

2. **ADR-002 (NULL handling)**: Explicit "(unattributed)" sentinel for NULL/empty topic. Sessions with NULL feature_cycle excluded from active_topics but included in injection JOINs. Addresses SR-06. Test coverage mandated for both NULL and empty string cases (R-02 in risk strategy).

3. **ADR-003 (DataWindow)**: Output includes session count and time range so consumers understand the GC-bounded analysis window. Addresses SR-02.

4. **ADR-004 (configurable noisy sources)**: `NOISY_TRUST_SOURCES: &[&str] = &["auto"]` instead of hardcoded string comparison. Addresses SR-05.

The component interaction diagram is clear: Store (SQL aggregation) -> Engine (pure classification) -> Server (formatting). The separation of `EffectivenessAggregates` (store) from `EffectivenessReport` (engine output) maintains the established crate boundary pattern.

The architecture correctly identifies `load_entry_classification_meta()` as a separate query method. This is not in the SCOPE's proposed approach but is a sound design decision — entry metadata (title, topic, trust_source, helpful_count) is needed for classification but is conceptually separate from injection/session aggregation.

### Specification Review

The specification faithfully translates all 15 SCOPE acceptance criteria and adds two more (AC-16, AC-17) traced to scope risk recommendations. Key observations:

- **Classification priority order** (FR-01): Noisy > Ineffective > Unmatched > Settled > Effective. This is specified clearly and is the correct design — auto-extracted noise should be flagged before general ineffectiveness.

- **Weighted success rate formula** (FR-02): Well-defined with named constants. The spec clarifies that sessions with NULL outcome are excluded from both numerator and denominator — important detail not explicit in SCOPE.

- **Three output formats** (FR-05, FR-06, FR-07): Each format is specified in detail with example output. The JSON schema matches the architecture's EffectivenessReport type. The summary format includes session count for coverage context.

- **Domain model naming**: The spec uses `weighted_success_rate` and `helpful_count` where the SCOPE's proposed EntryEffectiveness used `success_rate` and `helpfulness_ratio`. The architecture uses `success_rate` and `helpfulness_ratio`. This is a minor inconsistency between spec domain models and architecture types, but both communicate the same fields. The architecture types are what will be implemented.

- **Constraints**: All 6 SCOPE constraints are restated in the spec (some expanded). The spec adds constraints 7-10 from scope risk recommendations.

### Risk Strategy Review

The risk strategy is thorough:

- **13 risks** identified, covering classification logic (R-01), NULL handling (R-02), SQL aggregation correctness (R-03), calibration boundaries (R-04), division by zero (R-05), performance (R-06), GC race (R-07), JSON compatibility (R-08), Settled logic (R-09), case sensitivity (R-10), spawn_blocking failure (R-11), markdown injection (R-12), and NaN in aggregates (R-13).

- **Full scope risk traceability**: All 8 SCOPE-RISK-ASSESSMENT items (SR-01 through SR-08) are mapped to architecture resolutions and risk strategy items. The traceability table at the end of the document provides clear linkage.

- **Critical risks** (R-01, R-02) have the most test scenarios (10 combined). Both are genuine high-risk areas — classification priority ordering is easy to get wrong, and NULL handling has bitten the project twice before (#756, #981).

- **Edge cases** section covers 9 scenarios including the "all rework" case where utility_score = 0.5 means no entries are Ineffective despite poor outcomes. This is a correct observation and the behavior is acceptable per the spec's 30% threshold.

- **Security section** correctly identifies this as a low-risk feature (read-only analytics on internal data). The output size amplification concern is mitigated by capped lists.

One observation: R-12 (markdown table injection via entry titles) is Low severity but worth noting — the risk strategy correctly identifies it but does not mandate a specific sanitization approach. The implementation should escape pipe characters in entry titles rendered in markdown tables.
