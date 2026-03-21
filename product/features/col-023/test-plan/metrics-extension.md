# Test Plan: metrics-extension

**Component**: `crates/unimatrix-store/src/metrics.rs`
**AC Coverage**: AC-10 (UNIVERSAL_METRICS_FIELDS structural test)
**Risk Coverage**: R-11 (UNIVERSAL_METRICS_FIELDS count), R-02 (partial — compute_universal correctness), IR-03 (compute_universal domain guard)

---

## Unit Test Expectations

### Location: inline `#[cfg(test)]` in `crates/unimatrix-store/src/metrics.rs`

---

### UNIVERSAL_METRICS_FIELDS Structural Tests (R-11, AC-10)

### T-MET-01: UNIVERSAL_METRICS_FIELDS count == 22

```rust
// test_universal_metrics_fields_count_is_22
// Assert: UNIVERSAL_METRICS_FIELDS.len() == 22
// AC-10: updated from 21 to 22 to include domain_metrics_json
```

### T-MET-02: Original 21 field names preserved in declaration order (R-11)

```rust
// test_universal_metrics_fields_original_21_unchanged
// Assert: UNIVERSAL_METRICS_FIELDS[0..21] == EXPECTED_ORIGINAL_21_FIELDS
// where EXPECTED_ORIGINAL_21_FIELDS is the hardcoded list from before this feature
// AC-10: per-field alignment check for all 21 original fields must not be weakened
```

### T-MET-03: domain_metrics_json is the 22nd entry (R-11)

```rust
// test_universal_metrics_fields_22nd_is_domain_metrics_json
// Assert: UNIVERSAL_METRICS_FIELDS[21] == "domain_metrics_json"
// AC-10: verified separately from the 21-field alignment check per ADR-006
```

### T-MET-04: Negative test — removing one field fails structural test (R-11 validation)

```rust
// test_universal_metrics_fields_negative_removal_detected
// This test does NOT actually remove a field — it documents the expectation that
// if UNIVERSAL_METRICS_FIELDS were shortened to 21, T-MET-01 would fail.
// Implementation: the assertion in T-MET-01 is sufficient; this test is a
// documentation test that explains the intent of T-MET-01.
```

---

### MetricVector domain_metrics Field Tests

### T-MET-05: MetricVector has domain_metrics HashMap field

```rust
// test_metric_vector_has_domain_metrics_field
// Arrange: MetricVector { ..., domain_metrics: HashMap::new() }
// Assert: field exists and accepts HashMap<String, f64>
// Compile-time structural check.
```

### T-MET-06: store_metrics writes domain_metrics_json as NULL for claude-code

```rust
// test_store_metrics_null_domain_metrics_for_claude_code
// Arrange: MetricVector { domain_metrics: HashMap::new(), ... }
// Act: store_metrics(&mut conn, session_id, &metric_vector)
//      then raw SQL: SELECT domain_metrics_json FROM OBSERVATION_METRICS WHERE ...
// Assert: domain_metrics_json IS NULL
// FR-05.3: NULL for claude-code sessions (empty map)
```

### T-MET-07: store_metrics writes domain_metrics_json as JSON for non-empty map

```rust
// test_store_metrics_writes_domain_metrics_json
// Arrange: MetricVector { domain_metrics: HashMap { "error_rate" => 0.05, "mttr_secs" => 300.0 }, ... }
// Act: store_metrics(...), then SELECT domain_metrics_json
// Assert: domain_metrics_json == '{"error_rate":0.05,"mttr_secs":300.0}' (or equivalent JSON)
```

### T-MET-08: get_metrics reads domain_metrics_json as HashMap

```rust
// test_get_metrics_reads_domain_metrics_json
// Arrange: INSERT a row with domain_metrics_json = '{"key": 42.0}'
// Act: get_metrics(&conn, session_id)
// Assert: result.domain_metrics.get("key") == Some(&42.0)
```

### T-MET-09: get_metrics returns empty HashMap when domain_metrics_json is NULL (R-05, AC-09)

```rust
// test_get_metrics_null_domain_metrics_json_returns_empty_map
// Arrange: INSERT a row with domain_metrics_json = NULL
//          (simulates v13 row in v14 schema)
// Act: get_metrics(&conn, session_id)
// Assert: result.domain_metrics.is_empty()
// AC-09, FR-05.4: NULL deserializes as empty map
```

---

### compute_universal Domain Guard Tests (IR-03)

### T-MET-10: compute_universal returns zeros for non-claude-code records

```rust
// test_compute_universal_returns_zeros_for_non_claude_code_records
// Arrange: session with only source_domain = "sre" records,
//          one of which has event_type = "PostToolUse"
//          (deliberately matches a claude-code metric counter)
// Act: compute_universal(&records)
// Assert: ALL 21 UniversalMetrics fields == 0
// IR-03: source_domain guard prevents non-claude-code events from inflating metrics.
```

### T-MET-11: compute_universal counts only claude-code records in mixed slice

```rust
// test_compute_universal_ignores_non_claude_code_in_mixed_slice
// Arrange: 3 records source_domain = "claude-code" event_type = "PostToolUse" (tool calls)
//          + 5 records source_domain = "sre" event_type = "PostToolUse"
// Act: compute_universal(&mixed_records)
// Assert: post_tool_use_count (or equivalent field name) == 3, not 8
```

### T-MET-12: compute_universal with empty slice returns zero-value struct

```rust
// test_compute_universal_empty_slice_returns_zeros
// Act: compute_universal(&[])
// Assert: all 21 fields == 0 (or default values)
// EC-06: zero-observation session
```

---

## Integration Test Expectations (via Schema Migration)

The metrics-extension storage tests in T-MET-06 through T-MET-09 require a v14
schema database. They depend on the `schema-migration` component having been applied.
In practice, `TestDb::new()` creates a fresh database at the current schema version
(v14 after this feature), so these tests are self-contained.

---

## Edge Cases

- `domain_metrics_json` with Unicode key names: must round-trip correctly.
- `domain_metrics_json` with very large HashMap (100+ keys): no size limit at the
  application layer (the only size limit is at ingest, not at storage).
- `f64::NAN` or `f64::INFINITY` values in `domain_metrics`: JSON serialization may
  produce `null` for NaN (serde_json default). Document the behavior; do not panic.
