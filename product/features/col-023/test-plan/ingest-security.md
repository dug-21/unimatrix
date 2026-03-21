# Test Plan: ingest-security

**Component**: `crates/unimatrix-server/src/services/observation.rs` — `parse_observation_rows()`, `json_depth()`
**AC Coverage**: AC-06, AC-07, AC-11
**Risk Coverage**: SEC-01, SEC-02, SEC-03, R-06 (unknown event passthrough)

---

## Unit Test Expectations

### Location: inline `#[cfg(test)]` in `crates/unimatrix-server/src/services/observation.rs`

---

### Payload Size Bounds (AC-06, SEC-01)

### T-SEC-01: Payload exactly 65,536 bytes passes

```rust
// test_payload_size_boundary_exact_limit_passes
// Arrange: ObservationRecord with input = Some(Value) where serialized size == 65,536 bytes
//          (constructed by padding a JSON string to exactly 64 KB)
// Act: apply_security_bounds(&record) or equivalent ingest validation function
// Assert: Ok(()) — event is not rejected
// EC-01: boundary condition must pass, not reject.
```

### T-SEC-02: Payload 65,537 bytes rejects with PayloadTooLarge

```rust
// test_payload_size_one_byte_over_limit_rejects
// Arrange: input serialized to 65,537 bytes (64 KB + 1 byte)
// Act: apply_security_bounds(&record)
// Assert: Err(ObserveError::PayloadTooLarge { size: 65537, ... })
```

### T-SEC-03: Payload size measured on raw bytes before parse (ADR-007)

```rust
// test_payload_size_measured_in_bytes_not_chars
// Arrange: payload with multi-byte UTF-8 characters summing to exactly 65,537 raw bytes
//          but fewer than 65,537 Unicode characters (SEC-01)
// Assert: Err(ObserveError::PayloadTooLarge) — byte count is the measure, not char count
```

### T-SEC-04: Large payload with valid multi-byte UTF-8 at boundary passes

```rust
// test_payload_size_multibyte_utf8_boundary_passes
// Arrange: payload with multi-byte UTF-8 characters summing to exactly 65,536 raw bytes
// Assert: Ok(()) — valid UTF-8 at the byte limit passes
```

---

### JSON Nesting Depth Bounds (AC-06, SEC-02)

### T-SEC-05: Nesting depth exactly 10 levels passes

```rust
// test_nesting_depth_boundary_10_passes
// Arrange: JSON value with exactly 10 levels of nesting
//          e.g., {"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":{"i":{"j":1}}}}}}}}}}
// Act: apply depth check
// Assert: Ok(()) — depth 10 is accepted
// EC-02: boundary condition.
```

### T-SEC-06: Nesting depth 11 levels rejects with NestingTooDeep

```rust
// test_nesting_depth_11_rejects
// Arrange: JSON with 11 levels of nesting
// Assert: Err(ObserveError::PayloadNestingTooDeep { depth: 11, ... })
```

### T-SEC-07: json_depth() does not stack overflow on max permitted depth

```rust
// test_json_depth_no_stack_overflow_at_10_levels
// Assert: json_depth() with 10-level deep JSON completes without panic
// SEC-02: confirm O(depth) stack usage is bounded
```

### T-SEC-08: json_depth() short-circuits at max + 1

```rust
// test_json_depth_short_circuits_above_max
// Arrange: 15-level deep JSON (well beyond limit)
// Assert: returns false (depth exceeded) without traversing all nodes
// Behavioral test: verify the function exits early; this can be tested by
// constructing a value where traversal past level 11 would cause a panic —
// if the function short-circuits, no panic occurs.
```

---

### source_domain Validation (AC-07, NFR-03)

### T-SEC-09: Valid source_domain passes all boundary conditions

```rust
// test_source_domain_valid_boundary_cases
// Cases that must pass:
// - "a" (length 1)
// - "a".repeat(64) (length 64, all valid chars)
// - "sre-monitoring_v2" (mixed valid chars)
// - "claude-code" (existing production value)
// Assert: each returns Ok(()) from validation
```

### T-SEC-10: Invalid source_domain cases reject with InvalidSourceDomain (AC-07)

```rust
// test_source_domain_invalid_cases_all_reject
// Cases that must reject with Err(ObserveError::InvalidSourceDomain):
// - "" (empty)
// - "Claude-Code" (uppercase C)
// - "my domain" (space)
// - "a".repeat(65) (length 65)
// - "sre!" (exclamation mark)
// - "domain@host" (@ symbol)
// Assert: every case returns Err(ObserveError::InvalidSourceDomain { domain: ... })
```

### T-SEC-11: source_domain validation covers both registration and ingest paths

```rust
// test_source_domain_validation_at_registration
// Assert: DomainPackConfig with invalid source_domain fails at DomainPackRegistry::new()
//         (tested also in config-extension T-CFG-07)
// test_source_domain_validation_at_ingest
// Assert: a record arriving at parse_observation_rows() with source_domain failing
//         the regex (if client-declared domains are ever supported) returns
//         InvalidSourceDomain. For W1-5 the hook path always sets "claude-code",
//         so this is a forward-compatibility unit test on the validation function itself.
```

---

### Unknown Event Type Passthrough (AC-11, R-06)

### T-SEC-12: Unregistered event_type produces source_domain = "unknown" and is not dropped

```rust
// test_parse_rows_unknown_event_type_passthrough
// Arrange: a RawObservationRow with hook = "UnknownEventType" (not in any registered pack)
//          DomainPackRegistry with claude-code pack only
// Act: parse_observation_rows(rows, &registry)
// Assert: result contains one ObservationRecord with event_type = "UnknownEventType"
// Assert: record.source_domain == "unknown"
// Assert: record is NOT dropped
// AC-11, R-06
```

### T-SEC-13: source_domain = "claude-code" for hook-path records regardless of event_type

```rust
// test_parse_rows_hook_path_always_claude_code
// Arrange: hook-path ingress (RecordEvent from uds/hook.rs path)
//          event_type = "PreToolUse"
// Act: parse_observation_rows — hook path sets source_domain = "claude-code"
// Assert: record.source_domain == "claude-code"
// FR-03.3: domain assignment from ingress path, not event_type lookup
```

### T-SEC-14: Multiple events in a batch — invalid ones skip, valid ones pass

```rust
// test_parse_rows_partial_batch_invalid_skipped
// Arrange: batch of 3 rows:
//   - row 1: valid (65,536 bytes) — must pass
//   - row 2: oversized (65,537 bytes) — must be skipped with PayloadTooLarge logged
//   - row 3: valid — must pass
// Assert: result contains 2 ObservationRecords (rows 1 and 3)
// Assert: no panic
// FM-02: individual event rejection, session processing continues
```

---

### SEC-03: field_path JSON Pointer Safety

### T-SEC-15: JSON pointer with tilde escapes resolves correctly

```rust
// test_json_pointer_tilde_escape_sequences
// Arrange: payload = {"a~b": {"c/d": 42.0}}
//          field_path = "/a~0b/c~1d" (JSON pointer escaping)
// Act: use serde_json::Value::pointer(field_path) on payload
// Assert: returns Some(&Value::Number(42.0))
// Assert: no side effects (SEC-03: read-only path navigation)
```

---

## Edge Cases

- `input = None`: security bounds are not applied (no payload to check). Must return Ok immediately.
- `input = Some(Value::Null)`: depth = 0, size = 4 bytes ("null"). Both checks must pass trivially.
- `input = Some(Value::Array([...]))`: array nesting counts the same as object nesting.
  A flat array of 1000 elements has depth = 1 and passes the depth check regardless of size.
- Payload that is valid UTF-8 but invalid JSON (after having passed the byte-size check):
  behavior should be documented — either skip with error or store the raw string. Do not panic.
