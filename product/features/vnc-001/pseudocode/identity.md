# Pseudocode: identity.rs (C7 — Identity Resolution)

## Purpose

Extracts agent identity from tool call parameters and threads it through the request pipeline. Produces `ResolvedIdentity` for downstream capability checks and audit logging.

## Types

```
struct ResolvedIdentity {
    agent_id: String,
    trust_level: TrustLevel,
    capabilities: Vec<Capability>,
}
```

## Functions

### extract_agent_id(agent_id: &Option<String>) -> String

```
MATCH agent_id:
    Some(id) IF NOT id.trim().is_empty() => id.trim().to_string()
    _ => "anonymous".to_string()
```

Trims whitespace. Empty string after trimming defaults to "anonymous".

### resolve_identity(registry: &AgentRegistry, agent_id: &str) -> Result<ResolvedIdentity, ServerError>

```
record = registry.resolve_or_enroll(agent_id)?
registry.update_last_seen(agent_id)?

RETURN Ok(ResolvedIdentity {
    agent_id: record.agent_id,
    trust_level: record.trust_level,
    capabilities: record.capabilities,
})
```

Note: `resolve_identity` is a synchronous function. Even though the architecture shows `async` in the function signature, the underlying registry operations are synchronous redb transactions. Wrapping in `spawn_blocking` happens at the server level if needed, not here. If the server needs to call this from an async context, it wraps the call in `tokio::task::spawn_blocking`.

Actually, per the IMPLEMENTATION-BRIEF function signatures, `resolve_identity` is `async`. This is because the server calls it from async tool handlers. The simplest approach: make it async and have the server call it directly. The registry operations are fast (single redb transaction) so calling them from an async context without spawn_blocking is acceptable for the registry workload (unlike bulk store operations which use AsyncEntryStore).

Revised:

```
async fn resolve_identity(registry: &AgentRegistry, agent_id: &str) -> Result<ResolvedIdentity, ServerError>
```

The body is synchronous but the function is async for ergonomic use in tool handlers. This follows the pattern where lightweight operations don't need spawn_blocking.

## Error Handling

- `resolve_identity` propagates registry errors
- `extract_agent_id` is infallible (always returns a valid string)

## Key Test Scenarios

1. extract_agent_id with Some("test-agent") returns "test-agent"
2. extract_agent_id with None returns "anonymous"
3. extract_agent_id with Some("") returns "anonymous"
4. extract_agent_id with Some("  ") returns "anonymous" (whitespace-only)
5. extract_agent_id with Some("  test  ") returns "test" (trimmed)
6. resolve_identity with known agent returns correct trust level and capabilities
7. resolve_identity with unknown agent auto-enrolls and returns Restricted
8. resolve_identity updates last_seen timestamp
