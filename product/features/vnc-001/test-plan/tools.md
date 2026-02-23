# Test Plan: tools.rs

## Risks Covered
- R-11: Tool schema mismatch (Critical)
- R-12: Agent identity not threaded through audit (Critical)
- R-14: Server panics on malformed input (Critical)

## Unit Tests

### Tool stubs

```
test_context_search_stub_returns_not_implemented
  Arrange: server with all subsystems
  Act: call context_search with valid SearchParams
  Assert: CallToolResult with text containing "not yet implemented"

test_context_lookup_stub_returns_not_implemented
  Act: call context_lookup with valid LookupParams
  Assert: same pattern

test_context_store_stub_returns_not_implemented
  Act: call context_store with valid StoreParams
  Assert: same pattern

test_context_get_stub_returns_not_implemented
  Act: call context_get with valid GetParams
  Assert: same pattern
```

### Audit logging from stubs

```
test_stub_logs_audit_event
  Arrange: server with all subsystems
  Act: call any tool stub with agent_id="test-agent"
  Assert: audit log contains event with agent_id="test-agent", operation matches tool name, outcome=NotImplemented

test_stub_audit_agent_id_threaded (R-12)
  Act: call context_search with agent_id="uni-architect"
  Assert: audit event has agent_id="uni-architect" (not "anonymous" or empty)

test_stub_audit_no_agent_id
  Act: call context_search without agent_id
  Assert: audit event has agent_id="anonymous"
```

### Schema validation (R-11)

These tests verify the param structs deserialize correctly from JSON, confirming the schema is correct:

```
test_search_params_required_query
  Act: deserialize JSON { "query": "test" }
  Assert: Ok, query == "test", all optionals are None

test_search_params_all_fields
  Act: deserialize JSON with all fields
  Assert: Ok, all fields populated

test_store_params_required_fields
  Act: deserialize JSON { "content": "...", "topic": "...", "category": "..." }
  Assert: Ok

test_store_params_missing_required
  Act: deserialize JSON { "topic": "..." } (missing content)
  Assert: Err (deserialization failure)

test_get_params_required_id
  Act: deserialize JSON { "id": 42 }
  Assert: Ok, id == 42

test_lookup_params_all_optional
  Act: deserialize JSON {}
  Assert: Ok, all fields None
```

### Malformed input handling (R-14)

```
test_wrong_type_doesnt_panic
  Act: attempt to deserialize { "id": "not-a-number" } as GetParams
  Assert: Err (not panic)

test_extra_fields_ignored
  Act: deserialize { "id": 42, "extra": "field" } as GetParams
  Assert: Ok (extra field ignored)
```

## Integration Notes

Full MCP-level tool discovery tests (tools/list, schema inspection) are in integration tests. These unit tests verify the Rust-level behavior of tool handlers and parameter types.
