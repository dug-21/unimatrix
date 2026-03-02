# Test Plan Overview: col-007 Automatic Context Injection

## Test Strategy

### Unit Tests (hook.rs, wire.rs, uds_listener.rs)

Primary coverage for:
- Hook handler request construction (UserPromptSubmit arm)
- Injection formatting (byte budget, UTF-8 safety, edge cases)
- Wire protocol changes (HookInput.prompt deserialization)
- CoAccessDedup operations (insert, check, clear)
- Dispatch handler migration (async correctness for all existing handlers)

### Integration Tests (product/test/infra-001)

End-to-end validation for:
- ContextSearch through UDS transport
- MCP/UDS pipeline equivalence
- Session warming flow
- Co-access pair generation

## Risk-to-Test Mapping

| Risk | Priority | Unit Tests | Integration Tests |
|------|----------|------------|-------------------|
| R-01 (pipeline drift) | High | -- | Pipeline equivalence: 3+ queries via MCP and UDS |
| R-02 (async dispatch breaks handlers) | Low | All 6 dispatch tests with .await | Existing suites pass unchanged |
| R-03 (byte budget overflow) | Medium | format_injection with CJK, emoji, mixed | -- |
| R-04 (threshold suppression) | Medium | Floor filtering with boundary values | ContextSearch with real embeddings |
| R-05 (race condition) | High | -- | SessionRegister -> ContextSearch sequence |
| R-06 (memory leak) | Low | CoAccessDedup insert/check/clear | -- |
| R-07 (HookInput flatten) | Low | 4 deserialization scenarios | -- |
| R-08 (UDS timeout) | Medium | Timeout graceful degradation | Latency benchmark |
| R-09 (content parsing) | Medium | Adversarial content formatting | -- |
| R-10 (oversized prompt) | Low | Empty/long prompt handling | -- |
| R-11 (spawn_blocking) | Medium | -- | Concurrent ContextSearch |
| R-12 (warming failure) | Low | Mock embed states | -- |

## Cross-Component Test Dependencies

- injection-format tests depend on EntryPayload type (wire.rs)
- uds-dispatch tests depend on Store, mock embed service
- hook-handler tests are self-contained (build_request is pure logic)

## Integration Harness Plan

### Existing Suites Applicable

| Suite | Relevance | Why |
|-------|-----------|-----|
| `smoke` | Mandatory gate | Any server change requires smoke pass |
| `tools` | High | context_search tool behavior must be unchanged |
| `lifecycle` | Medium | Multi-step store->search flows verify no regression |
| `protocol` | Medium | MCP handshake still works with async UDS changes |

### Gaps in Existing Suites

The existing integration suites test MCP tools only. They do not test:
1. UDS ContextSearch requests (new transport path)
2. Hook process end-to-end (stdin -> UDS -> stdout)
3. Session warming flow (SessionRegister -> ContextSearch readiness)
4. MCP vs UDS result equivalence

### New Integration Tests Needed

These would be added to the integration harness if the UDS transport is testable from Python. Since the UDS protocol is a custom binary framing (not HTTP/JSON-RPC), the Python harness cannot directly test UDS. New integration-level tests should be Rust integration tests in the server crate:

1. **UDS ContextSearch end-to-end**: Start server, connect via UDS, send ContextSearch, verify Entries response
2. **Pipeline equivalence**: Same query via MCP context_search and UDS ContextSearch, compare entry IDs
3. **SessionRegister warming**: Send SessionRegister, then ContextSearch, verify results

For the Python integration harness (Stage 3c), run the existing suites to verify no regression. The UDS-specific integration testing happens through Rust-level tests in Stage 3b.

### Stage 3c Execution Plan

1. `cargo test --workspace` -- all unit tests pass
2. `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60` -- smoke gate
3. `python -m pytest suites/test_tools.py -v --timeout=60` -- context_search unchanged
4. `python -m pytest suites/test_lifecycle.py -v --timeout=60` -- lifecycle flows intact
5. `python -m pytest suites/test_protocol.py -v --timeout=60` -- MCP protocol intact
