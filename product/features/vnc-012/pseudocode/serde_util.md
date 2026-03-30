# Component: mcp/serde_util.rs (new)

## Purpose

Provide three `pub(crate)` serde deserializer functions that accept either a JSON Number
(integer) or a JSON String containing a base-10 integer literal, and return the typed
Rust value. Contain their own unit tests covering all acceptance, rejection, null, and
absent paths.

This module is private to the `mcp` namespace. It must not be promoted to crate-level.

**File**: `crates/unimatrix-server/src/mcp/serde_util.rs`

---

## Function Signatures

```rust
pub(crate) fn deserialize_i64_or_string<'de, D>(d: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>

pub(crate) fn deserialize_opt_i64_or_string<'de, D>(d: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>

pub(crate) fn deserialize_opt_usize_or_string<'de, D>(d: D) -> Result<Option<usize>, D::Error>
where
    D: serde::Deserializer<'de>
```

---

## Visitor Structures (private to module)

```
struct I64OrStringVisitor;
struct OptI64OrStringVisitor;
struct OptUsizeOrStringVisitor;
```

Each is a zero-size marker struct. Each implements `serde::de::Visitor` with the
appropriate `Value` associated type.

---

## Function Bodies (pseudocode)

### deserialize_i64_or_string

```
FUNCTION deserialize_i64_or_string<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error>

  DEFINE struct I64OrStringVisitor

  IMPLEMENT serde::de::Visitor<'de> for I64OrStringVisitor:
    type Value = i64

    FUNCTION expecting(&self, formatter) -> fmt::Result:
      -- Used in serde error messages
      formatter.write_str("an integer or a string containing a base-10 integer")

    FUNCTION visit_i64(self, v: i64) -> Result<i64, E>:
      -- JSON Number (signed integer) — pass through unchanged
      RETURN Ok(v)

    FUNCTION visit_u64(self, v: u64) -> Result<i64, E>:
      -- JSON Number (unsigned integer, e.g. large positive values serde routes as u64)
      -- Attempt lossless conversion to i64
      i64::try_from(v).map_err(|_| E::invalid_value(Unexpected::Unsigned(v), &self))

    FUNCTION visit_str(self, v: &str) -> Result<i64, E>:
      -- JSON String containing base-10 integer literal
      -- str::parse::<i64>() rejects floats, hex, whitespace-padded, non-numeric text
      v.parse::<i64>().map_err(|_| E::custom(format!("invalid integer string: {v:?}")))

    FUNCTION visit_string(self, v: String) -> Result<i64, E>:
      -- Owned string variant — delegate to visit_str (avoids code duplication)
      self.visit_str(&v)

    FUNCTION visit_f64(self, v: f64) -> Result<i64, E>:
      -- Float JSON Numbers (e.g., 3.0 as Number type) MUST be rejected (FR-13)
      -- Silent truncation is forbidden — schema advertises type: integer
      RETURN Err(E::invalid_type(Unexpected::Float(v), &self))

    FUNCTION visit_f32(self, v: f32) -> Result<i64, E>:
      -- Same rejection for f32 path
      RETURN Err(E::invalid_type(Unexpected::Float(v as f64), &self))

    -- All other visit_* methods fall through to the default serde error
    -- (serde::de::Visitor provides default impls returning invalid_type errors)

  CALL d.deserialize_any(I64OrStringVisitor)
  -- deserialize_any: serde calls the visitor method matching the actual JSON type
  -- This allows both Number and String JSON values to reach our visitor
```

### deserialize_opt_i64_or_string

```
FUNCTION deserialize_opt_i64_or_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error>

  DEFINE struct OptI64OrStringVisitor

  IMPLEMENT serde::de::Visitor<'de> for OptI64OrStringVisitor:
    type Value = Option<i64>

    FUNCTION expecting(&self, formatter) -> fmt::Result:
      formatter.write_str("an integer, a string containing a base-10 integer, or null")

    FUNCTION visit_none(self) -> Result<Option<i64>, E>:
      -- Handles JSON null when using deserialize_option
      -- Also handles absent fields when serde invokes this via deserialize_option
      RETURN Ok(None)

    FUNCTION visit_unit(self) -> Result<Option<i64>, E>:
      -- Some deserializers call visit_unit for JSON null (defensive coverage)
      RETURN Ok(None)

    FUNCTION visit_some<D2: Deserializer<'de>>(self, d: D2) -> Result<Option<i64>, E>:
      -- Wraps the inner deserialization for present, non-null values
      -- Delegates to the required (non-optional) deserializer
      deserialize_i64_or_string(d).map(Some)

    -- Note: visit_i64, visit_u64, visit_str, visit_string, visit_f64, visit_f32
    -- are NOT needed here because visit_some handles all non-null cases by
    -- delegating to deserialize_i64_or_string which implements those.

  CALL d.deserialize_option(OptI64OrStringVisitor)
  -- deserialize_option: serde calls visit_none for JSON null, visit_some for all others
  -- This is the correct idiom for Option<T> deserializers (NOT deserialize_any)
  --
  -- ABSENT FIELD NOTE: When a field has #[serde(default)] and is missing from the JSON
  -- object, serde does NOT call this function at all. It uses Default::default() = None.
  -- The absent path is therefore handled entirely by #[serde(default)] on the struct
  -- field, not by any code here. This is documented here to prevent confusion.
```

### deserialize_opt_usize_or_string

```
FUNCTION deserialize_opt_usize_or_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<usize>, D::Error>

  DEFINE struct OptUsizeOrStringVisitor

  IMPLEMENT serde::de::Visitor<'de> for OptUsizeOrStringVisitor:
    type Value = Option<usize>

    FUNCTION expecting(&self, formatter) -> fmt::Result:
      formatter.write_str("a non-negative integer, a string containing a non-negative integer, or null")

    FUNCTION visit_none(self) -> Result<Option<usize>, E>:
      RETURN Ok(None)

    FUNCTION visit_unit(self) -> Result<Option<usize>, E>:
      RETURN Ok(None)

    FUNCTION visit_some<D2: Deserializer<'de>>(self, d: D2) -> Result<Option<usize>, E>:
      -- Delegate to inner usize deserialization and wrap result
      inner_deserialize_usize(d).map(Some)

  -- Inner helper (private fn, not pub(crate)):
  FUNCTION inner_deserialize_usize<'de, D: Deserializer<'de>>(d: D) -> Result<usize, D::Error>:

    DEFINE struct UsizeOrStringVisitor

    IMPLEMENT serde::de::Visitor<'de> for UsizeOrStringVisitor:
      type Value = usize

      FUNCTION expecting(&self, formatter) -> fmt::Result:
        formatter.write_str("a non-negative integer or a string containing a non-negative integer")

      FUNCTION visit_u64(self, v: u64) -> Result<usize, E>:
        -- JSON Number (unsigned) — convert u64 -> usize safely
        -- MUST use try_from, NOT 'as usize' (C-06: silent truncation on 32-bit targets)
        usize::try_from(v).map_err(|_| E::invalid_value(Unexpected::Unsigned(v), &self))

      FUNCTION visit_i64(self, v: i64) -> Result<usize, E>:
        -- JSON Number (signed). Reject negatives before usize conversion.
        IF v < 0 THEN
          RETURN Err(E::invalid_value(Unexpected::Signed(v), &self))
        END IF
        -- v is non-negative; convert via u64 to usize safely
        usize::try_from(v as u64).map_err(|_| E::invalid_value(Unexpected::Signed(v), &self))

      FUNCTION visit_str(self, v: &str) -> Result<usize, E>:
        -- Parse via u64 first — this rejects negative strings at parse time (C-06)
        -- str::parse::<u64>() fails on "-1", guaranteeing no negative values reach usize
        LET val_u64 = v.parse::<u64>().map_err(|_| E::custom(format!("invalid non-negative integer string: {v:?}")))?
        -- Now convert u64 -> usize safely (rejects overflow on 32-bit targets)
        usize::try_from(val_u64).map_err(|_| E::custom(format!("integer too large for usize: {v:?}")))

      FUNCTION visit_string(self, v: String) -> Result<usize, E>:
        self.visit_str(&v)

      FUNCTION visit_f64(self, v: f64) -> Result<usize, E>:
        -- Float JSON Numbers rejected (FR-13)
        RETURN Err(E::invalid_type(Unexpected::Float(v), &self))

      FUNCTION visit_f32(self, v: f32) -> Result<usize, E>:
        RETURN Err(E::invalid_type(Unexpected::Float(v as f64), &self))

    CALL d.deserialize_any(UsizeOrStringVisitor)

  CALL d.deserialize_option(OptUsizeOrStringVisitor)
```

---

## Initialization Sequence

No initialization required. All three functions are stateless and allocate nothing for
the happy path (integer JSON input). String inputs allocate only during `str::parse`
on the borrowed `&str`.

---

## Error Handling

| Input | Function | Error Type | Notes |
|-------|----------|-----------|-------|
| Non-numeric string `"abc"` | all | `de::Error::custom(...)` | Not panic, not Ok(0) |
| Float string `"3.5"` | all | `de::Error::custom(...)` | str::parse::<i64>() rejects |
| Float JSON Number `3.0` | all | `de::Error::invalid_type(Float, &self)` | visit_f64 fires |
| Negative string `"-1"` | opt_usize | `de::Error::custom(...)` | u64 parse fails |
| u64 overflow string `"9999...9"` | opt_usize | `de::Error::custom(...)` | u64 parse fails |
| usize overflow on 32-bit | opt_usize | `de::Error::custom(...)` | try_from returns Err |
| i64 overflow string | i64 variants | `de::Error::custom(...)` | str::parse::<i64>() fails |
| JSON null | opt variants | `Ok(None)` | visit_none fires |
| Absent key | opt variants | `Ok(None)` | serde uses Default, visitor not called |
| Boolean `true` | all | default serde error | visit_bool not implemented |
| Array / Object | all | default serde error | not implemented |

---

## Key Test Scenarios (serde_util.rs test block)

These test the helpers directly via `serde_json::from_value` or a custom de::Deserializer.
Most tests can use `serde_json::from_str` on a wrapper struct for convenience.

### Helper wrapper structs for unit testing (private, test-only)

```
#[derive(Deserialize)] struct TestI64 {
    #[serde(deserialize_with = "deserialize_i64_or_string")]
    val: i64
}

#[derive(Deserialize)] struct TestOptI64 {
    #[serde(default, deserialize_with = "deserialize_opt_i64_or_string")]
    val: Option<i64>
}

#[derive(Deserialize)] struct TestOptUsize {
    #[serde(default, deserialize_with = "deserialize_opt_usize_or_string")]
    val: Option<usize>
}
```

### Scenarios

**deserialize_i64_or_string:**
- `{"val": 42}` -> `val == 42i64`  (integer input, regression guard)
- `{"val": "42"}` -> `val == 42i64`  (string input, new acceptance)
- `{"val": "0"}` -> `val == 0i64`
- `{"val": "-5"}` -> `val == -5i64`  (negative strings valid for i64)
- `{"val": "9223372036854775807"}` -> `val == i64::MAX`  (boundary)
- `{"val": "-9223372036854775808"}` -> `val == i64::MIN`  (boundary)
- `{"val": "abc"}` -> Err (non-numeric string)
- `{"val": ""}` -> Err (empty string)
- `{"val": "3.5"}` -> Err (float string)
- `{"val": " 42 "}` -> Err (whitespace-padded, str::parse rejects)
- `{"val": 3.0}` -> Err (float JSON Number, visit_f64)
- `{"val": "9999999999999999999999"}` -> Err (i64 overflow)

**deserialize_opt_i64_or_string:**
- `{"val": 42}` -> `val == Some(42i64)`
- `{"val": "42"}` -> `val == Some(42i64)`
- `{"val": null}` -> `val == None`  (JSON null -- key present)
- `{}` (absent key) -> `val == None`  (absent -- via #[serde(default)])
- `{"val": "abc"}` -> Err
- `{"val": 3.0}` -> Err (float JSON Number)

**deserialize_opt_usize_or_string:**
- `{"val": 5}` -> `val == Some(5usize)`
- `{"val": "5"}` -> `val == Some(5usize)`
- `{"val": "0"}` -> `val == Some(0usize)`  (boundary: zero valid)
- `{"val": null}` -> `val == None`
- `{}` -> `val == None`
- `{"val": "-1"}` -> Err  (negative rejected at u64 parse stage)
- `{"val": "99999999999999999999"}` -> Err  (u64 overflow at parse)
- `{"val": 3.0}` -> Err  (float JSON Number)
- `{"val": "-5"}` -> Err  (negative string)

Total minimum test count for serde_util.rs: 25+ tests covering all paths above.
