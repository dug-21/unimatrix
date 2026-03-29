# Component: mcp/tools.rs (modified)

## Purpose

Apply `#[serde(deserialize_with)]`, `#[serde(default)]`, and `#[schemars(with)]`
attribute annotations to nine fields across five parameter structs. Add new unit tests
to the existing `#[cfg(test)]` block at the bottom of the file.

No handler logic changes. No new imports (serde_util is in the same crate). No type
changes. This is annotation-only work plus test additions.

**File**: `crates/unimatrix-server/src/mcp/tools.rs`

---

## New / Modified Functions

No new functions. No modified function bodies. All changes are struct field annotations.

---

## Struct Field Annotations (FR-07)

The nine fields listed below receive paired `#[serde(...)]` and `#[schemars(...)]`
attributes. The changes are shown in diff-like pseudocode.

### GetParams — field `id`

```
-- BEFORE:
pub id: i64,

-- AFTER:
#[serde(deserialize_with = "serde_util::deserialize_i64_or_string")]
#[schemars(with = "i64")]
pub id: i64,
```

### DeprecateParams — field `id`

```
-- BEFORE:
pub id: i64,

-- AFTER:
#[serde(deserialize_with = "serde_util::deserialize_i64_or_string")]
#[schemars(with = "i64")]
pub id: i64,
```

### QuarantineParams — field `id`

```
-- BEFORE:
pub id: i64,

-- AFTER:
#[serde(deserialize_with = "serde_util::deserialize_i64_or_string")]
#[schemars(with = "i64")]
pub id: i64,
```

### CorrectParams — field `original_id`

```
-- BEFORE:
pub original_id: i64,

-- AFTER:
#[serde(deserialize_with = "serde_util::deserialize_i64_or_string")]
#[schemars(with = "i64")]
pub original_id: i64,
```

### LookupParams — field `id`

```
-- BEFORE:
pub id: Option<i64>,

-- AFTER (note: #[serde(default)] required -- ADR-004, R-01):
#[serde(default, deserialize_with = "serde_util::deserialize_opt_i64_or_string")]
#[schemars(with = "Option<i64>")]
pub id: Option<i64>,
```

### LookupParams — field `limit`

```
-- BEFORE:
pub limit: Option<i64>,

-- AFTER:
#[serde(default, deserialize_with = "serde_util::deserialize_opt_i64_or_string")]
#[schemars(with = "Option<i64>")]
pub limit: Option<i64>,
```

### SearchParams — field `k`

```
-- BEFORE:
pub k: Option<i64>,

-- AFTER:
#[serde(default, deserialize_with = "serde_util::deserialize_opt_i64_or_string")]
#[schemars(with = "Option<i64>")]
pub k: Option<i64>,
```

### BriefingParams — field `max_tokens`

```
-- BEFORE:
pub max_tokens: Option<i64>,

-- AFTER:
#[serde(default, deserialize_with = "serde_util::deserialize_opt_i64_or_string")]
#[schemars(with = "Option<i64>")]
pub max_tokens: Option<i64>,
```

### RetrospectiveParams — field `evidence_limit`

```
-- BEFORE:
pub evidence_limit: Option<usize>,

-- AFTER:
#[serde(default, deserialize_with = "serde_util::deserialize_opt_usize_or_string")]
#[schemars(with = "Option<u64>")]
pub evidence_limit: Option<usize>,
```

Note on `evidence_limit`: The existing test `test_retrospective_params_evidence_limit`
passes an integer-typed evidence_limit. After annotation it will route through
`deserialize_opt_usize_or_string`'s `visit_u64`/`visit_i64` path, which passes through
unchanged. The existing test will continue to pass without modification (R-10 confirmed
low risk).

---

## AC-13: Rust In-Process Integration Test

### OQ-04 Resolution

`RequestContext<RoleServer>` is not constructible outside rmcp (Peer::new is pub(crate)
in rmcp). The AC-13 test must NOT use `rmcp::ServerHandler::call_tool` directly.

The correct test vehicle for AC-13 is:

`serde_json::from_value(serde_json::Value::Object(args))` applied to the struct directly.
This is the EXACT line executed by `Parameters<T>: FromContextPart` in rmcp
(`rmcp/src/handler/server/tool.rs` line ~173). Testing the struct deserialization IS
testing the rmcp dispatch path's critical section, because `Parameters<P>` is a
transparent wrapper that does nothing except call `serde_json::from_value`.

The test lives in the `#[cfg(test)]` block in `tools.rs`. It does NOT need `make_server()`.

### AC-13 Test Structure

```
#[test]
fn test_get_params_string_id_coercion() {
    -- AC-13: verify serde_json::from_value (the rmcp Parameters<T> dispatch path)
    -- accepts string-encoded id

    -- Build arguments as a JSON object (the type rmcp passes to from_value)
    LET args = serde_json::json!({
        "id": "3770",     -- string-encoded integer, not a JSON Number
        "agent_id": "human"
    })

    -- This is the EXACT call in rmcp Parameters<T>: FromContextPart:
    --   serde_json::from_value::<GetParams>(Value::Object(arguments))
    LET result = serde_json::from_value::<GetParams>(args)

    -- Assert: no "invalid type: string" error
    assert!(result.is_ok(), "AC-13: string id must not produce serde error; got: {:?}", result.err())
    assert_eq!(result.unwrap().id, 3770i64, "AC-13: string id must coerce to i64")
}
```

A second AC-13 variant for DeprecateParams (covers the other affected required-i64 tool):

```
#[test]
fn test_deprecate_params_string_id_coercion() {
    LET args = serde_json::json!({ "id": "42", "agent_id": "human" })
    LET result = serde_json::from_value::<DeprecateParams>(args)
    assert!(result.is_ok())
    assert_eq!(result.unwrap().id, 42i64)
}
```

These two tests satisfy the AC-13 requirement: they exercise the exact code path where
the bug fires, confirm the coercion works, and include the word "coercion" or "string_id"
in the test name.

---

## AC-10: JSON Schema Snapshot Test

### Approach

AC-10 requires constructing a `UnimatrixServer` to call `tool_router.list_all()`.

From reading `server.rs`:
- `tool_router` is a private field on `UnimatrixServer`
- `make_server()` is `pub(crate)` and lives inside the `#[cfg(test)]` block of `server.rs`
- From `tools.rs` tests, `use super::*;` gives access to `UnimatrixServer` but NOT to
  `make_server()` which is defined in a different module's test block

Therefore AC-10 must live in `server.rs` tests (not `tools.rs`), where `make_server()`
and `tool_router` are both accessible.

Alternatively, `tool_router` can be exposed for testing by adding a `#[cfg(test)]`
accessor method on `UnimatrixServer` in `server.rs`:

```
-- In server.rs, inside #[cfg(test)] impl block:
#[cfg(test)]
impl UnimatrixServer {
    pub(crate) fn tool_router_for_test(&self) -> &rmcp::handler::server::router::tool::ToolRouter<UnimatrixServer> {
        &self.tool_router
    }
}
```

Then the AC-10 test in `server.rs` test block:

```
#[tokio::test(flavor = "multi_thread")]
async fn test_schema_snapshot_integer_fields() {
    -- AC-10: verify #[schemars(with = "T")] preserves type: integer for all 9 affected fields

    LET server = make_server().await
    LET tools = server.tool_router_for_test().list_all()

    -- Build a map: tool_name -> input_schema
    LET schema_by_name: HashMap<String, serde_json::Value> = tools.into_iter()
        .map(|t| (t.name.to_string(), serde_json::to_value(&t.input_schema).unwrap()))
        .collect()

    -- Define the 9 fields to check as (tool_name, field_name) pairs
    LET checks = [
        ("context_get",          "id"),
        ("context_deprecate",    "id"),
        ("context_quarantine",   "id"),
        ("context_correct",      "original_id"),
        ("context_lookup",       "id"),
        ("context_lookup",       "limit"),
        ("context_search",       "k"),
        ("context_briefing",     "max_tokens"),
        ("context_cycle_review", "evidence_limit"),  -- tool name for RetrospectiveParams
    ]

    FOR (tool_name, field_name) IN checks:
        LET schema = schema_by_name.get(tool_name)
            .expect("tool must exist")
        LET properties = schema["properties"][field_name]
            .as_object().expect("field must have schema")

        -- Assert type: integer (ADR-002, NFR-05)
        LET field_type = &properties["type"]
        assert_eq!(
            field_type,
            "integer",
            "AC-10: field {field_name} on {tool_name} must have type: integer, got: {field_type}"
        )
    END FOR

    -- Special check: evidence_limit may have minimum: 0 (NFR-05 permits this)
    LET el_schema = schema_by_name["context_cycle_review"]["properties"]["evidence_limit"]
    IF el_schema.get("minimum").is_some() THEN
        assert_eq!(el_schema["minimum"], 0, "AC-10: evidence_limit minimum must be 0 if present")
    END IF
}
```

Note: The tool name for `RetrospectiveParams` must be verified against the actual
`#[tool(name = "...")]` annotation in `tools.rs`. Search for `feature_cycle` in
`tools.rs` to find the correct tool name.

---

## New Tests to Add to `#[cfg(test)]` in tools.rs

All tests use `serde_json::from_str` or `serde_json::from_value` — no server required.
Tests are grouped by AC identifier. The existing `test_wrong_type_doesnt_panic` test
(which uses `"id": "not-a-number"` and asserts `is_err()`) will continue to pass after
annotation — the annotation makes it a better test, not a breaking change.

### AC-01: Required integer fields accept string input

```
test_get_params_string_id_accepted:
    from_str::<GetParams>(r#"{"id": "3770"}"#) -> Ok(id == 3770)

test_deprecate_params_string_id_accepted:
    from_str::<DeprecateParams>(r#"{"id": "3770"}"#) -> Ok(id == 3770)

test_quarantine_params_string_id_accepted:
    from_str::<QuarantineParams>(r#"{"id": "3770"}"#) -> Ok(id == 3770)
```

### AC-02: CorrectParams.original_id accepts string input

```
test_correct_params_string_original_id_accepted:
    from_str::<CorrectParams>(r#"{"original_id": "3770", "content": "c"}"#) -> Ok(original_id == 3770)
```

### AC-03: LookupParams optional fields accept string + absent + null

```
test_lookup_params_string_id_accepted:
    from_str::<LookupParams>(r#"{"id": "42"}"#) -> Ok(id == Some(42))

test_lookup_params_string_limit_accepted:
    from_str::<LookupParams>(r#"{"limit": "10"}"#) -> Ok(limit == Some(10))

test_lookup_params_absent_id_is_none:   -- AC-03-ABSENT-ID (R-01 critical coverage)
    from_str::<LookupParams>(r#"{}"#) -> Ok(id.is_none())

test_lookup_params_absent_limit_is_none:   -- AC-03-ABSENT-LIMIT
    from_str::<LookupParams>(r#"{}"#) -> Ok(limit.is_none())

test_lookup_params_null_id_is_none:   -- AC-03-NULL-ID (R-03 coverage)
    from_str::<LookupParams>(r#"{"id": null}"#) -> Ok(id.is_none())

test_lookup_params_null_limit_is_none:   -- AC-03-NULL-LIMIT
    from_str::<LookupParams>(r#"{"limit": null}"#) -> Ok(limit.is_none())
```

### AC-04: SearchParams.k accepts string + absent + null

```
test_search_params_string_k_accepted:
    from_str::<SearchParams>(r#"{"query": "q", "k": "5"}"#) -> Ok(k == Some(5))

test_search_params_absent_k_is_none:   -- AC-04-ABSENT
    from_str::<SearchParams>(r#"{"query": "q"}"#) -> Ok(k.is_none())

test_search_params_null_k_is_none:   -- AC-04-NULL
    from_str::<SearchParams>(r#"{"query": "q", "k": null}"#) -> Ok(k.is_none())
```

### AC-05: BriefingParams.max_tokens accepts string + absent + null

```
test_briefing_params_string_max_tokens_accepted:
    from_str::<BriefingParams>(r#"{"task": "t", "max_tokens": "3000"}"#) -> Ok(max_tokens == Some(3000))

test_briefing_params_absent_max_tokens_is_none:   -- AC-05-ABSENT
    from_str::<BriefingParams>(r#"{"task": "t"}"#) -> Ok(max_tokens.is_none())

test_briefing_params_null_max_tokens_is_none:   -- AC-05-NULL
    from_str::<BriefingParams>(r#"{"task": "t", "max_tokens": null}"#) -> Ok(max_tokens.is_none())
```

### AC-06: RetrospectiveParams.evidence_limit accepts string + zero + absent + null

```
test_retrospective_params_string_evidence_limit_accepted:   -- AC-06
    from_str::<RetrospectiveParams>(r#"{"feature_cycle": "col-001", "evidence_limit": "5"}"#)
    -> Ok(evidence_limit == Some(5usize))

test_retrospective_params_string_evidence_limit_zero:   -- AC-06-ZERO
    from_str::<RetrospectiveParams>(r#"{"feature_cycle": "col-001", "evidence_limit": "0"}"#)
    -> Ok(evidence_limit == Some(0usize))

test_retrospective_params_absent_evidence_limit_is_none:   -- AC-06-ABSENT
    from_str::<RetrospectiveParams>(r#"{"feature_cycle": "col-001"}"#)
    -> Ok(evidence_limit.is_none())

test_retrospective_params_null_evidence_limit_is_none:   -- AC-06-NULL
    from_str::<RetrospectiveParams>(r#"{"feature_cycle": "col-001", "evidence_limit": null}"#)
    -> Ok(evidence_limit.is_none())
```

### AC-07: Required integer fields continue to accept JSON integer input (regression)

```
test_get_params_integer_id_unchanged:
    from_str::<GetParams>(r#"{"id": 42}"#) -> Ok(id == 42)

test_deprecate_params_integer_id_unchanged:
    from_str::<DeprecateParams>(r#"{"id": 42}"#) -> Ok(id == 42)

test_quarantine_params_integer_id_unchanged:
    from_str::<QuarantineParams>(r#"{"id": 42}"#) -> Ok(id == 42)

test_correct_params_integer_original_id_unchanged:
    from_str::<CorrectParams>(r#"{"original_id": 42, "content": "c"}"#) -> Ok(original_id == 42)
```

### AC-08: Non-numeric strings rejected for required and optional fields

```
test_get_params_nonnumeric_string_rejected:   -- AC-08
    from_str::<GetParams>(r#"{"id": "abc"}"#) -> Err

test_deprecate_params_nonnumeric_string_rejected:
    from_str::<DeprecateParams>(r#"{"id": "abc"}"#) -> Err

test_quarantine_params_nonnumeric_string_rejected:
    from_str::<QuarantineParams>(r#"{"id": "abc"}"#) -> Err

test_correct_params_nonnumeric_string_rejected:
    from_str::<CorrectParams>(r#"{"original_id": "abc", "content": "c"}"#) -> Err

test_lookup_params_nonnumeric_id_rejected:   -- AC-08-OPT
    from_str::<LookupParams>(r#"{"id": "abc"}"#) -> Err

test_lookup_params_nonnumeric_limit_rejected:
    from_str::<LookupParams>(r#"{"limit": "abc"}"#) -> Err

test_search_params_nonnumeric_k_rejected:
    from_str::<SearchParams>(r#"{"query": "q", "k": "abc"}"#) -> Err

test_briefing_params_nonnumeric_max_tokens_rejected:
    from_str::<BriefingParams>(r#"{"task": "t", "max_tokens": "abc"}"#) -> Err

test_retrospective_params_nonnumeric_evidence_limit_rejected:
    from_str::<RetrospectiveParams>(r#"{"feature_cycle": "col-001", "evidence_limit": "abc"}"#) -> Err
```

### AC-09: Negative strings and float strings rejected

```
test_retrospective_params_negative_evidence_limit_rejected:   -- AC-09
    from_str::<RetrospectiveParams>(r#"{"feature_cycle": "col-001", "evidence_limit": "-1"}"#) -> Err

test_get_params_float_string_rejected:   -- AC-09-FLOAT
    from_str::<GetParams>(r#"{"id": "3.5"}"#) -> Err

test_search_params_float_string_k_rejected:
    from_str::<SearchParams>(r#"{"query": "q", "k": "3.5"}"#) -> Err
```

### AC-09-FLOAT-NUMBER: Float JSON Numbers rejected (FR-13)

```
test_get_params_float_number_rejected:   -- AC-09-FLOAT-NUMBER
    -- 3.0 is a JSON float Number, not a string -- must invoke visit_f64 -> Err
    from_str::<GetParams>(r#"{"id": 3.0}"#) -> Err

test_search_params_float_number_k_rejected:
    from_str::<SearchParams>(r#"{"query": "q", "k": 5.0}"#) -> Err

test_lookup_params_float_number_id_rejected:
    from_str::<LookupParams>(r#"{"id": 3.0}"#) -> Err
```

### AC-13: In-process rmcp dispatch path test (serde_json::from_value)

```
test_get_params_string_id_coercion:   -- AC-13 primary
    from_value::<GetParams>(json!({"id": "3770", "agent_id": "human"}))
    -> Ok(id == 3770)
    -- name includes "coercion" -- satisfies AC-13 findability requirement

test_deprecate_params_string_id_coercion:
    from_value::<DeprecateParams>(json!({"id": "42"}))
    -> Ok(id == 42)
```

---

## Data Flow

Input: existing `tools.rs` struct definitions
Output: same structs with additional `#[serde(...)]` and `#[schemars(...)]` attributes

The `serde_util` path strings (e.g., `"serde_util::deserialize_i64_or_string"`) are
resolved at macro expansion time relative to the crate root. Because `serde_util` is
declared as `mod serde_util;` in `mcp/mod.rs`, the path is `serde_util::` within the
`mcp` module. The `deserialize_with` string path must match the Rust module path visible
from `tools.rs` — which is `serde_util::deserialize_i64_or_string` (not
`mcp::serde_util::` and not `crate::mcp::serde_util::`).

Verify by checking that `serde_util` is in scope as a sibling module from `tools.rs`.
Since `tools.rs` is `crate::mcp::tools`, and `serde_util` is `crate::mcp::serde_util`,
the relative path `serde_util::` is correct.

---

## Error Handling

No new error types introduced. The annotations either successfully deserialize to the
typed value, or produce a `serde::de::Error` that rmcp wraps as:
```
ErrorData::invalid_params("failed to deserialize parameters: <serde error message>", None)
```

This is returned to the MCP caller as a JSON-RPC error response. No panics.

---

## Risks to Watch During Implementation

- R-05: Typos in `#[schemars(with = "...")]` string literals compile silently but produce
  empty `{}` schemas. The AC-10 schema snapshot test is the only guard.
- R-07: The `deserialize_with` path strings `"serde_util::deserialize_..."` are string
  literals. A rename of the module or functions would compile silently; only `cargo build`
  would catch it at macro expansion.
- R-10: The existing `test_retrospective_params_evidence_limit` test must pass after
  annotation. The implementation agent must read it before adding annotations to confirm
  it uses integer (not string) evidence_limit values.
