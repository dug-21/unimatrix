# SPECIFICATION: vnc-012 — Server-Side Coercion of String-Encoded Integers for MCP Parameters

## Objective

Agents frequently emit MCP tool call arguments with integer parameters encoded as JSON
strings (e.g., `"id": "3770"` instead of `"id": 3770`). The rmcp 0.16.0 deserializer
is strict and rejects these with `invalid type: string "3770", expected i64`, returning
an MCP error to the caller. This feature adds three deserializer helpers and applies them
to all nine numeric fields across five parameter structs in `unimatrix-server`, so that
both integer and string representations are accepted without altering JSON Schema output
or any downstream validation logic.

---

## Functional Requirements

FR-01: A new module `crates/unimatrix-server/src/mcp/serde_util.rs` must contain exactly
three `pub(crate)` deserializer functions: `deserialize_i64_or_string`,
`deserialize_opt_i64_or_string`, and `deserialize_opt_usize_or_string`. No other
deserialization helpers may be added as part of this feature.

FR-02: `deserialize_i64_or_string` must accept a `serde::Deserializer` and produce an
`i64` from either a JSON Number (integer) or a JSON String containing a base-10 integer
literal. Any other input type (float, boolean, object, array) must produce a serde
deserialization error.

FR-03: `deserialize_opt_i64_or_string` must accept a `serde::Deserializer` and produce
`Option<i64>`. A JSON `null` value must produce `None`. An absent field (when
`#[serde(default)]` is combined with this deserializer) must produce `None`. A JSON
Number or String containing an integer literal must produce `Some(i64)`.

FR-04: Null and absent must be handled as two distinct paths in
`deserialize_opt_i64_or_string`:
- Null: the JSON value `null` is present and must deserialize to `None`.
- Absent: the field is entirely missing from the JSON object. Because serde calls the
  deserializer for missing fields only when `#[serde(default)]` is present, the default
  implementation (`Default::default()`) must return `None`. The Visitor must not be
  invoked for absent fields; this is enforced by pairing `#[serde(default)]` with
  `#[serde(deserialize_with)]` on every optional field in the affected structs.

FR-05: `deserialize_opt_usize_or_string` must accept a `serde::Deserializer` and produce
`Option<usize>`. It must parse via `u64` first, then convert to `usize` using
`usize::try_from(val_u64)`, not `as usize`. Negative string inputs (e.g., `"-1"`) must
produce a serde error. Strings that parse as values exceeding `u64::MAX` must produce a
serde error.

FR-06: `serde_util.rs` must be declared as a module in
`crates/unimatrix-server/src/mcp/mod.rs` with `mod serde_util;` (private visibility to
the `mcp` module).

FR-07: The following nine fields in `crates/unimatrix-server/src/mcp/tools.rs` must be
annotated with paired `#[serde(deserialize_with)]` and `#[schemars(with)]` attributes:

| Struct               | Field          | Type          | deserialize_with                    | schemars with       |
|----------------------|----------------|---------------|-------------------------------------|---------------------|
| `GetParams`          | `id`           | `i64`         | `deserialize_i64_or_string`         | `"i64"`             |
| `DeprecateParams`    | `id`           | `i64`         | `deserialize_i64_or_string`         | `"i64"`             |
| `QuarantineParams`   | `id`           | `i64`         | `deserialize_i64_or_string`         | `"i64"`             |
| `CorrectParams`      | `original_id`  | `i64`         | `deserialize_i64_or_string`         | `"i64"`             |
| `LookupParams`       | `id`           | `Option<i64>` | `deserialize_opt_i64_or_string`     | `"Option<i64>"`     |
| `LookupParams`       | `limit`        | `Option<i64>` | `deserialize_opt_i64_or_string`     | `"Option<i64>"`     |
| `SearchParams`       | `k`            | `Option<i64>` | `deserialize_opt_i64_or_string`     | `"Option<i64>"`     |
| `BriefingParams`     | `max_tokens`   | `Option<i64>` | `deserialize_opt_i64_or_string`     | `"Option<i64>"`     |
| `RetrospectiveParams`| `evidence_limit`| `Option<usize>`| `deserialize_opt_usize_or_string` | `"Option<u64>"`     |

FR-08: Every optional field in FR-07 that uses `deserialize_opt_i64_or_string` or
`deserialize_opt_usize_or_string` must also carry `#[serde(default)]` to ensure absent
fields produce `None` without invoking the Visitor. Fields that already carry
`#[serde(default)]` must not have it removed.

FR-09: No changes may be made to `crates/unimatrix-server/src/infra/validation.rs`.
The functions `validated_id`, `validated_k`, `validated_limit`, and `validated_max_tokens`
receive the already-coerced `i64` value and require no modification. The `evidence_limit`
field has no `validated_*` function; its handler uses `.unwrap_or(3)` directly, which
is unchanged.

FR-10: No new crate-level dependencies may be introduced. The helpers must use only
`serde::Deserializer`, `serde::de::Visitor`, and `serde::de::Error` from the existing
`serde` dependency.

FR-11: String inputs that represent non-numeric text (e.g., `"abc"`, `"3.5"`, `""`)
must cause `deserialize_i64_or_string` and `deserialize_opt_i64_or_string` to return a
`serde::de::Error::custom` message. They must not panic and must not silently coerce to
zero or any default value.

FR-12: Float strings (e.g., `"3.5"`) must be rejected as non-numeric for integer fields.
Only base-10 integer strings accepted by `str::parse::<i64>()` are valid.

FR-13: Float JSON Numbers (e.g., the JSON value `3.0` as a Number type, distinct from the
string `"3.0"`) passed to any integer or usize field must be rejected with a serde error.
The Visitor implementations must implement `visit_f64` (and `visit_f32`) to return
`de::Error::invalid_type(de::Unexpected::Float(v), &self)`. Silent truncation (e.g.,
treating `3.0` as `3`) is forbidden because the schema advertises `type: integer` and
accepting float Numbers would invite ambiguity for downstream callers.

---

## Non-Functional Requirements

NFR-01: The three deserializer functions must be zero-allocation for the happy path
(integer input). String inputs allocate only during `str::parse` on the borrowed `&str`.

NFR-02: All existing tests in `tools.rs` and `infra/validation.rs` must pass without
modification. The baseline test count is 2169 unit tests; new tests must be additive.

NFR-03: `cargo clippy --workspace -- -D warnings` must pass with no new warnings. The
Visitor implementations must not trigger `clippy::needless_pass_by_value` or similar
lints.

NFR-04: `cargo fmt` must produce no diff after implementation.

NFR-05: The JSON Schema advertised by the MCP server for all nine affected fields must
retain `type: integer`. The `#[schemars(with = "Option<u64>")]` annotation on
`evidence_limit` may emit `minimum: 0` — this is acceptable because `usize` is
non-negative by definition. No other schema changes beyond `minimum: 0` on
`evidence_limit` are permitted.

NFR-06: No changes may be made to rmcp 0.16.0 (pinned at `version = "=0.16.0"`).

---

## Acceptance Criteria

### Deserialization — Required Integer Fields

**AC-01** [unit] `GetParams`, `DeprecateParams`, `QuarantineParams` each deserialize
successfully from `{"id": "3770"}`, producing `id == 3770i64`. Verification: unit tests
in the `tools.rs` test block, one test per struct, asserting the field value.

**AC-02** [unit] `CorrectParams` deserializes successfully from
`{"original_id": "3770", "content": "c"}`, producing `original_id == 3770i64`.
Verification: unit test in the `tools.rs` test block.

**AC-07** [unit] All four required-integer fields continue to deserialize correctly when
passed as JSON integers (`{"id": 42}`). Verification: one unit test per struct asserting
both forms produce the same value.

### Deserialization — Optional Integer Fields

**AC-03** [unit] `LookupParams` deserializes `{"id": "42"}` to `id == Some(42i64)` and
`{"limit": "10"}` to `limit == Some(10i64)`.
Verification: two unit tests, one per optional field.

**AC-03-ABSENT-ID** [unit] `LookupParams` with no `id` key in JSON produces
`id == None`. Verification: dedicated unit test asserting `params.id.is_none()`. (SR-02)

**AC-03-ABSENT-LIMIT** [unit] `LookupParams` with no `limit` key in JSON produces
`limit == None`. Verification: dedicated unit test asserting `params.limit.is_none()`.
(SR-02)

**AC-03-NULL-ID** [unit] `LookupParams` with `{"id": null}` produces `id == None`.
Verification: dedicated unit test asserting `params.id.is_none()`. This is distinct
from the absent case: the key is present with a null value. (SR-02)

**AC-03-NULL-LIMIT** [unit] `LookupParams` with `{"limit": null}` produces
`limit == None`. Verification: dedicated unit test. (SR-02)

**AC-04** [unit] `SearchParams` deserializes `{"query": "q", "k": "5"}` to
`k == Some(5i64)`.
Verification: unit test.

**AC-04-ABSENT** [unit] `SearchParams` with `{"query": "q"}` (no `k` key) produces
`k == None`. Verification: dedicated unit test. (SR-02)

**AC-04-NULL** [unit] `SearchParams` with `{"query": "q", "k": null}` produces
`k == None`. Verification: dedicated unit test. (SR-02)

**AC-05** [unit] `BriefingParams` deserializes `{"task": "t", "max_tokens": "3000"}` to
`max_tokens == Some(3000i64)`.
Verification: unit test.

**AC-05-ABSENT** [unit] `BriefingParams` with `{"task": "t"}` (no `max_tokens` key)
produces `max_tokens == None`. Verification: dedicated unit test. (SR-02)

**AC-05-NULL** [unit] `BriefingParams` with `{"task": "t", "max_tokens": null}` produces
`max_tokens == None`. Verification: dedicated unit test. (SR-02)

**AC-06** [unit] `RetrospectiveParams` deserializes
`{"feature_cycle": "col-001", "evidence_limit": "5"}` to `evidence_limit == Some(5usize)`.
Verification: unit test.

**AC-06-ZERO** [unit] `RetrospectiveParams` with `evidence_limit: "0"` produces
`evidence_limit == Some(0usize)`. Verification: unit test.

**AC-06-ABSENT** [unit] `RetrospectiveParams` with no `evidence_limit` key produces
`evidence_limit == None`. Verification: dedicated unit test. (SR-02)

**AC-06-NULL** [unit] `RetrospectiveParams` with `{"feature_cycle": "col-001", "evidence_limit": null}`
produces `evidence_limit == None`. Verification: dedicated unit test. (SR-02)

### Rejection Behavior

**AC-08** [unit] Non-numeric strings (e.g., `"abc"`) passed for any required integer
field produce a serde deserialization error (not a panic, not `Some(0)`).
Verification: one unit test per required-integer struct asserting `is_err()`.

**AC-08-OPT** [unit] Non-numeric strings passed for any optional integer field produce a
serde deserialization error.
Verification: one unit test per optional-integer field asserting `is_err()`.

**AC-09** [unit] Negative strings (e.g., `"-1"`) passed for `evidence_limit` produce a
serde deserialization error.
Verification: unit test asserting `serde_json::from_str::<RetrospectiveParams>(...)`.is_err()`.

**AC-09-FLOAT** [unit] Float strings (e.g., `"3.5"`) passed for any integer field (both
required and optional) produce a serde deserialization error. Verification: unit tests.

**AC-09-FLOAT-NUMBER** [unit] Float JSON Numbers (e.g., `{"id": 3.0}` where `3.0` is a
JSON Number, not a string) passed for any integer field produce a serde deserialization
error. Verification: unit tests using `serde_json::from_str` with numeric float literals
(e.g., `r#"{"id": 3.0}"#`). Tests must confirm `visit_f64` returns an error, not a
truncated integer. (FR-13)

### JSON Schema Invariants

**AC-10** [schema-snapshot] The JSON Schema advertised by `ToolRouter::list_all()` for
all nine affected fields retains `type: integer`. No affected field schema changes to
`type: string` or `oneOf`. Verification: unit test that constructs a
`UnimatrixServer` via `server::tests::make_server()`, calls
`server.tool_router.list_all()`, extracts the `input_schema` for each affected tool, and
asserts `type == "integer"` for the affected properties.

### Regression

**AC-11** [unit] All existing tests in `tools.rs` and `infra/validation.rs` pass without
modification. Verification: `cargo test --workspace` produces no regression.

### Deserializer Unit Coverage

**AC-12** [unit] The three deserializer functions in `serde_util.rs` are covered by unit
tests in `serde_util.rs` or the `tools.rs` test block for: integer input, string input,
non-numeric string rejection, null (`None`) for optional variants, and absent (`None`)
for optional variants.

### Integration Test — MCP Dispatch Path (SR-03)

**AC-13** [integration] An integration test in `crates/unimatrix-server/src/server.rs`
or a dedicated file (e.g., `crates/unimatrix-server/tests/mcp_coercion.rs`) must:

1. Construct a `UnimatrixServer` via the `make_server()` helper (or equivalent).
2. Insert a test entry into the store and obtain its `u64` id.
3. Call `rmcp::ServerHandler::call_tool` on the server with a
   `CallToolRequestParams` where:
   - `name` is `"context_get"`
   - `arguments` contains `{"id": "<id-as-string>", "agent_id": "human"}` (the id
     field is a JSON String, not a Number)
4. Assert the result is `Ok(_)` (not an `Err` containing
   `"invalid type: string"`).
5. Assert the returned content is non-empty.

This test exercises the full rmcp `Parameters<T>` deserialization path
(`serde_json::from_value(serde_json::Value::Object(arguments))`), which is the exact
path where the bug occurs and which unit tests in `tools.rs` do not cover. The test must
use a `RequestContext` constructed with a no-op peer handle or the pattern already
established in `server.rs` tests.

Verification: `cargo test --workspace` passes; the test name must include `coercion` or
`string_id` to make its purpose findable.

**IT-01** [integration/infra-001] A Python infra-001 integration test `test_get_with_string_id`
must call the `context_get` MCP tool over the stdio transport with a string-encoded id
(e.g., `{"id": "<id-as-string>", "agent_id": "human"}`), assert the tool returns success,
and assert the returned content is non-empty. This test exercises the full rmcp dispatch
path including JSON-RPC framing over stdio — the exact path where the live bug fires.
Must be marked `@pytest.mark.smoke`.

**IT-02** [integration/infra-001] A Python infra-001 integration test `test_deprecate_with_string_id`
must call `context_deprecate` with a string-encoded id over stdio, assert success.
Must be marked `@pytest.mark.smoke`.

Both IT-01 and IT-02 are required in addition to AC-13. They cover the transport layer
that AC-13's Rust test cannot exercise directly.

---

## Domain Models

**MCP parameter struct**: A Rust struct that derives `Deserialize` and `JsonSchema`,
annotated with `#[rmcp::tool(...)]` parameters and deserialized from the `arguments`
field of an MCP `CallToolRequest` JSON-RPC message by rmcp's
`Parameters<T>: FromContextPart<ToolCallContext<'_, S>>` implementation.

**String-encoded integer**: A JSON String value whose content is a valid base-10 integer
literal accepted by `str::parse::<i64>()` (e.g., `"3770"`, `"0"`, `"-5"`). Does not
include floats, hex literals, or whitespace-padded strings.

**Coercion**: The act of accepting a string-encoded integer where a JSON Number is
specified by the schema. Coercion occurs at deserialization time and is transparent to
validation and business logic layers.

**Absent field**: A JSON object that does not contain the key at all. Distinguished from
a null field (key present, value `null`) at the serde Visitor level.

**Null field**: A JSON object containing the key with value `null`. Must deserialize to
`None` for optional fields via the `visit_none` / `visit_unit` Visitor method.

**`serde_util` module**: A private submodule of `mcp` at
`crates/unimatrix-server/src/mcp/serde_util.rs`. Contains only deserializer helper
functions scoped to MCP parameter struct fields. Not a crate-level utility.

**rmcp dispatch path**: The code path from an MCP JSON-RPC `tools/call` request, through
`rmcp::ServerHandler::call_tool`, through `ToolRouter::call`, through
`Parameters<T>: FromContextPart`, to `serde_json::from_value(Value::Object(arguments))`.

---

## User Workflows

**Agent emits string-encoded id (fixed path):**
1. Agent calls `context_get` with `{"id": "3770", "agent_id": "agent-x"}`.
2. rmcp deserializes arguments via `serde_json::from_value`.
3. `GetParams::id` field uses `deserialize_i64_or_string`; the string `"3770"` is
   parsed to `3770i64`.
4. Deserialization succeeds; handler proceeds with `params.id == 3770`.
5. Agent receives the entry content.

**Agent emits correctly-typed integer (unchanged path):**
1. Agent calls `context_get` with `{"id": 3770, "agent_id": "agent-x"}`.
2. Deserialization proceeds as before; `deserialize_i64_or_string` accepts the JSON
   Number and delegates to the standard integer visitor.
3. No behavioral change.

**Agent emits non-numeric string (rejection):**
1. Agent calls `context_get` with `{"id": "abc"}`.
2. `deserialize_i64_or_string` calls `str::parse::<i64>()` on `"abc"`, which fails.
3. A serde error is returned; rmcp wraps it as `ErrorData::invalid_params(...)`.
4. Agent receives an MCP error response indicating the parameter is invalid.

---

## Constraints

**C-01 (pinned dependency)**: rmcp is pinned at `version = "=0.16.0"`. No version bump.
The deserialization entry point is `serde_json::from_value(serde_json::Value::Object(arguments))`
in `rmcp::handler::server::tool::FromContextPart for Parameters<P>`. This is confirmed
from rmcp source and must not be assumed to change.

**C-02 (no new crate dependencies)**: The implementation must use only `serde` and
`serde_json`, which are already dependencies of `unimatrix-server`.

**C-03 (null vs. absent distinction)**: The `deserialize_opt_i64_or_string` and
`deserialize_opt_usize_or_string` helpers must handle null and absent as two distinct
code paths:
- Null: the serde Visitor receives a `visit_none()` or `visit_unit()` call when the
  JSON value is `null`. Must return `Ok(None)`.
- Absent: serde does not invoke the Visitor when the field is missing; instead, the
  field's `Default::default()` is used (which is `None` for `Option<T>`). This requires
  `#[serde(default)]` on every optional field using these helpers.
  Failure to pair `#[serde(default)]` with `#[serde(deserialize_with)]` causes serde to
  emit a "missing field" error for absent optional fields. This is a greenfield trap
  (SR-02) and must be explicitly tested.

**C-04 (schema type must remain integer)**: The `#[schemars(with = "i64")]` and
`#[schemars(with = "Option<i64>")]` annotations must produce a schema fragment
equivalent to what the plain `i64` / `Option<i64>` fields produced before this feature.
The `#[schemars(with = "Option<u64>")]` annotation on `evidence_limit` may emit
`minimum: 0`; this is acceptable.

**C-05 (no coercion of non-numeric types)**: The helpers must not coerce float JSON
Numbers (e.g., `3.5`), booleans, arrays, or objects. Only JSON String and JSON Integer
Number inputs are valid; all others must return a serde error.

**C-06 (usize overflow safety)**: `deserialize_opt_usize_or_string` must use
`usize::try_from(val_u64)` for the u64-to-usize conversion. The `as usize` cast is
forbidden because it silently truncates on 32-bit targets.

**C-07 (scope boundary)**: String coercion must not be applied to non-numeric fields
(`format`, `agent_id`, `status`, `category`, `query`, `topic`, `feature`, `content`,
`reason`, `tags`). No change may be made to `infra/validation.rs`.

**C-08 (module placement)**: Helpers live in `mcp/serde_util.rs` (private submodule of
`mcp`). They must not be promoted to a crate-level `serde_util` or shared across crates
as part of this feature.

---

## Dependencies

| Dependency | Version | Role |
|------------|---------|------|
| `serde` | workspace (derive feature) | `Deserializer`, `Visitor`, `de::Error` traits used in `serde_util.rs` |
| `serde_json` | workspace | Used in tests (`serde_json::from_str`) and in the rmcp dispatch path |
| `schemars` | 1.2.1 | `#[schemars(with = "T")]` attribute for schema override |
| `rmcp` | =0.16.0 | `Parameters<T>`, `ToolCallContext`, `CallToolRequestParams`, `ServerHandler::call_tool` |
| `unimatrix-server` (existing) | — | `UnimatrixServer`, `tools.rs` structs, `server::tests::make_server()` |
| `unimatrix-store` | workspace | `NewEntry`, `Store::insert` used in AC-13 integration test setup |

---

## NOT In Scope

- String coercion for non-numeric fields (`format`, `agent_id`, `category`, `status`,
  `query`, `topic`, `content`, `reason`, `tags`, `feature`, `title`, `source`).
- Changes to `infra/validation.rs` — `validated_id`, `validated_k`, `validated_limit`,
  `validated_max_tokens` are unchanged.
- Coercion of float strings (e.g., `"3.5"`) to integer fields.
- Coercion of boolean parameters.
- A new `serde_util` crate or any shared cross-crate utility.
- Version bump of rmcp.
- Updates to agent definitions (`.claude/agents/`), protocol files
  (`.claude/protocols/`), or `CLAUDE.md` (tracked as GH #448 follow-up).
- Schema changes beyond the `minimum: 0` permitted on `evidence_limit`.
- Coercion of JSON float Number types that are passed as numbers but happen to equal an
  integer (e.g., `3.0` as Number) — these are rejected per FR-13, not coerced.

---

## Open Questions

**OQ-01 (resolved)**: `#[schemars(with = "i64")]` approach — resolved in SCOPE.md
Open Question 1. Use `#[schemars(with = "i64")]` for required, `#[schemars(with = "Option<i64>")]`
for optional i64, `#[schemars(with = "Option<u64>")]` for `evidence_limit`.

**OQ-02 (resolved)**: Module placement — resolved in SCOPE.md Open Question 2. Use
`mcp/serde_util.rs` submodule.

**OQ-03 (resolved)**: usize overflow — resolved in SCOPE.md Open Question 3. Use
`usize::try_from(val_u64)`.

**OQ-04 (for architect)**: The AC-13 integration test requires constructing a
`RequestContext<RoleServer>` to pass to `call_tool`. The existing `server.rs` tests use
`rmcp::ServerHandler::get_info` which does not require a `RequestContext`. The architect
must confirm how to construct a minimal `RequestContext` for tests — either from rmcp's
public API or by using the `tool_router` field directly with a `ToolCallContext`.
If `tool_router` is private and `RequestContext` is not constructible in tests, the
architect must propose an alternative integration test vehicle (e.g., exposing a
`pub(crate) fn call_tool_for_test` helper on `UnimatrixServer`).

**OQ-05 (resolved)**: Float JSON Numbers (e.g., `3.0` as a Number type) must be **rejected
strictly**. `visit_f64` and `visit_f32` must return `de::Error::invalid_type(...)`. The
schema advertises `type: integer`; accepting `3.0` as `3` would invite ambiguity and
violate the schema contract. Codified as FR-13 and AC-09-FLOAT-NUMBER.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 20 entries; entries #3786
  (integration test requirement for deserialization fixes) and #3784
  (schemars `with` + `deserialize_with` pairing pattern) were directly relevant and
  confirmed the spec approach. Entry #3786 confirmed that AC-13 (infra-level integration
  test over MCP transport) is the established requirement pattern for this class of fix.
  MCP tool calls to retrieve full entry content failed with the exact bug this feature
  fixes (`invalid type: string "3786", expected i64`), confirming the bug is live on
  the current server.
