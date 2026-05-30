# Test Plan: server-test-migration (C3)

## Component

`crates/unimatrix-server/src/server.rs` (test module, `#[cfg(test)]`) -- replace `ClientInfo` and `Implementation` struct literal construction in test helpers with constructors/builders.

## Risks Covered

- **R-01 (Critical)**: Extension propagation regression (test infrastructure must work to validate R-01)
- **R-08 (Medium)**: serve_client test helper renamed or moved

## Unit Test Expectations

No new tests for this component -- the test infrastructure IS the test. Success is measured by existing tests compiling and passing.

### T-01: Test module compiles (R-08, AC-03)
```
assert: cargo test -p unimatrix-server --no-run exits 0
```

### T-02: ClientInfo construction uses builder/constructor (AC-03)
```
assert: grep -n 'ClientInfo {' in test module returns zero matches
```

### T-03: serve_client call compiles (R-08)
```
assert: rmcp::serve_client(client_info, transport) call site compiles
        If renamed in 1.7, update import path. Mechanical fix.
```

### T-04: Existing client_type_map tests pass (R-01 partial, AC-12)
```
assert: cargo test -p unimatrix-server -- client_type_map exits 0
        These tests exercise the initialize handshake and validate
        that client names are captured correctly.
```

## Compile Gate

### C-01: No struct literal construction of #[non_exhaustive] types in test module (AC-03)
- **Assert**: `grep -n 'ClientInfo {' crates/unimatrix-server/src/server.rs` returns zero matches (test module, lines 3250+)
- **Assert**: `grep -n 'Implementation {' crates/unimatrix-server/src/server.rs` returns zero matches in test module

## Integration Test Expectations

- All existing integration tests that use the MCP handshake continue to pass. The test helper constructs `ClientInfo` -- if broken, every handshake-dependent test fails.

## Edge Cases from Risk Strategy

- **ClientInfo gained new required fields in 1.7 (Risk Strategy edge case 6)**: If `ClientInfo` has new non-optional fields without defaults, constructor must set them. Compiler catches this.
- **serve_client renamed (R-08)**: If `rmcp::serve_client` moved to a submodule (e.g., `rmcp::service::serve_client`), the fix is a one-line import change. Compiler catches this.
- **ProtocolVersion::LATEST removed or renamed**: Test helper references `ProtocolVersion::LATEST`. If renamed, compiler catches it. Fix is mechanical.
