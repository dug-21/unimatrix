# Agent Report: 536-investigator

**Feature**: bugfix-536
**GH Issue**: https://github.com/dug-21/unimatrix/issues/536
**Agent ID**: 536-investigator
**Model**: claude-sonnet-4-6

---

## Summary

Diagnosed two reported bugs. Bug 1 confirmed and root-caused. Bug 2 is not a code defect.

---

## Bug 1 — Confirmed: Phase Timeline knowledge columns always zero

**Root cause**: `compute_phase_stats` and `categorize_tool_for_phase` in `crates/unimatrix-server/src/mcp/tools.rs` match tool names against bare strings (`"context_search"`) but `ObservationRecord.tool` always stores the MCP-prefixed form (`"mcp__unimatrix__context_search"`). No normalization is applied, so every filter misses and all counts stay zero.

**Introduced by**: col-026 (commit `56fec29c`) which added these functions without replicating the `normalize_tool_name` step from `session_metrics.rs::classify_tool`.

**Why tests missed it**: Existing tests (`test_phase_stats_knowledge_served_counted`, `test_phase_stats_tool_distribution`) use bare names through `make_obs_at()`. They pass because bare names do match the match arms — the MCP-prefixed form was never tested.

**Affected lines**:
- `tools.rs:3424` — `categorize_tool_for_phase`: `search` arm is dead code
- `tools.rs:3613-3618` — `knowledge_served` filter: never matches
- `tools.rs:3625` — `knowledge_stored` filter: never matches

**Fix**: Add `fn normalize_tool_name(tool: &str) -> &str { tool.strip_prefix("mcp__unimatrix__").unwrap_or(tool) }` to `tools.rs` and apply it in `categorize_tool_for_phase` and the two knowledge filters in `compute_phase_stats`. Mirrors ADR-001 (Unimatrix entry #918).

---

## Bug 2 — Not a code defect: Knowledge Reuse total vs. session sum

**Finding**: The two numbers measure different things and should not be expected to match.

| Source | What it counts |
|--------|---------------|
| Sessions table `knowledge_served` | PreToolUse invocation count (from ObservationRecords) |
| Knowledge Reuse "Total served" | Distinct entry IDs resolved from query_log + injection_log |

`compute_knowledge_reuse` sets `total_served = delivery_count = resolved_entries.len()` (distinct set cardinality). The session sum counts tool calls. One call returns multiple entries; the same entry is returned by many calls. No code is wrong; the presentation is ambiguous.

**Recommendation**: Rename the label in `render_knowledge_reuse` (retrospective.rs:1004) from `**Total served**` to `**Distinct entries served**` to prevent reader confusion.

---

## Proposed Fix Files

1. `crates/unimatrix-server/src/mcp/tools.rs`
   - Add `normalize_tool_name` (private fn, ~3 lines)
   - Update `categorize_tool_for_phase` (~1 line change)
   - Update two filter closures in `compute_phase_stats` (~4 lines changed)

2. `crates/unimatrix-server/src/mcp/response/retrospective.rs`
   - Label change in `render_knowledge_reuse` (~1 line)

3. Tests to add in `tools.rs::phase_stats_tests`:
   - `test_phase_stats_knowledge_served_mcp_prefixed`
   - `test_phase_stats_tool_distribution_mcp_prefixed`
   - `test_categorize_tool_for_phase_mcp_prefixed`

---

## GH Issue Comment

Posted at: https://github.com/dug-21/unimatrix/issues/536#issuecomment-4195821558

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entries #918 and #922 surfaced prior MCP prefix normalization decisions (col-020b ADR-001) which directly informed the diagnosis
- Stored: entry #4203 "Replicated match arms must replicate normalization — MCP prefix silently drops all tool matches" via `/uni-store-lesson`
