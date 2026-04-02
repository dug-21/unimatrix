# Test Plan: store-constant (Wave 1a)

**Files modified:**
- `crates/unimatrix-store/src/read.rs` — add `EDGE_SOURCE_COSINE_SUPPORTS` constant
- `crates/unimatrix-store/src/lib.rs` — re-export `EDGE_SOURCE_COSINE_SUPPORTS`

**Risk coverage:** AC-08 directly; R-02 indirectly (constant is the guard against string
drift between write site and any reader).

---

## Unit Test Expectations

All tests live in `#[cfg(test)] mod tests` inside `read.rs`, following the pattern of the
existing `test_edge_source_co_access_value` and `test_co_access_constants_colocated_with_nli`
tests.

### TC-01: Constant value is "cosine_supports"

```
fn test_edge_source_cosine_supports_value()
```

- Arrange: import `EDGE_SOURCE_COSINE_SUPPORTS` from `super::*`
- Act: (none — compile-time constant)
- Assert: `assert_eq!(EDGE_SOURCE_COSINE_SUPPORTS, "cosine_supports")`
- Covers: AC-08 (first half — value correctness)
- Note: mirrors `test_edge_source_co_access_value` pattern exactly

### TC-02: Constant is accessible from crate root

```
fn test_edge_source_cosine_supports_crate_root_accessible()
```

- Arrange: import `unimatrix_store::EDGE_SOURCE_COSINE_SUPPORTS` in a crate-level test
  (or assert via `use super::super::EDGE_SOURCE_COSINE_SUPPORTS` within `lib.rs` tests)
- Act: bind the constant to a local variable
- Assert: binding succeeds (compile test) and `assert_eq!(val, "cosine_supports")`
- Covers: AC-08 (second half — re-export from `lib.rs`)
- Note: the re-export is the mechanism tested, not the constant value itself. A compile
  failure here means `lib.rs` is missing the re-export.

### TC-03: All three EDGE_SOURCE constants co-located in read.rs

```
fn test_edge_source_constants_colocated()
```

- Arrange: `use super::*` in read.rs test module
- Act: bind all three constants
  ```rust
  let _nli: &str = EDGE_SOURCE_NLI;
  let _co: &str = EDGE_SOURCE_CO_ACCESS;
  let _cos: &str = EDGE_SOURCE_COSINE_SUPPORTS;
  ```
- Assert: compile-only (no runtime assertion needed — structure test)
- Covers: structural compliance — all three constants in the same module, accessible
  via the same import, preventing drift between modules
- Pattern: mirrors `test_co_access_constants_colocated_with_nli` from crt-034

### TC-04: Constant is `&'static str` with correct length

```
fn test_edge_source_cosine_supports_length()
```

- Assert: `EDGE_SOURCE_COSINE_SUPPORTS.len() == "cosine_supports".len()`
- This is a trivial sanity guard but catches accidental trailing whitespace or
  typo in the string literal.

---

## Integration Test Expectations

No infra-001 integration tests are needed for this component in isolation. The constant's
effect is visible only when used by `write_graph_edge` to write `graph_edges.source`. That
integration is tested at the path-c-loop level.

---

## Edge Cases

| Edge Case | Expectation |
|-----------|-------------|
| `EDGE_SOURCE_COSINE_SUPPORTS` used as SQL parameter | Parameterized query (not interpolated); no injection risk. Tested implicitly by write_graph_edge unit tests. |
| Constant renamed in future | Rename must update all call sites — having a named constant rather than a string literal makes this refactor safe via `cargo check` |

---

## Assertions Summary

| AC-ID | Test | Assertion |
|-------|------|-----------|
| AC-08 | TC-01 | `EDGE_SOURCE_COSINE_SUPPORTS == "cosine_supports"` |
| AC-08 | TC-02 | Constant accessible as `unimatrix_store::EDGE_SOURCE_COSINE_SUPPORTS` |
| (structural) | TC-03 | All three constants importable from `read.rs` via single `use super::*` |
