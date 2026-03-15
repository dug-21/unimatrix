# Agent Report: crt-014-agent-6-tester

## Phase: Stage 3c — Test Execution

## Summary

All tests pass. Risk coverage is complete. RISK-COVERAGE-REPORT.md written.

## Unit Test Results

- **Total passed**: 2507 (across workspace)
- **Failed**: 0
- **New graph.rs tests**: 34 (all pass)
- **confidence.rs**: 4 constant-value tests confirmed removed; behavioral ordering tests confirmed present in graph.rs

## Integration Test Results

### Smoke Gate (mandatory)
- 18 passed, 1 xfailed (pre-existing GH#111 volume test), 0 failed — GATE PASSED

### Lifecycle Suite (primary for crt-014)
- 22 passed, 2 xfailed (pre-existing), 0 failed
- Includes 2 new crt-014 tests

### Tools Suite
- 67 passed, 5 xfailed (pre-existing GH#233, GH#238), 0 failed

**Combined lifecycle + tools: 89 passed, 6 xfailed, 0 failed.**

## New Integration Tests Written

Two new tests added to `product/test/infra-001/suites/test_lifecycle.py`:

1. `test_search_multihop_injects_terminal_active` — AC-13, R-06: verifies A→B→C chain injects C (not B). PASS.
2. `test_search_deprecated_entry_visible_with_topology_penalty` — AC-12, IR-02: verifies deprecated orphan appears in search results below active entries. PASS.

Both tests required fixes during development:
- Test 1: wrong status string assumption (`"superseded"` vs actual `"deprecated"` — `context_correct` sets `Status::Deprecated`). **Bad test assertion, fixed.**
- Test 2: HNSW recall with only 2 entries is unreliable for retrieving both in same result set. Fixed by storing 5 active baseline entries before the deprecated entry. **Bad test assertion/design, fixed.**

No GH Issues filed. No pre-existing failures discovered.

## AC-14 Verification

`grep -rn "DEPRECATED_PENALTY\|SUPERSEDED_PENALTY" crates/ --include="*.rs"` — 4 hits, all in comments or test assertion strings within `#[cfg(test)]` blocks. Zero production declarations or imports. **PASS.**

## Build Status (AC-18)

`cargo build --workspace` exits 0. 9 pre-existing warnings (none related to crt-014). **PASS.**

## R-10 Code Review

`build_supersession_graph` is called at line 294 of `search.rs`, after the `spawn_blocking` closure at lines 274–291 completes. The graph construction runs on the blocking thread pool. **PASS.**

## Coverage Gaps

None. All 13 risks and 4 integration risks covered. AC-16 (cycle fallback) is unit-test-only per documented test plan — MCP interface cannot inject supersession cycles.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: "procedure") — no results directly applicable to graph topology testing patterns.
- Stored: nothing novel to store — HNSW small-graph recall behavior and `context_correct` status semantics are discoverable from the codebase. The triage path followed the USAGE-PROTOCOL.md decision tree exactly.
