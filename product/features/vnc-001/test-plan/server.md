# Test Plan: server.rs

## Risks Covered
- R-01: MCP initialize handshake failure (Critical)

## Unit Tests

### ServerInfo

```
test_get_info_name
  Arrange: create UnimatrixServer with all subsystems
  Act: server.get_info()
  Assert: name == "unimatrix"

test_get_info_version_nonempty
  Act: server.get_info()
  Assert: version is non-empty string

test_get_info_instructions
  Act: server.get_info()
  Assert: instructions.is_some()
         instructions contains "knowledge engine"
         instructions contains "search for relevant patterns"

test_server_is_clone
  Arrange: create UnimatrixServer
  Act: let clone = server.clone()
  Assert: compiles, both server and clone usable
```

### resolve_agent

```
test_resolve_agent_with_id
  Arrange: server with bootstrapped registry
  Act: server.resolve_agent(&Some("human".into())).await
  Assert: identity.agent_id == "human", trust_level == Privileged

test_resolve_agent_without_id
  Act: server.resolve_agent(&None).await
  Assert: identity.agent_id == "anonymous"
```

## Integration Notes

Full MCP lifecycle tests (initialize -> tool call -> shutdown) are in the integration test module, not here. server.rs unit tests focus on the ServerInfo and resolve_agent logic.
