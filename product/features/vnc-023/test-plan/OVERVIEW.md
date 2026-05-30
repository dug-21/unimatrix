# Test Plan Overview: vnc-023 (rmcp 0.16 to 1.7 Migration)

## Test Strategy

This is a dependency upgrade with two bundled enhancements. Testing priorities:

1. **Compile gates** -- most risks (R-02, R-04, R-06, R-07, R-08, R-11) are compilation failures. The compiler is the first test.
2. **Integration tests** -- R-01 (extension propagation) and R-03 (ServerInfo correctness) require runtime validation through the full MCP protocol path.
3. **Config deserialization** -- R-04 and R-09 require unit tests on the config chain.
4. **Verification-only** -- R-05 (CVE), R-10 (http crate), R-12 (description) need assertion-based verification, not behavioral tests.

Test levels: unit tests in `crates/unimatrix-server`, integration tests via infra-001.

## Risk-to-Test Mapping

| Risk | Priority | Test Type | Component Test Plan |
|------|----------|-----------|-------------------|
| R-01 | Critical | Integration (infra-001 protocol/tools) + unit | server-struct-migration, server-test-migration |
| R-02 | Critical | Compile gate + integration | initialize-signature |
| R-03 | High | Unit (`get_info()` assertions) | server-struct-migration |
| R-04 | High | Unit (config deser + wiring) | config-allowed-origins, router-origin-wiring, main-call-site |
| R-05 | High | Verification (Cargo.lock + code review) | cargo-version-bump |
| R-06 | Medium | Existing test suite pass | initialize-signature |
| R-07 | Medium | Compile gate | cargo-version-bump |
| R-08 | Medium | Compile gate | server-test-migration |
| R-09 | Medium | Unit (config deser round-trip) | config-allowed-origins |
| R-10 | High | `cargo tree` verification + R-01 test | cargo-version-bump |
| R-11 | Medium | Compile gate + diff review | cargo-version-bump |
| R-12 | Low | Unit (`get_info()` description field) | server-struct-migration |
| R-13 | High | Code review + documentation | router-origin-wiring |

## Cross-Component Test Dependencies

1. **cargo-version-bump must complete first** -- all other components depend on rmcp 1.7.0 being resolvable.
2. **server-struct-migration before server-test-migration** -- production `get_info()` must compile before test module fixes.
3. **config-allowed-origins before router-origin-wiring** -- `HttpConfig.allowed_origins` field must exist before router can reference it.
4. **router-origin-wiring before main-call-site** -- `ProjectRouter::new()` signature must be updated before main.rs can pass the new parameter.
5. **All components before integration tests** -- infra-001 exercises the compiled binary.

## Integration Harness Plan

### Existing Suites to Run

| Suite | Relevance | Risk Coverage |
|-------|-----------|---------------|
| `smoke` (mandatory gate) | Minimum regression baseline -- covers store/search/correct/quarantine/capabilities | R-01, R-02, R-03 (partial) |
| `protocol` | MCP handshake validates initialize response (ServerInfo correctness) | R-02, R-03 |
| `tools` | All tool invocations exercise extension propagation (ResolvedIdentity) | R-01, R-10 |
| `security` | Capability enforcement depends on ResolvedIdentity; validates auth chain survives | R-01, R-13 |
| `lifecycle` | Multi-step flows validate session manager behavior (keep_alive default) | R-06 |

**Not needed:**
- `confidence`, `contradiction` -- no changes to confidence/contradiction logic
- `volume` -- no schema or storage changes
- `edge_cases` -- useful but lower priority; run if time permits
- `adaptation` -- no changes to adaptation logic

### Gap Analysis

Existing suites cover the MCP protocol path well. Gaps:

1. **allowed_origins propagation** -- no existing integration test validates Origin header enforcement. This is primarily a config-wiring concern testable via unit tests (the actual Origin enforcement is rmcp's responsibility, not ours). No new integration test needed -- unit tests on `McpAdapter::new()` suffice.
2. **Implementation description field** -- no existing test asserts the `description` field in the initialize response. Add one assertion to `protocol` suite or verify via unit test on `get_info()`.
3. **keep_alive edge case** -- session survival during long tool execution. Per PR notes, document this edge case. A mock slow tool is impractical in the harness without significant infrastructure.

### New Integration Tests

No new integration tests are needed in infra-001 for this feature:

- **Extension propagation (R-01)**: Existing `security` suite tests (capability enforcement) implicitly validate ResolvedIdentity propagation. If it breaks, capability-gated tool calls fail with permission errors. ADR-003 confirms this strategy.
- **allowed_origins (R-04)**: rmcp enforces Origin headers internally. Our responsibility is config wiring, testable via unit tests.
- **Description field (R-12)**: Low priority. The `protocol` suite's handshake test already validates the initialize response structure. Adding a description assertion is optional and low-risk.

If existing `security` suite tests do NOT exercise capability-gated tools through the HTTP transport path, one new test should be added:

```python
# suites/test_security.py (only if gap confirmed in Stage 3c)
def test_extension_propagation_identity(server):
    """Validate ResolvedIdentity survives rmcp processing (R-01, AC-07)."""
    # Store an entry (requires write capability, gated by identity)
    result = server.call_tool("context_store", {"content": "test", ...})
    assert result is not None
    # If this succeeds, identity was propagated
```

### keep_alive Edge Case (PR Note)

The keep_alive 5-minute default is a behavioral change. A test simulating >5min tool execution is impractical without harness infrastructure changes. Document in PR description. If a test is feasible (e.g., mock sleep in tool handler), add it in Stage 3c as a stretch goal.
