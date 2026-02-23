# Test Plan: C7 Server State Extension

## File: `crates/unimatrix-server/src/server.rs`

### Updated Tests

1. **make_server()** (MODIFY)
   - Pass `vector_index` as new parameter to `UnimatrixServer::new()`
   - All existing tests that call `make_server()` continue to work

### New Tests

2. **test_server_has_vector_index** (NEW)
   - `make_server()` -> verify vector_index field is accessible
   - Call `allocate_data_id()` on it to confirm it works

### No Additional Tests Needed

The C7 change is structural (adding a field + updating constructor).
Functional testing of the vector_index field happens in C6 (server-transactions)
and C1 (tool-handlers) tests.

### AC Coverage

| AC | Test |
|----|------|
| AC-30 | Verified through C6 tests (insert_with_audit uses vector_index) |
