# Component: store-display-cap-constant

## Purpose

Define the display cap as a **single named constant** `GET_EDGE_DISPLAY_LIMIT` so every cap-
application site (the SQL `LIMIT`, the `…N more` render threshold, the `N more` arithmetic, and
tests) references one source of truth. Retuning the cap is a one-line edit. The constant governs
**only** the rendered set size — never the uncapped totals (FR-10) and never canonicalization
(FR-8). (FR-18 / C-12 / AC-13 / ADR-006 #5054.)

## Location

- Defined in `crates/unimatrix-store/src/read.rs`, **immediately below** `CO_ACCESS_GRAPH_MIN_COUNT`
  (verified at `read.rs:1867`). Same co-location convention as `EDGE_SOURCE_*` /
  `CO_ACCESS_GRAPH_MIN_COUNT` (ADR-002 crt-034).
- Re-exported from `crates/unimatrix-store/src/lib.rs` in the existing `pub use read::{…}` block
  (verified at `lib.rs:51-52`, alphabetical neighbour of `CO_ACCESS_GRAPH_MIN_COUNT`).

## New Definition

```
// read.rs (below CO_ACCESS_GRAPH_MIN_COUNT)

/// Display cap for the context_get next-hop edge affordance (D-05, vnc-037).
/// At most this many ranked edges render on a single context_get; totals (COUNT)
/// are UNCAPPED and unaffected by this value. i64 matches the sqlx `LIMIT ?` bind
/// convention (parallel to CO_ACCESS_GRAPH_MIN_COUNT).
pub const GET_EDGE_DISPLAY_LIMIT: i64 = 3
```

```
// lib.rs — extend the existing pub use read::{…} block (keep alphabetical ordering)
pub use read::{ … , CO_ACCESS_GRAPH_MIN_COUNT, … , GET_EDGE_DISPLAY_LIMIT, … }
```

## Type rationale

`i64` (not `usize`/`i32`) so the ranked select can bind it directly as the SQLite `LIMIT ?`
parameter without a cast. Consumers needing a length comparison (`edges.len()`, `total`) cast to
the relevant integer type at the comparison site (see serializer-seam / get-edge-assembly).

## Data Flow

- **Inputs**: none (compile-time constant).
- **Outputs / consumers**:
  - `store-ranked-query`: binds it to `LIMIT ?`.
  - `serializer-seam`: `…N more` threshold (`total > GET_EDGE_DISPLAY_LIMIT`) and arithmetic
    (`N = total - GET_EDGE_DISPLAY_LIMIT`).
  - tests across components seed/assert relative to the constant.

## Error Handling

None — a constant. No fallible code.

## Key Test Scenarios

- **type/value**: `assert_eq!(GET_EDGE_DISPLAY_LIMIT, 3i64)` and `let _: i64 = GET_EDGE_DISPLAY_LIMIT;`
  (mirrors the existing `CO_ACCESS_GRAPH_MIN_COUNT` test at `read.rs:1978`).
- **re-export reachable**: `let _ = unimatrix_store::GET_EDGE_DISPLAY_LIMIT;` from the server crate.
- **no-literal-3 (AC-13a)**: grep/static assertion that no literal `3` appears at the SQL `LIMIT`
  site or the `…N more` render sites; all reference the constant. (Asserted by the tester across
  store-ranked-query and serializer-seam, not here.)
- **cap-isolation (AC-13b)**: overriding the constant (e.g. cap=2 in a test build) shrinks **only**
  the rendered set; inbound/outbound totals and `↔`-once canonicalization are byte-unchanged.
  (Cross-component; this file only guarantees the single source.)
