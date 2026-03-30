# Test Plan: mcp/mod.rs

## Component Summary

Existing file `crates/unimatrix-server/src/mcp/mod.rs` is modified with a single line:
`mod serde_util;` added to expose the new `serde_util` submodule within the `mcp` namespace.
This is the smallest possible change — one module declaration. The current mod.rs contents are:

```rust
pub(crate) mod context;
pub mod identity;
pub mod knowledge_reuse;
pub mod response;
pub mod tools;
// + new: mod serde_util;
```

---

## Unit Test Expectations

This component has no logic — it is a module declaration. There is no runtime behavior to
test in `mod.rs` itself.

**The only testable property is compile-time**: `mod serde_util;` must resolve successfully,
which means `mcp/serde_util.rs` must exist with valid Rust syntax.

**Verification**: `cargo build --workspace` passes without error. This implicitly tests:
1. The `mod serde_util;` declaration resolves to `mcp/serde_util.rs`
2. All three `pub(crate)` functions in `serde_util.rs` are syntactically valid
3. The nine `#[serde(deserialize_with = "serde_util::deserialize_...")]` path strings in
   `tools.rs` resolve to the correct functions in the `serde_util` module (R-07)

No dedicated `#[test]` function is required for `mod.rs`. The component is covered by build
verification.

---

## R-07 Coverage: Path String Literal Trap

The `deserialize_with` path strings in `tools.rs` are string literals, not identifiers.
A rename of `serde_util` to any other name would:
1. Compile successfully (the string `"serde_util::deserialize_..."` is just a literal)
2. Fail at `cargo build` when the serde macro resolves the path at macro expansion

This is caught by `cargo build --workspace` — not by `#[test]` functions.

**Documentation requirement**: The implementation agent must add a code comment in
`serde_util.rs` noting that the module path `serde_util::deserialize_...` is referenced as
a string literal in nine `#[serde(deserialize_with)]` attributes in `tools.rs`. Any rename
of this module or its functions requires updating all nine attributes.

---

## Integration Test Expectations

No integration tests are needed for `mod.rs`. The module declaration is validated
transitively by every test in `tools.rs` (since `tools.rs` uses `serde_util::*` functions)
and by IT-01/IT-02 (which exercise the full binary built from the workspace).

---

## Risk Coverage for This Component

| Risk | Coverage mechanism |
|------|-------------------|
| R-07 (path string literal trap) | `cargo build --workspace` — build-time verification |
| All others | Not applicable — no logic in mod.rs |
