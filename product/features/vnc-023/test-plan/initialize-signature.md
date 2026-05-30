# Test Plan: initialize-signature (C7)

## Component

`crates/unimatrix-server/src/server.rs` (lines 1038-1096) -- adapt `ServerHandler::initialize` signature if rmcp 1.7 changed the trait.

## Risks Covered

- **R-02 (Critical)**: ServerHandler::initialize trait signature incompatibility
- **R-06 (Medium)**: Behavioral default regression (keep_alive, init_timeout)

## Unit Test Expectations

### T-01: initialize populates client_type_map with client name (R-02, AC-12)
```
arrange: construct UnimatrixServer, create duplex transport
         build ClientInfo with name = "test-agent"
act:     run server.serve(transport) + rmcp::serve_client(client_info, transport)
         allow handshake to complete
assert:  server.client_type_map contains entry with value "test-agent"
```
Note: This test already exists in the test module. It must continue to pass after migration.

### T-02: initialize truncates long client names (R-02, AC-12)
```
arrange: ClientInfo with name = "a".repeat(300)
act:     run initialize handshake
assert:  client_type_map value is truncated to 256 chars
```
Note: This test already exists. Must pass unchanged.

### T-03: initialize handles empty client name (R-02)
```
arrange: ClientInfo with name = ""
act:     run initialize handshake
assert:  client_type_map does NOT contain an entry for this session
         (empty names are skipped per current logic)
```

### T-04: initialize returns correct ServerInfo (R-02, R-03)
```
arrange: construct UnimatrixServer
act:     call initialize with valid InitializeRequestParams
assert:  returned InitializeResult contains correct ServerInfo
         (capabilities, instructions, implementation name/version)
```

## Compile Gate

### C-01: initialize signature matches trait (R-02, AC-12)
- **Assert**: `cargo build -p unimatrix-server` exits 0
- If trait changed to `async fn`:
  - Function signature becomes `async fn initialize(&self, request, context) -> Result<InitializeResult, ErrorData>`
  - Body changes from `std::future::ready(Ok(self.get_info()))` to `Ok(self.get_info())`
  - All internal logic (client_type_map population, session key extraction) unchanged

### C-02: No logic changes in initialize body (AC-12)
- **Assert**: Diff shows only signature adaptation (if any). Internal logic lines (1045-1096) are identical.
- client_type_map.lock(), truncation logic, session_key extraction, map.insert -- all unchanged.

## Integration Test Expectations

- Protocol suite: MCP handshake completes successfully. Server returns valid InitializeResult.
- Tools suite: After initialize, tool calls work correctly -- confirms the full handshake + session setup path.
- Lifecycle suite: Multi-step flows (store then search) validate session persistence across requests.

## Edge Cases from Risk Strategy

- **keep_alive 5min default (R-06)**: `LocalSessionManager::default()` now has `keep_alive = 5min`. If a tool call takes >5min, the session may be cleaned up mid-execution. Document in PR. No test -- simulating >5min execution is impractical.
- **init_timeout 60s (R-06)**: New in rmcp 1.6. If client takes >60s to complete initialize handshake, it times out. Acceptable for our use case. Existing tests complete handshake in <1s.
- **Concurrent initialize calls (Risk Strategy edge case 5)**: rmcp 1.6+ adds init_timeout per session. Independent timeout tracking per client. Not tested -- multi-client concurrent init is not a current deployment pattern.
