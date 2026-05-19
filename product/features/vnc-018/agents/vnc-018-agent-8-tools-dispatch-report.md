# Agent Report: vnc-018-agent-8-tools-dispatch

## Task

Add the `context_graph` `#[tool]` handler to `mcp/tools.rs` and wire the `graph_read` module in `mcp/mod.rs`.

## Files Modified

- `crates/unimatrix-server/src/mcp/mod.rs` — added `pub(crate) mod graph_read;` and `pub use graph_read::EdgeRecord;` (ADR-004)
- `crates/unimatrix-server/src/mcp/tools.rs` — added `context_graph` `#[tool]` handler + 6 unit tests
- `crates/unimatrix-server/src/mcp/graph_read.rs` — created initial stub; replaced by agent-7 full implementation

## Implementation Summary

### mcp/mod.rs

Added module declaration and re-export per ADR-004:

```rust
pub(crate) mod graph_read;
pub use graph_read::EdgeRecord;
```

### mcp/tools.rs — context_graph handler

Handler follows exact pattern of `context_edge` (vnc-015):

1. `build_context_with_external_identity` — identity + session ceremony
2. `require_cap(Read)` — capability gate BEFORE `handle_graph` (FR-02, Constraints §3)
3. `self.services.typed_graph_handle()` — acquire Arc clone
4. `crate::mcp::graph_read::handle_graph(...)` — fully-qualified per Pattern #4436

Validation ordering is correct: `require_cap` in `tools.rs` fires before `handle_graph` is entered; `validate_no_unsupported_params` runs inside `handle_graph`.

### Stub (graph_read.rs)

Created a minimal stub to allow `tools.rs` to compile independently of agent-7. The stub defined `GraphParams`, `EdgeRecord`, and a `handle_graph` that returned an INTERNAL_ERROR. Agent-7 replaced the stub with the full implementation during parallel delivery.

**Key fix during stub creation:** `TypedGraphStateHandle` wraps `std::sync::RwLock`, not `tokio::sync::RwLock`. Initial stub used the wrong import; fixed after the first build attempt surfaced "similar names, but actually distinct types" error.

## Unit Tests Added (6)

All in `mcp::tools::tests` section `-- vnc-018: GraphParams --`:

| Test | Verifies |
|------|---------|
| `test_graph_params_mode_only_deserializes` | Minimal deserialization; all optional fields absent |
| `test_graph_params_neighbors_full_deserializes` | Full neighbors params round-trip |
| `test_graph_params_chain_with_forward_compat_fields_deserializes` | Forward-compat fields parse without error (ADR-003) |
| `test_graph_params_missing_mode_rejected` | Required `mode` field absent → deserialize fails |
| `test_context_graph_description_contains_staleness_text` | Description contains depth=1 live DB, depth>1 cache, tick interval, asymmetry text (R-03/FR-13) |
| `test_context_graph_uses_fully_qualified_module_path` | Compile-time proof that `crate::mcp::graph_read::handle_graph` is resolvable (Pattern #4436/R-13) |

## Test Results

- `cargo build -p unimatrix-server`: 0 errors, 20 pre-existing warnings (none new)
- `cargo test -p unimatrix-server`: 3013 passed, 0 failed (lib), +46+16+16+7 integration = 3098 total, 0 failures

Note: An earlier run showed 2 transient failures that did not reproduce on the next run — consistent with the known pool-timeout flakiness documented in GH #303.

## Deviations from Pseudocode

None. Handler body matches pseudocode/tools_dispatch.md exactly:
- `build_context_with_external_identity` called with `(agent_id, format, &None, request_context, None)`
- `require_cap(Capability::Read)` before delegation
- `services.typed_graph_handle()` for the Arc clone
- `crate::mcp::graph_read::handle_graph` with fully-qualified path

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #4478, #4482, #4477 (vnc-018 ADRs), #4436 (Pattern #4436 constraint), #4475 (chain/current CTE requirement). All relevant and applied.
- Stored: entry #4487 "TypedGraphStateHandle wraps std::sync::RwLock — not tokio::sync::RwLock" via /uni-store-pattern — novel gotcha discovered during stub creation; the Rust error message names both types identically, making this non-obvious.
