# Risk-Based Test Strategy: vnc-012

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `#[serde(default)]` missing on one or more optional fields — absent inputs produce a missing-field error instead of `None` | High | Med | Critical |
| R-02 | AC-13 (Rust) + IT-01/IT-02 (Python) both required — rmcp dispatch path and stdio transport must both be tested | High | Med | Critical |
| R-03 | `deserialize_opt_i64_or_string` returns `Some(0)` or panics on JSON null instead of `None` | High | Low | High |
| R-04 | `deserialize_opt_usize_or_string` silently truncates on 32-bit targets due to `as usize` cast | Med | Low | High |
| R-05 | `#[schemars(with = "T")]` string literal typo generates empty schema `{}` — not caught by compiler | Med | Low | High |
| R-06 | Float JSON Numbers (e.g., `3.0` as Number type) routed to `visit_f64` — not handled, may panic or produce wrong error message | Med | Med | High |
| R-07 | `deserialize_with` path string is a string literal — a rename of `serde_util` silently breaks all nine fields at build time (not compile-time) | Med | Low | Med |
| R-08 | Non-numeric string rejection silently coerces to `0` rather than returning an error | Med | Low | Med |
| R-09 | Schema snapshot test constructs `UnimatrixServer` via `make_server()` — if `make_server()` is not pub(crate) to the test context, AC-10 test cannot be written | Med | Med | Med |
| R-10 | Existing `test_retrospective_params_evidence_limit` test in `tools.rs` breaks due to struct annotation change | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Missing `#[serde(default)]` on Optional Fields
**Severity**: High
**Likelihood**: Med
**Impact**: When an agent omits an optional parameter (the normal case for `k`, `limit`, `max_tokens`, `evidence_limit`), serde returns a "missing field" error instead of `None`. Every call without these parameters would fail — a regression worse than the original bug. This is a compile-silent trap identified in SR-02 and codified in ADR-004.

**Test Scenarios**:
1. `LookupParams` deserialized from `{}` (no `id`, no `limit`) — both fields must be `None`.
2. `SearchParams` deserialized from `{"query": "q"}` (no `k`) — `k` must be `None`.
3. `BriefingParams` deserialized from `{"task": "t"}` (no `max_tokens`) — `max_tokens` must be `None`.
4. `RetrospectiveParams` deserialized from `{"feature_cycle": "col-001"}` (no `evidence_limit`) — must be `None`.

**Coverage Requirement**: One absent-field test per optional field (5 tests). Must be separate from the null-field tests. AC-03-ABSENT-ID, AC-03-ABSENT-LIMIT, AC-04-ABSENT, AC-05-ABSENT, AC-06-ABSENT.

---

### R-02: AC-13 Integration Test — `RequestContext<RoleServer>` Not Constructible
**Severity**: High
**Likelihood**: Med
**Impact**: OQ-04 is unresolved: `rmcp::ServerHandler::call_tool` requires a `RequestContext<RoleServer>` which existing `server.rs` tests do not construct. If it is not constructible from rmcp's public API, the Rust integration test in AC-13 cannot be implemented as specified. The result: the rmcp dispatch path (the exact code path containing the bug) is never tested — only the serde layer is. SR-03 is the parent risk. Confirmed live: the bug was reproduced during spec writing when `context_get` was called with a string id and failed.

**Test Scenarios**:
1. Attempt to construct `RequestContext<RoleServer>` via rmcp public API in a `#[cfg(test)]` context — determine if it is feasible.
2. If not feasible: expose `pub(crate) fn call_tool_for_test(name, args) -> Result<...>` on `UnimatrixServer` that invokes the `ToolRouter` directly, bypassing `RequestContext`.
3. AC-13: store an entry, call `context_get` with `{"id": "<string_id>", "agent_id": "human"}`, assert success and non-empty content.
4. IT-01 (infra-001): call `context_get` via `call_tool("context_get", {"id": "<string_id>"})` over stdio transport — asserts rmcp dispatch layer, not just serde.

**Coverage Requirement**: Both Rust AC-13 AND Python IT-01/IT-02 are required. AC-13 covers the serde dispatch path in Rust; IT-01/IT-02 cover the full stdio transport layer. Neither alone is sufficient. Marked `smoke` so CI catches regressions.

---

### R-03: JSON Null Produces `Some(0)` or Error Instead of `None`
**Severity**: High
**Likelihood**: Low
**Impact**: An agent passing `{"k": null}` explicitly would get either a parse error or a silently wrong `Some(0)` for `SearchParams.k`. Tools using `.unwrap_or(default)` would use the wrong value without error. SR-02 and ADR-004 flag this as distinct from the absent-field case.

**Test Scenarios**:
1. `LookupParams` deserialized from `{"id": null}` — `id` must be `None`.
2. `LookupParams` deserialized from `{"limit": null}` — `limit` must be `None`.
3. `SearchParams` deserialized from `{"query": "q", "k": null}` — `k` must be `None`.
4. `BriefingParams` deserialized from `{"task": "t", "max_tokens": null}` — `max_tokens` must be `None`.
5. `RetrospectiveParams` deserialized from `{"feature_cycle": "col-001", "evidence_limit": null}` — must be `None`.

**Coverage Requirement**: One null-field test per optional field (5 tests). Must use `"field": null` explicitly, distinct from absent-field tests. AC-03-NULL-ID, AC-03-NULL-LIMIT, AC-04-NULL, AC-05-NULL, AC-06-NULL.

---

### R-04: `usize` Truncation on 32-bit Targets
**Severity**: Med
**Likelihood**: Low
**Impact**: If `as usize` is used instead of `usize::try_from(val_u64)`, a value like `4294967296` (2^32) would silently truncate to `0` on 32-bit targets. CI targets include 32-bit. ADR-001 and C-06 mandate `usize::try_from`.

**Test Scenarios**:
1. `RetrospectiveParams` deserialized from `{"evidence_limit": "99999999999999999999"}` — must return a serde error (u64 overflow at parse).
2. `RetrospectiveParams` deserialized from `{"evidence_limit": "-1"}` — must return a serde error (negative rejected before usize conversion). AC-09.
3. `RetrospectiveParams` deserialized from `{"evidence_limit": "0"}` — must return `Some(0usize)`. AC-06-ZERO.

**Coverage Requirement**: Three tests for boundary and rejection paths on `evidence_limit`. Overflow test covers `usize::try_from` indirectly on any target.

---

### R-05: `#[schemars(with = "T")]` Typo Produces Empty Schema
**Severity**: Med
**Likelihood**: Low
**Impact**: `#[schemars(with = "Option<i64>")]` with a typo (e.g., `"Option<164>"`) compiles silently but emits `{}` for that field's schema instead of `{"type": "integer"}`. AC-10 fails. Any MCP client that validates against the schema would reject valid calls. SR-01 and ADR-002 identify this — the only guard is a schema snapshot test.

**Test Scenarios**:
1. Schema snapshot: construct `UnimatrixServer`, call `tool_router.list_all()`, extract `input_schema` for each of the nine affected tools, assert each affected property has `"type": "integer"`.
2. For `evidence_limit` specifically: assert `"type": "integer"` and optionally `"minimum": 0` (the only permitted delta per NFR-05).

**Coverage Requirement**: One schema snapshot test covering all nine fields. AC-10.

---

### R-06: Float JSON Numbers Rejected by Visitor (`visit_f64`) — RESOLVED
**Severity**: Med
**Likelihood**: Med
**Resolution**: OQ-05 resolved. Float JSON Numbers must be **strictly rejected**. FR-13 in SPECIFICATION.md requires `visit_f64` and `visit_f32` to return `de::Error::invalid_type(de::Unexpected::Float(v), &self)`. No coercion, no truncation.

**Test Scenarios**:
1. `GetParams` deserialized from `{"id": 3.0}` (JSON float Number, not string) — must return a serde error, not panic.
2. `SearchParams` deserialized from `{"query": "q", "k": 5.0}` (JSON float Number) — must return a serde error.
3. `GetParams` deserialized from `{"id": "3.5"}` (float string) — must return a serde error (covered by FR-12, AC-09-FLOAT).

**Coverage Requirement**: At least one float Number rejection test per helper function (one for `deserialize_i64_or_string`, one for an optional helper). AC-09-FLOAT and AC-09-FLOAT-NUMBER.

---

### R-07: `deserialize_with` Path String Literal Not Compiler-Validated
**Severity**: Med
**Likelihood**: Low
**Impact**: The nine `#[serde(deserialize_with = "serde_util::deserialize_...")]` annotations are string literals. A rename of `serde_util` or any of the three functions would compile without error until the serde macro tries to resolve the path at macro-expansion time. The build would fail, but only at `cargo build` — the compiler does not validate attribute string contents in advance. ADR-001 "Harder" section identifies this.

**Test Scenarios**:
1. `cargo build --workspace` passes without error — validates all nine `deserialize_with` path strings resolve correctly. This is a build-time verification, not a test.
2. Any rename of `serde_util` must update all nine attribute strings — document this in ADR-001 or a code comment.

**Coverage Requirement**: Covered implicitly by `cargo build` and `cargo test`. No dedicated test, but the implementation agent must be made aware of this trap.

---

### R-08: Non-Numeric String Silently Coerces to Zero
**Severity**: Med
**Likelihood**: Low
**Impact**: If `str::parse::<i64>()` failure is swallowed (e.g., returning `Ok(0)` on parse error instead of `Err`), then `context_get` with `{"id": "abc"}` would call the handler with `id = 0` rather than returning a serde error. The handler would then attempt to look up entry 0, which may not exist — producing a confusing "not found" error rather than "invalid parameter". FR-11 and AC-08 require a serde error. Identified in entry #885 as a recurring test coverage gap for serde types.

**Test Scenarios**:
1. `GetParams` deserialized from `{"id": "abc"}` — must `is_err()`, not `Ok(GetParams { id: 0 })`. AC-08.
2. `LookupParams` deserialized from `{"id": "abc"}` — must `is_err()`. AC-08-OPT.
3. `SearchParams` deserialized from `{"query": "q", "k": "abc"}` — must `is_err()`. AC-08-OPT.
4. Empty string `""` for any integer field — must `is_err()` (covered by FR-11).

**Coverage Requirement**: One non-numeric string rejection test per required field (4 tests for GetParams, DeprecateParams, QuarantineParams, CorrectParams) plus one per optional field (5 tests). AC-08 and AC-08-OPT.

---

### R-09: `make_server()` Not Accessible in Test Context for Schema Snapshot
**Severity**: Med
**Likelihood**: Med
**Impact**: AC-10 requires constructing a `UnimatrixServer` via `make_server()` or equivalent to call `tool_router.list_all()`. If `make_server()` is `pub(crate)` or `#[cfg(test)]` only, the schema snapshot test may need to live inside `server.rs` or a specific test module. If `tool_router` is private, the test cannot extract the schema at all. This is parallel to OQ-04: the test infrastructure constraint may require a helper or visibility change.

**Test Scenarios**:
1. Verify `make_server()` is accessible in the `#[cfg(test)]` context of `tools.rs` or a sibling test file.
2. If `tool_router` is private: expose a `pub(crate) fn schema_for_test()` on `UnimatrixServer` returning the tool schema map for test use only.

**Coverage Requirement**: Schema snapshot test must exist — how it is wired is an implementation detail. The gate requires evidence that `"type": "integer"` is asserted for all nine fields.

---

### R-10: Existing `test_retrospective_params_evidence_limit` Regression
**Severity**: Low
**Likelihood**: Low
**Impact**: The existing test in `tools.rs` uses `serde_json::from_str` on `RetrospectiveParams`. Adding `#[serde(default, deserialize_with = "...")]` to `evidence_limit` changes the deserialization behavior for that field. If the existing test passes a string-encoded `evidence_limit` that previously worked as an integer but now triggers the new helper, the test could break unexpectedly. NFR-02 requires all existing tests pass without modification.

**Test Scenarios**:
1. Run existing `test_retrospective_params_evidence_limit` unmodified after applying annotations — must pass. AC-11.
2. Inspect the existing test to confirm it passes `evidence_limit` as an integer (not a string) — if so, behavior is unchanged.

**Coverage Requirement**: No new test needed. Regression check is `cargo test --workspace` after implementation. AC-11.

---

## Integration Risks

**rmcp `Parameters<T>` transparent delegation**: The architecture relies on `Parameters<T>`
being `#[serde(transparent)]` so that `#[serde(deserialize_with)]` on `T`'s fields is fully
respected. If rmcp 0.16.0 performs any pre-processing of arguments before delegating to
`serde_json::from_value`, the coercion would not fire. This is the core premise of the
feature — confirmed from rmcp source but not verified by a passing test until AC-13 exists
(see R-02).

**Five structs across 8+ tools**: The nine field annotations span five structs. A regression
in any one helper function (e.g., `deserialize_opt_i64_or_string`) silently corrupts
optional parameters across `context_lookup`, `context_search`, `context_briefing` — tools
used heavily in every agent session. The `None`-for-absent coverage (R-01) is the primary
guard.

**Validation layer unchanged**: `validated_id`, `validated_k`, `validated_limit`,
`validated_max_tokens` in `infra/validation.rs` receive the already-coerced value. No
integration risk at this boundary — but the tester must confirm `evidence_limit` (which has
no `validated_*` function) flows through `.unwrap_or(3)` correctly for both `None` and
`Some(usize)` inputs.

---

## Edge Cases

1. **String `"0"`**: Valid integer string; must produce `Some(0usize)` for `evidence_limit` (AC-06-ZERO) and `0i64` for required fields.
2. **String `"-5"` for `i64` fields**: Valid negative integer; must produce `-5i64` (not an error). Negative values are in range for `i64`.
3. **String `"-1"` for `evidence_limit`**: Invalid for `usize`; must produce a serde error (AC-09).
4. **Very large string for `i64` field** (e.g., `"9999999999999999999999"`): Must produce a serde error when it overflows `i64::MAX` during `str::parse`.
5. **Whitespace-padded string** (e.g., `" 42 "`): `str::parse::<i64>()` rejects leading/trailing whitespace. Behavior must be error, not coerce to `42`. Verify this is consistent with FR-11.
6. **JSON boolean `true`**: Not a Number or String. Visitor's `visit_bool` path — must return a serde error (C-05).
7. **JSON array or object**: Must return a serde error (C-05).
8. **`i64::MAX` as a string** (`"9223372036854775807"`): Must parse successfully.
9. **`i64::MIN` as a string** (`"-9223372036854775808"`): Must parse successfully for `i64` fields; must fail for `evidence_limit` (negative).

---

## Security Risks

**Untrusted input surface**: All nine affected fields accept input from MCP clients, which in production are agent processes. The coercion helpers operate on client-supplied strings.

- **`deserialize_i64_or_string`**: Calls `str::parse::<i64>()` on client-supplied `&str`. `str::parse` does not allocate for failure and returns `Err` on invalid input — no injection risk. The parsed `i64` value is passed to `validated_id` which enforces further range constraints.
- **`deserialize_opt_usize_or_string`**: Calls `str::parse::<u64>()` on client-supplied input, then `usize::try_from`. Both are safe operations with bounded output. The `u64` intermediate prevents negative value injection.
- **Blast radius if a helper is compromised**: A buggy helper that silently coerces arbitrary strings to valid integers could cause lookups or mutations of unintended entries. The scope of damage is limited to valid entry IDs in the caller's session — no path traversal or SQL injection risk (IDs flow through `validated_id` → typed store queries).
- **Float string injection** (e.g., `"3.5"`): Rejected by `str::parse::<i64>()`. No coercion to `3` or `4`.
- **Oversized string input**: `str::parse` on a very long string does not allocate beyond the borrowed `&str` — processing is O(n) scan with early exit on first invalid char. No amplification risk.
- **No new deserialization surface for non-numeric fields**: FR-11 and C-07 explicitly prohibit extending coercion to string-typed fields such as `format`, `category`, `status`. This prevents coercion from becoming a broad injection vector.

---

## Failure Modes

| Scenario | Expected Behavior |
|----------|------------------|
| Non-numeric string for any field | `serde::de::Error::custom("invalid digit...")` — rmcp wraps as `ErrorData::invalid_params` |
| Float string `"3.5"` for integer field | `serde::de::Error::custom` — not coerced, not panic |
| Float Number `3.0` for integer field | `serde::de::Error` from unimplemented `visit_f64` — not panic |
| Negative string for `evidence_limit` | `serde::de::Error::custom` — string rejected at `u64` parse stage |
| `u64` overflow string for `evidence_limit` | `serde::de::Error::custom` — `str::parse::<u64>()` fails |
| Missing optional field (absent key) | `None` — `#[serde(default)]` provides default without invoking Visitor |
| `null` value for optional field | `None` — Visitor's `visit_none`/`visit_unit` returns `Ok(None)` |
| `serde_util` module path string typo | `cargo build` fails with macro resolution error — caught at build, not runtime |
| Server receives any of the above | MCP error response to caller — no panic, no process crash |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (schema type unchanged) | R-05 | ADR-002 mandates `#[schemars(with = "T")]`; AC-10 schema snapshot test is the enforcement gate |
| SR-02 (serde Visitor greenfield trap — null/absent/overflow) | R-01, R-03, R-04 | ADR-004 mandates explicit tests per optional field for null and absent paths; spec enumerates AC-03-NULL, AC-03-ABSENT, etc. |
| SR-03 (no integration test for rmcp dispatch path) | R-02 | ADR-003 mandates IT-01 and IT-02 in infra-001 `test_tools.py`; Rust AC-13 covers in-process dispatch. OQ-04 remains open — implementation agent must resolve `RequestContext` constructibility |
| SR-04 (non-numeric fields still fail after fix) | — | Accepted out of scope. Architecture doc documents the remaining failure surface. GH #448 follow-up tracks non-numeric field coercion |
| SR-05 (absent-field returns `Some(0)` not `None`) | R-01, R-03 | ADR-004 mandates `#[serde(default)]` on all five optional fields and 20+ explicit tests |
| SR-06 (test fixtures using string ids cause self-inflicted failures) | — | ADR-003 requires test fixtures to use integer ids obtained from prior `context_store` calls |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 5 absent-field tests + IT-01/IT-02 infra-001 + AC-13 Rust integration |
| High | 4 (R-03, R-04, R-05, R-06) | 5 null-field tests + 3 usize boundary tests + 1 schema snapshot + 2 float rejection tests |
| Medium | 4 (R-07, R-08, R-09, R-10) | Build verification + 9 non-numeric rejection tests + make_server visibility check + 1 regression check |
| Low | 0 | — |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection serde deserialization" — found entry #885 (serde types need explicit test coverage; gate failure on col-020) and entry #3786 (MCP tool param deserialization fixes require transport-level validation, unit tests insufficient). Both directly informed R-02 severity and the mandatory integration test requirement.
- Queried: `/uni-knowledge-search` for "risk pattern integration test coverage MCP transport" — found entry #3526 (infra-001 is the correct vehicle for JSON Schema boundary risk). Confirmed ADR-003 approach.
- Queried: `/uni-knowledge-search` for "serde Visitor null absent optional field" — found entry #3557 (dual-direction serde test pattern) and entry #3548 (test exists but omits assertion — coverage weaker than specified). Informed R-01 and R-03 severity.
- Stored: nothing novel to store — R-01/R-03 pattern (missing `#[serde(default)]` on optional fields with `deserialize_with`) is specific to this feature's greenfield implementation; not yet recurring across 2+ features to warrant a stored pattern.
