# Architecture: vnc-012 — Server-Side Integer Coercion for MCP Parameters

## System Overview

Agents frequently construct MCP tool call payloads where numeric parameters are
serialized as JSON strings (`"id": "3770"` instead of `"id": 3770`). The rmcp
0.16.0 deserialization layer uses `serde_json::from_value` with typed Rust
structs; it has no coercion pass. When an `i64` field receives a JSON string,
serde returns `invalid type: string "3770", expected i64` and the tool call
fails with an MCP error.

This feature adds three serde deserializer helpers and applies them to nine
fields in five parameter structs. No rmcp changes are required. No validation
logic changes are required. The coercion occurs at the serde deserialization
boundary — before any handler code runs — so validation functions
(`validated_id`, `validated_k`, etc.) receive already-typed values exactly as
before.

## Component Breakdown

### Component 1: mcp/serde_util.rs (new)

**Responsibility**: Provide three `pub(crate)` serde deserializer functions that
accept a JSON integer or a JSON string representation of an integer and return
the typed Rust value. Reject non-numeric strings and out-of-range values with a
serde error. Contain their own unit tests.

**File**: `crates/unimatrix-server/src/mcp/serde_util.rs`

**Functions**:

```rust
pub(crate) fn deserialize_i64_or_string<'de, D>(d: D) -> Result<i64, D::Error>
where D: serde::Deserializer<'de>

pub(crate) fn deserialize_opt_i64_or_string<'de, D>(d: D) -> Result<Option<i64>, D::Error>
where D: serde::Deserializer<'de>

pub(crate) fn deserialize_opt_usize_or_string<'de, D>(d: D) -> Result<Option<usize>, D::Error>
where D: serde::Deserializer<'de>
```

Each function implements a `serde::de::Visitor` that handles:
- `visit_i64` / `visit_u64`: pass through unchanged (or convert to usize via
  `usize::try_from`)
- `visit_str`: parse via `str::parse::<i64>()` or `str::parse::<u64>()`;
  return `serde::de::Error::custom` on parse failure
- `visit_none` / absent: return `None` (optional variants only, via
  `deserialize_option`)

### Component 2: mcp/tools.rs (modified — struct field annotations only)

**Responsibility**: Apply `#[serde(deserialize_with)]` and
`#[schemars(with = "T")]` attributes to the nine affected fields. No handler
logic changes. Add unit tests in the existing `#[cfg(test)]` block.

**Fields modified**:

| Struct | Field | Type | Attribute pair |
|--------|-------|------|----------------|
| `GetParams` | `id` | `i64` | `deserialize_with = "serde_util::deserialize_i64_or_string"`, `schemars(with = "i64")` |
| `DeprecateParams` | `id` | `i64` | same |
| `QuarantineParams` | `id` | `i64` | same |
| `CorrectParams` | `original_id` | `i64` | same |
| `LookupParams` | `id` | `Option<i64>` | `deserialize_with = "serde_util::deserialize_opt_i64_or_string"`, `schemars(with = "Option<i64>")`, `serde(default)` |
| `LookupParams` | `limit` | `Option<i64>` | same |
| `SearchParams` | `k` | `Option<i64>` | same |
| `BriefingParams` | `max_tokens` | `Option<i64>` | same |
| `RetrospectiveParams` | `evidence_limit` | `Option<usize>` | `deserialize_with = "serde_util::deserialize_opt_usize_or_string"`, `schemars(with = "Option<u64>")`, `serde(default)` |

**Note on `#[serde(default)]`**: The five optional fields with
`deserialize_opt_*` helpers must also carry `#[serde(default)]`. Without it,
serde returns a missing-field error when the key is absent from the JSON object.
The `default` attribute causes serde to use `Option::default()` (`None`) when
the key is absent, bypassing the deserialize function entirely for the absent
case.

### Component 3: mcp/mod.rs (modified — module declaration)

**Responsibility**: Expose the new `serde_util` submodule to the `mcp` namespace.

**Change**: Add `mod serde_util;` to `crates/unimatrix-server/src/mcp/mod.rs`.

### Component 4: infra-001 integration tests (modified — two new tests)

**Responsibility**: Verify the full rmcp dispatch path accepts string-encoded
integers end-to-end over the stdio transport (SR-03 resolution). See ADR-003.

**File**: `product/test/infra-001/suites/test_tools.py`

**Tests added**:
- `test_get_with_string_id` (IT-01): stores an entry, retrieves with string-
  encoded id via `call_tool`; asserts success.
- `test_deprecate_with_string_id` (IT-02): stores an entry, deprecates with
  string-encoded id via `call_tool`; asserts success.
Both marked `@pytest.mark.smoke`.

## Component Interactions

```
JSON-RPC over stdio
      |
      v
rmcp 0.16.0
  serde_json::from_value(Value::Object(arguments))
      |
      v  [transparent Parameters<T>]
serde Deserialize for T (e.g., GetParams)
      |
      +-- field "id": Value::String("3770")
      |       |
      |       v
      |   serde_util::deserialize_i64_or_string
      |       |
      |       v  [str::parse::<i64>()]
      |   Ok(3770i64)
      |
      v
GetParams { id: 3770, ... }
      |
      v
handler: validated_id(params.id) -> same as before
      |
      v
store lookup, format, audit
```

No other component in the call chain changes.

## Technology Decisions

See ADRs:
- ADR-001: `mcp/serde_util.rs` submodule placement
- ADR-002: `#[schemars(with = "T")]` for schema preservation
- ADR-003: Mandatory integration test in infra-001 (SR-03)
- ADR-004: Mandatory `None`-for-absent tests for optional fields (SR-05)

**No new dependencies**: `serde` (with `derive`) and `serde_json` are already
in `Cargo.toml`. The `serde::Deserializer` and `serde::de::Visitor` traits are
available from the existing `serde` dep. No new crate-level dependencies added.

## Integration Points

### Existing integration unchanged

- `crates/unimatrix-server/src/infra/validation.rs` — `validated_id`,
  `validated_k`, `validated_limit`, `validated_max_tokens` all receive the
  already-coerced `i64` value and require no changes.
- All handler implementations in `tools.rs` — coercion is transparent to
  handlers; they see the same typed value regardless of whether the caller sent
  an integer or a string.
- `rmcp 0.16.0` — no changes. The `Parameters<T>` transparent serde wrapper
  fully delegates to `T`'s `Deserialize` impl, which respects
  `#[serde(deserialize_with)]` on `T`'s fields.

### Remaining failure surface (out of scope, SR-04)

This fix covers numeric fields only. String-typed fields (`format`, `category`,
`status`, `agent_id`, `topic`, `action`) are not coerced. Agents that
stringifiy those values will continue to receive serde errors. GH #448 notes
this boundary. Follow-on scope (not in vnc-012) would need to address
non-numeric coercion if observed.

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|-----------------|--------|
| `deserialize_i64_or_string` | `fn<'de, D: Deserializer<'de>>(D) -> Result<i64, D::Error>` | `mcp/serde_util.rs` (new) |
| `deserialize_opt_i64_or_string` | `fn<'de, D: Deserializer<'de>>(D) -> Result<Option<i64>, D::Error>` | `mcp/serde_util.rs` (new) |
| `deserialize_opt_usize_or_string` | `fn<'de, D: Deserializer<'de>>(D) -> Result<Option<usize>, D::Error>` | `mcp/serde_util.rs` (new) |
| `GetParams.id` annotation | `#[serde(deserialize_with = "serde_util::deserialize_i64_or_string")] #[schemars(with = "i64")]` | `mcp/tools.rs` |
| `DeprecateParams.id` annotation | same as above | `mcp/tools.rs` |
| `QuarantineParams.id` annotation | same as above | `mcp/tools.rs` |
| `CorrectParams.original_id` annotation | same as above | `mcp/tools.rs` |
| `LookupParams.id` annotation | `#[serde(default, deserialize_with = "serde_util::deserialize_opt_i64_or_string")] #[schemars(with = "Option<i64>")]` | `mcp/tools.rs` |
| `LookupParams.limit` annotation | same as above | `mcp/tools.rs` |
| `SearchParams.k` annotation | same as above | `mcp/tools.rs` |
| `BriefingParams.max_tokens` annotation | same as above | `mcp/tools.rs` |
| `RetrospectiveParams.evidence_limit` annotation | `#[serde(default, deserialize_with = "serde_util::deserialize_opt_usize_or_string")] #[schemars(with = "Option<u64>")]` | `mcp/tools.rs` |
| Schema snapshot test | Rust unit test asserting `"type": "integer"` for all 9 fields | `mcp/tools.rs` `#[cfg(test)]` |
| IT-01 (string id get) | infra-001 `test_get_with_string_id`, marked `smoke` | `test_tools.py` |
| IT-02 (string id deprecate) | infra-001 `test_deprecate_with_string_id`, marked `smoke` | `test_tools.py` |

## Test Plan Summary

### Unit tests (Rust, in-process)

Located in `#[cfg(test)]` in `serde_util.rs` (helper-level) and `tools.rs`
(struct-level):

For each of the nine fields:
- Integer input -> correct typed value (regression guard)
- String input -> correct typed value (new acceptance)
- Non-numeric string -> `serde` error (not panic, not 0)

For each of the five optional fields additionally:
- JSON null -> `None`
- Absent field -> `None`

For `evidence_limit` additionally:
- Negative string (`"-1"`) -> serde error (cannot represent as usize)
- `u64` overflow string (`"99999999999999999999"`) -> serde error

Schema snapshot test:
- Serialize the tool list; assert `"type": "integer"` present for all nine
  affected field schemas.

### Integration tests (Python, infra-001)

- IT-01: `context_get` accepts string id over stdio transport
- IT-02: `context_deprecate` accepts string id over stdio transport

## Open Questions

None. All open questions from SCOPE.md are resolved:

1. schemars override approach: `#[schemars(with = "T")]` — resolved in ADR-002.
2. serde_util module placement: `mcp/serde_util.rs` — resolved in ADR-001.
3. usize overflow: `usize::try_from(val_u64)` — resolved in SCOPE.md and
   codified in ADR-004.
4. Integration test path: infra-001 `test_tools.py`, `call_tool` method —
   resolved in ADR-003.
