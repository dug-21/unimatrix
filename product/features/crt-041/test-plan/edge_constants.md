# Test Plan: `edge_constants` Component

**Source files:**
- `crates/unimatrix-store/src/read.rs` — constant definitions
- `crates/unimatrix-store/src/lib.rs` — re-exports
**Risk coverage:** R-07
**AC coverage:** AC-22

---

## Component Scope

Three new named string constants following the established `EDGE_SOURCE_NLI` and
`EDGE_SOURCE_CO_ACCESS` pattern (col-029 ADR-001, crt-034 ADR-002):

```rust
// In crates/unimatrix-store/src/read.rs
pub const EDGE_SOURCE_S1: &str = "S1";
pub const EDGE_SOURCE_S2: &str = "S2";
pub const EDGE_SOURCE_S8: &str = "S8";
```

Re-exported from `crates/unimatrix-store/src/lib.rs` so that `unimatrix-server` code
can import them as `unimatrix_store::EDGE_SOURCE_S1` etc.

The sole risk is R-07: if these constants have wrong values (e.g., `"nli"`, `"s1"` lowercase,
or `"S 1"` with a space), every edge written by S1/S2/S8 will have the wrong source tag,
silently corrupting GNN feature construction. The values are compile-time constants and
are tested once — the test is trivial but mandatory.

---

## Unit Test Expectations

All tests in the `read.rs::tests` module using `#[test]` (sync). Compilation tests that
verify the re-export from `lib.rs` are confirmed implicitly by the test module importing
from the crate root.

### Constant Value Tests (R-07, AC-22)

**`test_edge_source_s1_value`** — R-07, AC-22
- Assert: `EDGE_SOURCE_S1 == "S1"`.
- Exact string, case-sensitive: uppercase "S", digit "1". Not "s1", not "S-1", not "S1 ".

**`test_edge_source_s2_value`** — R-07, AC-22
- Assert: `EDGE_SOURCE_S2 == "S2"`.
- Exact string: uppercase "S", digit "2".

**`test_edge_source_s8_value`** — R-07, AC-22
- Assert: `EDGE_SOURCE_S8 == "S8"`.
- Exact string: uppercase "S", digit "8".

**`test_edge_source_s1_s2_s8_distinct`** — R-07
- Assert: `EDGE_SOURCE_S1 != EDGE_SOURCE_S2`.
- Assert: `EDGE_SOURCE_S2 != EDGE_SOURCE_S8`.
- Assert: `EDGE_SOURCE_S1 != EDGE_SOURCE_S8`.
- Guards against copy-paste errors where two constants end up with the same value.

**`test_edge_source_s1_distinct_from_nli`** — R-07
- Assert: `EDGE_SOURCE_S1 != EDGE_SOURCE_NLI` (where EDGE_SOURCE_NLI = "nli").
- Assert: `EDGE_SOURCE_S2 != EDGE_SOURCE_NLI`.
- Assert: `EDGE_SOURCE_S8 != EDGE_SOURCE_NLI`.
- Critical guard: if any of S1/S2/S8 constants were accidentally set to "nli", edges
  written by those sources would be counted in `inferred_edge_count` (the NLI-only metric),
  violating R-13/AC-30.

**`test_edge_source_s1_distinct_from_co_access`** — R-07
- Assert: `EDGE_SOURCE_S1 != EDGE_SOURCE_CO_ACCESS`.
- Assert: `EDGE_SOURCE_S2 != EDGE_SOURCE_CO_ACCESS`.
- Assert: `EDGE_SOURCE_S8 != EDGE_SOURCE_CO_ACCESS`.
- S8 writes CoAccess relation_type edges but with source='S8', not 'co_access'.

### Re-Export Test (AC-22)

**`test_edge_source_constants_re_exported_from_crate_root`**
- This is verified by compilation: use `unimatrix_store::EDGE_SOURCE_S1` in the test
  module (importing from crate root, not from `unimatrix_store::read`). If the re-export
  in `lib.rs` is missing, this test fails to compile.
- Assert: `unimatrix_store::EDGE_SOURCE_S1 == "S1"`.
- Assert: `unimatrix_store::EDGE_SOURCE_S2 == "S2"`.
- Assert: `unimatrix_store::EDGE_SOURCE_S8 == "S8"`.

### Consistency with Existing Constants

**`test_existing_edge_source_constants_unchanged`**
- Assert: `EDGE_SOURCE_NLI == "nli"` (unchanged from col-029).
- Assert: `EDGE_SOURCE_CO_ACCESS == "co_access"` (unchanged from col-029).
- Guards against accidentally modifying existing constants while adding new ones.
- Note: if the exact values of EDGE_SOURCE_NLI or EDGE_SOURCE_CO_ACCESS differ, this test
  must be updated to match the actual existing values.

---

## Integration Test Expectations

The edge_constants component has no direct MCP-visible interface. Integration coverage
is indirect through `graph_enrichment_tick` tests that assert `source='S1'` etc. in the
DB. The constant values are validated at unit test level; if they are wrong, the graph
enrichment tick tests will fail with mismatched source values.

There is no separate integration test needed for this component alone.

---

## Implementation Notes for Delivery Agent

1. Place constants in `read.rs` adjacent to the existing `EDGE_SOURCE_NLI` and
   `EDGE_SOURCE_CO_ACCESS` constants.
2. Add re-export lines in `lib.rs` adjacent to the existing re-exports for NLI/CO_ACCESS.
3. Do NOT change `EDGE_SOURCE_NLI` or `EDGE_SOURCE_CO_ACCESS` values.
4. Do NOT add any new GraphCohesionMetrics fields (ADR-004: both `cross_category_edge_count`
   and `isolated_entry_count` already exist from col-029; crt-041 adds no new fields).

---

## Assertions Checklist

- [ ] `EDGE_SOURCE_S1 == "S1"` (uppercase S, digit 1) — AC-22
- [ ] `EDGE_SOURCE_S2 == "S2"` (uppercase S, digit 2) — AC-22
- [ ] `EDGE_SOURCE_S8 == "S8"` (uppercase S, digit 8) — AC-22
- [ ] All three constants are distinct from each other — R-07
- [ ] All three constants are distinct from `"nli"` — R-07, R-13
- [ ] All three constants are distinct from `"co_access"` — R-07
- [ ] Constants accessible from `unimatrix_store` crate root (not just `unimatrix_store::read`) — AC-22
- [ ] Existing `EDGE_SOURCE_NLI` and `EDGE_SOURCE_CO_ACCESS` values unchanged
