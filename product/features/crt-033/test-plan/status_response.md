# Component Test Plan: status_response

## Component Scope

`crates/unimatrix-server/src/mcp/response/status.rs` — modifications to:
- `StatusReport` struct: new `pending_cycle_reviews: Vec<String>` field
- `StatusReport::default()`: initialise as `vec![]`
- `StatusReportJson` struct: corresponding `pending_cycle_reviews` field
- `From<&StatusReport>` impl: map the new field
- Summary formatter: render list when non-empty under "Pending cycle reviews" label
- JSON formatter: include field as array

**AC coverage**: AC-09 (partial — formatter rendering), AC-10 (partial — empty case),
FR-09 (partial — struct), FR-11 (formatters).
**Risk coverage**: I-04 (StatusReport struct extension — Default + JSON + summary).

---

## Unit Tests (in `#[cfg(test)]` inside `status.rs` or adjacent `status_test.rs`)

### SR-U-01: StatusReport::default() has empty pending_cycle_reviews (I-04)

```rust
#[test]
fn test_status_report_default_has_empty_pending_cycle_reviews() {
    let report = StatusReport::default();
    assert!(report.pending_cycle_reviews.is_empty(),
        "default StatusReport must have empty pending_cycle_reviews");
}
```

### SR-U-02: StatusReportJson contains pending_cycle_reviews field (I-04)

```rust
#[test]
fn test_status_report_json_contains_pending_cycle_reviews() {
    let report = StatusReport {
        pending_cycle_reviews: vec!["crt-033".to_string(), "col-034".to_string()],
        ..StatusReport::default()
    };
    let json_report: StatusReportJson = StatusReportJson::from(&report);
    assert_eq!(json_report.pending_cycle_reviews.len(), 2);
    assert!(json_report.pending_cycle_reviews.contains(&"crt-033".to_string()));
    assert!(json_report.pending_cycle_reviews.contains(&"col-034".to_string()));
}
```

### SR-U-03: From<&StatusReport> maps pending_cycle_reviews correctly (I-04)

```rust
#[test]
fn test_status_report_json_from_maps_pending_cycle_reviews() {
    let report = StatusReport {
        pending_cycle_reviews: vec!["feat-A".to_string()],
        ..StatusReport::default()
    };
    let json: StatusReportJson = StatusReportJson::from(&report);
    assert_eq!(json.pending_cycle_reviews, vec!["feat-A".to_string()]);
}
```

### SR-U-04: From<&StatusReport> empty vec maps to empty vec (FR-10, FR-11)

```rust
#[test]
fn test_status_report_json_from_empty_pending_reviews() {
    let report = StatusReport::default();  // empty vec
    let json: StatusReportJson = StatusReportJson::from(&report);
    assert!(json.pending_cycle_reviews.is_empty());
}
```

### SR-U-05: Summary formatter includes "Pending cycle reviews" label when non-empty (FR-11)

```rust
#[test]
fn test_summary_formatter_renders_pending_cycle_reviews_when_non_empty() {
    let report = StatusReport {
        pending_cycle_reviews: vec!["col-022".to_string(), "crt-031".to_string()],
        ..StatusReport::default()
    };
    let summary = format_status_summary(&report);  // or equivalent formatter call
    assert!(summary.contains("Pending cycle reviews"),
        "summary must contain 'Pending cycle reviews' label when list is non-empty");
    assert!(summary.contains("col-022"));
    assert!(summary.contains("crt-031"));
}
```

### SR-U-06: Summary formatter produces no "Pending cycle reviews" section when empty (FR-11)

```rust
#[test]
fn test_summary_formatter_omits_pending_section_when_empty() {
    let report = StatusReport::default();  // empty vec
    let summary = format_status_summary(&report);
    assert!(!summary.contains("Pending cycle reviews"),
        "summary must not render 'Pending cycle reviews' when list is empty");
}
```

### SR-U-07: JSON formatter includes pending_cycle_reviews as array (FR-11, AC-09)

```rust
#[test]
fn test_json_formatter_includes_pending_cycle_reviews_array() {
    let report = StatusReport {
        pending_cycle_reviews: vec!["nxs-005".to_string()],
        ..StatusReport::default()
    };
    let json_str = format_status_json(&report);  // or equivalent JSON formatter call
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let arr = parsed["pending_cycle_reviews"].as_array()
        .expect("pending_cycle_reviews must be an array in JSON output");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0].as_str().unwrap(), "nxs-005");
}
```

### SR-U-08: JSON formatter produces empty array when pending_cycle_reviews is empty (FR-11, AC-10)

```rust
#[test]
fn test_json_formatter_pending_cycle_reviews_empty_is_array() {
    let report = StatusReport::default();
    let json_str = format_status_json(&report);
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    let arr = parsed["pending_cycle_reviews"].as_array()
        .expect("pending_cycle_reviews must exist as empty array, not absent");
    assert!(arr.is_empty());
}
```

---

## Integration Tests

These are covered at the infra-001 level (via `tools` suite `context_status`
call) and at the `status_service` level (TH-I-01 in `status_service.md`). The
`status_response` component itself is thin and does not require additional store
integration tests.

### SR-I-01: StatusReport with non-empty pending_cycle_reviews round-trips through JSON (I-04)

```rust
#[test]
fn test_status_report_json_round_trip_preserves_pending_cycle_reviews() {
    let report = StatusReport {
        pending_cycle_reviews: vec!["col-022".to_string()],
        ..StatusReport::default()
    };
    let json: StatusReportJson = StatusReportJson::from(&report);
    // Verify the JSON value round-trips via serde
    let serialized = serde_json::to_string(&json).unwrap();
    let recovered: StatusReportJson = serde_json::from_str(&serialized).unwrap();
    assert_eq!(recovered.pending_cycle_reviews, report.pending_cycle_reviews);
}
```

---

## Assertions and Expected Behaviors

| Behavior | Assertion |
|----------|-----------|
| `StatusReport::default().pending_cycle_reviews` | `== vec![]` |
| `StatusReportJson::from(&report)` with non-empty list | Mapped vec has same elements |
| Summary formatter, non-empty list | Output string contains `"Pending cycle reviews"` label |
| Summary formatter, empty list | Output string does NOT contain `"Pending cycle reviews"` |
| JSON formatter output, non-empty list | `"pending_cycle_reviews"` key present; value is JSON array |
| JSON formatter output, empty list | `"pending_cycle_reviews"` key present; value is `[]` (NOT absent) |

---

## Edge Cases

| Edge Case | Test | Expected |
|-----------|------|---------|
| `pending_cycle_reviews` list has one entry | SR-U-02 with single item | Formatted correctly |
| `pending_cycle_reviews` list has 50 entries | Not required separately — summary formatter must handle long lists without truncation | All cycle IDs rendered |
| Cycle ID contains a slash or hyphen | Not tested separately — these are normal cycle IDs | Rendered as-is |

---

## Open Question for Implementation

The exact names of the formatter functions (`format_status_summary`,
`format_status_json`, `StatusReportJson`) depend on the existing code structure in
`status.rs`. Stage 3c tester must read the actual function signatures at execution
time and adjust test assertions to match. The behavioral contracts described above
are the authoritative test targets; function names are illustrative.
