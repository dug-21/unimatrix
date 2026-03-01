## ADR-002: Bootstrap Agent Protection

### Context

The "system" and "human" agents are bootstrapped at server startup (`bootstrap_defaults()`) with full capabilities (Read, Write, Search, Admin). SCOPE.md requires protecting the "system" agent from modification via `context_enroll`.

SR-03 from the scope risk assessment identified that "human" should also be protected — demotion of the human agent would break the primary MCP client interaction path.

### Decision

Both "system" and "human" are protected bootstrap agents. `enroll_agent()` rejects modification of either with `ServerError::ProtectedAgent`.

The protection is implemented as a constant set check in `enroll_agent()`:
```rust
const PROTECTED_AGENTS: &[&str] = &["system", "human"];
```

This is preferred over checking the `TrustLevel::System` or `TrustLevel::Privileged` variants because:
1. Protection is based on identity (specific well-known agent IDs), not trust level
2. A future Admin agent enrolled at Privileged level should be modifiable
3. The set is explicit and auditable

### Consequences

- Neither bootstrap agent can be modified through the MCP tool
- An Admin who wants to change the human agent's capabilities must do so through direct database access (outside the MCP tool surface)
- The protected set is small and static — no configuration needed
- Future bootstrap agents (if any) require a code change to add protection
