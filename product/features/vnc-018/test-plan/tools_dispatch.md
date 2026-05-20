# vnc-018 Test Plan: mcp/tools.rs — context_graph Handler

## Component Scope

The `context_graph` handler in `crates/unimatrix-server/src/mcp/tools.rs`:
- `#[tool(description = "...")]` attribute registration
- Capability check: `require_cap(Capability::Read)` before `handle_graph`
- Dispatch call: `graph_read::handle_graph(store, typed_graph_state, params, &ctx)`
- Tool count: 14th `context_*` tool
- Module path usage: fully-qualified `graph_read::handle_graph` per Pattern #4436

This component is deliberately thin — all mode logic lives in `graph_read.rs`. The
`tools.rs` tests verify the wiring, capability enforcement, tool registration, and
tool description text.

---

## Unit Test Expectations

### Capability Enforcement

**Test: `test_context_graph_requires_read_capability`**

```rust
// Arrange: caller with no capabilities (or explicit non-Read capability)
// Act: call context_graph handler
// Assert: standard capability error response returned
// Assert: handle_graph is NOT called (no traversal attempted)
// This verifies require_cap runs BEFORE handle_graph is entered
```

The ordering contract (R-13, ARCHITECTURE.md):
```
require_cap(Read) → runs in tools.rs BEFORE handle_graph
validate_no_unsupported_params → runs INSIDE handle_graph
mode dispatch → runs AFTER validation
```

The delivery agent must NOT move `require_cap` after `handle_graph` is called.

### Module Path Qualification (R-13 / Pattern #4436)

**Code review check (not a runtime test):**

Search `tools.rs` for the `context_graph` handler body. Confirm the call is:
```rust
graph_read::handle_graph(store, typed_graph_state, params, &ctx).await
```
and NOT:
```rust
handle_graph(store, typed_graph_state, params, &ctx).await  // wrong — unqualified
```

This is a static inspection requirement. The Pattern #4436 lesson (#4437 gate failure in
vnc-015) mandates fully-qualified paths in `tools.rs` dispatch points to prevent
future name collisions in this 9,610-line file.

---

## Integration Test Expectations

### Tool Registration and Discovery (AC-16 / R-14)

**test_protocol.py P-03 update (mandatory, non-negotiable):**

This test must be modified from asserting 13 tools to asserting 14 tools:

```python
# In product/test/infra-001/suites/test_protocol.py
# BEFORE (must be updated):
# assert len(tools) == 13

# AFTER:
def test_list_tools_returns_fourteen(server):  # renamed from test_list_tools_returns_thirteen
    result = server.call_tool("initialize", ...)
    tools = result["tools"]
    assert len(tools) == 14, f"Expected 14 tools, got {len(tools)}"
    tool_names = [t["name"] for t in tools]
    assert "context_graph" in tool_names, f"context_graph not in tools: {tool_names}"
```

Lesson #4437 (vnc-015 gate failure): this test MUST be updated before the integration
harness runs or P-03 will fail.

### Tool Description Text (R-03 / ADR-005 / FR-13)

**Test: `test_context_graph_description_contains_staleness_text`** (R-03)

```python
# Get tool definitions from server; find context_graph
# Assert description contains text about depth=1 live database vs depth>1 in-memory
# Exact required text (FR-13):
#   "depth=1 queries the live database and reflects all committed writes"
#   "depth>1 queries the in-memory graph"
#   "lag" (or "tick interval" — documents the staleness window)
```

This verifies the tool description includes the asymmetry documentation mandated by
ADR-005 and FR-13. An agent calling the tool needs this in the description to understand
why depth=2 may not show a just-written edge.

### End-to-End Dispatch Proof (AC-20 / R-13)

**test_tools.py — all three modes at minimum one test each:**

The three AC-20 tests (`test_graph_chain_basic`, `test_graph_current_resolves_deprecated`,
`test_graph_neighbors_outgoing_depth1`) collectively prove the full dispatch chain:

```
MCP call → JSON-RPC decode → context_graph handler → require_cap(Read)
         → graph_read::handle_graph → validate_no_unsupported_params
         → mode handler → SQL query functions → response
```

Any wiring defect (wrong module path, missing require_cap, wrong parameter passing)
will cause these tests to fail. They are the runtime proof of correct dispatch (R-13).

---

## Specific Assertions

### Tool attribute text (must be present in `#[tool(description = "...")]`)

The tool description must contain the exact text from FR-13 about depth=1 vs depth>1
freshness behavior. The exact wording per ARCHITECTURE.md:

> "depth=1 queries the live database and reflects all committed writes immediately.
> depth>1 queries the in-memory graph cache, which may lag recent writes by up to
> one tick interval (typically 30–60 seconds). This asymmetry is intentional..."

If the description is absent or truncated, the R-03 integration test will fail.

### Parameters exposed through tool schema

`context_graph` tool must expose parameters in its JSON schema. At minimum:
- `mode` (required String)
- `id` (optional u64)
- `direction` (optional String)
- `edge_types` (optional array of String)
- `depth` (optional u8)
- `resolve_supersessions` (optional bool)
- Forward-compat fields: `seed_ids`, `from_id`, `to_id`, `max_nodes`

Verify via the MCP `list_tools` response that all parameters appear in the schema.

---

## Risks Specifically Addressed in This Component

- R-13: Fully-qualified `graph_read::handle_graph` call (code review + AC-20 runtime proof)
- R-14: P-03 updated to assert 14 tools (non-negotiable; lesson #4437 gate failure precedent)
- R-03: Tool description text verified to contain staleness documentation (FR-13)
