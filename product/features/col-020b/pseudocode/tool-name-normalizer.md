# C1: Tool Name Normalizer

## Purpose

Strip `mcp__unimatrix__` prefix from tool names before classification and knowledge flow counting. This fixes bug #192 where MCP-prefixed tool names fall through to "other" category and knowledge counters stay at 0.

## File: `crates/unimatrix-observe/src/session_metrics.rs`

## New Function: normalize_tool_name

```
fn normalize_tool_name(tool: &str) -> &str
    // Private helper. O(1), zero allocation.
    // strip_prefix returns Option<&str> -- use unwrap_or for passthrough.
    return tool.strip_prefix("mcp__unimatrix__").unwrap_or(tool)
```

Insert this function immediately before `classify_tool` (before line 187 in current source).

## Integration Points

This function is consumed by three call sites, all within `session_metrics.rs`:

1. **classify_tool** (C2) -- first line calls `normalize_tool_name(tool)` before the match
2. **knowledge_served counter** (C3, lines 157-166) -- normalize tool name before matching against `context_search`/`context_lookup`/`context_get`
3. **knowledge_stored counter** (C3, lines 168-171) -- normalize tool name before matching against `context_store`

It is NOT applied to `extract_file_path` -- Claude-native tools (`Read`, `Edit`, etc.) are never MCP-prefixed.

## Error Handling

No errors possible. Empty string input returns empty string. Partial prefix (e.g., `"mcp__unimatrix__"` with no tool name) returns empty string. Both are correct behavior -- they fall through to "other" in classify_tool.

## Key Test Scenarios

1. `normalize_tool_name("mcp__unimatrix__context_search")` -> `"context_search"`
2. `normalize_tool_name("Read")` -> `"Read"` (passthrough)
3. `normalize_tool_name("")` -> `""` (empty passthrough, no panic)
4. `normalize_tool_name("mcp__unimatrix__")` -> `""` (prefix-only, returns empty)
5. `normalize_tool_name("mcp__unimatrix__mcp__unimatrix__context_search")` -> `"mcp__unimatrix__context_search"` (single-layer strip only)
6. `normalize_tool_name("MCP__UNIMATRIX__context_search")` -> unchanged (case-sensitive)
7. `normalize_tool_name("mcp__other_server__context_search")` -> unchanged (only unimatrix prefix)
8. `normalize_tool_name("context_search")` -> `"context_search"` (bare name passthrough)
