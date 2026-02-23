# Test Plan: main.rs + lib.rs

## Risks Covered
- R-01: MCP initialize handshake failure (Critical)
- R-04: Table creation backward compatibility (Critical)

## Unit Tests

main.rs is primarily wiring code. Its test surface is:

```
test_binary_compiles (AC-01)
  Act: cargo build -p unimatrix-server
  Assert: exit code 0

test_forbid_unsafe_code (AC-19)
  Act: grep 'forbid(unsafe_code)' in lib.rs
  Assert: present
```

## Integration Tests (in tests/ directory)

### MCP Lifecycle

```
test_mcp_initialize_handshake (AC-02)
  Arrange: start server binary via Command, pipe stdin/stdout
  Act: send initialize JSON-RPC request
  Assert: response contains serverInfo with name="unimatrix", version, instructions

test_mcp_tools_list (AC-13)
  Act: after init, send tools/list request
  Assert: 4 tools returned: context_search, context_lookup, context_store, context_get

test_mcp_tool_call_stub (AC-18)
  Act: after init, call context_get with id=1
  Assert: response contains "not yet implemented"

test_mcp_full_lifecycle (AC-18)
  Act: initialize -> tool call -> close stdin (session ends)
  Assert: server exits cleanly
```

### Subsystem wiring

```
test_all_subsystems_initialized
  Arrange: start server in temp dir
  Act: call tool stub
  Assert: audit event recorded (proves audit, registry, identity all working)
```

## Notes

Integration tests that start the actual binary use `std::process::Command` and pipe JSON-RPC messages. They set `--project-dir` to a temp directory to isolate data.

Tests requiring the embedding model are `#[ignore]` tagged.
