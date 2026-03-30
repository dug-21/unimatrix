# store_constants — Pseudocode

## Component: `unimatrix-store` public constants for co_access promotion

### Purpose

Expose two new public constants from `unimatrix-store` so that the promotion tick module
and any future consumers share a single authoritative definition. Modeled directly on the
existing `EDGE_SOURCE_NLI` pattern (ADR-002, #3824).

No new files. No new sub-modules. Both constants are added to `read.rs` at line ~1630
immediately after `EDGE_SOURCE_NLI`, then re-exported via `lib.rs`.

---

## Files to Modify

### 1. `crates/unimatrix-store/src/read.rs`

**Location**: Immediately after line 1630 (`pub const EDGE_SOURCE_NLI: &str = "nli";`)

**Additions**:

```
// After EDGE_SOURCE_NLI:

/// Named constant for the co_access-origin edge source value.
///
/// Written to `graph_edges.source` for all edges promoted by the recurring
/// co_access promotion tick (`run_co_access_promotion_tick`). Parallel to
/// `EDGE_SOURCE_NLI` — prevents silent string divergence between read.rs,
/// migration.rs, and co_access_promotion_tick.rs.
///
/// Matches the `source = 'co_access'` literal already written by the v13
/// migration bootstrap (migration.rs `CO_ACCESS_BOOTSTRAP_MIN_COUNT` step).
pub const EDGE_SOURCE_CO_ACCESS: &str = "co_access";

/// Minimum co_access pair count required for promotion to GRAPH_EDGES.
///
/// A co_access pair where count >= CO_ACCESS_GRAPH_MIN_COUNT qualifies for
/// a CoAccess edge in GRAPH_EDGES. Equals the bootstrap threshold used in
/// the v12→v13 migration (migration.rs `CO_ACCESS_BOOTSTRAP_MIN_COUNT = 3`).
///
/// Type is i64 to match sqlx binding conventions for SQLite INTEGER parameters.
/// Both the promotion tick and the migration must use the same threshold value —
/// this constant is the single authoritative source for the tick path; the
/// migration has its own file-private copy (not removed, out of scope for crt-034).
pub const CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3;
```

**No other changes to read.rs.** The existing read.rs already exceeds 1570 lines (noted
in the file's own comment at line 1627); the two new constants add 2 declaration lines +
doc comments, well within the 500-line guidance for new additions to an existing file.

---

### 2. `crates/unimatrix-store/src/lib.rs`

**Location**: The existing `pub use read::{...}` block (currently at lines 37-39):

```
// Current:
pub use read::{
    ContradictEdgeRow, EDGE_SOURCE_NLI, GraphCohesionMetrics, GraphEdgeRow, StatusAggregates,
};

// Modified — add CO_ACCESS_GRAPH_MIN_COUNT and EDGE_SOURCE_CO_ACCESS:
pub use read::{
    CO_ACCESS_GRAPH_MIN_COUNT, ContradictEdgeRow, EDGE_SOURCE_CO_ACCESS, EDGE_SOURCE_NLI,
    GraphCohesionMetrics, GraphEdgeRow, StatusAggregates,
};
```

Alphabetical ordering within the braced list is the existing convention (ContradictEdgeRow
before EDGE_SOURCE_NLI). New symbols inserted in alphabetical position.

---

## Data Flow

```
read.rs defines:
    EDGE_SOURCE_CO_ACCESS  → used by co_access_promotion_tick.rs (INSERT source column)
    CO_ACCESS_GRAPH_MIN_COUNT  → used by co_access_promotion_tick.rs (WHERE threshold, LIMIT bind)

lib.rs re-exports both:
    unimatrix_store::EDGE_SOURCE_CO_ACCESS
    unimatrix_store::CO_ACCESS_GRAPH_MIN_COUNT
```

---

## Error Handling

No functions; constants only. No error paths. Compile-time correctness only.

---

## Key Test Scenarios

**AC-07**: `CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3` exists and is re-exported from `unimatrix_store`.
- Test: `assert_eq!(unimatrix_store::CO_ACCESS_GRAPH_MIN_COUNT, 3i64);`
- Verify the type is `i64` (not `i32` or `usize`) — sqlx SQLite binding convention.

**AC-08**: `EDGE_SOURCE_CO_ACCESS: &str = "co_access"` exists and is re-exported from `unimatrix_store`.
- Test: `assert_eq!(unimatrix_store::EDGE_SOURCE_CO_ACCESS, "co_access");`

**R-08 (threshold divergence guard)**: Both `CO_ACCESS_GRAPH_MIN_COUNT` and
`CO_ACCESS_BOOTSTRAP_MIN_COUNT` (migration.rs) must equal 3. This cannot be enforced
structurally (migration constant is file-private, out of scope to change). The test
strategy calls for a code-review note confirming both are 3 at the time of delivery.

**Structural test**: After adding to the `pub use read::{...}` block, `cargo check` on
`unimatrix-store` must succeed with no ambiguity warnings from the re-export.
