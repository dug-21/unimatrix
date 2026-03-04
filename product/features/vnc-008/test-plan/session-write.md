# Test Plan: session-write

## Risk Coverage: R-05, R-08

## Tests

### T-SW-01: Capability::SessionWrite variant exists
- **Type**: Compilation + grep
- **Command**: `grep 'SessionWrite' src/infra/registry.rs`
- **Expected**: Found in Capability enum
- **Risk**: R-05

### T-SW-02: SessionWrite serde round-trip
- **Type**: Unit test
- **Method**: Serialize vec![Capability::SessionWrite] with bincode v2, deserialize back. Verify equality.
- **Risk**: R-05

### T-SW-03: Backward-compatible deserialization
- **Type**: Unit test
- **Method**: Serialize vec![Capability::Read, Capability::Write] (without SessionWrite). Deserialize. Verify it works and does NOT contain SessionWrite.
- **Risk**: R-05

### T-SW-04: UDS_CAPABILITIES contains exact set
- **Type**: Unit test
- **Method**: Assert UDS_CAPABILITIES == &[Capability::Read, Capability::Search, Capability::SessionWrite].
- **Risk**: AC-20

### T-SW-05: uds_has_capability(SessionWrite) returns true
- **Type**: Unit test
- **Method**: Call uds_has_capability(Capability::SessionWrite). Assert true.
- **Risk**: R-08

### T-SW-06: uds_has_capability(Write) returns false
- **Type**: Unit test
- **Method**: Call uds_has_capability(Capability::Write). Assert false.
- **Risk**: R-08, AC-22

### T-SW-07: uds_has_capability(Admin) returns false
- **Type**: Unit test
- **Method**: Call uds_has_capability(Capability::Admin). Assert false.
- **Risk**: R-08, AC-23

### T-SW-08: UDS SessionRegister succeeds with SessionWrite
- **Type**: Integration test
- **Method**: Send SessionRegister via UDS dispatch path. Verify success response.
- **Risk**: R-08, AC-21

### T-SW-09: UDS ContextSearch succeeds with Search
- **Type**: Integration test / existing test
- **Method**: Send ContextSearch via UDS. Verify success.
- **Risk**: R-08

### T-SW-10: UDS Admin operation rejected
- **Type**: Integration test
- **Method**: If a HookRequest variant required Admin, send it via UDS. Verify error response. (Note: no current UDS operation requires Admin, so this tests the boundary enforcement mechanism itself.)
- **Risk**: AC-23

### T-SW-11: Existing UDS tests pass unchanged
- **Type**: Existing test suite
- **Command**: `cargo test -p unimatrix-server uds_listener:: listener::`
- **Expected**: All existing UDS tests pass.
- **Risk**: R-08

### T-SW-12: All changes confined to crates/unimatrix-server/
- **Type**: Git diff scope
- **Command**: `git diff --name-only main | grep -v 'crates/unimatrix-server/' | grep -v 'product/'`
- **Expected**: No matches (all changes in server crate or product docs)
- **Risk**: AC-27
