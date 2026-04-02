# Wave 1a: Store Constant — EDGE_SOURCE_COSINE_SUPPORTS

## Purpose

Define the named string constant for the new edge signal source so all downstream code
(Path C write call, test assertions, future graph queries) shares one source of truth.
Follows the pattern established by `EDGE_SOURCE_NLI` (col-029) and `EDGE_SOURCE_CO_ACCESS`
(crt-034) in the same file.

---

## Files Modified

| File | Change Type |
|------|-------------|
| `crates/unimatrix-store/src/read.rs` | Add constant declaration |
| `crates/unimatrix-store/src/lib.rs` | Add constant to re-export list |

---

## New Constant: `EDGE_SOURCE_COSINE_SUPPORTS`

### Location in read.rs

Insert immediately after the `EDGE_SOURCE_CO_ACCESS` constant block and before the
`CO_ACCESS_GRAPH_MIN_COUNT` constant (which currently follows `EDGE_SOURCE_CO_ACCESS`).

```
// read.rs — insertion site (after line ~1676)

/// Named constant for the cosine-similarity-derived Supports edge source value.
///
/// Written to `graph_edges.source` for all edges produced by the pure-cosine
/// Supports detection path (Path C) in `run_graph_inference_tick`. Parallel to
/// `EDGE_SOURCE_NLI` and `EDGE_SOURCE_CO_ACCESS` — prevents silent string
/// divergence between read.rs, nli_detection_tick.rs, and test assertions.
///
/// The `source` column in `graph_edges` is NOT part of the UNIQUE constraint
/// `UNIQUE(source_id, target_id, relation_type)` — so edges written by Path C
/// and edges written by Path B (NLI) for the same pair are deduplicated by
/// `INSERT OR IGNORE` on the relation_type dimension, not the source dimension.
/// First writer wins. (ARCHITECTURE.md SR-04, confirmed from db.rs DDL)
pub const EDGE_SOURCE_COSINE_SUPPORTS: &str = "cosine_supports";
```

### No schema change required

`graph_edges.source` is `TEXT NOT NULL DEFAULT ''`. The column already exists and
accepts arbitrary string values. No migration step.

---

## Re-export in lib.rs

### Current re-export line (around line 39)

```
pub use read::{
    CO_ACCESS_GRAPH_MIN_COUNT, ContradictEdgeRow, EDGE_SOURCE_CO_ACCESS, EDGE_SOURCE_NLI,
    GraphCohesionMetrics, GraphEdgeRow, StatusAggregates,
};
```

### Updated re-export line

```
pub use read::{
    CO_ACCESS_GRAPH_MIN_COUNT, ContradictEdgeRow, EDGE_SOURCE_CO_ACCESS,
    EDGE_SOURCE_COSINE_SUPPORTS, EDGE_SOURCE_NLI,
    GraphCohesionMetrics, GraphEdgeRow, StatusAggregates,
};
```

Add `EDGE_SOURCE_COSINE_SUPPORTS` alphabetically between `EDGE_SOURCE_CO_ACCESS` and
`EDGE_SOURCE_NLI` to maintain readability.

---

## Error Handling

None. This is a `const` declaration — no runtime failure modes.

---

## Key Test Scenarios

### AC-08: Constant value is exactly "cosine_supports"

```
// In read.rs test module — mirror the existing test_edge_source_co_access_value pattern

fn test_edge_source_cosine_supports_value() {
    assert_eq!(EDGE_SOURCE_COSINE_SUPPORTS, "cosine_supports");
}
```

### AC-08: Constant is accessible via crate root import

```
// Verifies lib.rs re-export — accessible via unimatrix_store::EDGE_SOURCE_COSINE_SUPPORTS

fn test_cosine_supports_constant_accessible_from_crate_root() {
    // Import via unimatrix_store::* or explicit path — assert equality
    let _: &str = unimatrix_store::EDGE_SOURCE_COSINE_SUPPORTS;
    assert_eq!(unimatrix_store::EDGE_SOURCE_COSINE_SUPPORTS, "cosine_supports");
}
```

### Structural cohabitation test (mirror of test_co_access_constants_colocated_with_nli)

```
fn test_all_three_edge_source_constants_colocated() {
    // All three constants must be accessible from the same super::* import (read.rs)
    let _nli: &str = EDGE_SOURCE_NLI;
    let _co: &str = EDGE_SOURCE_CO_ACCESS;
    let _cos: &str = EDGE_SOURCE_COSINE_SUPPORTS;
    // All three defined in read.rs — compile success verifies co-location
}
```

---

## Checklist

- [ ] Constant is `pub const`, not `pub(crate)` — must be re-exportable from `lib.rs`
- [ ] Value string is exactly `"cosine_supports"` (no spaces, no underscore variant)
- [ ] Doc comment references the UNIQUE constraint clarification (SR-04)
- [ ] `lib.rs` re-export updated (adding to existing comma-separated list, not replacing)
- [ ] Unit test asserts exact value
- [ ] Unit test verifies crate-root accessibility
