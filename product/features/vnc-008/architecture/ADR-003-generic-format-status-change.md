## ADR-003: Generic format_status_change Replaces Three Identical Formatters

### Context

`response.rs` contains three near-identical functions: `format_deprecate_success`, `format_quarantine_success`, and `format_restore_success`. They differ only in:
- The action verb ("Deprecated" / "Quarantined" / "Restored")
- The JSON key ("deprecated" / "quarantined" / "restored")
- The displayed status ("deprecated" / "quarantined" / "active")

This is Refactor #6 from `product/research/optimizations/refactoring-analysis.md`. The response.rs split into `mcp/response/` is the natural time to unify these.

### Decision

Introduce a single `format_status_change()` function that accepts parameters for the variable parts:

```rust
pub(crate) fn format_status_change(
    entry: &EntryRecord,
    action: &str,         // "Deprecated", "Quarantined", "Restored"
    status_key: &str,     // "deprecated", "quarantined", "restored"
    status_display: &str, // "deprecated", "quarantined", "active"
    reason: Option<&str>,
    format: ResponseFormat,
) -> CallToolResult
```

The three original functions become thin wrappers:

```rust
pub(crate) fn format_deprecate_success(entry: &EntryRecord, reason: Option<&str>, format: ResponseFormat) -> CallToolResult {
    format_status_change(entry, "Deprecated", "deprecated", "deprecated", reason, format)
}
```

This preserves backward compatibility — existing callers in tools.rs continue calling the named functions. New code can call `format_status_change` directly.

### Consequences

- ~100 lines of duplicated formatting code reduced to ~35 lines (one generic + three one-line wrappers)
- Adding future status changes (e.g., "Archived") requires only a new one-line wrapper
- `format_enroll_success` is not unified with `format_status_change` because its structure differs (it has "created vs updated" logic, capability lists, trust levels — not a simple status change)
- Existing test coverage for the three functions remains valid
