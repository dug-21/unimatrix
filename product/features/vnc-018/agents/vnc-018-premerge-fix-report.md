# vnc-018 Pre-Merge Fix Report

Agent IDs: vnc-018-premerge-fix-d1, vnc-018-premerge-fix-d2

## Implementation Summary

### D1 — handle_chain direction validation (fix-d1)
Files modified:
- `crates/unimatrix-server/src/mcp/graph_read_supersession.rs` — signature changed to `Result<ChainResult, ErrorData>`; invalid direction arm now returns `Err(ERROR_INVALID_PARAMS)`; 3 existing tests updated to `.unwrap()`
- `crates/unimatrix-server/src/mcp/graph_read.rs` — chain dispatch arm updated to propagate with `?`

New test: `test_handle_chain_invalid_direction_returns_error` — asserts `Err(ERROR_INVALID_PARAMS)` with substring checks for "chain", "forward", "backward", "both".

### D2 — Advances/Motivates PPR/BFS revert (fix-d2)
Files modified:
- `crates/unimatrix-engine/src/graph_ppr.rs` — 4 `edges_of_type` blocks for Advances/Motivates removed
- `crates/unimatrix-engine/src/graph_expand.rs` — 2 `edges_of_type` blocks removed
- `crates/unimatrix-engine/src/graph_ppr_tests.rs` — 5 positive tests removed; 2 write-only negative tests restored verbatim
- `crates/unimatrix-engine/src/graph_expand_tests.rs` — 2 positive tests removed; 2 write-only negative tests restored verbatim
- `product/features/vnc-018/architecture/ADR-006-advances-motivates-ppr-bfs.md` — reversal decision recorded
- `positive_out_degree_weight_pub_for_test` retained (used by 4 surviving tests)

Tests: 3045 unimatrix-server passed, 417 unimatrix-engine passed, 0 failed.

## Knowledge Stewardship

Queried: `context_get(id: 4429)` — vnc-015 ADR-006 deferral rationale used to write accurate reversal notes in ADR and code comments. `context_get(id: 4473)` — warn+continue lesson confirmed before D1 fix approach.

Stored: Nothing novel. The reversal pattern is straightforward and the Unimatrix knowledge correction (entry #4480 superseded by #4496) was already handled by the architect before fix execution.
