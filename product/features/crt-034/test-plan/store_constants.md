# Test Plan: store_constants

## Component

**Files modified:**
- `crates/unimatrix-store/src/read.rs` — add `EDGE_SOURCE_CO_ACCESS` and `CO_ACCESS_GRAPH_MIN_COUNT`
- `crates/unimatrix-store/src/lib.rs` — re-export both constants in existing `pub use read::{...}` block

**Risks covered:** R-08 (threshold divergence between tick and migration)

---

## Unit Test Expectations

### Test: `test_edge_source_co_access_value`

**Covers:** AC-08, R-08

**Arrange:**
- No setup required — tests a `pub const` value

**Act:**
- Reference `unimatrix_store::EDGE_SOURCE_CO_ACCESS`

**Assert:**
- `assert_eq!(EDGE_SOURCE_CO_ACCESS, "co_access")`
- The constant is accessible at the crate root (`use unimatrix_store::EDGE_SOURCE_CO_ACCESS`)

**Location:** `crates/unimatrix-store/src/read.rs` `#[cfg(test)]` block

---

### Test: `test_co_access_graph_min_count_value`

**Covers:** AC-07, R-08

**Arrange:**
- No setup required

**Act:**
- Reference `unimatrix_store::CO_ACCESS_GRAPH_MIN_COUNT`

**Assert:**
- `assert_eq!(CO_ACCESS_GRAPH_MIN_COUNT, 3i64)`
- Type is `i64` (sqlx bind type for the WHERE threshold parameter)
- Constant is accessible at crate root

**Location:** `crates/unimatrix-store/src/read.rs` `#[cfg(test)]` block

---

### Test: `test_co_access_constants_colocated_with_nli`

**Covers:** ADR-002 structural compliance, R-08 (single authoritative value)

**Arrange:**
- Static / compile-time test

**Assert:**
- Both constants are defined in `read.rs` (not a separate `constants.rs` module)
- Both are re-exported via `lib.rs`

**Verification method:** Grep-based code review at Gate 3c — confirm `EDGE_SOURCE_CO_ACCESS`
and `CO_ACCESS_GRAPH_MIN_COUNT` appear in `read.rs` immediately following `EDGE_SOURCE_NLI`.

---

## Integration Test Expectations

No infra-001 integration tests required for this component. Constants are compile-time values
with no MCP-visible surface. Correctness is fully covered by unit tests.

---

## Edge Cases

None specific to constant values. The migration-constant divergence risk (R-08) is a future
mutation risk, not a current test scenario. The test `test_co_access_graph_min_count_value`
is the guard: if the constant is changed, it fails.

---

## Acceptance Criteria Mapped

| AC-ID | Test Function | Expected Result |
|-------|--------------|-----------------|
| AC-07 | `test_co_access_graph_min_count_value` | `CO_ACCESS_GRAPH_MIN_COUNT == 3i64`, accessible from crate root |
| AC-08 | `test_edge_source_co_access_value` | `EDGE_SOURCE_CO_ACCESS == "co_access"`, accessible from crate root |
