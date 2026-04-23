## ADR-003: build_context_with_external_identity() as Seam 2 Overload; build_context() Removed

### Context

The existing `build_context(&self, agent_id, format, session_id)` has no `RequestContext`
parameter and cannot access the rmcp-level `Mcp-Session-Id` header. A new overload is required
to:

1. Extract the rmcp session ID from `request_context.extensions`
2. Look up `client_type` in `self.client_type_map`
3. Attach `client_type` to `ToolContext` for audit construction

Two design options for handling `build_context()` post-migration (SR-04):

(A) Retain as thin wrapper — `build_context()` calls `build_context_with_external_identity()`
    with a stub `request_context`. This requires fabricating or threading a `RequestContext`
    where none exists, adding complexity and hiding call sites that were not migrated.

(B) Remove `build_context()` after migration so any un-migrated call site fails to compile.
    SR-04 explicitly recommends this: "remove or rename `build_context()` after migration so
    any missed call site fails to compile rather than silently calling the old path."

There are exactly 10 `build_context` call sites in `tools.rs` (confirmed by grep) plus additional
sites in `server.rs` (background-tick and retention paths). Sites outside `tools.rs` are in
contexts that do not have a `RequestContext` available.

The implementation spec calls `build_context_with_external_identity()` from all 12 tool handlers.
The background-tick and retention audit paths in `server.rs` and `background.rs` are not
tool-call paths and have no `RequestContext`. These require a different handling strategy.

The resolution: `build_context_with_external_identity()` is the new tool-call path. For non-tool
call sites that construct `AuditEvent` directly (background, import, UDS listener), they
continue constructing `AuditEvent` directly and populate the four new fields with their
appropriate defaults (`credential_type: "none"`, `capability_used: ""`,
`agent_attribution: ""`, `metadata: "{}"`). `build_context()` is removed from the tool-call
path, but no thin wrapper that would silently swallow un-migrated tool handlers is retained.

The `ResolvedIdentity` stub parameter (`Option<&ResolvedIdentity>`) is part of the Seam 2
signature for W2-3. In vnc-014, it is always `None`. When `Some`, it bypasses
`resolve_agent()` (W2-3 activation path). Shipping the full signature now costs nothing and
means W2-3 only needs to wire the bearer result — not change the function signature.

`ResolvedIdentity` already exists in `mcp/identity.rs` as a struct with `agent_id`, `trust_level`,
and `capabilities`. No new type is needed; the stub is `Option<&ResolvedIdentity>` referencing
the existing type.

### Decision

1. Add `build_context_with_external_identity()` to `UnimatrixServer` in `server.rs` with the
   full Seam 2 signature:
   ```rust
   pub(crate) async fn build_context_with_external_identity(
       &self,
       params_agent_id: &Option<String>,
       format: &Option<String>,
       session_id: &Option<String>,
       request_context: &RequestContext<RoleServer>,
       external_identity: Option<&ResolvedIdentity>,
   ) -> Result<ToolContext, rmcp::ErrorData>
   ```

2. Remove `build_context()` after all tool handler call sites are migrated. The compile-time
   enforcement is the migration completeness gate (SR-04).

3. `ToolContext` gains a `client_type: Option<String>` field populated from the map lookup.
   `AuditContext` is NOT extended — `client_type` is held directly on `ToolContext` and
   consumed at `AuditEvent` construction sites in the handlers.

4. Non-tool-call `AuditEvent` construction sites (background, UDS listener, import) populate
   the four new fields directly with their defaults. They do not use
   `build_context_with_external_identity()`.

5. `external_identity: Some(identity)` path: bypass `resolve_agent()`, use identity fields
   directly. `None` path: call `resolve_agent()` as before.

### Consequences

Easier:
- Compile-time enforcement of complete migration (SR-04 satisfied)
- W2-3 activation is a single-line change per tool handler: pass bearer identity instead of None
- No silent fallback to old path

Harder:
- O(n) mechanical change across all 10+ tool-call build_context sites (delivery risk is mitigated
  by compile enforcement)
- Non-tool-call sites must be updated separately (different pattern: direct AuditEvent field
  population, not build_context)
- Seam 2 parameter is in the signature from day one — implementers must not ignore it or stub
  incorrectly
