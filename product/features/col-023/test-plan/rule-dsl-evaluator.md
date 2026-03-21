# Test Plan: rule-dsl-evaluator

**Component**: `crates/unimatrix-observe/src/domain/mod.rs` — `RuleEvaluator`, `RuleDescriptor`, `ThresholdRule`, `TemporalWindowRule`
**AC Coverage**: AC-05 (partial — DSL rules fire for the sre pack)
**Risk Coverage**: R-07 (temporal window sort), R-08 (field_path numeric extraction silent skip)

---

## Unit Test Expectations

### Location: inline `#[cfg(test)]` in `unimatrix-observe/src/domain/mod.rs`
### or: `crates/unimatrix-observe/tests/domain_pack_tests.rs` (same file as DPR tests)

---

### Threshold Rules

### T-DSL-01: Threshold rule fires when count exceeds threshold

```rust
// test_threshold_rule_fires_on_count_exceeded
// Arrange: ThresholdRule { name: "many-events", source_domain: "sre",
//          event_type_filter: vec!["incident_opened"], field_path: "",
//          threshold: 3.0, severity: "warn", claim_template: "..." }
// Input: 4 ObservationRecord { event_type: "incident_opened", source_domain: "sre" }
// Act: RuleEvaluator::detect(&records)
// Assert: returns Vec with one HotspotFinding
```

### T-DSL-02: Threshold rule does not fire at exact threshold

```rust
// test_threshold_rule_does_not_fire_at_threshold
// Same setup as T-DSL-01 but only 3 records (== threshold, not >)
// Assert: returns empty Vec
```

### T-DSL-03: Threshold rule domain guard — non-matching source_domain produces no findings

```rust
// test_threshold_rule_ignores_wrong_source_domain
// Arrange: ThresholdRule with source_domain = "sre"
// Input: 10 records with source_domain = "claude-code" and event_type = "incident_opened"
// Assert: returns empty Vec (R-01 domain guard check at DSL level)
```

### T-DSL-04: Threshold rule with field_path — numeric extraction

```rust
// test_threshold_rule_field_path_numeric_extraction
// Arrange: ThresholdRule { field_path: "/response_size", threshold: 1000.0, ... }
// Input: records with input = Some(json!({"response_size": 2000}))
//        (field_path resolves to a numeric value > threshold)
// Assert: HotspotFinding emitted
```

### T-DSL-05: field_path resolves to non-numeric value — no panic, no finding (R-08)

```rust
// test_threshold_field_path_non_numeric_silent_skip
// Arrange: ThresholdRule { field_path: "/tool_name", threshold: 5.0, ... }
// Input: records with input = Some(json!({"tool_name": "Bash"})) — string value
// Act: detect(&records)
// Assert: returns empty Vec (no finding)
// Assert: no panic
// Note: a WARN-level log is emitted (verify via tracing subscriber if available,
//       or document as a "best effort" observable)
```

### T-DSL-06: field_path missing from payload — no panic, no finding (R-08)

```rust
// test_threshold_field_path_missing_key_no_panic
// Arrange: ThresholdRule { field_path: "/nonexistent/path", threshold: 1.0 }
// Input: records with input = Some(json!({"other_key": 42}))
// Assert: returns empty Vec
// Assert: no panic
```

### T-DSL-07: field_path empty — count-based threshold

```rust
// test_threshold_empty_field_path_counts_events
// Arrange: ThresholdRule { field_path: "", threshold: 2.0, source_domain: "sre",
//          event_type_filter: vec!["tick"] }
// Input: 3 records { event_type: "tick", source_domain: "sre" }
// Assert: HotspotFinding emitted (count 3 > threshold 2)
```

### T-DSL-08: event_type_filter — only matching event types counted

```rust
// test_threshold_event_type_filter_excludes_non_matching
// Arrange: ThresholdRule { event_type_filter: vec!["incident_opened"], threshold: 2.0 }
// Input: 3 records { event_type: "incident_resolved", source_domain: "sre" }
//        + 1 record { event_type: "incident_opened", source_domain: "sre" }
// Assert: empty Vec (only 1 match; threshold > 2 not met)
```

---

### Temporal Window Rules

### T-DSL-09: window_secs = 0 rejected at load time (EC-08)

```rust
// test_temporal_window_zero_secs_rejected
// Arrange: TemporalWindowRule { window_secs: 0, ... }
// Act: DomainPackRegistry::new(vec![pack_with_zero_window_rule])
// Assert: Err(ObserveError::InvalidRuleDescriptor { rule_name: ..., reason: ... })
// This is a startup validation, not a runtime check.
```

### T-DSL-10: Temporal window rule fires on N+1 events within window

```rust
// test_temporal_window_fires_within_window
// Arrange: TemporalWindowRule { window_secs: 60, threshold: 3.0,
//          event_type_filter: vec!["deploy_triggered"], source_domain: "sre" }
// Input: 4 records with ts values all within 60 seconds of each other
// Assert: HotspotFinding emitted
```

### T-DSL-11: Temporal window rule does not fire when events span beyond window

```rust
// test_temporal_window_does_not_fire_outside_window
// Input: 4 records where max - min ts > window_secs * 1000 ms
// Assert: empty Vec
```

### T-DSL-12: Temporal window rule with unsorted input fires correctly (R-07, CRITICAL)

```rust
// test_temporal_window_unsorted_input_fires
// Arrange: TemporalWindowRule { window_secs: 60, threshold: 2.0, source_domain: "sre",
//          event_type_filter: vec!["alarm"] }
// Input: records in reverse ts order — all within 60 seconds
// Assert: HotspotFinding emitted
// This test FAILS if detect() does not sort by ts before the two-pointer scan.
```

### T-DSL-13: Temporal window sorted vs unsorted produces equivalent result (R-07)

```rust
// test_temporal_window_sorted_vs_unsorted_equivalent
// Arrange: same records in sorted and reverse-sorted order
// Assert: detect(sorted) == detect(reverse_sorted)
// Proves sort-independence of the output.
```

### T-DSL-14: Temporal window boundary — exactly N events in window does not fire

```rust
// test_temporal_window_boundary_exact_threshold_does_not_fire
// Input: exactly threshold events within window_secs
// Assert: empty Vec (threshold means >, not >=)
```

### T-DSL-15: Temporal window boundary — N+1 events in window fires

```rust
// test_temporal_window_boundary_one_over_threshold_fires
// Input: threshold + 1 events within window_secs
// Assert: one HotspotFinding
```

### T-DSL-16: Temporal window domain guard (R-01)

```rust
// test_temporal_window_rule_ignores_wrong_source_domain
// Arrange: TemporalWindowRule { source_domain: "sre", ... }
// Input: records with source_domain = "claude-code"
// Assert: empty Vec
```

---

### Rule Descriptor Validation

### T-DSL-17: Missing source_domain in rule descriptor rejected at load

```rust
// test_rule_descriptor_missing_source_domain_rejected
// Arrange: ThresholdRule with source_domain = "" (empty)
// Act: validate at DomainPackRegistry::new()
// Assert: Err(ObserveError::InvalidRuleDescriptor { reason contains "source_domain" })
```

### T-DSL-18: rule_file with source_domain mismatch rejected (EC-09)

```rust
// test_rule_file_source_domain_mismatch_rejected
// Arrange: DomainPack { source_domain: "sre" } loads a rule_file containing
//          a rule with source_domain = "claude-code"
// Assert: startup failure with clear mismatch error naming both domains
```

---

## Edge Cases

- Empty `records` slice → all rules return empty Vec without panic (EC-06).
- Zero records matching `event_type_filter` → no finding, no panic.
- `claim_template` with `{count}` or `{session_id}` placeholders: verify they are
  interpolated correctly in the emitted HotspotFinding (not tested here but documented
  as a correctness obligation for the implementor).
