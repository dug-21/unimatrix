# Agent Report: col-030-agent-1-architect

## Outputs

### ARCHITECTURE.md
`product/features/col-030/architecture/ARCHITECTURE.md`

### ADR Files

| ADR | File | Unimatrix ID |
|-----|------|--------------|
| ADR-001 | `architecture/ADR-001-graph-suppression-module-split.md` | #3626 |
| ADR-002 | `architecture/ADR-002-edges-of-type-boundary.md` | #3627 |
| ADR-003 | `architecture/ADR-003-bidirectional-contradicts-query.md` | #3628 |
| ADR-004 | `architecture/ADR-004-single-indexed-mask-application.md` | #3629 |
| ADR-005 | `architecture/ADR-005-no-config-toggle.md` | #3630 |

## Key Decisions Made

1. **`graph_suppression.rs` module split (ADR-001, #3626)**: `suppress_contradicts` lives in a new
   sibling module re-exported from `graph.rs`. `graph.rs` is 587 lines — inlining would violate
   the 500-line limit. `lib.rs` needs no new top-level `pub mod`.

2. **`edges_of_type` sole boundary (ADR-002, #3627)**: `suppress_contradicts` uses only
   `edges_of_type(RelationType::Contradicts, Direction::Outgoing/Incoming)`. No direct
   `.edges_directed()` calls. Unit tests exercise Contradicts for the first time, resolving SR-01.

3. **Bidirectional query required (ADR-003, #3628)**: NLI writes edges unidirectionally —
   direction is determined by detection order, not semantics. Both Outgoing and Incoming must
   be unioned. O(n * 2 * degree_c), negligible for k≤20.

4. **Single indexed pass for mask (ADR-004, #3629)**: After Step 10 floors,
   `results_with_scores.len()` ≤ `final_scores.len()`. Step 10b zips the aligned prefix of
   both Vecs and applies the keep-mask in one pass. `final_scores` must be shadowed (it is
   a `let` binding at line 893 — implementation brief must call this out).

5. **No config toggle (ADR-005, #3630)**: Suppression is always-on when `use_fallback = false`.
   SR-04 observability via `debug!()` log line. False-positive escape via
   `nli_contradiction_threshold`, not a suppression toggle.

## Critical Implementation Notes for Downstream Agents

- `final_scores` at line 893 is `let` not `let mut`. Step 10b must shadow it:
  `let final_scores = new_fs;`
- The aligned prefix is `final_scores[..results_with_scores.len()]` — must not use
  `final_scores.iter()` directly in the mask pass (would include post-floor-count elements).
- `node_index: HashMap<u64, NodeIndex>` on `TypedRelationGraph` provides O(1) ID→NodeIndex
  lookup needed by `suppress_contradicts` before calling `edges_of_type`.
- Integration test must use `Store::insert_graph_edge()` (or production write path) for
  GRAPH_EDGES setup — NOT `create_graph_edges_table` (SR-07, pre-v13 schema only, entry #3600).
- SR-05 mandatory: the positive suppression integration test in search.rs is a required gate,
  not optional. Zero-regression eval gate alone is insufficient.

## Open Questions

None. All open questions from SCOPE.md are resolved by this architecture.
