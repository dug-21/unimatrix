//! Serde deserializer helpers for MCP tool parameter structs.
//!
//! These three `pub(crate)` functions are referenced as string literals in nine
//! `#[serde(deserialize_with = "serde_util::deserialize_...")]` attributes in
//! `mcp/tools.rs`. Any rename of this module or its functions requires updating
//! all nine attributes in `tools.rs`.
//!
//! Each helper accepts either a JSON Number (integer) or a JSON String containing
//! a base-10 integer literal. Float JSON Numbers are strictly rejected per FR-13.

use serde::de::{self, Unexpected, Visitor};
use std::fmt;

// ---------------------------------------------------------------------------
// deserialize_i64_or_string
// ---------------------------------------------------------------------------

struct I64OrStringVisitor;

impl<'de> Visitor<'de> for I64OrStringVisitor {
    type Value = i64;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an integer or a string containing a base-10 integer")
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> {
        Ok(v)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> {
        i64::try_from(v).map_err(|_| E::invalid_value(Unexpected::Unsigned(v), &self))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<i64, E> {
        v.parse::<i64>()
            .map_err(|_| E::custom(format!("invalid integer string: {v:?}")))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<i64, E> {
        self.visit_str(&v)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<i64, E> {
        Err(E::invalid_type(Unexpected::Float(v), &self))
    }

    fn visit_f32<E: de::Error>(self, v: f32) -> Result<i64, E> {
        Err(E::invalid_type(Unexpected::Float(v as f64), &self))
    }
}

/// Accept a JSON Number (integer) or a JSON String containing a base-10
/// integer literal. Rejects float Numbers, non-numeric strings, booleans,
/// arrays, and objects.
pub(crate) fn deserialize_i64_or_string<'de, D>(d: D) -> Result<i64, D::Error>
where
    D: de::Deserializer<'de>,
{
    d.deserialize_any(I64OrStringVisitor)
}

// ---------------------------------------------------------------------------
// deserialize_opt_i64_or_string
// ---------------------------------------------------------------------------

struct OptI64OrStringVisitor;

impl<'de> Visitor<'de> for OptI64OrStringVisitor {
    type Value = Option<i64>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an integer, a string containing a base-10 integer, or null")
    }

    fn visit_none<E: de::Error>(self) -> Result<Option<i64>, E> {
        Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Option<i64>, E> {
        Ok(None)
    }

    fn visit_some<D2: de::Deserializer<'de>>(self, d: D2) -> Result<Option<i64>, D2::Error> {
        deserialize_i64_or_string(d).map(Some)
    }
}

/// Accept a JSON Number (integer), a JSON String containing a base-10 integer,
/// or JSON null. Returns `None` for null and `Some(i64)` for valid values.
///
/// Absent fields are handled by `#[serde(default)]` on the struct field —
/// this function is not called for absent keys.
pub(crate) fn deserialize_opt_i64_or_string<'de, D>(d: D) -> Result<Option<i64>, D::Error>
where
    D: de::Deserializer<'de>,
{
    d.deserialize_option(OptI64OrStringVisitor)
}

// ---------------------------------------------------------------------------
// deserialize_opt_usize_or_string
// ---------------------------------------------------------------------------

struct UsizeOrStringVisitor;

impl<'de> Visitor<'de> for UsizeOrStringVisitor {
    type Value = usize;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a non-negative integer or a string containing a non-negative integer")
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<usize, E> {
        // MUST use try_from, never `as usize` (C-06: silent truncation on 32-bit)
        usize::try_from(v).map_err(|_| E::invalid_value(Unexpected::Unsigned(v), &self))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<usize, E> {
        if v < 0 {
            return Err(E::invalid_value(Unexpected::Signed(v), &self));
        }
        // v is non-negative; convert via u64 to usize safely (C-06)
        usize::try_from(v as u64).map_err(|_| E::invalid_value(Unexpected::Signed(v), &self))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<usize, E> {
        // Parse via u64 first — rejects negative strings at parse time (C-06)
        let val_u64 = v
            .parse::<u64>()
            .map_err(|_| E::custom(format!("invalid non-negative integer string: {v:?}")))?;
        // On 32-bit targets, u64 values > usize::MAX are rejected here.
        // On 64-bit targets all u64 values fit in usize, so this path is not
        // exercised in tests — document it here rather than with a cfg conditional.
        usize::try_from(val_u64)
            .map_err(|_| E::custom(format!("integer too large for usize: {v:?}")))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<usize, E> {
        self.visit_str(&v)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<usize, E> {
        Err(E::invalid_type(Unexpected::Float(v), &self))
    }

    fn visit_f32<E: de::Error>(self, v: f32) -> Result<usize, E> {
        Err(E::invalid_type(Unexpected::Float(v as f64), &self))
    }
}

fn inner_deserialize_usize<'de, D>(d: D) -> Result<usize, D::Error>
where
    D: de::Deserializer<'de>,
{
    d.deserialize_any(UsizeOrStringVisitor)
}

struct OptUsizeOrStringVisitor;

impl<'de> Visitor<'de> for OptUsizeOrStringVisitor {
    type Value = Option<usize>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "a non-negative integer, a string containing a non-negative integer, or null",
        )
    }

    fn visit_none<E: de::Error>(self) -> Result<Option<usize>, E> {
        Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Option<usize>, E> {
        Ok(None)
    }

    fn visit_some<D2: de::Deserializer<'de>>(self, d: D2) -> Result<Option<usize>, D2::Error> {
        inner_deserialize_usize(d).map(Some)
    }
}

/// Accept a non-negative JSON Number (integer), a JSON String containing a
/// non-negative base-10 integer, or JSON null. Returns `None` for null and
/// `Some(usize)` for valid values. Rejects negatives, floats, and overflow.
///
/// Parses strings via `u64` first to reject negatives before `usize::try_from`
/// conversion (never uses `as usize` per C-06).
///
/// Absent fields are handled by `#[serde(default)]` on the struct field.
pub(crate) fn deserialize_opt_usize_or_string<'de, D>(d: D) -> Result<Option<usize>, D::Error>
where
    D: de::Deserializer<'de>,
{
    d.deserialize_option(OptUsizeOrStringVisitor)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    // Thin wrapper structs for direct testing via serde_json::from_str.
    // The `#[serde(deserialize_with)]` path refers to the function within this
    // module directly (without `serde_util::` prefix since we are inside it).

    #[derive(Debug, Deserialize, PartialEq)]
    struct WrapI64 {
        #[serde(deserialize_with = "deserialize_i64_or_string")]
        v: i64,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct WrapOptI64 {
        #[serde(default, deserialize_with = "deserialize_opt_i64_or_string")]
        v: Option<i64>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct WrapOptUsize {
        #[serde(default, deserialize_with = "deserialize_opt_usize_or_string")]
        v: Option<usize>,
    }

    // -- deserialize_i64_or_string --

    #[test]
    fn test_deserialize_i64_integer_input() {
        let r: WrapI64 = serde_json::from_str(r#"{"v": 42}"#).unwrap();
        assert_eq!(r.v, 42i64);
    }

    #[test]
    fn test_deserialize_i64_string_input() {
        let r: WrapI64 = serde_json::from_str(r#"{"v": "3770"}"#).unwrap();
        assert_eq!(r.v, 3770i64);
    }

    #[test]
    fn test_deserialize_i64_negative_string() {
        let r: WrapI64 = serde_json::from_str(r#"{"v": "-5"}"#).unwrap();
        assert_eq!(r.v, -5i64);
    }

    #[test]
    fn test_deserialize_i64_zero_string() {
        let r: WrapI64 = serde_json::from_str(r#"{"v": "0"}"#).unwrap();
        assert_eq!(r.v, 0i64);
    }

    #[test]
    fn test_deserialize_i64_max_string() {
        let r: WrapI64 = serde_json::from_str(r#"{"v": "9223372036854775807"}"#).unwrap();
        assert_eq!(r.v, i64::MAX);
    }

    #[test]
    fn test_deserialize_i64_min_string() {
        let r: WrapI64 = serde_json::from_str(r#"{"v": "-9223372036854775808"}"#).unwrap();
        assert_eq!(r.v, i64::MIN);
    }

    #[test]
    fn test_deserialize_i64_overflow_string() {
        let result: Result<WrapI64, _> = serde_json::from_str(r#"{"v": "9999999999999999999999"}"#);
        assert!(result.is_err(), "expected error for i64 overflow string");
    }

    #[test]
    fn test_deserialize_i64_nonnumeric_string() {
        let result: Result<WrapI64, _> = serde_json::from_str(r#"{"v": "abc"}"#);
        assert!(result.is_err(), "expected error for non-numeric string");
    }

    #[test]
    fn test_deserialize_i64_empty_string() {
        let result: Result<WrapI64, _> = serde_json::from_str(r#"{"v": ""}"#);
        assert!(result.is_err(), "expected error for empty string");
    }

    #[test]
    fn test_deserialize_i64_float_string() {
        let result: Result<WrapI64, _> = serde_json::from_str(r#"{"v": "3.5"}"#);
        assert!(result.is_err(), "expected error for float string");
    }

    #[test]
    fn test_deserialize_i64_whitespace_string() {
        let result: Result<WrapI64, _> = serde_json::from_str(r#"{"v": " 42 "}"#);
        assert!(
            result.is_err(),
            "expected error for whitespace-padded string"
        );
    }

    #[test]
    fn test_deserialize_i64_float_number() {
        let result: Result<WrapI64, _> = serde_json::from_str(r#"{"v": 3.0}"#);
        assert!(result.is_err(), "expected error for float JSON Number");
        // Guard against silent truncation: must not succeed with value 3
        assert!(
            !matches!(result, Ok(WrapI64 { v: 3 })),
            "float must not be silently truncated to 3"
        );
    }

    #[test]
    fn test_deserialize_i64_bool_input() {
        let result: Result<WrapI64, _> = serde_json::from_str(r#"{"v": true}"#);
        assert!(result.is_err(), "expected error for boolean input");
    }

    #[test]
    fn test_deserialize_i64_array_input() {
        let result: Result<WrapI64, _> = serde_json::from_str(r#"{"v": [1]}"#);
        assert!(result.is_err(), "expected error for array input");
    }

    // -- deserialize_opt_i64_or_string --

    #[test]
    fn test_deserialize_opt_i64_integer_input() {
        let r: WrapOptI64 = serde_json::from_str(r#"{"v": 42}"#).unwrap();
        assert_eq!(r.v, Some(42i64));
    }

    #[test]
    fn test_deserialize_opt_i64_string_input() {
        let r: WrapOptI64 = serde_json::from_str(r#"{"v": "5"}"#).unwrap();
        assert_eq!(r.v, Some(5i64));
    }

    #[test]
    fn test_deserialize_opt_i64_null_input() {
        let r: WrapOptI64 = serde_json::from_str(r#"{"v": null}"#).unwrap();
        assert_eq!(r.v, None);
    }

    #[test]
    fn test_deserialize_opt_i64_absent_field() {
        // #[serde(default)] on WrapOptI64::v ensures absent key -> None
        let r: WrapOptI64 = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(r.v, None);
    }

    #[test]
    fn test_deserialize_opt_i64_nonnumeric_string() {
        let result: Result<WrapOptI64, _> = serde_json::from_str(r#"{"v": "abc"}"#);
        assert!(result.is_err(), "expected error for non-numeric string");
    }

    #[test]
    fn test_deserialize_opt_i64_float_string() {
        let result: Result<WrapOptI64, _> = serde_json::from_str(r#"{"v": "3.5"}"#);
        assert!(result.is_err(), "expected error for float string");
    }

    #[test]
    fn test_deserialize_opt_i64_float_number() {
        let result: Result<WrapOptI64, _> = serde_json::from_str(r#"{"v": 3.0}"#);
        assert!(result.is_err(), "expected error for float JSON Number");
        assert!(
            !matches!(result, Ok(WrapOptI64 { v: Some(3) })),
            "float must not be silently truncated to Some(3)"
        );
    }

    #[test]
    fn test_deserialize_opt_i64_negative_string() {
        let r: WrapOptI64 = serde_json::from_str(r#"{"v": "-5"}"#).unwrap();
        assert_eq!(r.v, Some(-5i64));
    }

    // -- deserialize_opt_usize_or_string --

    #[test]
    fn test_deserialize_opt_usize_integer_input() {
        let r: WrapOptUsize = serde_json::from_str(r#"{"v": 5}"#).unwrap();
        assert_eq!(r.v, Some(5usize));
    }

    #[test]
    fn test_deserialize_opt_usize_string_input() {
        let r: WrapOptUsize = serde_json::from_str(r#"{"v": "5"}"#).unwrap();
        assert_eq!(r.v, Some(5usize));
    }

    #[test]
    fn test_deserialize_opt_usize_zero_string() {
        let r: WrapOptUsize = serde_json::from_str(r#"{"v": "0"}"#).unwrap();
        assert_eq!(r.v, Some(0usize));
    }

    #[test]
    fn test_deserialize_opt_usize_null_input() {
        let r: WrapOptUsize = serde_json::from_str(r#"{"v": null}"#).unwrap();
        assert_eq!(r.v, None);
    }

    #[test]
    fn test_deserialize_opt_usize_absent_field() {
        // #[serde(default)] on WrapOptUsize::v ensures absent key -> None
        let r: WrapOptUsize = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(r.v, None);
    }

    #[test]
    fn test_deserialize_opt_usize_negative_string() {
        let result: Result<WrapOptUsize, _> = serde_json::from_str(r#"{"v": "-1"}"#);
        assert!(result.is_err(), "expected error for negative string");
    }

    #[test]
    fn test_deserialize_opt_usize_u64_overflow_string() {
        let result: Result<WrapOptUsize, _> =
            serde_json::from_str(r#"{"v": "99999999999999999999"}"#);
        assert!(result.is_err(), "expected error for u64 overflow string");
    }

    #[test]
    fn test_deserialize_opt_usize_nonnumeric_string() {
        let result: Result<WrapOptUsize, _> = serde_json::from_str(r#"{"v": "abc"}"#);
        assert!(result.is_err(), "expected error for non-numeric string");
    }

    #[test]
    fn test_deserialize_opt_usize_float_number() {
        let result: Result<WrapOptUsize, _> = serde_json::from_str(r#"{"v": 3.0}"#);
        assert!(result.is_err(), "expected error for float JSON Number");
        assert!(
            !matches!(result, Ok(WrapOptUsize { v: Some(3) })),
            "float must not be silently truncated to Some(3)"
        );
    }

    #[test]
    fn test_deserialize_opt_usize_float_string() {
        let result: Result<WrapOptUsize, _> = serde_json::from_str(r#"{"v": "3.5"}"#);
        assert!(result.is_err(), "expected error for float string");
    }

    #[test]
    fn test_deserialize_opt_usize_negative_five_string() {
        let result: Result<WrapOptUsize, _> = serde_json::from_str(r#"{"v": "-5"}"#);
        assert!(result.is_err(), "expected error for negative string -5");
    }
}
