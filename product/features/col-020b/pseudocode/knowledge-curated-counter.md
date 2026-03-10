# C3: Knowledge Curated Counter

## Purpose

Add `knowledge_curated` counter to `build_session_summary` and apply `normalize_tool_name` to all three knowledge flow counters (served, stored, curated). This fixes the core bug where MCP-prefixed tool names never matched the counter filters.

## File: `crates/unimatrix-observe/src/session_metrics.rs`

## Modified Function: build_session_summary

### Change 1: Normalize tool names in knowledge_served counter (lines 157-166)

Current code:
```rust
let knowledge_in = session_records
    .iter()
    .filter(|r| {
        r.hook == HookType::PreToolUse
            && matches!(
                r.tool.as_deref(),
                Some("context_search") | Some("context_lookup") | Some("context_get")
            )
    })
    .count() as u64;
```

New pseudocode:
```
let knowledge_served = session_records.iter()
    .filter(|r| {
        r.hook == HookType::PreToolUse
            && r.tool.as_deref()
                .map(normalize_tool_name)
                .map_or(false, |t| matches!(t,
                    "context_search" | "context_lookup" | "context_get"
                ))
    })
    .count() as u64
```

Key changes: (a) variable renamed from `knowledge_in` to `knowledge_served`, (b) `normalize_tool_name` applied before matching.

### Change 2: Normalize tool names in knowledge_stored counter (lines 168-171)

Current code:
```rust
let knowledge_out = session_records
    .iter()
    .filter(|r| r.hook == HookType::PreToolUse && r.tool.as_deref() == Some("context_store"))
    .count() as u64;
```

New pseudocode:
```
let knowledge_stored = session_records.iter()
    .filter(|r| {
        r.hook == HookType::PreToolUse
            && r.tool.as_deref()
                .map(normalize_tool_name)
                .map_or(false, |t| t == "context_store")
    })
    .count() as u64
```

### Change 3: Add knowledge_curated counter (NEW, insert after knowledge_stored)

```
let knowledge_curated = session_records.iter()
    .filter(|r| {
        r.hook == HookType::PreToolUse
            && r.tool.as_deref()
                .map(normalize_tool_name)
                .map_or(false, |t| matches!(t,
                    "context_correct" | "context_deprecate" | "context_quarantine"
                ))
    })
    .count() as u64
```

### Change 4: Update SessionSummary construction (lines 173-184)

Replace `knowledge_in` and `knowledge_out` with new names and add `knowledge_curated`:
```
SessionSummary {
    session_id: session_id.to_string(),
    started_at: min_ts,
    duration_secs,
    tool_distribution,
    top_file_zones,
    agents_spawned,
    knowledge_served,     // was: knowledge_in
    knowledge_stored,     // was: knowledge_out
    knowledge_curated,    // NEW
    outcome: None,
}
```

## Error Handling

No new error paths. `normalize_tool_name` is infallible. `map_or(false, ...)` handles `None` tool names correctly (returns false, tool not counted).

## Key Test Scenarios

1. **MCP-prefixed knowledge_served**: Session with `mcp__unimatrix__context_search` events -> `knowledge_served > 0`
2. **MCP-prefixed knowledge_stored**: Session with `mcp__unimatrix__context_store` events -> `knowledge_stored > 0`
3. **MCP-prefixed knowledge_curated**: Session with `mcp__unimatrix__context_correct`, `mcp__unimatrix__context_deprecate`, `mcp__unimatrix__context_quarantine` events -> `knowledge_curated > 0`
4. **Mixed bare and prefixed**: Session with both `context_search` and `mcp__unimatrix__context_search` -> `knowledge_served` equals total of both
5. **Update existing test `test_session_summaries_knowledge_in_out`**: Rename field assertions from `.knowledge_in` to `.knowledge_served` and `.knowledge_out` to `.knowledge_stored`
6. **Curate in tool_distribution**: Session with curation tools -> `tool_distribution` contains `"curate"` key with correct count
7. **No curation tools**: Session without curation tools -> no `"curate"` key in `tool_distribution`, `knowledge_curated == 0`
