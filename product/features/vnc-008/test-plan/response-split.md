# Test Plan: response-split

## Risk Coverage: R-04, R-09

## Tests

### T-RESP-01: Compilation after split
- **Type**: Build verification
- **Command**: `cargo check --workspace`
- **Expected**: Zero errors
- **Risk**: R-01, R-09

### T-RESP-02: All existing response tests pass
- **Type**: Test suite
- **Command**: `cargo test -p unimatrix-server response::`
- **Expected**: All tests pass in new module locations
- **Risk**: R-09

### T-RESP-03: format_status_change produces same output as format_deprecate_success
- **Type**: Unit test (6 cases: 3 formats x 2 reason states)
- **Location**: `src/mcp/response/mutations.rs` tests module
- **Method**: For each (format, reason) pair, call both functions with same EntryRecord, assert output equality
- **Risk**: R-04

### T-RESP-04: format_status_change produces same output as format_quarantine_success
- **Type**: Unit test (6 cases)
- **Method**: Same as T-RESP-03 for quarantine variant
- **Risk**: R-04

### T-RESP-05: format_status_change produces same output as format_restore_success
- **Type**: Unit test (6 cases)
- **Method**: Same as T-RESP-03 for restore variant
- **Risk**: R-04

### T-RESP-06: parse_format accessible from mod.rs
- **Type**: Compilation verification
- **Expected**: `use crate::mcp::response::parse_format` compiles
- **Risk**: R-09

### T-RESP-07: ResponseFormat accessible from mod.rs
- **Type**: Compilation verification
- **Expected**: `use crate::mcp::response::ResponseFormat` compiles
- **Risk**: R-09

### T-RESP-08: StatusReport accessible from status.rs
- **Type**: Compilation verification
- **Expected**: `use crate::mcp::response::StatusReport` compiles
- **Risk**: R-09

### T-RESP-09: entry_to_json accessible within sub-modules
- **Type**: Compilation verification
- **Expected**: `super::entry_to_json` resolves in entries.rs, mutations.rs
- **Risk**: R-09

### T-RESP-10: No standalone response.rs at root
- **Type**: File absence check
- **Command**: `test ! -f crates/unimatrix-server/src/response.rs`
- **Expected**: Returns 0 (file does not exist)
- **Risk**: AC-12
