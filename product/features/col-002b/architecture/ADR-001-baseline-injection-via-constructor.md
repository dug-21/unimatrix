## ADR-001: Baseline Data Injection via Constructor

### Context

The phase duration outlier rule (scope hotspot) needs historical MetricVector data to compute per-phase baseline means. The `DetectionRule` trait defined by col-002 provides only `detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>` — there is no mechanism to pass additional data.

Three options:
1. Extend `DetectionRule` trait with an optional `set_context()` method — breaks col-002's established interface.
2. Pass baseline data through a wrapper around `ObservationRecord` — pollutes the record type for one rule.
3. Inject historical data via the rule struct's constructor — the struct holds the data, `detect()` uses it.

SR-07 flags that col-002's interfaces are not yet in code. Extending the trait now risks creating an interface that col-002 must then conform to. Constructor injection avoids modifying col-002's trait entirely.

### Decision

`PhaseDurationOutlierRule` receives `Option<Vec<MetricVector>>` in its constructor. When present, it computes per-phase mean durations and uses 2x mean as the threshold. When absent (or fewer than 3 data points for a phase), it falls back to an absolute threshold.

```rust
impl PhaseDurationOutlierRule {
    pub fn new(history: Option<&[MetricVector]>) -> Self { ... }
}
```

The `default_rules()` function signature changes to accept `history: Option<&[MetricVector]>`, which it passes only to `PhaseDurationOutlierRule::new()`. All other rules ignore it.

### Consequences

- **Easier**: col-002's `DetectionRule` trait is completely unchanged. All 17 other rules use the trait identically to col-002's 3 rules. Adding future rules that need context follows the same constructor pattern.
- **Harder**: `default_rules()` signature changes from `fn() -> Vec<...>` to `fn(Option<&[MetricVector]>) -> Vec<...>`, requiring a call-site update in the server handler. This is a one-line change.
