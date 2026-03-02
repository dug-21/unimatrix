# Test Plan Overview: col-008 Compaction Resilience

## Test Strategy

Unit tests verify each component in isolation. Integration tests verify the full lifecycle (register -> inject -> compact). No new redb tables, so schema-level tests are not needed. Benchmark tests validate latency requirements.

## Risk-to-Test Mapping

| Risk | Priority | Component(s) | Test Type | Key Scenarios |
|------|----------|-------------|-----------|---------------|
| R-01 | Medium | session-registry | Unit | Sequential access patterns (lock contention proxy) |
| R-02 | Medium | compact-dispatch | Unit + Integration | Quarantined entry excluded, deprecated included |
| R-03 | High | compact-dispatch | Unit | Budget overflow, multi-byte UTF-8, category caps |
| R-04 | Medium | compact-dispatch | Unit + Integration | Fallback with/without entries, feature tag filtering |
| R-05 | High | injection-tracking | Unit + Integration | session_id present/absent, history accumulates |
| R-06 | Medium | injection-tracking | Integration | End-to-end session_id consistency |
| R-07 | Medium | session-registry | Unit | Replicate CoAccessDedup tests exactly |
| R-08 | Low | compact-dispatch | Benchmark | p95 < 15ms for 20 entries |
| R-09 | Low | wire-protocol | Unit | Backward compat with/without session_id |
| R-10 | High | hook-handler | Unit | CompactPayload excluded from fire-and-forget |
| R-11 | Medium | compact-dispatch | Unit | Entry fetch failures skipped gracefully |
| R-12 | High | injection-tracking | Integration | ContextSearch without prior SessionRegister |

## Cross-Component Test Dependencies

- compact-dispatch tests need session-registry (to populate injection history)
- injection-tracking tests need wire-protocol changes (session_id on ContextSearch)
- Integration lifecycle test needs all components

## Integration Harness Plan

### Existing Suites to Run

| Suite | Reason | Expected Impact |
|-------|--------|----------------|
| smoke | Mandatory gate | No impact -- additive changes only |
| tools | Wire protocol changes affect tool deserialization | session_id addition uses serde(default), no impact |
| lifecycle | Multi-step flows that exercise server restart | No impact -- no schema changes |

### New Integration Tests NOT Needed

The col-008 changes are in the hook/UDS path, not the MCP tool path. The integration harness tests MCP tools via JSON-RPC over stdio. The hook handler uses UDS transport, which the integration harness does not exercise. Therefore:

- No new integration tests in `product/test/infra-001/suites/`
- The hook/UDS path is tested via unit tests and manual verification
- The only MCP-visible effect is backward compatibility (session_id defaults to None)

### Integration Test Execution Plan (Stage 3c)

1. `cargo test --workspace` -- all unit tests pass
2. `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60` -- mandatory gate
3. `python -m pytest suites/test_tools.py -v --timeout=60` -- verify wire compat
4. `python -m pytest suites/test_lifecycle.py -v --timeout=60` -- verify no regression
