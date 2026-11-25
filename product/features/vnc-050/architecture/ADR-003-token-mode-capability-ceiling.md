## ADR-003: Token Mode Is a Capability Ceiling, Intersected with the Registry. agent_id Stays Self-Asserted. The Intersection Only Narrows.

### Context
HTTP MCP tool calls resolve the self-asserted `agent_id` against the per-slug `AgentRegistry` (`build_context_with_external_identity(..., None)`, `server.rs:541`). Without a ceiling, a read token claiming `agent_id:"human"` could write (SR-11). The prior version of this ADR (#5812) accepted that as a known gap; HUMAN-DECISIONS Decision 5 reverses that descope. An earlier version (#5807) proposed replacing identity with `token:<label>` plus a registry overlay; Decision 5 rejects identity replacement — attribution stays self-asserted, only authority is bounded. #4453: one check point, no parallel authorization system.

Verified in code: rmcp 1.7.0 (`transport/streamable_http_server/tower.rs:1036-1179`) inserts the full `http::request::Parts` — including `Parts.extensions` populated by the auth layer — into every MCP request's extensions; `server.rs:578` already reads `Parts` from `request_context.extensions`, and the `McpAdapter` R-01 note (`http/router.rs:300`) records the same result.

### Decision
- **Effective capabilities = registry grants for declared `agent_id` ∩ token-mode ceiling.** Ceiling: `read` → `{Read, Search}`; `read-write` → `{Read, Write, Search, SessionWrite}`; never `Admin`. Legacy token, stdio and UDS → no ceiling (`None`, today's semantics). The intersection can only remove capabilities, never add (fails closed; PL-4).
- **`agent_id` stays self-asserted** for attribution, audit and registry lookup. The bearer-identity path (`external_identity: Some`, `server.rs:541`) stays dormant; vnc-049 ADR-008 (#5745) remains the OAuth seam.
- **Carry, don't re-derive**: new `mcp/cap_ceiling.rs` exposes `ceiling_from(ctx: &RequestContext<RoleServer>) -> Option<CapCeiling>`, reading `BearerPrincipal` from `Parts.extensions` (same mechanism the auth layer used to build `ResolvedIdentity`). HTTP request with `Parts` present but no `BearerPrincipal` → empty ceiling (deny all; auth-bypass guard). No `Parts` (stdio) → `None`.
- `build_context_with_external_identity` adds one line setting `ToolContext.cap_ceiling: Option<CapCeiling>`.
- **One check point**: `require_cap` (`server.rs:631`) changes signature to `require_cap(&self, ctx: &ToolContext, cap)`; it calls `cap_ceiling::permits(ctx.cap_ceiling.as_ref(), cap)` first (deny → existing `CapabilityDenied` error), then the unchanged registry check. The 15 `tools.rs` call sites change `&ctx.agent_id` → `ctx` (mechanical, net 0 lines).
- **Fallback if delivery's R-01 test shows `BearerPrincipal` absent at the tool** (e.g. a future rmcp path rebuilding the request): `McpAdapter::call` re-inserts the `BearerPrincipal` into the request extensions immediately before `StreamableHttpService`. The first delivery step is a test asserting presence at a tool for POST in a stateful session.
- The ceiling also gates learning recording (ADR-004).

### Consequences
- A read token can never write, regardless of claimed `agent_id`; a read-write token can never use `Admin`. Restores FR-14/AC-08, FR-15/AC-15, the "write rejected" half of FR-32/AC-13, and #1010 "site never holds a write-capable token".
- **Residual gap (accepted)**: within a read-write token, an agent can still claim any `agent_id` and get that agent's non-Admin write grants. Closed by future agent-identity/OAuth work.
- `server.rs`/`tools.rs` (over #693 cap) get signature-level touches only; logic lives in `mcp/cap_ceiling.rs`.
