# vnc-012 Pseudocode Overview
# Accept String-Encoded Integers for All Numeric MCP Parameters

## Feature Summary

Add server-side coercion of string-encoded integers at the serde deserialization boundary
for nine numeric fields across five MCP parameter structs. Three `pub(crate)` helper
functions in a new `mcp/serde_util.rs` module implement the Visitor pattern. Annotation
changes in `tools.rs` wire the helpers to the affected fields. No validation logic,
handler code, or rmcp changes are required.

---

## Components Involved

| Component | File | Action | Why Touched |
|-----------|------|---------|-------------|
| serde_util | `crates/unimatrix-server/src/mcp/serde_util.rs` | Create | Houses the three deserializer helper functions + their unit tests |
| tools | `crates/unimatrix-server/src/mcp/tools.rs` | Modify | Add 9 field annotations (serde + schemars), add AC tests to `#[cfg(test)]` block |
| mod | `crates/unimatrix-server/src/mcp/mod.rs` | Modify | Declare `mod serde_util;` so helpers are visible inside `mcp` namespace |
| infra_001 | `product/test/infra-001/suites/test_tools.py` | Modify | Add IT-01 and IT-02 smoke tests over full stdio transport |

---

## Data Flow

```
JSON-RPC over stdio
      |
      v
rmcp 0.16.0 -- serde_json::from_value(Value::Object(arguments))
      |
      v  [Parameters<T>: transparent serde wrapper - no pre-processing]
serde Deserialize for T (e.g., GetParams)
      |
      +-- required i64 field "id": Value::String("3770")
      |       |
      |       v  [#[serde(deserialize_with = "serde_util::deserialize_i64_or_string")]]
      |   I64OrStringVisitor::visit_str("3770")
      |       |
      |       v  str::parse::<i64>() -> Ok(3770i64)
      |   Ok(3770i64)
      |
      +-- optional i64 field "k": absent key
      |       |
      |       v  [#[serde(default)] fires -- visitor NOT called for absent keys]
      |   Option::default() = None
      |
      +-- optional i64 field "k": Value::Null
      |       |
      |       v  [OptI64OrStringVisitor::visit_none()]
      |   Ok(None)
      |
      v
Fully typed param struct { id: 3770, k: None, ... }
      |
      v
handler: validated_id(params.id) -- receives already-typed i64, unchanged path
```

---

## Shared Types Introduced

All types are local to `serde_util.rs`. No new public or cross-module types.

```
struct I64OrStringVisitor;         -- Visitor for deserialize_i64_or_string
struct OptI64OrStringVisitor;      -- Visitor for deserialize_opt_i64_or_string
struct OptUsizeOrStringVisitor;    -- Visitor for deserialize_opt_usize_or_string
```

These are zero-size marker structs. They are private to `serde_util.rs`.

---

## Attribute Pattern (FR-07)

Two attribute patterns are used. The distinction is required vs. optional field:

**Required i64 fields** (GetParams.id, DeprecateParams.id, QuarantineParams.id,
CorrectParams.original_id):
```
#[serde(deserialize_with = "serde_util::deserialize_i64_or_string")]
#[schemars(with = "i64")]
pub id: i64,
```

**Optional i64 fields** (LookupParams.id, LookupParams.limit, SearchParams.k,
BriefingParams.max_tokens):
```
#[serde(default, deserialize_with = "serde_util::deserialize_opt_i64_or_string")]
#[schemars(with = "Option<i64>")]
pub k: Option<i64>,
```

**Optional usize field** (RetrospectiveParams.evidence_limit):
```
#[serde(default, deserialize_with = "serde_util::deserialize_opt_usize_or_string")]
#[schemars(with = "Option<u64>")]
pub evidence_limit: Option<usize>,
```

Note: `#[serde(default)]` MUST be paired with `#[serde(deserialize_with)]` on every
optional field. Without it, serde returns "missing field" when the key is absent from
the JSON object (R-01/SR-02 — the highest-severity compile-silent trap).

---

## Sequencing Constraints

1. `serde_util.rs` must be created first (it has no dependencies on other components).
2. `mod.rs` change (add `mod serde_util;`) must happen before or together with the
   `tools.rs` annotation changes, because `tools.rs` references `serde_util::*` in
   the `deserialize_with` path strings.
3. The AC-13 Rust integration test and the infra-001 Python tests can be added once
   the two Rust files compile.

---

## OQ-04 Resolution: AC-13 Rust Integration Test Vehicle

`RequestContext<RoleServer>` is NOT directly constructible in tests. Examining rmcp 0.16.0
source confirms that `Peer::new` is `pub(crate)` inside rmcp, and `RequestContext` has no
public constructor. `rmcp::ServerHandler::call_tool` requires a `RequestContext<RoleServer>`.

The chosen approach (confirmed viable from ToolRouter source):

Use `ToolCallContext` directly via `server.tool_router.call(...)`. The `tool_router` field
on `UnimatrixServer` is private, so access requires one of:

- A `pub(crate) fn call_tool_for_test(name, args) -> Result<CallToolResult, ErrorData>`
  helper method on `UnimatrixServer` placed in a `#[cfg(test)]` impl block in `server.rs`,
  which constructs a `ToolCallContext` with a dummy `RequestContext`.

OR, if `RequestContext` field construction is reachable via a `Default`-like path:

Examining `RequestContext`: it has fields `ct: CancellationToken`, `id: RequestId`,
`meta: Meta`, `extensions: Extensions`, `peer: Peer<R>`. Of these, `Peer<R>` requires
`Peer::new(...)` which is `pub(crate)` in rmcp. Therefore `RequestContext` is NOT
constructible outside rmcp.

**Decision**: The implementation agent must add a `#[cfg(test)] pub(crate) async fn
call_tool_for_test` method to `UnimatrixServer` in `server.rs` that:
1. Constructs arguments as a `serde_json::Map<String, Value>`
2. Calls `parse_json_object::<T>()` (rmcp public fn) on the arguments directly to test
   the deserialization path, OR invokes the handler function directly if
   `ToolCallContext` can be constructed with a placeholder.

Actually, examining `ToolCallContext::new` — it is public and takes a `RequestContext`.
Since `RequestContext` cannot be constructed, the cleanest test vehicle is:

**Use `serde_json::from_value` directly on the struct** for AC-13's serde dispatch
path verification. The Rust test lives in `tools.rs` `#[cfg(test)]` block and exercises
`serde_json::from_value(serde_json::Value::Object(args))` on the param struct, which is
the EXACT line executed by `Parameters<T>: FromContextPart` in rmcp. This is not a
shortcut — it exercises the same code path. The transport layer is covered by IT-01/IT-02.

See `infra_001.md` for the Python test structure that exercises the full stdio path.

---

## Note on tools.rs File Size

`tools.rs` currently contains 5020 lines. The implementation brief requires adding
approximately 9 field annotation pairs (~18 attribute lines) and approximately 40-50 new
test functions. The file will approach ~5100 lines total. This exceeds the 500-line
guideline but is a pre-existing condition. The implementation agent must NOT split
`tools.rs` into sub-modules as part of this feature — the architecture specifies
"struct field annotations only" and "add tests to existing `#[cfg(test)]` block". A
separate refactor (not in vnc-012 scope) would address the size.
