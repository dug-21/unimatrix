# Agent Report: 972-agent-1-fix

Fix summary posted: https://github.com/dug-21/unimatrix/issues/972#issuecomment-5160728876

## Outcome
Implemented the approved membership-filter in `VectorIndex::load` (`crates/unimatrix-vector/src/persistence.rs`). On the load-SUCCESS branch only, enumerate the loaded graph's `origin_id`s across ALL layers (IterPoint, GH#286), build a HashSet, filter `vector_map` mappings to graph-present `data_id`s before `from_parts`, and `warn!` recorded/actual/dropped. `contains()` is now truthful for graph-absent entries → existing capped heal (status.rs Sub-case B) repopulates them; retained mappings stay searchable. No changes to status.rs, no re-embedding path, no DELETE cure. Added `tracing` dep to unimatrix-vector (was absent).

## Files modified
- `crates/unimatrix-vector/src/persistence.rs` — filter logic in `load` + 2 tests
- `crates/unimatrix-vector/Cargo.toml` — add `tracing = "0.1"`
- `Cargo.lock` — dep resolution

## New tests
- `test_load_graph_undercounts_vector_map_filters_absent_entries` (N1 guard: 200 points, all retained incl. level>=1; injected absent rows dropped)
- `test_load_graph_undercounts_retained_points_searchable` (retained self-searchable; absent never surfaces)

## Tests
- `cargo test -p unimatrix-vector --lib`: 115 passed, 0 failed (2 new)
- `cargo clippy -p unimatrix-vector --all-targets`: clean
- `cargo build --workspace --lib`: clean

## Issues / blockers
- Full-workspace binary link fails on pre-existing environmental `-lonnxruntime` miss (server bin), unrelated to this change. All libs compile.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search(lesson-learned) — #5718 (incident/mechanism), #5272 (boot load-fallback), #1712 (all-layers trap), #2676 (round-trip test), ADR #66/#180.
- Stored: entry #5719 "Fix for DB-only-copy IdMap/graph divergence (#972): membership-filter at load, NOT DELETE FROM vector_map" via context_store (lesson-learned; edge Supports #5718) — captures shipped fix, corrects #5718's DELETE-cure framing.
