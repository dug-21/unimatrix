## ADR-001: normalize_tool_name as Private Function in session_metrics.rs

### Context

MCP tool names arrive as `mcp__unimatrix__context_search` in observation records but `session_metrics.rs` matches against bare names like `context_search`. A normalization function is needed. The question is where it lives and what its visibility is.

Options considered:
1. **Private fn in session_metrics.rs** -- used only by `classify_tool` and the knowledge flow counters in `build_session_summary`.
2. **Public fn in session_metrics.rs** -- exported for potential use by other modules.
3. **Public fn in unimatrix-core or unimatrix-observe::types** -- shared utility available to all crates.

### Decision

Private function in `session_metrics.rs`:

```rust
fn normalize_tool_name(tool: &str) -> &str {
    tool.strip_prefix("mcp__unimatrix__").unwrap_or(tool)
}
```

Rationale:
- The only consumers are `classify_tool`, the knowledge flow counters, and the new `knowledge_curated` counter -- all in the same file.
- No other module in any crate currently needs this normalization. `extract_file_path` operates on Claude-native tools which are never MCP-prefixed (col-020b SCOPE confirms this).
- The `mcp__unimatrix__` prefix is an artifact of Claude Code's MCP tool naming convention, not a domain concept. Elevating it to a shared utility creates a coupling to that convention across the codebase.
- If a second consumer appears later, promoting to `pub(crate)` is a trivial change.

### Consequences

- **Easier:** Single point of normalization logic. If Claude Code changes its prefix convention, only one function changes.
- **Easier:** No cross-crate dependency for a simple string operation.
- **Harder:** If another module needs the same normalization, it must either duplicate or the function must be promoted. This is an acceptable cost given no current second consumer exists.
