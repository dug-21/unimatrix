# Pseudocode: edge_constants

## Purpose

Add three named `EDGE_SOURCE_*` constants to `unimatrix-store/src/read.rs` and re-export
them from `unimatrix-store/src/lib.rs`. This follows the exact pattern established by
`EDGE_SOURCE_NLI` (col-029 ADR-001) and `EDGE_SOURCE_CO_ACCESS` (crt-034).

The constants serve two purposes:
1. Call sites in `graph_enrichment_tick.rs` import them to avoid hard-coded string literals.
2. Test assertions verify constant values (R-07, AC-22).

## Files Modified

- `crates/unimatrix-store/src/read.rs` — add three constants after `EDGE_SOURCE_COSINE_SUPPORTS`
- `crates/unimatrix-store/src/lib.rs` — extend the existing `pub use read::{...}` block

## New Constants in `read.rs`

Insert immediately after the `EDGE_SOURCE_COSINE_SUPPORTS` constant block (currently at line ~1690).
Follow the doc-comment style of the existing constants.

```
/// Named constant for the S1 tag co-occurrence edge source value.
///
/// Written to `graph_edges.source` for all Informs edges produced by the
/// S1 (tag co-occurrence) path in `run_s1_tick`. Parallel to `EDGE_SOURCE_NLI`,
/// `EDGE_SOURCE_CO_ACCESS`, and `EDGE_SOURCE_COSINE_SUPPORTS` — prevents silent
/// string divergence between read.rs, graph_enrichment_tick.rs, and test assertions.
///
/// S1 edges use relation_type='Informs'; they are distinct from NLI-origin Informs
/// edges in the `source` column only. The UNIQUE(source_id, target_id, relation_type)
/// constraint means an S1 Informs edge and an NLI Informs edge for the same pair are
/// the same row — first writer wins (INSERT OR IGNORE semantics).
pub const EDGE_SOURCE_S1: &str = "S1";

/// Named constant for the S2 structural vocabulary edge source value.
///
/// Written to `graph_edges.source` for all Informs edges produced by the
/// S2 (structural vocabulary matching) path in `run_s2_tick`. Parallel to
/// `EDGE_SOURCE_S1` — same INSERT OR IGNORE / first-writer-wins semantics.
pub const EDGE_SOURCE_S2: &str = "S2";

/// Named constant for the S8 search co-retrieval edge source value.
///
/// Written to `graph_edges.source` for all CoAccess edges produced by the
/// S8 (search co-retrieval) path in `run_s8_tick`. Uses relation_type='CoAccess',
/// distinct from S1/S2 Informs edges by relation_type.
pub const EDGE_SOURCE_S8: &str = "S8";
```

## Re-export in `lib.rs`

The existing `pub use read::{...}` block (line ~38–42) currently reads:

```
pub use read::{
    CO_ACCESS_GRAPH_MIN_COUNT, ContradictEdgeRow, EDGE_SOURCE_CO_ACCESS,
    EDGE_SOURCE_COSINE_SUPPORTS, EDGE_SOURCE_NLI, GraphCohesionMetrics, GraphEdgeRow,
    StatusAggregates,
};
```

Extend it to add the three new constants in alphabetical position:

```
pub use read::{
    CO_ACCESS_GRAPH_MIN_COUNT, ContradictEdgeRow, EDGE_SOURCE_CO_ACCESS,
    EDGE_SOURCE_COSINE_SUPPORTS, EDGE_SOURCE_NLI, EDGE_SOURCE_S1, EDGE_SOURCE_S2,
    EDGE_SOURCE_S8, GraphCohesionMetrics, GraphEdgeRow, StatusAggregates,
};
```

## Error Handling

None. These are compile-time string constants. No runtime error path exists.

## Key Test Scenarios

### T-EC-01: constant value assertions (R-07, AC-22)
In the existing `read.rs` tests block (or a new `#[cfg(test)] mod tests` if none exists):

```
assert_eq!(EDGE_SOURCE_S1, "S1");
assert_eq!(EDGE_SOURCE_S2, "S2");
assert_eq!(EDGE_SOURCE_S8, "S8");
// Regression: verify existing constants are unchanged
assert_eq!(EDGE_SOURCE_NLI, "nli");
assert_eq!(EDGE_SOURCE_CO_ACCESS, "co_access");
```

### T-EC-02: re-export accessibility
Confirm all three are importable from `unimatrix_store` crate root in test code:

```
use unimatrix_store::{EDGE_SOURCE_S1, EDGE_SOURCE_S2, EDGE_SOURCE_S8};
// compilation succeeds — no runtime assertion needed
```

## Notes

- `read.rs` is already >1690 lines (noted in existing comment near line 1662).
  Adding three constant blocks adds ~30 lines. Still oversized but this is a
  pre-existing tech debt; no new split required by crt-041 scope.
- `inferred_edge_count` in `GraphCohesionMetrics` queries `WHERE source = 'nli'`.
  S1/S2/S8 constants do not affect that query. Verify the SQL is not broadened
  (C-10, R-13). No change to `GraphCohesionMetrics` or its SQL is needed.
