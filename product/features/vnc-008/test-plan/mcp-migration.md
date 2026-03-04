# Test Plan: mcp-migration

## Risk Coverage: R-01, R-06

## Tests

### T-MCP-01: Compilation after move
- **Type**: Build verification
- **Command**: `cargo check --workspace`
- **Expected**: Zero errors
- **Risk**: R-01

### T-MCP-02: Test count preserved
- **Type**: Test count comparison
- **Command**: `cargo test --workspace`
- **Expected**: >= 1,664 tests passed, 0 failed
- **Risk**: R-06

### T-MCP-03: Identity tests pass in new location
- **Type**: Targeted test run
- **Command**: `cargo test -p unimatrix-server identity::`
- **Expected**: All identity module tests pass
- **Risk**: R-06

### T-MCP-04: Tools handler tests pass
- **Type**: Targeted test run
- **Command**: `cargo test -p unimatrix-server tools::`
- **Expected**: All tool handler tests pass (these are the bulk of behavioral verification)
- **Risk**: R-01, R-06

### T-MCP-05: No cross-transport imports
- **Type**: Grep verification
- **Command**: `grep -r 'use crate::uds' src/mcp/`
- **Expected**: Only the known exception in tools.rs (run_confidence_consumer etc.)
- **Risk**: R-07

Note: The cross-transport import exception (tools.rs importing from uds/listener.rs) is resolved when StatusService absorbs the maintain path.
