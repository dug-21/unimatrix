## ADR-003: Baseline Arithmetic Edge Cases

### Context

SR-01 flags that baseline standard deviation computation can produce degenerate values: if all historical values for a metric are identical, stddev is 0.0, and the outlier check `current > mean + 1.5 * stddev` becomes `current > mean`, which flags anything above the historical constant as an outlier. Additionally, metrics that are always 0 (e.g., cold restarts in a healthy project) would flag the first non-zero occurrence.

The scope specifies mean + 1.5 sigma for outlier flagging. This is a display threshold (shown in the report), not a detection threshold (rules use bootstrapped thresholds). Getting it wrong produces noisy comparison tables, not false hotspot detections.

### Decision

Explicit guards at three levels:

1. **Minimum sample count**: `compute_baselines()` returns `None` if fewer than 3 MetricVectors are provided. No comparison table is generated.

2. **Per-metric minimum variance**: When stddev is 0.0 (all identical values), the metric is marked as "no variance" in the comparison and never flagged as an outlier. The comparison table shows the current value and mean but the status column shows "---" instead of "normal" or "outlier".

3. **Zero-mean guard**: When mean is 0.0 and stddev is 0.0, any non-zero current value is shown as "new signal" rather than "outlier". This handles metrics that have historically been zero (e.g., cold restarts, rework events) appearing for the first time.

No NaN or Inf can propagate to the report. All division operations use checked arithmetic patterns:
```rust
let stddev = if variance > 0.0 { variance.sqrt() } else { 0.0 };
let is_outlier = stddev > 0.0 && current > mean + 1.5 * stddev;
```

### Consequences

- **Easier**: Report always contains valid, meaningful values. No NaN/Inf in MCP tool responses. Edge cases produce informative labels ("no variance", "new signal") rather than misleading "outlier" flags.
- **Harder**: Comparison table has three status modes (normal/outlier/no-variance/new-signal) instead of two (normal/outlier). Report formatting must handle all cases.
