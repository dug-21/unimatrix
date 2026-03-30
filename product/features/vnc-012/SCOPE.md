# vnc-012: Accept String-Encoded Integers for All Numeric MCP Parameters

## Problem Statement

Agents frequently emit MCP tool call arguments with integer parameters quoted as JSON
strings (e.g., `"id": "3770"` instead of `"id": 3770`). This happens because agents
construct tool call payloads from LLM-generated or templated text, where numeric values
are commonly stringified. The rmcp deserialization layer is strict: it uses
`serde_json::from_value` with typed Rust structs, so a JSON string where an integer is
expected fails immediately with:

```
invalid type: string "3770", expected i64
```

The failure returns an MCP error to the calling agent. No parameter coercion occurs.
This is a recurring breakage observed across agent sessions for `context_get`,
`context_deprecate`, `context_quarantine`, `context_correct`, `context_lookup`,
`context_search`, `context_briefing`, and `context_cycle_review`.

## Goals

1. Add two module-level serde deserializer helpers (`deserialize_i64_or_string`,
   `deserialize_opt_i64_or_string`) and a `usize` variant for `evidence_limit` that
   accept both JSON integer and JSON string representations of numbers.
2. Apply `#[serde(deserialize_with = "...")]` to all nine affected fields in the five
   affected parameter structs in `tools.rs`.
3. Preserve JSON Schema output for each field as `type: integer` — coercion operates
   at deserialization only, not at schema advertisement.
4. Add unit tests in the `tools.rs` test module covering: integer input (unchanged
   behavior), string input (new acceptance), negative string (rejected), non-numeric
   string (rejected), null/absent optional (still None).

## Non-Goals

- This feature does not add string coercion to non-numeric fields (e.g., `format`,
  `agent_id`, `status`, `category`).
- This feature does not change any validation logic in `infra/validation.rs` —
  `validated_id`, `validated_k`, `validated_limit`, `validated_max_tokens` remain
  unchanged.
- This feature does not update any agent definitions, protocol files, or CLAUDE.md
  instructions (GH #448 explicitly notes these are out of scope).
- This feature does not introduce a new `serde_util` crate or shared library — helpers
  live in `tools.rs` or its immediate module, scoped to MCP parameter structs.
- This feature does not coerce float strings (e.g., `"3.5"`) to integer fields.
- This feature does not change how `bool` parameters are handled.

## Background Research

### rmcp deserialization path
`rmcp 0.16.0` uses `serde_json::from_value(serde_json::Value::Object(arguments))` to
deserialize tool call arguments into the `Parameters<T>` wrapper (transparent serde).
The `Parameters<T>` struct is `#[serde(transparent)]`, so deserialization delegates
directly to `T`. A `#[serde(deserialize_with = "...")]` annotation on a field in `T`
is therefore fully respected. No changes to the rmcp layer are required.

### schemars 1.2.1 schema override
The project uses `schemars = "1"` (resolved to 1.2.1). When a field uses
`#[serde(deserialize_with = "...")]`, schemars can no longer infer the schema type.
Two approaches are available in schemars 1.x, both verified from schemars source:

- `#[schemars(with = "i64")]` — type shorthand; generates the same schema as `i64`
  with no function required. Preferred for simple type-identity overrides.
- `#[schemars(schema_with = "fn_name")]` — calls `fn(&mut SchemaGenerator) -> Schema`;
  needed only when the schema differs from any existing type's schema.

For this feature, `#[schemars(with = "i64")]` on required `i64` fields and
`#[schemars(with = "Option<i64>")]` on optional `i64` fields is the correct and
simplest approach. For `evidence_limit: Option<usize>`, `#[schemars(with = "Option<u64>")]`
produces a non-negative integer schema. Open Question 1 is resolved.

### Affected structs and fields
All nine affected fields are in `crates/unimatrix-server/src/mcp/tools.rs`:

| Struct | Field | Type | Required? |
|--------|-------|------|-----------|
| `GetParams` | `id` | `i64` | yes |
| `DeprecateParams` | `id` | `i64` | yes |
| `QuarantineParams` | `id` | `i64` | yes |
| `CorrectParams` | `original_id` | `i64` | yes |
| `LookupParams` | `id` | `Option<i64>` | no |
| `LookupParams` | `limit` | `Option<i64>` | no |
| `SearchParams` | `k` | `Option<i64>` | no |
| `BriefingParams` | `max_tokens` | `Option<i64>` | no |
| `RetrospectiveParams` | `evidence_limit` | `Option<usize>` | no |

### evidence_limit type difference
`evidence_limit` is typed `Option<usize>` rather than `Option<i64>`. This requires a
third helper `deserialize_opt_usize_or_string` that parses to `u64` first and converts
to `usize`, with overflow/negative rejection. The validation behavior (no existing
`validated_*` function for this field; the handler uses `.unwrap_or(3)` directly) is
unchanged.

### No existing coercion patterns in codebase
A search across all crates found zero uses of `deserialize_with`, `serde_util`, or
any string-to-integer coercion helpers. This is greenfield for the project.

### No new dependencies needed
`serde` (with `derive` feature) and `serde_json` are already dependencies of
`unimatrix-server`. The helpers use only `serde::Deserializer` and
`serde::de::{self, Visitor}` — both available from the existing `serde` dep.

### Placement decision
The issue proposes either in-module helpers in `tools.rs` or a shared `serde_util`
module. Given that no other file in the crate currently deserializes tool parameters,
and the helpers are small pure functions, placing them in a private `serde_util` submodule
within `crates/unimatrix-server/src/mcp/` keeps `tools.rs` clean and allows future reuse
within the `mcp` module namespace without becoming a crate-level dependency.

### Test infrastructure pattern
The existing test module in `tools.rs` uses `serde_json::from_str` directly on param
structs (e.g., `test_retrospective_params_evidence_limit`). The same pattern applies
for the new coercion tests.

## Proposed Approach

1. Create `crates/unimatrix-server/src/mcp/serde_util.rs` with three public(crate) fns:
   - `deserialize_i64_or_string` — accepts JSON Number or String, parses string via
     `str::parse::<i64>()`, rejects non-numeric strings with a serde error.
   - `deserialize_opt_i64_or_string` — wraps the above for `Option<i64>` fields; absent
     or null fields deserialize to `None`.
   - `deserialize_opt_usize_or_string` — for `evidence_limit: Option<usize>`; parses
     via `u64` to catch negatives, then `as usize` (or range check on 32-bit targets).

2. Expose the module in `crates/unimatrix-server/src/mcp/mod.rs`.

3. In `tools.rs`, annotate the nine affected fields with paired attributes:
   - Required `i64` fields: `#[serde(deserialize_with = "serde_util::deserialize_i64_or_string")]`
     plus `#[schemars(with = "i64")]`
   - Optional `i64` fields: `#[serde(deserialize_with = "serde_util::deserialize_opt_i64_or_string")]`
     plus `#[schemars(with = "Option<i64>")]`
   - `evidence_limit`: `#[serde(deserialize_with = "serde_util::deserialize_opt_usize_or_string")]`
     plus `#[schemars(with = "Option<u64>")]`

4. Add unit tests in the `#[cfg(test)]` block of `tools.rs` for each affected struct,
   covering both integer and string inputs.

**Rationale for serde_util submodule over inline helpers:** Keeps `tools.rs` focused on
tool handler code; the module boundary makes it obvious the helpers are reusable within
the `mcp` module; consistent with the existing `response/` submodule pattern.

## Acceptance Criteria

- AC-01: `GetParams`, `DeprecateParams`, `QuarantineParams` each deserialize
  successfully when `id` is a JSON string (`"3770"`), producing the same `i64` value
  as the integer form.
- AC-02: `CorrectParams` deserializes successfully when `original_id` is a JSON string,
  producing the same `i64` value as the integer form.
- AC-03: `LookupParams` deserializes successfully when `id` is a JSON string or absent;
  `limit` is a JSON string or absent; both produce correct `Option<i64>` values.
- AC-04: `SearchParams` deserializes successfully when `k` is a JSON string or absent.
- AC-05: `BriefingParams` deserializes successfully when `max_tokens` is a JSON string
  or absent.
- AC-06: `RetrospectiveParams` deserializes successfully when `evidence_limit` is a
  JSON string (`"5"`) or absent; string `"0"` produces `Some(0)`.
- AC-07: All nine fields continue to deserialize correctly when passed as JSON integers
  (no regression for correctly-typed callers).
- AC-08: Non-numeric strings (e.g., `"abc"`) passed for any affected field result in a
  serde deserialization error (not a panic, not silent coercion to 0).
- AC-09: Negative strings (e.g., `"-1"`) passed for `evidence_limit` (usize target)
  result in a serde deserialization error.
- AC-10: JSON Schema advertised by the MCP server for all affected fields retains
  `type: integer` (not `type: string` or `oneOf`).
- AC-11: All existing tests in `tools.rs` and `infra/validation.rs` continue to pass
  without modification.
- AC-12: The three deserializer helpers are covered by unit tests in `serde_util.rs`
  or within the `tools.rs` test block.

## Constraints

- **rmcp 0.16.0 is pinned** (`version = "=0.16.0"`). No version bump is in scope.
  The deserialization path (`serde_json::from_value`) is confirmed from rmcp source.
- **schemars 1.2.1** is the resolved version. The `#[schemars(schema_with = "fn")]`
  attribute must produce a schema fragment identical to the default `i64` schema
  (`{"type": "integer"}`). The exact function signature required by schemars 1.x
  for `schema_with` must be verified during implementation.
- **No new crate-level dependencies** — the implementation must use only existing deps
  (`serde`, `serde_json`).
- **`evidence_limit` is `Option<usize>` not `Option<i64>`** — a separate deserializer
  helper is needed; it must handle platform `usize` width (64-bit in production, but
  the CI target must also pass).
- **Test count**: The project has 2169 unit tests + integration tests as baseline.
  New tests must be additive; no existing tests may be deleted or modified to pass.

## Open Questions

1. **schemars schema override approach — RESOLVED**: Use `#[schemars(with = "i64")]`
   for required `i64` fields, `#[schemars(with = "Option<i64>")]` for optional `i64`
   fields, and `#[schemars(with = "Option<u64>")]` for `evidence_limit` (unsigned,
   emits `minimum: 0`). Verified against schemars 1.2.1 source and examples.

2. **serde_util module vs. inline in tools.rs — RESOLVED**: Use `mcp/serde_util.rs`
   submodule. Three functions applied across 9 fields are reusable utilities, not
   one-offs. Consistent with the existing `response/` submodule precedent — keep
   `tools.rs` focused on struct definitions, push deserialization mechanics into their
   own module. `#[serde(deserialize_with)]` at the call site keeps intent readable
   without inlining the implementation.

3. **`usize` overflow on 32-bit — RESOLVED**: Use `usize::try_from(val_u64)` —
   no `as usize` cast. The conversion is correct by construction on any target.
   Silent truncation is a logic bug; the cost is one fallible conversion.

4. **Integration-level regression test**: AC coverage is complete only if the MCP
   dispatch path has an integration test confirming `context_get` accepts `"3770"`
   (string id) without a full agent simulation. Check whether the infra-001 integration
   suite or eval harness already exercises this path; if not, add an integration test
   as part of this feature.

## Tracking

GitHub Issue: #448
