# Test Plan: C1 — Tool Name Normalizer

**File:** `crates/unimatrix-observe/src/session_metrics.rs`
**Function:** `normalize_tool_name(tool: &str) -> &str` (private)
**Risks:** R-01 (edge-case prefixes)

## Unit Test Expectations

All tests in `session_metrics.rs::tests`.

### test_normalize_tool_name_standard_prefix

Standard MCP prefix stripping.

```
Arrange: input = "mcp__unimatrix__context_search"
Act:     result = normalize_tool_name(input)
Assert:  result == "context_search"
```

### test_normalize_tool_name_passthrough_bare

Bare Unimatrix tool name passes through unchanged.

```
Arrange: input = "context_search"
Act:     result = normalize_tool_name(input)
Assert:  result == "context_search"
```

### test_normalize_tool_name_passthrough_claude_native

Claude-native tool passes through unchanged.

```
Arrange: input = "Read"
Act:     result = normalize_tool_name(input)
Assert:  result == "Read"
```

### test_normalize_tool_name_double_prefix

Only one layer of prefix stripped.

```
Arrange: input = "mcp__unimatrix__mcp__unimatrix__context_search"
Act:     result = normalize_tool_name(input)
Assert:  result == "mcp__unimatrix__context_search"
```

### test_normalize_tool_name_prefix_only

Prefix with no tool name after it.

```
Arrange: input = "mcp__unimatrix__"
Act:     result = normalize_tool_name(input)
Assert:  result == ""
```

### test_normalize_tool_name_empty_string

Empty input does not panic.

```
Arrange: input = ""
Act:     result = normalize_tool_name(input)
Assert:  result == ""
```

### test_normalize_tool_name_case_sensitive

Prefix matching is case-sensitive; uppercase does not match.

```
Arrange: input = "MCP__UNIMATRIX__context_search"
Act:     result = normalize_tool_name(input)
Assert:  result == "MCP__UNIMATRIX__context_search"
```

### test_normalize_tool_name_different_server

Different MCP server prefix is not stripped.

```
Arrange: input = "mcp__other_server__context_search"
Act:     result = normalize_tool_name(input)
Assert:  result == "mcp__other_server__context_search"
```

## Edge Cases from Risk Strategy

- R-01 scenarios 1-8 are covered 1:1 by the 8 tests above.
- `strip_prefix` returns `None` on mismatch, `unwrap_or` returns original -- no panic path.
- Function is O(1), no allocations (NFR-01).

## Integration Points

`normalize_tool_name` is called by `classify_tool` (C2) and the knowledge counter logic (C3). Those components' tests indirectly validate normalization through realistic MCP-prefixed inputs.
