# C2: Tool Classification Extension

## Purpose

Modify `classify_tool` to (a) call `normalize_tool_name` before matching, and (b) add the `curate` category for curation tools.

## File: `crates/unimatrix-observe/src/session_metrics.rs`

## Modified Function: classify_tool

Current code (line 187-197):
```rust
fn classify_tool(tool: &str) -> &'static str {
    match tool {
        "Read" | "Glob" | "Grep" => "read",
        ...
    }
}
```

New pseudocode:
```
fn classify_tool(tool: &str) -> &'static str
    let normalized = normalize_tool_name(tool)   // C1
    match normalized:
        "Read" | "Glob" | "Grep"                                       => "read"
        "Edit" | "Write"                                               => "write"
        "Bash"                                                         => "execute"
        "context_search" | "context_lookup" | "context_get"            => "search"
        "context_store"                                                => "store"
        "context_correct" | "context_deprecate" | "context_quarantine" => "curate"   // NEW
        "SubagentStart"                                                => "spawn"
        _                                                              => "other"
```

Two changes:
1. Line 1: `let normalized = normalize_tool_name(tool)` then match on `normalized`
2. New arm: `"context_correct" | "context_deprecate" | "context_quarantine" => "curate"`

Tools that remain in "other": `context_briefing`, `context_status`, `context_enroll`, `context_retrospective`. These are administrative/diagnostic, not knowledge flow tools (FR-02.3).

## Error Handling

No errors. Unknown/empty tool names fall through to "other" (existing behavior preserved).

## Key Test Scenarios

1. All bare tool names map to expected categories (update `test_classify_tool_all_categories` to add curate entries)
2. All MCP-prefixed variants map identically:
   - `classify_tool("mcp__unimatrix__context_search")` -> `"search"`
   - `classify_tool("mcp__unimatrix__context_store")` -> `"store"`
   - `classify_tool("mcp__unimatrix__context_correct")` -> `"curate"`
   - `classify_tool("mcp__unimatrix__context_deprecate")` -> `"curate"`
   - `classify_tool("mcp__unimatrix__context_quarantine")` -> `"curate"`
3. Administrative tools stay "other":
   - `classify_tool("context_briefing")` -> `"other"`
   - `classify_tool("context_status")` -> `"other"`
   - `classify_tool("mcp__unimatrix__context_briefing")` -> `"other"`
