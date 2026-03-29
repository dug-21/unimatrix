## ADR-001: Deserializer Helpers in mcp/serde_util.rs Submodule

### Context

Nine fields across five MCP parameter structs in `tools.rs` need custom serde
deserialization that accepts both JSON integers and JSON string-encoded integers
(e.g., `3770` and `"3770"`). The three helpers required are:
`deserialize_i64_or_string`, `deserialize_opt_i64_or_string`, and
`deserialize_opt_usize_or_string`.

Two placement options were evaluated:

- **Inline in tools.rs**: Keeps all code in one file but inflates a file that
  is already large (>500 lines), mixing struct definitions, handler
  implementations, and utility functions. Violates the single-responsibility
  rule.
- **Private serde_util submodule** at `crates/unimatrix-server/src/mcp/serde_util.rs`:
  Consistent with the existing `response/` submodule pattern in the same crate.
  Keeps `tools.rs` focused on struct definitions and handler logic.
  The `pub(crate)` visibility scopes helpers to the `mcp` module namespace,
  preventing leakage to the broader crate while enabling future reuse within
  `mcp/` without a crate-level dependency.

The scope confirms zero existing uses of `deserialize_with` in the codebase —
this is greenfield. The `response/` submodule precedent is the clearest
analogue.

### Decision

Create `crates/unimatrix-server/src/mcp/serde_util.rs` containing three
`pub(crate)` deserializer functions:

1. `deserialize_i64_or_string<'de, D>(d: D) -> Result<i64, D::Error>` —
   accepts `serde_json::Value::Number` or `Value::String`; parses string via
   `str::parse::<i64>()`; rejects non-numeric strings with
   `serde::de::Error::custom`.

2. `deserialize_opt_i64_or_string<'de, D>(d: D) -> Result<Option<i64>, D::Error>` —
   wraps the above for `Option<i64>` fields. A JSON `null` or absent field
   must produce `None`; a present string or integer produces `Some(i64)`. The
   absent-field case is handled by pairing this attribute with
   `#[serde(default)]` on the struct field.

3. `deserialize_opt_usize_or_string<'de, D>(d: D) -> Result<Option<usize>, D::Error>` —
   for `evidence_limit: Option<usize>`. Parses via `u64` first (rejecting
   negative strings at parse time), then converts with `usize::try_from(val)`
   to avoid silent truncation on 32-bit targets.

Expose the module in `mcp/mod.rs` with `mod serde_util;`. Reference in
`tools.rs` via `use crate::mcp::serde_util` or by path in the attribute
macro (`#[serde(deserialize_with = "serde_util::deserialize_i64_or_string")]`).

The module declares its own unit tests covering: integer input, string input,
null input (for optional variants), absent-field behavior, non-numeric string
rejection, negative string rejection on the usize variant, and 32-bit overflow
rejection on the usize variant.

### Consequences

Easier:
- `tools.rs` stays focused on struct definitions and handler implementations.
- The three helpers are reusable by any future parameter struct added to `mcp/`.
- Module-level unit tests for helpers are isolated from handler-level tests.
- Consistent with the existing `response/` submodule pattern reviewers already know.

Harder:
- One additional file to navigate; call sites in `tools.rs` require a module
  path prefix (`serde_util::...`) rather than a local function name.
- Any rename of the module requires updating all nine `deserialize_with` path
  strings (they are string literals, not Rust paths — not caught by the
  compiler until the build resolves the path).
