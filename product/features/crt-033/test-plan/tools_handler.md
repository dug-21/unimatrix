# Component Test Plan: tools_handler

## Component Scope

`crates/unimatrix-server/src/mcp/tools.rs` — modifications to the
`context_cycle_review` handler:
- `RetrospectiveParams` gains `force: Option<bool>` (fifth field)
- Step 2.5: memoization check (`check_stored_review` helper)
- Step 8a: memoization store (`build_cycle_review_record` helper)
- All force=true paths
- evidence_limit applied only at render time (step 9)

**AC coverage**: AC-03, AC-04, AC-04b, AC-05, AC-06, AC-07, AC-08, AC-12, AC-14,
AC-15.
**Risk coverage**: R-02 (partial), R-03, R-04, R-05, R-08, R-09.

---

## Unit Tests (in `#[cfg(test)]` inside `tools.rs` or adjacent `tools_test.rs`)

### TH-U-01: force field absent deserialized as None (AC-12)

```rust
#[test]
fn test_retrospective_params_force_absent_is_none() {
    let params: RetrospectiveParams =
        serde_json::from_str(r#"{"feature_cycle": "test-001"}"#).unwrap();
    assert!(params.force.is_none());
}
```

### TH-U-02: force field present deserialized correctly (AC-12)

```rust
#[test]
fn test_retrospective_params_force_true() {
    let params: RetrospectiveParams =
        serde_json::from_str(r#"{"feature_cycle": "test-001", "force": true}"#).unwrap();
    assert_eq!(params.force, Some(true));
}

#[test]
fn test_retrospective_params_force_false() {
    let params: RetrospectiveParams =
        serde_json::from_str(r#"{"feature_cycle": "test-001", "force": false}"#).unwrap();
    assert_eq!(params.force, Some(false));
}
```

### TH-U-03: check_stored_review — matching schema_version produces no advisory (R-08)

```rust
#[test]
fn test_check_stored_review_matching_version_no_advisory() {
    let record = CycleReviewRecord {
        feature_cycle: "x".to_string(),
        schema_version: SUMMARY_SCHEMA_VERSION,
        computed_at: 1_700_000_000,
        raw_signals_available: 1,
        summary_json: /* valid serialized RetrospectiveReport stub */,
    };
    let (_, advisory) = check_stored_review(&record, SUMMARY_SCHEMA_VERSION);
    assert!(advisory.is_none());
}
```

### TH-U-04: check_stored_review — mismatched schema_version produces advisory (AC-04b, R-08)

```rust
#[test]
fn test_check_stored_review_mismatched_version_produces_advisory() {
    let record = CycleReviewRecord {
        schema_version: 0,   // old version
        ..
    };
    let (_, advisory) = check_stored_review(&record, SUMMARY_SCHEMA_VERSION);
    let advisory_text = advisory.expect("advisory must be Some");
    assert!(advisory_text.contains("use force=true to recompute"),
        "advisory must contain 'use force=true to recompute', got: {}", advisory_text);
    assert!(advisory_text.contains("0"),   "must include stored version");
    assert!(advisory_text.contains(&SUMMARY_SCHEMA_VERSION.to_string()), "must include current version");
}
```

### TH-U-05: check_stored_review — future schema_version (higher than current) produces advisory (R-08)

```rust
#[test]
fn test_check_stored_review_future_version_produces_advisory() {
    let record = CycleReviewRecord { schema_version: 999, .. };
    let (_, advisory) = check_stored_review(&record, SUMMARY_SCHEMA_VERSION);
    assert!(advisory.is_some(), "future schema_version must also produce advisory");
}
```

### TH-U-06: check_stored_review — corrupted JSON falls through (R-06-3, ADR-003)

```rust
#[test]
fn test_check_stored_review_corrupted_json_does_not_panic() {
    let record = CycleReviewRecord {
        schema_version: SUMMARY_SCHEMA_VERSION,
        summary_json: "not valid json {{{{".to_string(),
        ..
    };
    // Must not panic. The function returns an error indicator / None deserialization result.
    // The handler is responsible for triggering recompute; this test verifies no panic.
    let result = std::panic::catch_unwind(|| check_stored_review(&record, SUMMARY_SCHEMA_VERSION));
    assert!(result.is_ok(), "check_stored_review must not panic on corrupted JSON");
}
```

### TH-U-07: build_cycle_review_record — serializes report correctly (AC-03)

```rust
#[test]
fn test_build_cycle_review_record_sets_correct_fields() {
    let report = minimal_test_retrospective_report("feat-x");
    let record = build_cycle_review_record("feat-x", &report).expect("build must succeed");
    assert_eq!(record.feature_cycle, "feat-x");
    assert_eq!(record.schema_version, SUMMARY_SCHEMA_VERSION);
    assert_eq!(record.raw_signals_available, 1);
    // summary_json must be deserializable back to RetrospectiveReport
    serde_json::from_str::<RetrospectiveReport>(&record.summary_json)
        .expect("summary_json must be valid JSON");
}
```

---

## Integration Tests (store-backed handler tests)

These tests require a real SqlxStore and a seeded database. They exercise the handler
end-to-end but without going through MCP JSON-RPC (they call handler internals
directly or use the store to verify state). MCP-level integration is covered by
infra-001 suites.

### TH-I-01: First call writes row with raw_signals_available=1 (AC-03, AC-11)

```
Arrange: open fresh SqlxStore; seed observations for "col-test" cycle.
Act: call context_cycle_review handler with feature_cycle="col-test", force=None.
Assert:
  SELECT raw_signals_available FROM cycle_review_index WHERE feature_cycle='col-test'
  returns 1.
  SELECT schema_version FROM cycle_review_index WHERE feature_cycle='col-test'
  returns SUMMARY_SCHEMA_VERSION (= 1).  [AC-11]
```

### TH-I-02: Second call returns stored record without re-running computation (AC-04, AC-14)

```
Arrange: seed observations; call handler once (first call writes the row).
  Record the initial computed_at value from cycle_review_index.
Act: call handler again with force=None on the same cycle.
Assert:
  computed_at in cycle_review_index is unchanged (row not overwritten).
  Returned feature_cycle matches.
  [Because no new observations were added, and the stored record was returned,
   observation-related tables were not queried — verified by observing that
   computed_at is unchanged as the surrogate for "no recompute occurred".]
```

### TH-I-03: Schema version mismatch triggers advisory, does not recompute (AC-04b, R-08)

```
Arrange: INSERT INTO cycle_review_index directly with schema_version=0
  and valid summary_json for "adv-test" cycle. No live observations.
Act: call handler with feature_cycle="adv-test", force=None.
Assert:
  response contains "use force=true to recompute"
  response contains "0" (stored version) and SUMMARY_SCHEMA_VERSION string (current version)
  computed_at in cycle_review_index is unchanged (no recompute)
```

### TH-I-04: force=true with live signals overwrites stored row (AC-05)

```
Arrange: seed observations for "force-test"; call handler once to write initial row.
  Record initial computed_at.
  Add a brief delay (or bump a seeded signal) to ensure T2 > T1.
Act: call handler with feature_cycle="force-test", force=Some(true).
Assert:
  computed_at in cycle_review_index > initial computed_at (row overwritten).
  Returned report is freshly computed.
```

### TH-I-05: force=true + purged signals + stored record returns stored record with note (AC-06, AC-15, R-04)

```
Arrange: INSERT INTO cycle_review_index directly for "purged-test" (no live observations).
Act: call handler with feature_cycle="purged-test", force=Some(true).
Assert:
  response is Ok (not ERROR_NO_OBSERVATION_DATA)
  response text contains "Raw signals have been purged"
  raw_signals_available in response is reported as false (0)
```

### TH-I-06: force=true + purged signals + no stored record returns ERROR_NO_OBSERVATION_DATA (AC-07, R-04)

```
Arrange: empty observations, no cycle_review_index row for "ghost-test".
Act: call handler with feature_cycle="ghost-test", force=Some(true).
Assert: response is ERROR_NO_OBSERVATION_DATA.
```

### TH-I-07: evidence_limit applied at render time only — raw JSON preserves full evidence (AC-08, R-03)

```
Arrange: seed a cycle whose full RetrospectiveReport has 10 hotspots each with 5 evidence items.
  Call handler with evidence_limit=2 (stores and returns).
Assert A (storage):
  SELECT summary_json FROM cycle_review_index WHERE feature_cycle=...
  Deserialize the raw JSON; assert each hotspot has 5 evidence items (NOT 2).
Assert B (response):
  The MCP response hotspots each have 2 evidence items (truncated at render).
Act 2: call handler again (memoization hit) with no evidence_limit.
Assert C (response):
  The returned hotspots have 5 evidence items (full evidence from stored JSON).
```

### TH-I-08: force=true path skips step 2.5 with live signals (I-03 integration risk)

```
Arrange: store a cycle_review_index row for "skip-test" with computed_at=T1.
  Seed fresh observations for the same cycle.
Act: call handler with force=Some(true).
Assert:
  computed_at in cycle_review_index > T1 (a fresh compute + INSERT OR REPLACE occurred).
  This proves step 2.5 was skipped — if it were not skipped, the stored row would
  have been returned at step 2.5 and step 8a would not have fired.
```

### TH-I-09: get_cycle_review read failure falls through to full computation (failure mode)

```
Arrange: configure store to fail on get_cycle_review (inject error or use a test double).
  Seed observations for "fallthrough-test".
Act: call handler with force=None.
Assert:
  response is Ok (not an error)
  a valid RetrospectiveReport is returned (full computation ran despite step 2.5 failure)
```

Note: this test requires a test double or sqlx connection failure injection. If that
infrastructure does not exist, this scenario is deferred to a comment in the test file
and documented as a gap.

---

## Static / Grep Checks

### TH-G-01: No spawn_blocking wrapping store_cycle_review (R-09, ADR-001)

```bash
grep -n 'spawn_blocking' crates/unimatrix-server/src/mcp/tools.rs
```
Assert: zero matches within the memoization and store-step functions.

If any `spawn_blocking` calls exist in `tools.rs`, confirm none of them wrap
`store_cycle_review` or `get_cycle_review`.

### TH-G-02: evidence_limit not applied before serde_json::to_string in step 8a (R-03)

Code review gate: In the `build_cycle_review_record` function and the step 8a call
site, assert that `evidence_limit` is not passed in and no truncation logic runs before
the JSON serialization call.

---

## Concurrent Test

### TH-I-10: Concurrent first-calls for different cycles both complete (R-02)

```
Arrange: open shared SqlxStore; seed observations for "concurrent-A" and "concurrent-B".
Act: tokio::join! two handler calls:
  - handle(feature_cycle="concurrent-A", force=None)
  - handle(feature_cycle="concurrent-B", force=None)
Assert:
  Both calls return Ok.
  cycle_review_index has rows for both "concurrent-A" and "concurrent-B".
  No deadlock or timeout (test has 5-second timeout).
```

---

## Edge Cases

| Edge Case | Test | Expected |
|-----------|------|---------|
| `force=None` and `force=Some(false)` both trigger memoization check | TH-I-02 with each value | Both hit the stored record |
| `feature_cycle` exceeds max length validation | Covered by existing `validate_retrospective_params` — `force` must not bypass it | ERROR returned, not panic |
| Concurrent first-call for the same cycle (race — last writer wins) | CRS-I-10 in cycle_review_index | One row, no corruption |
| `store_cycle_review` fails at step 8a (DB write error) | TH-I-09 variant: fail at write not read | Handler returns tool error, not server crash |
