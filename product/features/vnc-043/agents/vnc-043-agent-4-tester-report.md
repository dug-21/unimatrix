# vnc-043 — Agent Report: Tester (Stage 3c Execution)

Agent: `vnc-043-agent-4-tester` | Phase: Test Execution (Stage 3c) | Feature: vnc-043 (GH #903)

## Verdict: PASS

All unit + integration tests green. All 11 risks (R-01..R-11) covered and passing. All 15 ACs verified.
No feature-caused failures. No new GH Issues filed. 3 new wire tests added and passing.

## What ran (real counts)

### Unit / cargo
- `cargo test -p unimatrix-server --lib`: **4373 passed, 0 failed, 1 ignored** (run twice — before and after the Python-only test additions; identical, no Rust source changed).
- `cargo test -p unimatrix-server --test graph_subgraph_integration`: **3 passed, 0 failed**.
- Full-workspace LINK smoke (`infra-002/check-workspace-link-smoke.sh`, #878 guard): **PASS** (rc=0, link holds at configured parallelism).

### Integration / infra-001 MCP wire (release binary + ORT dylib)
| Suite | Passed | xfailed | xpassed | Failed |
|-------|--------|---------|---------|--------|
| smoke (`-m smoke`, MANDATORY gate) | 28 | 0 | 0 | 0 |
| protocol | 13 | 0 | 0 | 0 |
| tools | 198 | 1 | 0 | 0 |
| lifecycle | 85 | 6 | 1 | 0 |
| edge_cases | 23 | 1 | 0 | 0 |

`tools`/`lifecycle` were run in node-id chunks (4 + 2) to fit the execution ceiling; sums above are across all chunks. Suites chosen per harness plan; `confidence`/`contradiction`/`security`/`volume` not run standalone (no such behavior touched) — their smoke members passed via the gate.

### New wire tests added (3, all PASS)
- `test_lifecycle.py::test_graph_subgraph_depth1_write_then_read_visible` (AC-07/01/11 forward — DoD one-shot, no tick wait)
- `test_lifecycle.py::test_graph_subgraph_depth1_truncated_false_at_realistic_fanin` (AC-15 — ≥30 incoming Advances, truncated==false)
- `test_tools.py::test_graph_subgraph_depth1_ordering_deterministic` (AC-14 — order + run-twice determinism)

## R-06 fixed-order sweep

**Clean — zero assertions required reframing.** Swept `graph_read_subgraph_bfs_tests.rs`, `graph_subgraph_integration.rs`, and the python `test_graph_subgraph_*` suites. All were written set-level (membership / triple-set) from the start; the two `edges[0]` reads in the bfs tests are on single-edge fixtures (index-safe). The FR-9 presentation-only sort changes no returned SET, so nothing flipped. Added positive ordering-determinism tests at unit + wire level.

## Triage

No feature-caused failure. All xfails are pre-existing with existing GH refs (GH#111 rate-limit, GH#405 deprecated-confidence timing, GH#406 multi-hop terminal, tick-interval markers) — none added/changed/removed by this work. 1 pre-existing non-strict tick-timing xfail in `test_lifecycle.py` incidentally xpassed; left as-is (out of scope to remove a pre-existing marker in a feature PR — flagged for validator/human). **No new GH Issues filed.**

## Notable finding (fixed in-scope)

First-draft strict wire tests failed with `self-referential edge rejected: source_id equals target_id`: near-identical fixture strings tripped the store's `DUPLICATE_THRESHOLD = 0.92` cosine near-duplicate collapse (`services/store_ops.rs`), returning a shared id so `context_edge` became self-referential. Fixed by drawing fixtures from a cross-domain distinct-subject pool. Existing subgraph wire tests masked this (they are dedup-tolerant and pass vacuously with 0 edges).

## Deliverables
- `product/features/vnc-043/testing/RISK-COVERAGE-REPORT.md`
- `product/features/vnc-043/agents/vnc-043-agent-4-tester-report.md` (this file)
- Test additions: `product/test/infra-001/suites/test_tools.py`, `product/test/infra-001/suites/test_lifecycle.py`

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (agent_id `vnc-043-agent-4-tester`) — surfaced ADR-001/003 vnc-043 (#5448), depth-asymmetry ADR-005 vnc-018 (#4479), staleness-in-text ADR-004 vnc-019 (#4493), and gate-verification lessons (#2758 grep-every-named-test, #4473 warn+continue masks missing failure-path tests). Applied #2758 by naming every test per risk in the coverage report; applied #4473 framing to the R-10 both-ways freshness check.
- Stored: entry **#5462** "infra-001 wire tests: cross-domain distinct fixtures to clear the 0.92 dedup collapse" via `context_store` (topic `testing`, category `pattern`, agent_id `vnc-043-agent-4-tester`) — a reusable, 2+-feature testing gotcha not previously captured (the test_tools.py inline note mentions dedup generally but not the self-referential-edge failure mode or the cross-domain fixture fix for strict linked-entry wire tests).
