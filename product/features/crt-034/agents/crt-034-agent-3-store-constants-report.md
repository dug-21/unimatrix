# Agent Report: crt-034-agent-3-store-constants

## Task
Add `EDGE_SOURCE_CO_ACCESS` and `CO_ACCESS_GRAPH_MIN_COUNT` public constants to
`unimatrix-store/src/read.rs` immediately after `EDGE_SOURCE_NLI`, and re-export
both from `lib.rs` in the existing `pub use read::{...}` block.

## Files Modified

- `crates/unimatrix-store/src/read.rs` — added two `pub const` declarations with doc
  comments at line ~1631 (immediately after `EDGE_SOURCE_NLI`) plus 3 unit tests in the
  existing `#[cfg(test)] mod tests` block
- `crates/unimatrix-store/src/lib.rs` — added `CO_ACCESS_GRAPH_MIN_COUNT` and
  `EDGE_SOURCE_CO_ACCESS` to the `pub use read::{...}` re-export block in alphabetical order

## Tests

3 new unit tests added to `crates/unimatrix-store/src/read.rs`:

| Test | Covers | Result |
|------|--------|--------|
| `test_edge_source_co_access_value` | AC-08 | PASS |
| `test_co_access_graph_min_count_value` | AC-07, i64 type guard | PASS |
| `test_co_access_constants_colocated_with_nli` | ADR-002 structural compliance | PASS |

Full suite: **190 passed, 0 failed** (`cargo test -p unimatrix-store`)

## R-08 Code Review Note

`CO_ACCESS_BOOTSTRAP_MIN_COUNT` in `migration.rs` is file-private and equals `3`. The
new public `CO_ACCESS_GRAPH_MIN_COUNT` also equals `3i64`. Both are confirmed equal at
delivery time. The test `test_co_access_graph_min_count_value` is the regression guard:
any future change to either value will either fail the test or require an explicit update.

## Issues / Blockers

None.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entry #3824 (ADR-002 for this
  component), confirming co-location with `EDGE_SOURCE_NLI` in `read.rs` and alphabetical
  ordering in the `pub use` block. Applied directly.
- Stored: nothing novel to store — the pattern of adding pub const values alongside an
  existing sibling constant and re-exporting via lib.rs is already captured in entry #3591
  (ADR-001: EDGE_SOURCE_NLI named constant in unimatrix-store, col-029). This implementation
  followed that pattern exactly with no surprises.
