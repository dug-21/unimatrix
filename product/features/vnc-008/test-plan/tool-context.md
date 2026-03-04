# Test Plan: tool-context

## Risk Coverage: R-02

## Tests

### T-TC-01: build_context with valid agent_id and format
- **Type**: Unit test
- **Method**: Call build_context with known agent and "markdown" format. Verify ToolContext fields match expected identity, format, and AuditContext.
- **Risk**: R-02

### T-TC-02: build_context with unknown agent_id auto-enrolls
- **Type**: Unit test
- **Method**: Call build_context with unknown agent. Verify auto-enrollment as Restricted (same as current resolve_agent behavior).
- **Risk**: R-02

### T-TC-03: build_context with None agent_id defaults to "anonymous"
- **Type**: Unit test
- **Method**: Call build_context with None. Verify agent_id is "anonymous".
- **Risk**: R-02

### T-TC-04: build_context with invalid format returns error
- **Type**: Unit test
- **Method**: Call build_context with format="invalid". Verify error matches parse_format error.
- **Risk**: R-02

### T-TC-05: require_cap with insufficient capability returns error
- **Type**: Unit test
- **Method**: Call require_cap for Restricted agent with Admin capability. Verify error.
- **Risk**: R-02

### T-TC-06: require_cap with sufficient capability succeeds
- **Type**: Unit test
- **Method**: Call require_cap for System agent with any capability. Verify Ok(()).
- **Risk**: R-02

### T-TC-07: All 12 handlers produce identical output after refactoring
- **Type**: Existing integration tests
- **Method**: Full `cargo test --workspace` — existing handler tests verify behavioral equivalence.
- **Risk**: R-02

### T-TC-08: map_err count reduced by >= 50%
- **Type**: Grep verification
- **Command**: `grep -c 'map_err(rmcp::ErrorData::from)' src/mcp/tools.rs`
- **Expected**: Count < 40 (baseline was 78)
- **Risk**: AC-15

### T-TC-09: ToolContext struct exists in mcp/context.rs
- **Type**: Grep verification
- **Command**: `grep 'pub(crate) struct ToolContext' src/mcp/context.rs`
- **Expected**: Match found
- **Risk**: AC-13

### T-TC-10: All handlers use build_context
- **Type**: Grep verification
- **Command**: `grep -c 'build_context' src/mcp/tools.rs`
- **Expected**: >= 11 (one per handler that takes agent_id + format)
- **Risk**: AC-14
