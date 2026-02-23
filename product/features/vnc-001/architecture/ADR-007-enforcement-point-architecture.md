## ADR-007: Enforcement Point Architecture for Security

### Context

vnc-001 builds the security infrastructure (agent registry, audit log, identity resolution). vnc-002 adds the enforcement logic (input validation, capability checks, content scanning). The architecture must make security enforcement trivial to add without refactoring the server.

Options:
1. **Middleware pattern**: Generic request/response middleware that wraps all tool handlers
2. **Enforcement points in handlers**: Explicit check calls at the top of each tool handler
3. **Trait-based enforcement**: Security checks as trait methods that tool implementations must call

### Decision

Use explicit enforcement points in tool handlers. Each tool handler method has clearly marked locations where vnc-002 inserts security checks:

```rust
async fn context_store(&self, params: StoreParams) -> Result<CallToolResult, ErrorData> {
    let identity = self.resolve_agent(&params.agent_id).await?;

    // ENFORCEMENT: capability check
    // self.registry.require_capability(&identity.agent_id, Capability::Write)?;

    // ENFORCEMENT: input validation
    // validate_store_params(&params)?;

    // ENFORCEMENT: content scanning
    // scan_content(&params.content)?;

    // ... tool logic ...
}
```

vnc-001 ships with enforcement points as comments showing the exact interface. vnc-002 uncomments and implements the validation functions. The registry's `require_capability()` and audit's `log_event()` are already functional in vnc-001.

This is preferred over middleware because:
- Different tools have different enforcement needs (search needs no content scanning; store needs no output framing)
- Enforcement order matters (identity first, then capability, then validation, then scanning)
- Error responses are tool-specific (a denied search returns different guidance than a denied store)
- The pattern is explicit and auditable -- you can read a tool handler and see all its security checks

### Consequences

- **Easier:** vnc-002 adds security by filling in clearly marked slots. No architectural changes, no new abstractions, no middleware plumbing.
- **Easier:** Each tool's security profile is visible in its handler. Code review can verify "does this tool check capabilities? validate input? scan content?"
- **Easier:** Different enforcement per tool -- search is read-only, store needs write cap + validation + scanning. The pattern naturally supports this.
- **Harder:** Not automatic -- a new tool added without copying the enforcement pattern would lack security checks. Mitigated by: the pattern is documented, code review catches omissions, and the RISK-TEST-STRATEGY will include tests for enforcement presence.
