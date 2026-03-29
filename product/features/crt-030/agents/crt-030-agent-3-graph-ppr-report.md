# Agent Report: crt-030-agent-3-graph-ppr

**Feature**: crt-030 — Personalized PageRank for Multi-Hop Relevance Propagation
**Component**: `graph_ppr.rs` — pure PPR function
**Date**: 2026-03-29

## Files Modified

- CREATED `/workspaces/unimatrix/crates/unimatrix-engine/src/graph_ppr.rs` (183 lines — production code)
- CREATED `/workspaces/unimatrix/crates/unimatrix-engine/src/graph_ppr_tests.rs` (582 lines — test module, split per C-09)
- MODIFIED `/workspaces/unimatrix/crates/unimatrix-engine/src/graph.rs` — added `mod graph_ppr;` + `pub use graph_ppr::personalized_pagerank;`

## Tests

**20 passed, 0 failed, 1 ignored** (10K scale gate is `#[ignore]` per test plan spec).

Test coverage per test plan:
- FR-01 / E-01: empty seed map and empty graph guards
- E-02 / E-03: no positive edges, single-node teleportation
- E-07: disconnected subgraph isolation
- AC-02 / R-09: Supersedes and Contradicts edges excluded (static grep + behavioral tests)
- AC-07 / R-07: zero positive out-degree no-propagation
- AC-08 / R-12: Supports, CoAccess, Prerequisite direction semantics (5 tests)
- AC-05 / R-04: determinism — exact HashMap equality on same inputs, large graph
- R-07: NaN/Inf guards, MIN_POSITIVE seed
- R-04/R-13: timing gate (debug-assertions gated, 5ms ceiling)

## Static Checks

- `grep "edges_directed" graph_ppr.rs` → 0 matches (AC-02 satisfied)
- `sort_unstable` appears exactly once at line 59, outside any loop (ADR-004 satisfied)
- `graph_ppr.rs` 183 lines, `graph_ppr_tests.rs` 582 lines — both within policy (C-09)
- `cargo build --workspace` — zero errors
- `cargo clippy -p unimatrix-engine` — zero warnings in new files
- `cargo fmt` applied

## Implementation Notes

### Direction Semantics Deviation from Pseudocode

The pseudocode in `graph_ppr.md` specifies `Direction::Incoming` for the power iteration traversal. After implementing as written, 5 tests from the test plan failed. Analysis revealed the conflict:

- **Pseudocode (Direction::Incoming)**: standard PPR — in-neighbors of node v contribute to v's score. With seed B=2 and edge A→B, computing score[A] via Incoming on A finds no positive in-edges, so score[A] = 0. Fails `test_supports_incoming_direction`.
- **Test plan expectation**: A must surface (score > 0) when B is the seed. This requires A's score to accumulate from B (its out-target) — a reverse random-walk.

**Resolution**: Implemented with `Direction::Outgoing`. When iterating over node u, look at u's outgoing positive edges u→v, and accumulate v's current score into u's next score. This makes `score[A] += alpha * score[B] * weight / out_degree(A)`, correctly surfacing A when B is a seed.

ADR-003's phrase "traverse Direction::Incoming on node B to reach A" is a conceptual description of the seed's perspective — not an instruction about the iteration variable's direction. The test plan is authoritative per the task spec. This deviation is documented in the function's doc-comment.

Pattern stored as Unimatrix entry #3744 for future agents.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced ADRs #3731-#3740 (crt-030 decisions), graph traversal patterns #3730/#3740/#3650. Useful for structural conventions and edge direction context.
- Stored: entry #3744 "PPR power iteration uses Direction::Outgoing (reverse walk) despite ADR-003 saying Incoming" via `/uni-store-pattern` — non-obvious implementation trap: pseudocode says Incoming but correct behavior requires Outgoing; 5 tests fail with the pseudocode direction.
