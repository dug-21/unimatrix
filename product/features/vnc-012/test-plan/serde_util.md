# Test Plan: mcp/serde_util.rs

## Component Summary

New file `crates/unimatrix-server/src/mcp/serde_util.rs` containing three `pub(crate)`
deserializer functions. All tests live in the `#[cfg(test)]` block at the bottom of this
file. Tests exercise each Visitor method directly by calling `serde_json::from_str` on a
minimal wrapper struct or via direct deserializer invocation.

---

## Unit Test Expectations

### Test Module: `serde_util_tests` in `#[cfg(test)]`

Tests should define thin wrapper structs for direct testing:

```rust
#[derive(Deserialize)] struct Wrap { v: i64 }
#[derive(Deserialize)] struct WrapOpt { v: Option<i64> }
#[derive(Deserialize, Default)] struct WrapUsize { #[serde(default)] v: Option<usize> }
```

#### `deserialize_i64_or_string`

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_deserialize_i64_integer_input` | `{"v": 42}` | `Ok(Wrap { v: 42 })` | AC-07, AC-12 |
| `test_deserialize_i64_string_input` | `{"v": "3770"}` | `Ok(Wrap { v: 3770 })` | AC-01, AC-12 |
| `test_deserialize_i64_negative_string` | `{"v": "-5"}` | `Ok(Wrap { v: -5 })` | edge case (negative i64 valid) |
| `test_deserialize_i64_zero_string` | `{"v": "0"}` | `Ok(Wrap { v: 0 })` | edge case |
| `test_deserialize_i64_max_string` | `{"v": "9223372036854775807"}` | `Ok(Wrap { v: i64::MAX })` | boundary |
| `test_deserialize_i64_min_string` | `{"v": "-9223372036854775808"}` | `Ok(Wrap { v: i64::MIN })` | boundary |
| `test_deserialize_i64_overflow_string` | `{"v": "9999999999999999999999"}` | `Err(_)` | R-04 boundary |
| `test_deserialize_i64_nonnumeric_string` | `{"v": "abc"}` | `Err(_)` | AC-08, R-08 |
| `test_deserialize_i64_empty_string` | `{"v": ""}` | `Err(_)` | FR-11 |
| `test_deserialize_i64_float_string` | `{"v": "3.5"}` | `Err(_)` | AC-09-FLOAT, FR-12 |
| `test_deserialize_i64_whitespace_string` | `{"v": " 42 "}` | `Err(_)` | FR-11, edge case |
| `test_deserialize_i64_float_number` | `{"v": 3.0}` | `Err(_)` | AC-09-FLOAT-NUMBER, FR-13 |
| `test_deserialize_i64_bool_input` | `{"v": true}` | `Err(_)` | C-05 |
| `test_deserialize_i64_array_input` | `{"v": [1]}` | `Err(_)` | C-05 |

Assertion for error cases: `assert!(result.is_err(), "expected error for input: ...")`.
Do not assert on the exact error message text — assert `is_err()` only.

#### `deserialize_opt_i64_or_string`

All tests use a struct with `#[serde(default)]` paired on the optional field.

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_deserialize_opt_i64_integer_input` | `{"v": 42}` | `Ok(WrapOpt { v: Some(42) })` | AC-07 |
| `test_deserialize_opt_i64_string_input` | `{"v": "5"}` | `Ok(WrapOpt { v: Some(5) })` | AC-03, AC-04, AC-12 |
| `test_deserialize_opt_i64_null_input` | `{"v": null}` | `Ok(WrapOpt { v: None })` | R-03, AC-03-NULL-ID |
| `test_deserialize_opt_i64_absent_field` | `{}` | `Ok(WrapOpt { v: None })` | R-01, AC-03-ABSENT-ID |
| `test_deserialize_opt_i64_nonnumeric_string` | `{"v": "abc"}` | `Err(_)` | AC-08-OPT, R-08 |
| `test_deserialize_opt_i64_float_string` | `{"v": "3.5"}` | `Err(_)` | AC-09-FLOAT |
| `test_deserialize_opt_i64_float_number` | `{"v": 3.0}` | `Err(_)` | AC-09-FLOAT-NUMBER, FR-13 |
| `test_deserialize_opt_i64_negative_string` | `{"v": "-5"}` | `Ok(WrapOpt { v: Some(-5) })` | edge case |

**Critical**: The absent-field test (`test_deserialize_opt_i64_absent_field`) is only
valid when `#[serde(default)]` is present on the wrapper struct field. If omitted, the
test itself is invalid (will return `Err` regardless of the helper implementation).

#### `deserialize_opt_usize_or_string`

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_deserialize_opt_usize_integer_input` | `{"v": 5}` | `Ok(WrapUsize { v: Some(5) })` | AC-07 |
| `test_deserialize_opt_usize_string_input` | `{"v": "5"}` | `Ok(WrapUsize { v: Some(5) })` | AC-06, AC-12 |
| `test_deserialize_opt_usize_zero_string` | `{"v": "0"}` | `Ok(WrapUsize { v: Some(0) })` | AC-06-ZERO |
| `test_deserialize_opt_usize_null_input` | `{"v": null}` | `Ok(WrapUsize { v: None })` | R-03, AC-06-NULL |
| `test_deserialize_opt_usize_absent_field` | `{}` | `Ok(WrapUsize { v: None })` | R-01, AC-06-ABSENT |
| `test_deserialize_opt_usize_negative_string` | `{"v": "-1"}` | `Err(_)` | AC-09, R-04 |
| `test_deserialize_opt_usize_u64_overflow_string` | `{"v": "99999999999999999999"}` | `Err(_)` | R-04 |
| `test_deserialize_opt_usize_nonnumeric_string` | `{"v": "abc"}` | `Err(_)` | AC-08-OPT |
| `test_deserialize_opt_usize_float_number` | `{"v": 3.0}` | `Err(_)` | AC-09-FLOAT-NUMBER, FR-13 |
| `test_deserialize_opt_usize_float_string` | `{"v": "3.5"}` | `Err(_)` | AC-09-FLOAT |

---

## Specific Assertions

- All `Ok` cases: `assert_eq!(result.unwrap().v, expected_value)`
- All `Err` cases: `assert!(result.is_err())` — do not check error message text
- Float Number rejection: assert `is_err()` AND confirm no truncation by asserting the
  value is NOT `Ok(WrapOpt { v: Some(3) })` when input is `3.0` (prevents silent truncation)

---

## Edge Cases from Risk Strategy

1. **Whitespace-padded string** (`" 42 "`): `str::parse::<i64>()` rejects this. Test
   `test_deserialize_i64_whitespace_string` must return `Err` (FR-11).

2. **`i64::MIN` as string** (`"-9223372036854775808"`): Must succeed for `deserialize_i64_or_string`.
   Must fail for `deserialize_opt_usize_or_string` (negative rejected at `u64` parse stage).

3. **u64 overflow for usize**: String `"99999999999999999999"` exceeds `u64::MAX`. Must fail
   at `str::parse::<u64>()`, never reach `usize::try_from`.

4. **`usize::try_from` path**: On 64-bit targets, `u64::MAX` would overflow `usize` on 32-bit
   targets. The `u64` overflow string test covers the `str::parse` rejection; the
   `usize::try_from` rejection is only reachable on 32-bit targets. Document this constraint
   in a code comment; do not write a target-conditional test.

---

## Integration Test Expectations

`serde_util.rs` is a pure function module with no I/O or storage dependencies. No
integration tests beyond unit tests are needed at this component boundary. The integration
test coverage for the helpers is provided transitively by the tools.rs struct tests, AC-13,
and IT-01/IT-02.
