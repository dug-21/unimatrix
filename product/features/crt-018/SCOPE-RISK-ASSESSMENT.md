# Scope Risk Assessment: crt-018

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Multi-table SQL JOINs (injection_log x sessions x entries) on every context_status call may exceed 500ms budget as data grows, especially without composite indexes on join columns | High | Med | Architect should design queries to use SQL-side aggregation (GROUP BY) and verify index coverage for all join paths; consider pre-aggregated materialized stats |
| SR-02 | Session GC (30-day DELETE_THRESHOLD) deletes injection_log rows, creating a sliding data window that makes effectiveness classifications non-deterministic across calls -- same entry may flip between Settled and Unmatched as sessions age out | Med | High | Architect should document that classifications are ephemeral snapshots; "Settled" logic must handle edge case where GC removes the only success-outcome session for a topic |
| SR-03 | StatusReport already has ~45 fields and ~700 lines of formatting; adding effectiveness section risks bloating context_status output beyond useful size for LLM consumers | Med | Med | Architect should consider conditional inclusion or summary-only defaults; use skip_serializing_if aggressively |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Scope explicitly excludes wiring effectiveness into retrieval (crt-018b), but the five-category classification and "noisy"/"ineffective" labels create implicit pressure to act on the data -- risk of scope creep into automated confidence decay or quarantine | Med | Med | Spec should draw a hard boundary: effectiveness module produces read-only reports with no side effects on entry state or confidence scores |
| SR-05 | "Noisy" classification is scoped only to trust_source="auto", but neural-extracted entries (trust_source="neural") share similar quality concerns; limiting to "auto" may miss a growing category | Low | Med | Architect should make the trust_source filter for Noisy configurable rather than hardcoded to "auto" |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Effectiveness analysis depends on injection_log and sessions tables (col-010), but NULL feature_cycle in sessions has caused silent downstream failures before (Unimatrix #981); entries with NULL topic or sessions with NULL feature_cycle will produce incorrect classifications | High | Med | Architect must handle NULL topic/feature_cycle explicitly -- either exclude from classification or assign to an "unattributed" bucket; do not silently drop |
| SR-07 | ADR-004 (Unimatrix #704) consolidated status queries into a single StatusAggregates method for performance; adding 4 new scan methods breaks this consolidation pattern and may regress status query performance | Med | Med | Architect should evaluate whether effectiveness queries can be folded into the existing StatusAggregates pattern rather than adding independent scan methods |
| SR-08 | Rework weighting (success=1.0, rework=0.5, abandoned=0.0) is a product decision embedded in code; if these weights are wrong, all effectiveness classifications are miscalibrated, but there is no feedback loop to validate the weights themselves | Med | Low | Spec should define weights as named constants with rationale; architect should ensure they are tunable without code changes |

## Assumptions

1. **injection_log has sufficient data density** (SCOPE Background Research) -- The analysis assumes injection_log contains meaningful volume. A fresh or small knowledge base will produce degenerate classifications (mostly Unmatched). The scope does not define minimum data thresholds for meaningful output.
2. **Session outcomes are reliable ground truth** (SCOPE AC-02) -- success/rework/abandoned outcomes are the sole signal for entry utility. If outcome attribution is noisy (e.g., a session marked "rework" due to unrelated issues), effectiveness scores inherit that noise.
3. **GC retention window is sufficient** (SCOPE Constraints 3) -- 30-day GC window is assumed adequate for meaningful analysis. Short-lived features that complete within a few days may have all sessions GC'd before effectiveness is ever computed.

## Design Recommendations

1. **SR-01, SR-07**: Evaluate consolidating effectiveness queries into the existing StatusAggregates single-method pattern rather than adding 4 independent scan methods. This aligns with ADR-004 and keeps the status path performant.
2. **SR-06**: Add explicit NULL-handling for topic and feature_cycle in all classification logic. Test with NULL data -- this is a known failure mode (#981).
3. **SR-02**: Document in the spec that classifications are transient snapshots bounded by GC retention. Include a "data window" indicator in the output (e.g., "Analysis covers N sessions over M days") so consumers understand coverage.
