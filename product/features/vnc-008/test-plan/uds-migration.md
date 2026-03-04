# Test Plan: uds-migration

## Risk Coverage: R-01, R-06

## Tests

### T-UDS-01: Compilation after move
- **Type**: Build verification
- **Command**: `cargo check --workspace`
- **Expected**: Zero errors
- **Risk**: R-01

### T-UDS-02: Test count preserved
- **Type**: Test count comparison
- **Command**: `cargo test --workspace`
- **Expected**: >= 1,664 tests passed, 0 failed
- **Risk**: R-06

### T-UDS-03: UDS listener tests pass in new location
- **Type**: Targeted test run
- **Command**: `cargo test -p unimatrix-server listener:: uds_listener::`
- **Expected**: All UDS listener tests pass
- **Risk**: R-06

### T-UDS-04: Hook tests pass in new location
- **Type**: Targeted test run
- **Command**: `cargo test -p unimatrix-server hook::`
- **Expected**: All hook tests pass
- **Risk**: R-06

### T-UDS-05: No cross-transport imports
- **Type**: Grep verification
- **Command**: `grep -r 'use crate::mcp' src/uds/`
- **Expected**: No matches
- **Risk**: R-07
