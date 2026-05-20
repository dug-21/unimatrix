# Pseudocode: mcp/tools.rs — context_graph #[tool] Handler

## Purpose

Addition to `crates/unimatrix-server/src/mcp/tools.rs`. Adds the 14th
`context_*` MCP tool as a `#[tool(...)]` attributed method on `McpServerImpl`
(the `UnimatrixServer` impl block). The body contains ONLY the dispatch ceremony:
build context, capability check, delegate to `graph_read::handle_graph`. No mode
logic lives in `tools.rs`.

---

## Modified Files

- `crates/unimatrix-server/src/mcp/tools.rs` — add handler method + `GraphParams` struct import
- `crates/unimatrix-server/src/mcp/mod.rs` — add `pub(crate) mod graph_read;` and re-export

---

## mod.rs Changes

```
// Add after "pub(crate) mod edge_write;"
pub(crate) mod graph_read;

// Add after existing use declarations or pub-use block:
pub use graph_read::EdgeRecord;    // ADR-004: re-export for #597/#598 consumers
```

---

## GraphParams Import

`GraphParams` is defined in `mcp/graph_read.rs`. The `tools.rs` handler uses:

```
// At the top of the context_graph handler method:
use crate::mcp::graph_read::GraphParams;
```

Or via `Parameters<crate::mcp::graph_read::GraphParams>` at the function signature.

---

## New/Modified Functions

### context_graph — #[tool] Handler

```
// Place after the context_edge handler (vnc-015) in the impl block.
// Comment: -- vnc-018: context_graph --

#[tool(
    name = "context_graph",
    description = "Traverse the Unimatrix knowledge graph in three modes:\n\
        - chain: walk the supersession history of an entry (forward toward newer, \
          backward toward older, or both). forward: returns descendants (entries that \
          supersede X); backward: returns ancestors (entries X supersedes).\n\
        - current: resolve any entry to its terminal active successor, following \
          superseded_by links until an Active entry is found.\n\
        - neighbors: retrieve entries connected by typed graph edges. \
          Accepts edge_types filter, direction (incoming/outgoing/both), and depth (1..=10). \
          depth=1 queries the live database and reflects all committed writes immediately. \
          depth>1 queries the in-memory graph cache, which may lag recent writes by up to \
          one tick interval (typically 30-60 seconds). This asymmetry is intentional: \
          depth=1 is the precise lookup case where freshness matters; depth>1 is exploratory \
          multi-hop traversal where a tick-window lag is acceptable.\n\
        Requires Read capability. All three modes are read-only."
)]
async fn context_graph(
    &self,
    Parameters(params): Parameters<crate::mcp::graph_read::GraphParams>,
    request_context: rmcp::service::RequestContext<rmcp::RoleServer>,
) -> Result<CallToolResult, rmcp::ErrorData>
```

**Body**:

```
    // ── Step 1: Build context (standard ceremony) ─────────────────────────────
    let ctx = self
        .build_context_with_external_identity(
            &params.agent_id,
            &params.format,
            &None,
            &request_context,
            None,
        )
        .await?

    // ── Step 2: Capability check — runs BEFORE handle_graph (FR-02, Constraints §3) ─
    // Capability check is in tools.rs; validate_no_unsupported_params is inside handle_graph.
    // Order is mandated: capability check → parameter validation → mode dispatch.
    self.require_cap(&ctx.agent_id, Capability::Read).await?

    // ── Step 3: Acquire typed_graph_state handle ──────────────────────────────
    // Arc::clone is cheap — the handle is shared with the background tick service.
    let typed_graph_state = self.services.typed_graph_handle()

    // ── Step 4: Delegate to graph_read module (fully-qualified path per Pattern #4436) ─
    // All mode logic (chain/current/neighbors) lives in graph_read.rs.
    // tools.rs contains only this dispatch call.
    crate::mcp::graph_read::handle_graph(
        &self.store,
        &typed_graph_state,
        params,
        &ctx,
    )
    .await
```

---

## Validation Ordering (Mandatory)

The three-step sequence inside this handler:

1. `build_context_with_external_identity` — identity resolution (standard ceremony)
2. `require_cap(Read)` — capability gate in `tools.rs` (before `handle_graph` is entered)
3. `graph_read::handle_graph(...)` — inside here: `validate_no_unsupported_params` runs first, then mode dispatch

This ordering is architecturally mandated (ARCHITECTURE.md §Component Interactions,
SPEC Constraints §3). Do NOT move `validate_no_unsupported_params` before the
capability check. Do NOT add mode logic to `tools.rs`.

---

## Data Flow

```
MCP client sends { mode: "neighbors", id: 42, depth: 2, edge_types: ["Supports"] }
  ↓
tools.rs: context_graph(Parameters(params))
  → build_context_with_external_identity → ctx
  → require_cap(Read)  ← returns rmcp::ErrorData if insufficient
  → services.typed_graph_handle() → Arc<RwLock<TypedGraphState>>
  → graph_read::handle_graph(&self.store, &typed_graph_state, params, &ctx)
        → validate_no_unsupported_params(&params) → Ok(())
        → id = params.id (Some(42)) → 42
        → mode = "neighbors"
        → handle_neighbors(store, typed_graph_state, &params, 42)
              → validation (depth, direction, edge_types)
              → depth=2 > 1 → handle_neighbors_bfs(...)
              → NeighborsResponse { edges: [...] }
        → serde_json::to_string(NeighborsResponse)
        → Ok(CallToolResult::text(json))
  ↓
MCP client receives JSON with edges array
```

---

## Error Handling

| Error | Where handled | Response |
|-------|--------------|---------|
| Insufficient Read capability | `require_cap` in tools.rs | Standard capability error (not rmcp INVALID_PARAMS) |
| Context build failure | `build_context_with_external_identity` | Standard identity error |
| All graph_read errors | Inside handle_graph | rmcp::ErrorData with appropriate code and message |

`tools.rs` does not inspect or post-process errors from `handle_graph`. It returns
the `Result<CallToolResult, rmcp::ErrorData>` directly from `handle_graph`.

---

## Key Test Scenarios

1. AC-16: `test_protocol.py` P-03 must assert 14 `context_*` tools after this
   handler is registered. This is a non-negotiable test update (lesson #4437).

2. AC-20: At least one infra-001 integration test exercises the full dispatch chain:
   MCP call → context_graph handler → graph_read::handle_graph → mode handler.
   This is the end-to-end proof of correct wiring (R-13).

3. Code review check: confirm `graph_read::handle_graph` is called with the fully
   qualified module path `crate::mcp::graph_read::handle_graph` (Pattern #4436, R-13).

4. Capability test: call context_graph without Read capability → standard capability
   error response (FR-02).

5. `build_context_with_external_identity` called with correct params — same pattern
   as `context_edge` handler (verify params: agent_id, format, None, request_context, None).
