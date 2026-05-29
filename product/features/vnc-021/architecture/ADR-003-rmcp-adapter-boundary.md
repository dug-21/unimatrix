## ADR-003: Adapter Boundary Isolating rmcp StreamableHttpService

### Context

rmcp 0.16.0's `StreamableHttpService<S, M>` implements `tower_service::Service<Request<RequestBody>>`. This is a new API surface that is lightly adopted (SR-01). Known risks (Unimatrix #4367):
- Extension propagation behavior is unproven -- `http::Extensions` inserted by upstream middleware may not survive rmcp's internal request processing
- Session lifecycle management (SSE connections, Mcp-Session-Id) may have undocumented constraints
- The pinned `=0.16.0` version constraint prevents upgrading to fix bugs

If issues emerge, restructuring the entire listener around rmcp internals would be expensive.

Alternatives:
1. **Direct composition**: Pass `StreamableHttpService` directly into the tower stack with no wrapper. Cheapest to implement, but rmcp bugs affect the entire HTTP stack.
2. **Thin adapter**: Wrap `StreamableHttpService` in a local `McpAdapter` service that handles extension injection, error mapping, and provides a seam for workarounds. Small code cost, large isolation benefit.
3. **Full abstraction**: Define a `TransportAdapter` trait abstracting over rmcp. Over-engineered for a single implementation.

### Decision

Thin adapter (option 2). The `PathRouter` (C3) holds a `StreamableHttpService` internally and delegates MCP-path requests to it. Before delegation, the adapter:

1. Copies `ResolvedIdentity` from the outer request's extensions into the inner request if rmcp strips extensions
2. Maps rmcp error responses to consistent JSON error format
3. Enforces request body size limit (`max_request_body_bytes`) before handing to rmcp

The adapter is a single struct in `router.rs` (~30 lines), not a separate module. If rmcp extension propagation works correctly (the expected case), the copy step becomes a no-op verified by a debug assertion.

If rmcp has session lifecycle bugs, workarounds are localized to this adapter without touching auth, health, or listener code.

### Consequences

Easier: rmcp behavioral issues are isolated to a single code location. The rest of the HTTP stack (auth, health, routing) is rmcp-agnostic. Workarounds for SR-01/SR-02 do not cascade.

Harder: An extra layer of indirection for MCP requests. The adapter must be kept thin -- it is a workaround seam, not a feature surface.
