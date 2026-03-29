# Component: mcp/mod.rs (modified)

## Purpose

Expose the new `serde_util` submodule to the `mcp` namespace. This is a single-line
change. Without it, `tools.rs` cannot resolve the `serde_util::` path in its
`#[serde(deserialize_with = "...")]` attribute strings.

**File**: `crates/unimatrix-server/src/mcp/mod.rs`

---

## Current State

```rust
//! MCP transport layer modules.
//!
//! Contains MCP tool handlers, identity resolution, response formatting,
//! and ToolContext for handler ceremony reduction.

pub(crate) mod context;
pub mod identity;
pub mod knowledge_reuse;
pub mod response;
pub mod tools;
```

---

## Required Change

Add one line: `mod serde_util;`

The module is declared as private (no `pub`) because it is implementation-internal to
the `mcp` module. The functions inside it are `pub(crate)` which allows them to be
referenced in `#[serde(deserialize_with = "...")]` attribute strings from `tools.rs`.

```
-- AFTER (add mod serde_util; before or after mod tools;):

pub(crate) mod context;
pub mod identity;
pub mod knowledge_reuse;
pub mod response;
mod serde_util;        -- NEW: private submodule, functions are pub(crate) inside
pub mod tools;
```

Placement: alphabetical order (between `response` and `tools`) or immediately before
`mod tools;`. Either is acceptable. The `cargo fmt` pass will determine final ordering
only for imports, not module declarations — the declaration order is preserved by `fmt`.

---

## New / Modified Functions

None. This file only gets one new module declaration line.

---

## Initialization Sequence

No initialization. Module declarations are compile-time-only.

---

## Error Handling

If this line is absent, `cargo build` will fail with:
```
error[E0433]: failed to resolve: use of undeclared crate or module `serde_util`
  --> src/mcp/tools.rs:...
   |
   | #[serde(deserialize_with = "serde_util::deserialize_i64_or_string")]
   |                             ^^^^^^^^^^ use of undeclared crate or module `serde_util`
```

This is a build-time-only error, not a runtime error.

---

## Key Test Scenarios

No tests for `mod.rs` itself — it is a single declaration line. The presence and
correctness of the declaration is verified implicitly by:
1. `cargo build --workspace` passing (all nine `deserialize_with` paths resolve)
2. All AC-01 through AC-13 tests in `tools.rs` passing (they exercise the helpers)
