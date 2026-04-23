## ADR-002: ServerHandler::initialize Override for clientInfo.name Capture

### Context

`UnimatrixServer` currently implements `impl rmcp::ServerHandler for UnimatrixServer` with only
`get_info()`. The `initialize` method is not overridden; the default (which calls `get_info()`)
is in effect.

VNC-014 requires capturing `params.client_info.name` at session establishment time. The rmcp
0.16.0 `ServerHandler` trait exposes `initialize` as a provided method — overriding it is the
documented extension point. The signature is:

```rust
fn initialize(
    &self,
    request: InitializeRequestParams,
    context: RequestContext<RoleServer>,
) -> impl Future<Output = Result<InitializeResult, McpError>> + Send + '_;
```

`clientInfo.name` is available at `initialize` time directly on `request.client_info.name`.
It is NOT available via `context.extensions` or `peer_info()` at this stage — those paths apply
at tool call time. At `initialize` time, the parameter struct carries it directly.

The rmcp session ID at initialize time may or may not be present in `context.extensions` — it is
injected by the session manager for HTTP. For stdio, it is absent. The same `""` fallback key
strategy (ADR-001) applies.

A concern from SR-03 is that Future return type and lifetime bounds in the trait method are
fragile across rmcp versions. The feature pins rmcp to 0.16.0 and uses `std::future::ready()`
as the immediate return form — no additional async machinery needed since we only write to the
Mutex and call `self.get_info()`.

A second concern (from the Assumptions section of the risk assessment) is that the default
`initialize` implementation may have side effects beyond `get_info()`. Inspection of rmcp 0.16.0
confirms the default is literally `Ok(self.get_info())` wrapped in a ready future. Overriding
with the same `get_info()` call is behaviorally identical.

### Decision

Override `ServerHandler::initialize` on `UnimatrixServer` in `server.rs`:

1. Extract `request.client_info.name`; if empty, do nothing (AC-02: empty name not stored)
2. Extract the rmcp session ID from `context.extensions` via `http::request::Parts` +
   `Mcp-Session-Id` header; fall back to `""` for stdio (same access path as tool calls)
3. Truncate name to 256 chars, emit `tracing::warn!` if truncation occurs (AC-10)
4. If the key is `""` and already exists in the map, emit `tracing::warn!` (SR-06 invariant)
5. Insert into `self.client_type_map`
6. Return `Ok(self.get_info())` — identical to the default

The override returns `std::future::ready(Ok(self.get_info()))` after the synchronous map
mutation, keeping the Mutex lock scope minimal and avoiding any async suspension while holding it.

### Consequences

Easier:
- Zero-ceremony: the default behavior is preserved exactly
- Attribution is captured before any tool calls can execute, so no tool call in a session
  can slip through without attribution being available

Harder:
- Any future rmcp upgrade must verify this override still compiles and the default behavior
  has not acquired new side effects (standard upgrade checklist item)
- The `initialize` method fires once per rmcp session manager session, not once per connection
  for stateless mode — must confirm stateless mode behavior (see open questions in ARCHITECTURE.md)
