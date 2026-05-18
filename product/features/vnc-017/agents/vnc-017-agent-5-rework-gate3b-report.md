# Agent Report: vnc-017-agent-5-rework-gate3b

> Feature: vnc-017
> Task: Gate 3b rework — AC-04 test + missing agent report
> Agent ID: vnc-017-agent-5-rework-gate3b

## Files Modified

- `crates/unimatrix-server/src/mcp/tools.rs` — added `test_redirect_loop_correction_succeeds_when_redirect_fails` to `redirect_loop_tests` module (AC-04)
- `product/features/vnc-017/agents/vnc-017-agent-3-query-incoming-edges-report.md` — created missing store-layer agent report

## Tests

12 passed, 0 failed in `redirect_loop_tests` (includes the new AC-04 test).

## AC-04 Test Technique

The test uses a view-substitution trick to make `redirect_graph_edge` fail while allowing `query_incoming_edges` to succeed:

1. Seeds entry A, source entry C, and edge C→A
2. Renames `graph_edges` → `graph_edges_broken` via DDL
3. Creates a plain read-only VIEW named `graph_edges` that SELECTs from `graph_edges_broken`
4. `query_incoming_edges` (SELECT) succeeds via the view
5. `redirect_graph_edge` (DELETE + INSERT) fails — SQLite disallows DML on a plain view without INSTEAD OF triggers
6. Asserts: `Some(rs)` returned, `failed==1`, `redirected==0`, `skipped==0`, `found==1`

This validates the ADR-003 posture: redirect failures are warn+counted, never propagated to the correction caller.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — not called (no novel cross-feature context needed; task scope was narrow rework of a single test and a missing report file)
- Stored: nothing novel to store — the view-substitution technique for forcing DML failure while preserving SELECT is a useful test pattern, but it was devised here rather than discovered from the codebase. May be worth storing if it recurs in another feature.
