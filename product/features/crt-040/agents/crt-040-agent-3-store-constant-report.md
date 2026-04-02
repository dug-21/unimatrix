# Agent Report: crt-040-agent-3-store-constant

## Task

Add `pub const EDGE_SOURCE_COSINE_SUPPORTS: &str = "cosine_supports"` to
`crates/unimatrix-store/src/read.rs` following the `EDGE_SOURCE_NLI` and
`EDGE_SOURCE_CO_ACCESS` pattern, and re-export it from `lib.rs`.

## Files Modified

- `crates/unimatrix-store/src/read.rs` — constant declaration + doc comment + 3 unit tests
- `crates/unimatrix-store/src/lib.rs` — re-export added to existing `pub use read::{...}` line

## Implementation Notes

The constant is inserted immediately after `EDGE_SOURCE_CO_ACCESS` and before
`CO_ACCESS_GRAPH_MIN_COUNT` at line ~1676, matching the pseudocode insertion site.

The lib.rs re-export places `EDGE_SOURCE_COSINE_SUPPORTS` alphabetically between
`EDGE_SOURCE_CO_ACCESS` and `EDGE_SOURCE_NLI` in the comma-separated list, matching
the pseudocode spec. `cargo fmt` compacted the multi-line list form.

The doc comment on the constant includes the SR-04 UNIQUE constraint clarification
as mandated by the pseudocode checklist.

## Tests

All tests live in `read::tests` (the existing `#[cfg(test)] mod tests` block).

| Test | ID | Result |
|------|----|--------|
| `test_edge_source_cosine_supports_value` | TC-01 | pass |
| `test_edge_source_cosine_supports_length` | TC-04 | pass |
| `test_edge_source_constants_colocated` | TC-03 | pass |

TC-02 (crate-root accessibility) is covered structurally: the re-export compiles and
the workspace builds cleanly, which is the mechanism the test plan specifies.

**Unit test totals: 218 passed / 0 failed** (3 new, 215 pre-existing).

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] `cargo test -p unimatrix-store --lib` passes (218/0)
- [x] No `todo!()`, `unimplemented!()`, TODO, FIXME, or HACK in modified code
- [x] All modified files are within scope defined in the brief
- [x] Constant is `pub const` (not `pub(crate)`) — re-exportable
- [x] Value string is exactly `"cosine_supports"`
- [x] Doc comment references SR-04 UNIQUE constraint clarification
- [x] lib.rs re-export updated (not replaced)
- [x] No source file exceeds 500 lines (read.rs is pre-existing large file; change adds ~45 lines)
- [x] Committed on `feature/crt-040`: `impl(store-constant): add EDGE_SOURCE_COSINE_SUPPORTS constant and re-export (#487)`

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaces entries #3882 (crt-034 pattern) and
  #3591 (col-029 ADR-001). Both document exactly this constant-in-read.rs + re-export pattern.
  No novel findings emerged; the pattern is already fully documented.
- Stored: nothing novel to store — the EDGE_SOURCE_* constant + re-export pattern is already
  captured in entries #3882 and #3591. Storing a duplicate would dilute search quality.
