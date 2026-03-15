# Agent Report: crt-014-agent-1-pseudocode

## Summary

Produced pseudocode for all three crt-014 components (graph.rs NEW, search.rs MODIFIED, confidence.rs MODIFIED) plus OVERVIEW.md.

## Files Produced

- `/workspaces/unimatrix-crt-014/product/features/crt-014/pseudocode/OVERVIEW.md`
- `/workspaces/unimatrix-crt-014/product/features/crt-014/pseudocode/graph.md`
- `/workspaces/unimatrix-crt-014/product/features/crt-014/pseudocode/search.md`
- `/workspaces/unimatrix-crt-014/product/features/crt-014/pseudocode/confidence.md`

## Source Files Read

- `product/features/crt-014/IMPLEMENTATION-BRIEF.md`
- `product/features/crt-014/architecture/ARCHITECTURE.md`
- `product/features/crt-014/specification/SPECIFICATION.md`
- `product/features/crt-014/RISK-TEST-STRATEGY.md`
- `crates/unimatrix-engine/src/confidence.rs` (actual test lines: 890-920, not 720-752 per stale brief)
- `crates/unimatrix-engine/src/lib.rs`
- `crates/unimatrix-engine/Cargo.toml`
- `crates/unimatrix-server/src/services/search.rs`

## Revalidation Findings (confirmed from actual source)

- `DEPRECATED_PENALTY` / `SUPERSEDED_PENALTY` are at lines 60 and 65 in `confidence.rs` (not 52-57 per SCOPE.md).
- The 4 tests to remove are at lines 891-920 in `confidence.rs` (not 720-752 per the brief; the brief line numbers are stale post-crt-019 changes).
- `thiserror` is confirmed absent from `unimatrix-engine/Cargo.toml` — must be added. Implementation must verify whether it is a workspace dependency before choosing `{ workspace = true }` vs `"1"`.
- `search.rs` line 18 imports `DEPRECATED_PENALTY, SUPERSEDED_PENALTY` via `crate::confidence` (not `unimatrix_engine::confidence`) — the import path uses the re-export from `unimatrix-server/src/crate::confidence`. This is the correct import to modify.
- The search.rs tests that reference `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` are: T-SP-01, T-SP-02, T-SP-04, T-SP-05, T-SP-06, T-SP-07, T-SP-08, plus `test_utility_delta_inside_deprecated_penalty` and `test_utility_delta_inside_superseded_penalty` from the crt-018b block. All documented in search.md.
- `Store::query` appears accessible as a synchronous method on `Arc<Store>` for the `spawn_blocking` pattern. The search.rs field `store: Arc<Store>` is the correct handle to use. The async `AsyncEntryStore` wrapper is a separate field and must not be used inside `spawn_blocking`.

## Open Questions / Gaps

### Gap 1: ServiceError::Internal variant
`search.md` uses `ServiceError::Internal(e.to_string())` for the spawn_blocking join error. If `ServiceError` has no `Internal` variant, the implementer must use the closest existing variant. Check `crates/unimatrix-server/src/services/mod.rs` for the actual `ServiceError` enum.

### Gap 2: Store::query call site inside spawn_blocking
The pseudocode shows `unimatrix_core::Store::query(&store_for_graph, QueryFilter::default())`. Implementation must verify the exact method signature — it may be `store_for_graph.query(QueryFilter::default())` or accessed via `StoreAdapter`. Check `unimatrix-store/src/read.rs:282` as noted in the Integration Surface.

### Gap 3: thiserror workspace dependency
If `thiserror` is not in the workspace `[dependencies]` in the root `Cargo.toml`, use `thiserror = "1"` (standalone version) instead of `{ workspace = true }`.

### Gap 4: find_terminal_active depth boundary
The exact `>= MAX_TRAVERSAL_DEPTH` vs `> MAX_TRAVERSAL_DEPTH` boundary in the DFS loop must be verified by running AC-11 (11-hop chain → None) and the 10-hop boundary test during implementation. The pseudocode documents the correct intent: "A chain of 11 entries → None; exactly 10 → Some". Implementation must verify the off-by-one with unit tests before committing.

## Knowledge Stewardship

- Queried: /uni-query-patterns for unimatrix-engine graph penalty supersession -- no results returned (skill loaded but no MCP query results in output, likely no prior patterns stored for graph module work)
- Deviations from established patterns: none identified — the `spawn_blocking` pattern for synchronous Store calls matches existing engine usage throughout search.rs; the `tracing::warn!` pattern for dangling references matches existing patterns in build functions across the codebase.

## Self-Check

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names — every name traced to ARCHITECTURE.md Integration Surface
- [x] Output is per-component (OVERVIEW.md + 3 component files), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO or placeholder functions — all gaps flagged explicitly in this report
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/crt-014/pseudocode/`
- [x] Knowledge Stewardship report block included
