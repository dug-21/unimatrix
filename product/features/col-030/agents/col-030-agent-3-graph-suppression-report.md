# Agent Report: col-030-agent-3-graph-suppression

## Task

Implement Component 1 — `suppress_contradicts` in `graph_suppression.rs` for feature col-030 (Contradicts Collision Suppression).

## Files Modified

1. **CREATED** `/workspaces/unimatrix/crates/unimatrix-engine/src/graph_suppression.rs`
   - `pub fn suppress_contradicts(result_ids: &[u64], graph: &TypedRelationGraph) -> (Vec<bool>, Vec<Option<u64>>)`
   - Bidirectional traversal (Direction::Outgoing + Direction::Incoming) per ADR-003
   - All graph access via `edges_of_type` per ADR-002 (no direct `.edges_directed()`)
   - Option B chain propagation: outer loop processes all entries including suppressed ones
   - 8 unit tests inline in `#[cfg(test)]` (T-GS-01 through T-GS-08)

2. **MODIFIED** `/workspaces/unimatrix/crates/unimatrix-engine/src/graph.rs`
   - Added `#[path = "graph_suppression.rs"] mod graph_suppression;`
   - Added `pub use graph_suppression::suppress_contradicts;`
   - Two lines only; no other changes

## Tests

- **306 passed, 0 failed** in `unimatrix-engine`
- All 8 T-GS-* tests pass:
  - T-GS-01: Empty graph — all kept (R-06, AC-01)
  - T-GS-02: Outgoing rank-0→rank-1 suppressed (AC-02)
  - T-GS-03: Outgoing rank-0→rank-3 non-adjacent (FR-02)
  - T-GS-04: Chain — suppressed node propagates (Option B, T-GS-04)
  - T-GS-05: Non-Contradicts edges — no suppression (FR-04)
  - T-GS-06: Incoming direction — rank-1 suppressed (R-05, ADR-003, AC-03) — the critical bidirectional test
  - T-GS-07: Edge only rank-2→rank-3; mask `[true, true, true, false]` (corrected per spawn prompt)
  - T-GS-08: Empty TypedRelationGraph cold-start — all kept (FR-08, AC-05)
- **Full workspace**: zero failures across all test suites

## Issues Encountered

### E0583: file not found for module (resolved)

`mod graph_suppression;` inside `graph.rs` caused E0583 — Rust looked for `src/graph/graph_suppression.rs` (subdirectory) not `src/graph_suppression.rs` (sibling). Fixed with `#[path = "graph_suppression.rs"] mod graph_suppression;`, following the identical pattern already used by `graph.rs` for its test file (`#[path = "graph_tests.rs"] mod tests`).

### Missing `petgraph::visit::EdgeRef` trait import (resolved)

`.target()` and `.source()` methods on `EdgeReference` require the `EdgeRef` trait to be in scope. Added `use petgraph::visit::EdgeRef;` to `graph_suppression.rs`. The parent module imports this via `use petgraph::visit::{EdgeRef, IntoEdgeReferences};` but submodules do not inherit parent imports.

### Type inference on empty vec comparison (resolved)

`assert_eq!(mask_empty, vec![])` failed with E0282/E0283 due to ambiguous type inference with `serde_json::Value` in scope. Fixed with `Vec::<bool>::new()` and `Vec::<Option<u64>>::new()`.

### Pre-existing clippy warnings (not introduced by this agent)

`cargo clippy -D warnings` reports `collapsible_if` in `auth.rs` and `event_queue.rs`. These are pre-existing. `graph_suppression.rs` has zero clippy warnings.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 16 entries including #3627 (ADR-002 edges_of_type boundary), #3628 (ADR-003 bidirectional), #3631 (inline tests in new sibling module), #3616 (post-scoring filter pattern). All applied.
- Queried: `context_search` for graph traversal patterns — returned #3568, #3602, #3601. No exact match for `#[path]` attribute pattern.
- Stored: entry #3636 "Use #[path] attribute when declaring a submodule inside a file-based module (not mod.rs)" via /uni-store-pattern — documents the E0583 trap where `mod submodule;` inside a file-based module resolves to a subdirectory path rather than a sibling file.
