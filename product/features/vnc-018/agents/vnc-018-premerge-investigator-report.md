# vnc-018 Pre-Merge Investigator Report

Agent ID: vnc-018-premerge-investigator

## Diagnosis

### Defect 1 — handle_chain infallible signature
Root cause: `handle_chain` in `graph_read_supersession.rs` has return type `ChainResult` (infallible). Invalid direction arm uses `warn! + empty return` — indistinguishable from AC-04 (non-existent ID returns empty). `handle_neighbors` already returns `Result<NeighborsResponse, ErrorData>`. Lesson #4473 (warn+continue masking failure paths) applies directly.

Fix: Change signature to `Result<ChainResult, ErrorData>`, replace warn+empty with `Err(ErrorData::new(ERROR_INVALID_PARAMS, ...))`, propagate with `?` in dispatch arm.

### Defect 2 — Advances/Motivates in PPR/BFS
Root cause: vnc-018 delivery inverted four write-only negative tests (installed by vnc-015 as gates against premature PPR promotion), then added code that those tests guarded against. vnc-015 ADR-006 (entry #4429) explicitly deferred these types.

Fix: Pure revert of the four `edges_of_type` blocks; restore original negative tests verbatim from git diff; update ADR-006.

## Affected Files
- `graph_read_supersession.rs` lines 22, 29–41 (D1)
- `graph_read.rs` lines 156–163 (D1 dispatch)
- `graph_ppr.rs` lines 139–152, 226–232 (D2)
- `graph_expand.rs` lines 149–156 (D2)
- `graph_ppr_tests.rs`, `graph_expand_tests.rs` (D2 tests)
- `ADR-006-advances-motivates-ppr-bfs.md` (D2 reversal record)

## Knowledge Stewardship

Queried: `context_briefing` — surfaced entries #4483 (PPR/BFS promotion pattern), #4429 (vnc-015 ADR-006 deferral), #4476 (chain mode envelope ADR), #4473 (warn+continue lesson).

Stored: Entry #4495 "Write-only negative tests are production gates — never invert without a reviewed ADR" via `/uni-store-lesson`.
