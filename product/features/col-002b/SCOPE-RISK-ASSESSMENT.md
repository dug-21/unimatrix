# Scope Risk Assessment: col-002b

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Baseline stddev computation produces NaN/Inf with degenerate data (all identical values -> stddev=0, division by zero in outlier check) | Med | Med | Architect should define explicit handling for zero-stddev metrics and edge cases in baseline arithmetic |
| SR-02 | MetricVector field additions for baseline comparison break bincode deserialization of col-002-era stored vectors | High | Low | RetrospectiveReport extension (not MetricVector) — confirm no MetricVector schema changes. Architect should verify serde(default) coverage |
| SR-03 | 18 rules with independent regex/scanning passes over records could degrade retrospective performance on large sessions | Med | Low | Architect should consider whether rules share a single scan pass or accept N-pass overhead for simplicity |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Phase duration outlier rule depends on baseline infrastructure — creates an ordering dependency within col-002b implementation | Low | High | Spec writer should define implementation ordering: baseline module first, then phase duration outlier rule |
| SR-05 | "No changes to MetricVector structure" constraint (AC-14) may conflict if new rules need new universal metric fields to store their computed values | Med | Med | Architect should confirm all 18 rule metrics map to existing UniversalMetrics fields defined in col-002 |
| SR-06 | Baseline comparison minimum 3 data points — early adopters will rarely have enough history, making the feature effectively dormant initially | Low | High | Accepted. Scope correctly handles this with "insufficient history" message. No design action needed. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | col-002 not yet implemented — col-002b design depends on interfaces (DetectionRule trait, MetricVector, RetrospectiveReport) that exist only in col-002's design docs, not in code | High | Med | Architect should design against col-002's specified interfaces exactly. Any col-002 implementation deviation requires col-002b rework |
| SR-08 | Rules reference ObservationRecord fields (tool, input, response_size) — if col-002's parser normalization differs from what rules expect, detection logic fails silently | Med | Med | Spec writer should define each rule's expected record field patterns precisely against col-002's ObservationRecord spec |
| SR-09 | Baseline computation needs list_all_metrics() from store — if col-002 stores MetricVectors with different bincode config or format, deserialization fails | Med | Low | Architect should ensure baseline module uses same serialize/deserialize helpers defined in col-002's ADR-002 |

## Assumptions

1. **col-002's DetectionRule trait is sufficient for all 18 rules** (Goals section) — the trait signature `detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>` gives each rule the full record set. If any rule needs additional context (e.g., historical MetricVectors for phase duration outlier), the trait may need extension.
2. **All universal metric fields for the 18 rules already exist in col-002's UniversalMetrics** (Constraints section) — col-002 computes ALL universal metrics regardless of shipped rules. If any col-002b rule needs a metric not in UniversalMetrics, the "no MetricVector changes" constraint is violated.
3. **Baseline comparison output fits within MCP tool response size limits** — the comparison table with 20+ metrics across potentially many features could produce large responses.

## Design Recommendations

1. **(SR-01, SR-05)** Architect should map each of the 18 rules to the specific UniversalMetrics or PhaseMetrics fields they populate, confirming no gaps before designing the baseline module.
2. **(SR-07)** Design col-002b as a pure extension of col-002's specified interfaces. Do not introduce new traits or modify DetectionRule. If phase duration outlier needs baseline data, pass it through a separate mechanism (constructor injection, not trait change).
3. **(SR-08)** Each rule's specification should include the exact ObservationRecord field access pattern and regex patterns used for detection.
