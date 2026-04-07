# Agent Report: 536-agent-1-fix

**GH Issue:** #536
**Branch:** bugfix/536-phase-stats-tool-normalization
**Commit:** 22185b17

## Summary

Implemented all fixes described in the approved approach for GH #536.

## Files Modified

- `crates/unimatrix-observe/src/session_metrics.rs` — promoted `normalize_tool_name` from `fn` to `pub fn`
- `crates/unimatrix-observe/src/lib.rs` — added `normalize_tool_name` to the `session_metrics` re-export line
- `crates/unimatrix-server/src/mcp/tools.rs` — fixed `categorize_tool_for_phase` (normalize before match), fixed `compute_phase_stats` knowledge_served and knowledge_stored filter chains, added `make_mcp_obs_at` test helper, updated 5 existing bare-name MCP call sites, added new end-to-end test
- `crates/unimatrix-server/src/mcp/response/retrospective.rs` — changed `"**Total served**"` to `"**Distinct entries served**"` and updated 2 test assertions
- `crates/unimatrix-server/src/mcp/response/mod.rs` — formatting-only change from `cargo fmt`
- `crates/unimatrix-server/src/services/status.rs` — formatting-only change from `cargo fmt`

## New Tests

- `test_phase_stats_mcp_prefix_normalized_correctly` — exercises `compute_phase_stats` and `categorize_tool_for_phase` with production-prefixed names (`mcp__unimatrix__context_search`, etc.), asserting correct knowledge_served (3), knowledge_stored (1), and tool_distribution.search (3) counts

## Test Results

```
unimatrix-server: 2824 passed; 0 failed (lib)
                    46 passed; 0 failed (import tests)
                    16 passed; 0 failed (migration integration)
                    16 passed; 0 failed (import integration)
                     7 passed; 0 failed (pipeline e2e)
unimatrix-observe: 423 passed; 0 failed (lib)
                    22 passed; 0 failed + 44 + 6 (other suites)
```

No new failures. Zero errors.

## Issues

None.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entry #4203 directly documented this bug (categorize_tool_for_phase omitted normalize_tool_name pre-pass, all MCP-prefixed tools fell through to "other"). Entry #918 confirmed ADR-001 (normalize_tool_name is the canonical site, promote to pub for second consumer).
- Stored: entry #4204 "Use normalize_tool_name from unimatrix-observe for all MCP tool-name match sites; bare-name tests mask production failure" via /uni-store-pattern — captures the filter chain pattern, the pub-promotion approach, and the test-helper trap that makes bare-name tests pass while production is broken.
