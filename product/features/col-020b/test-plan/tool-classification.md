# Test Plan: C2 — Tool Classification Extension

**File:** `crates/unimatrix-observe/src/session_metrics.rs`
**Function:** `classify_tool(tool: &str) -> &'static str` (private)
**Risks:** R-08 (MCP-prefix gap), R-09 (curate mapping error)

## Unit Test Expectations

All tests in `session_metrics.rs::tests`.

### test_classify_tool_all_categories (UPDATE EXISTING)

Update existing test to include `curate` category entries.

```
Arrange: (inline assertions)
Act/Assert:
  classify_tool("Read") == "read"
  classify_tool("Glob") == "read"
  classify_tool("Grep") == "read"
  classify_tool("Edit") == "write"
  classify_tool("Write") == "write"
  classify_tool("Bash") == "execute"
  classify_tool("context_search") == "search"
  classify_tool("context_lookup") == "search"
  classify_tool("context_get") == "search"
  classify_tool("context_store") == "store"
  classify_tool("context_correct") == "curate"      // NEW
  classify_tool("context_deprecate") == "curate"     // NEW
  classify_tool("context_quarantine") == "curate"    // NEW
  classify_tool("SubagentStart") == "spawn"
  classify_tool("anything_else") == "other"
  classify_tool("") == "other"
```

### test_classify_tool_mcp_prefixed (NEW)

All MCP-prefixed tool names resolve to correct categories via normalization.

```
Arrange: (inline assertions)
Act/Assert:
  classify_tool("mcp__unimatrix__context_search") == "search"
  classify_tool("mcp__unimatrix__context_lookup") == "search"
  classify_tool("mcp__unimatrix__context_get") == "search"
  classify_tool("mcp__unimatrix__context_store") == "store"
  classify_tool("mcp__unimatrix__context_correct") == "curate"
  classify_tool("mcp__unimatrix__context_deprecate") == "curate"
  classify_tool("mcp__unimatrix__context_quarantine") == "curate"
```

### test_classify_tool_admin_tools_are_other (NEW)

Administrative/diagnostic tools remain in "other", not "curate".

```
Arrange: (inline assertions)
Act/Assert:
  classify_tool("context_briefing") == "other"
  classify_tool("context_status") == "other"
  classify_tool("context_enroll") == "other"
  classify_tool("context_retrospective") == "other"
  classify_tool("mcp__unimatrix__context_briefing") == "other"
  classify_tool("mcp__unimatrix__context_status") == "other"
```

## Risk Coverage

- R-08: `test_classify_tool_mcp_prefixed` covers all 7 MCP-prefixed Unimatrix tools that have non-"other" categories.
- R-09: `test_classify_tool_all_categories` covers exhaustive bare-name mapping. `test_classify_tool_admin_tools_are_other` verifies non-curation tools are excluded from "curate".
