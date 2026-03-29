# Test Plan: mcp/tools.rs

## Component Summary

Existing file `crates/unimatrix-server/src/mcp/tools.rs` is modified with paired
`#[serde(deserialize_with)]` + `#[schemars(with)]` + `#[serde(default)]` attributes on
nine fields across five structs. No handler logic changes. All new tests are additive to
the existing `#[cfg(test)]` block. The file currently has tests at lines 2785, 4089, 4107,
4149, 4167, 4324, 4779, 4871 — new tests must be added in a new `mod vnc012_coercion_tests`
module at the bottom of the file.

---

## Unit Test Expectations

### Test Module: `vnc012_coercion_tests` in `#[cfg(test)]` at bottom of tools.rs

All tests use `serde_json::from_str::<StructName>(...)` — this exercises the full struct
deserialization path including the `deserialize_with` attribute routing.

---

### Required Integer Fields — Happy Path (AC-01, AC-02, AC-07)

#### `GetParams`

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_get_params_string_id` | `{"id": "3770", "agent_id": "human"}` | `id == 3770i64` | AC-01 |
| `test_get_params_integer_id` | `{"id": 3770, "agent_id": "human"}` | `id == 3770i64` | AC-07 |
| `test_get_params_string_and_integer_equal` | both forms | both fields equal | AC-07 regression guard |

#### `DeprecateParams`

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_deprecate_params_string_id` | `{"id": "3770"}` | `id == 3770i64` | AC-01 |
| `test_deprecate_params_integer_id` | `{"id": 3770}` | `id == 3770i64` | AC-07 |

#### `QuarantineParams`

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_quarantine_params_string_id` | `{"id": "3770"}` | `id == 3770i64` | AC-01 |
| `test_quarantine_params_integer_id` | `{"id": 3770}` | `id == 3770i64` | AC-07 |

#### `CorrectParams`

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_correct_params_string_original_id` | `{"original_id": "3770", "content": "c"}` | `original_id == 3770i64` | AC-02 |
| `test_correct_params_integer_original_id` | `{"original_id": 3770, "content": "c"}` | `original_id == 3770i64` | AC-07 |

---

### Optional Integer Fields — Happy Path (AC-03, AC-04, AC-05, AC-06)

#### `LookupParams` — id

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_lookup_params_string_id` | `{"id": "42"}` | `id == Some(42i64)` | AC-03 |
| `test_lookup_params_absent_id` | `{}` | `id == None` | AC-03-ABSENT-ID, R-01 |
| `test_lookup_params_null_id` | `{"id": null}` | `id == None` | AC-03-NULL-ID, R-03 |

#### `LookupParams` — limit

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_lookup_params_string_limit` | `{"limit": "10"}` | `limit == Some(10i64)` | AC-03 |
| `test_lookup_params_absent_limit` | `{}` | `limit == None` | AC-03-ABSENT-LIMIT, R-01 |
| `test_lookup_params_null_limit` | `{"limit": null}` | `limit == None` | AC-03-NULL-LIMIT, R-03 |

#### `SearchParams` — k

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_search_params_string_k` | `{"query": "q", "k": "5"}` | `k == Some(5i64)` | AC-04 |
| `test_search_params_absent_k` | `{"query": "q"}` | `k == None` | AC-04-ABSENT, R-01 |
| `test_search_params_null_k` | `{"query": "q", "k": null}` | `k == None` | AC-04-NULL, R-03 |

#### `BriefingParams` — max_tokens

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_briefing_params_string_max_tokens` | `{"task": "t", "max_tokens": "3000"}` | `max_tokens == Some(3000i64)` | AC-05 |
| `test_briefing_params_absent_max_tokens` | `{"task": "t"}` | `max_tokens == None` | AC-05-ABSENT, R-01 |
| `test_briefing_params_null_max_tokens` | `{"task": "t", "max_tokens": null}` | `max_tokens == None` | AC-05-NULL, R-03 |

#### `RetrospectiveParams` — evidence_limit

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_retro_params_string_evidence_limit` | `{"feature_cycle": "col-001", "evidence_limit": "5"}` | `evidence_limit == Some(5usize)` | AC-06 |
| `test_retro_params_zero_evidence_limit` | `{"feature_cycle": "col-001", "evidence_limit": "0"}` | `evidence_limit == Some(0usize)` | AC-06-ZERO |
| `test_retro_params_absent_evidence_limit` | `{"feature_cycle": "col-001"}` | `evidence_limit == None` | AC-06-ABSENT, R-01 |
| `test_retro_params_null_evidence_limit` | `{"feature_cycle": "col-001", "evidence_limit": null}` | `evidence_limit == None` | AC-06-NULL, R-03 |

---

### Rejection Behavior (AC-08, AC-08-OPT, AC-09, AC-09-FLOAT, AC-09-FLOAT-NUMBER)

#### Non-numeric string rejection for required fields (AC-08)

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_get_params_nonnumeric_id_is_err` | `{"id": "abc"}` | `Err(_)` | AC-08, R-08 |
| `test_deprecate_params_nonnumeric_id_is_err` | `{"id": "abc"}` | `Err(_)` | AC-08 |
| `test_quarantine_params_nonnumeric_id_is_err` | `{"id": "abc"}` | `Err(_)` | AC-08 |
| `test_correct_params_nonnumeric_original_id_is_err` | `{"original_id": "abc", "content": "c"}` | `Err(_)` | AC-08 |

#### Non-numeric string rejection for optional fields (AC-08-OPT)

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_lookup_params_nonnumeric_id_is_err` | `{"id": "abc"}` | `Err(_)` | AC-08-OPT |
| `test_lookup_params_nonnumeric_limit_is_err` | `{"limit": "abc"}` | `Err(_)` | AC-08-OPT |
| `test_search_params_nonnumeric_k_is_err` | `{"query": "q", "k": "abc"}` | `Err(_)` | AC-08-OPT |
| `test_briefing_params_nonnumeric_max_tokens_is_err` | `{"task": "t", "max_tokens": "abc"}` | `Err(_)` | AC-08-OPT |
| `test_retro_params_nonnumeric_evidence_limit_is_err` | `{"feature_cycle": "c", "evidence_limit": "abc"}` | `Err(_)` | AC-08-OPT |

#### Negative/overflow rejection for evidence_limit (AC-09)

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_retro_params_negative_evidence_limit_is_err` | `{"feature_cycle": "c", "evidence_limit": "-1"}` | `Err(_)` | AC-09, R-04 |

#### Float string rejection (AC-09-FLOAT)

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_get_params_float_string_is_err` | `{"id": "3.5"}` | `Err(_)` | AC-09-FLOAT (required field) |
| `test_search_params_float_string_k_is_err` | `{"query": "q", "k": "3.5"}` | `Err(_)` | AC-09-FLOAT (optional field) |

#### Float JSON Number rejection (AC-09-FLOAT-NUMBER)

| Test Name | Input JSON | Expected | Covers |
|-----------|-----------|----------|--------|
| `test_get_params_float_number_is_err` | `{"id": 3.0}` | `Err(_)` + not `Ok(id=3)` | AC-09-FLOAT-NUMBER, FR-13 |
| `test_search_params_float_number_k_is_err` | `{"query": "q", "k": 5.0}` | `Err(_)` | AC-09-FLOAT-NUMBER, FR-13 |

For the float-Number tests, assert both `is_err()` AND that `result.ok()` is not `Some(struct_with_truncated_int)`.
This double assertion catches silent truncation.

---

### Schema Snapshot Test (AC-10, R-05)

| Test Name | Method | Expected | Covers |
|-----------|--------|----------|--------|
| `test_schema_integer_type_preserved_for_all_nine_fields` | Construct `UnimatrixServer` via `make_server()`, call `tool_router.list_all()`, extract `input_schema` JSON, assert `"type": "integer"` for all nine affected properties | All nine schemas contain `"type": "integer"` | AC-10, R-05 |

This test must verify each of the nine fields across the five tools:
- `context_get` → property `id`
- `context_deprecate` → property `id`
- `context_quarantine` → property `id`
- `context_correct` → property `original_id`
- `context_lookup` → properties `id` and `limit`
- `context_search` → property `k`
- `context_briefing` → property `max_tokens`
- `context_retrospective` → property `evidence_limit` (also check `minimum: 0` is present or absent per NFR-05)

Use `serde_json::Value` to navigate the schema. Assert `schema["properties"]["id"]["type"] == "integer"`.
This test is async if `make_server()` is async; use `#[tokio::test]` in that case.

---

### Regression Check (AC-11, R-10)

No new test needed. The existing `test_retrospective_params_evidence_limit` test (already
in tools.rs) is the regression guard. After applying struct annotations, the implementation
agent must verify this test passes unmodified. The tester confirms by running
`cargo test --workspace 2>&1 | tail -30` and verifying no regression.

The tester must locate the existing test and confirm it uses an integer (not a string) for
`evidence_limit`. If it uses a string, the test behavior is now changed (string coercion
applies) — but the test should still pass because the coercion is a superset of the
original behavior.

---

## Integration Test Expectations

The tools.rs component test boundary includes AC-13 (Rust in-process integration test).
This test validates the rmcp `Parameters<T>` dispatch path which unit tests do not cover.

**AC-13 test location**: `crates/unimatrix-server/tests/mcp_coercion.rs` (preferred)
or within a `#[cfg(test)]` block in `server.rs`.

**AC-13 test name**: Must contain `coercion` or `string_id` (e.g.,
`test_get_string_id_coercion` or `test_context_get_accepts_string_id`).

**AC-13 steps**:
1. Call `make_server()` — must be `pub(crate)` or `pub` in `server.rs`
2. Insert an entry via `server.service_layer.store.insert(NewEntry { ... })` or equivalent
3. Obtain the returned `u64` id and convert to string: `id.to_string()`
4. Build `CallToolRequestParams { name: "context_get".into(), arguments: serde_json::json!({"id": id_string, "agent_id": "human"}) }`
5. Call `server.call_tool(request, ctx)` where `ctx` is a no-op `RequestContext<RoleServer>`
6. Assert `result.is_ok()` — not `Err` containing `"invalid type: string"`
7. Assert returned `CallToolResult.content` is non-empty

**OQ-04 fallback**: If `RequestContext<RoleServer>` cannot be constructed from public API,
the implementation agent must expose `pub(crate) async fn call_tool_for_test(name: &str, args: serde_json::Value)` on `UnimatrixServer` that invokes `tool_router` directly. The
test calls this function instead.

---

## Specific Assertions Summary

- Required field happy path: `assert_eq!(params.id, 3770i64)`
- Optional field present: `assert_eq!(params.k, Some(5i64))`
- Optional field absent: `assert!(params.k.is_none())`
- Optional field null: `assert!(params.k.is_none())`
- Error cases: `assert!(result.is_err())`
- Schema: `assert_eq!(schema_value["properties"]["id"]["type"], "integer")`
- Float truncation guard: `assert!(result.is_err())` — not `assert_ne!(result.ok().map(|p| p.id), Some(3i64))`
