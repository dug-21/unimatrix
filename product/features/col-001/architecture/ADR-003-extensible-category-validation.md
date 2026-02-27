## ADR-003: Extensible Per-Category Validation

### Context

col-001 introduces per-category tag validation for outcome entries (required `type` tag, recognized key set). Scope risk SR-05 flags that this pattern may need to extend to other categories. The validation design should not hardcode outcome-specific logic in a way that prevents reuse.

### Decision

The outcome tag validation module (`outcome_tags.rs`) is structured as a standalone validation function with a clear entry point:

```rust
pub fn validate_outcome_tags(tags: &[String]) -> Result<(), ServerError>
```

The context_store handler calls this function conditionally:

```rust
if params.category == "outcome" {
    validate_outcome_tags(&tags)?;
}
```

If future categories need tag validation, the pattern is:
1. Create a new validation module (e.g., `process_tags.rs`)
2. Add a conditional call in context_store

This is intentionally simple. A trait-based or registry-based dispatch is premature given that only one category (outcome) has validation rules today. The conditional approach:
- Has zero overhead for categories without rules
- Is obvious and debuggable
- Can be refactored to a dispatch table when a second category needs validation

### Consequences

- **Easier**: No framework overhead. Clear, testable validation functions. Adding a second category is a 2-line change in context_store.
- **Harder**: If many categories gain validation rules, the conditional chain in context_store grows. At that point, refactor to a dispatch map. With 8 categories and at most 2-3 expected to have validation, this is unlikely.
