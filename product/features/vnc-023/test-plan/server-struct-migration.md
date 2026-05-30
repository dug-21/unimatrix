# Test Plan: server-struct-migration (C2)

## Component

`crates/unimatrix-server/src/server.rs` (production code) -- replace `Implementation` and `ServerInfo` struct literal construction with constructors/builders. Add `.with_description()`.

## Risks Covered

- **R-01 (Critical)**: Extension propagation regression (partial -- ServerInfo correctness affects initialize response)
- **R-03 (High)**: #[non_exhaustive] struct literal migration logic error
- **R-12 (Low)**: Description string not returned in initialize response

## Unit Test Expectations

### T-01: get_info returns correct server name (R-03, AC-02)
```
arrange: construct UnimatrixServer with test dependencies
act:     call server.get_info()
assert:  result.server_info.name == "unimatrix"
```

### T-02: get_info returns correct version (R-03, AC-02)
```
arrange: construct UnimatrixServer
act:     call server.get_info()
assert:  result.server_info.version == env!("CARGO_PKG_VERSION")
```

### T-03: get_info returns description (R-12, AC-08)
```
arrange: construct UnimatrixServer
act:     call server.get_info()
assert:  result.server_info.description == Some("Self-learning knowledge engine for agentic workflows")
         (or equivalent field path depending on rmcp 1.7 Implementation structure)
```

### T-04: get_info returns tools capability (R-03, AC-02)
```
arrange: construct UnimatrixServer
act:     call server.get_info()
assert:  result.capabilities.tools is Some (tools capability advertised)
```

### T-05: get_info returns instructions (R-03, AC-02)
```
arrange: construct UnimatrixServer with instructions = None (uses default)
act:     call server.get_info()
assert:  result.instructions is Some and non-empty
```

### T-06: get_info respects custom instructions (R-03)
```
arrange: construct UnimatrixServer with instructions = Some("custom instructions")
act:     call server.get_info()
assert:  result.instructions == Some("custom instructions")
```

## Compile Gate

### C-01: No struct literal construction of #[non_exhaustive] types (AC-02)
- **Assert**: `grep -n 'Implementation {' crates/unimatrix-server/src/server.rs` returns zero matches in production code (lines 1-1097, excluding `#[cfg(test)]` module)
- **Assert**: `grep -n 'ServerInfo {' crates/unimatrix-server/src/server.rs` returns zero matches in production code

## Integration Test Expectations

- Protocol suite `test_initialize_handshake` (or equivalent): MCP handshake returns valid ServerInfo with capabilities and instructions. Validates R-03 end-to-end.

## Edge Cases from Risk Strategy

- **ServerInfo with missing capabilities (R-03)**: If constructor defaults omit capabilities, clients refuse to send tool calls. T-04 catches this.
- **Empty instructions (R-03)**: If default instructions are lost during migration, clients get no guidance. T-05 catches this.
- **Description field absent (R-12)**: If `with_description()` is not called or builder chain is wrong, description is None. T-03 catches this.
