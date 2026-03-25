# Component: core-constants

**File:** `crates/unimatrix-core/src/observation.rs`
**Wave:** 1 (no dependencies)
**Action:** Modify

---

## Purpose

Add the `POSTTOOLUSEFAILURE` string constant to the `hook_type` module so that all downstream
consumers (friction.rs, metrics.rs, detection rules) can reference the canonical event type string
without repeating inline literals. Update the `response_snippet` doc comment to acknowledge
`PostToolUseFailure` as a valid source.

No struct changes. No enum changes. Per col-023 ADR-001: hook types are `pub const &str`, not
enum variants.

---

## New/Modified Declarations

### hook_type module — add POSTTOOLUSEFAILURE

The existing module currently has four constants:

```
pub mod hook_type {
    pub const PRETOOLUSE: &str = "PreToolUse";
    pub const POSTTOOLUSE: &str = "PostToolUse";
    pub const SUBAGENTSTART: &str = "SubagentStart";
    pub const SUBAGENTSTOPPED: &str = "SubagentStop";
}
```

Add one constant after `POSTTOOLUSE`, maintaining alphabetical grouping by lifecycle order:

```
pub mod hook_type {
    pub const PRETOOLUSE: &str = "PreToolUse";
    pub const POSTTOOLUSE: &str = "PostToolUse";
    pub const POSTTOOLUSEFAILURE: &str = "PostToolUseFailure";  // NEW -- col-027
    pub const SUBAGENTSTART: &str = "SubagentStart";
    pub const SUBAGENTSTOPPED: &str = "SubagentStop";
}
```

The constant value must exactly match the string Claude Code sends as the hook event name.
Casing is critical: `"PostToolUseFailure"` not `"post_tool_use_failure"`, not `"postToolUseFailure"`.

### ObservationRecord.response_snippet — update doc comment

Current doc comment (line 38 in existing file):
```
/// First 500 chars of response (PostToolUse only).
```

Replace with:
```
/// First 500 chars of response. Populated for PostToolUse (from tool_response object)
/// and PostToolUseFailure (from error string). None for all other event types.
```

This is a doc-comment-only change. The field type (`Option<String>`) is unchanged.

---

## Initialization Sequence

No constructor logic. The `hook_type` module is a static declaration block with no runtime
initialization.

---

## Data Flow

This component produces no output at runtime. It defines compile-time string constants consumed by:
- `friction.rs` `PermissionRetriesRule::detect()` — compares `record.event_type == hook_type::POSTTOOLUSEFAILURE`
- `friction.rs` `ToolFailureRule::detect()` — filters records by `record.event_type == hook_type::POSTTOOLUSEFAILURE`
- `metrics.rs` `compute_universal()` — widens terminal bucket with `hook_type::POSTTOOLUSEFAILURE`
- `listener.rs` test assertions — uses constant instead of inline string literal for R-11 mitigation

---

## Error Handling

No runtime errors. Compile-time only. If the constant value is misspelled:
- All detection rule comparisons silently miss every failure record (R-11)
- Mitigated by AC-02 test: `assert_eq!(hook_type::POSTTOOLUSEFAILURE, "PostToolUseFailure")`

---

## Key Test Scenarios

### T-CC-01: Constant value is exactly "PostToolUseFailure" (AC-02, R-11)

```
test hook_type_posttoolusefailure_value:
  assert_eq!(hook_type::POSTTOOLUSEFAILURE, "PostToolUseFailure")
  // Compile-time type assertion: must be &str
  let _: &str = hook_type::POSTTOOLUSEFAILURE;
```

Location: extend existing `test_hook_type_constants_are_str` test block in `observation.rs`.

The existing test asserts PRETOOLUSE, POSTTOOLUSE, SUBAGENTSTART, SUBAGENTSTOPPED. Add
POSTTOOLUSEFAILURE to the same test (additive, no modification to existing assertions).

### T-CC-02: Constant participates in detection rule comparison (integration concern — verified in friction-metrics tests)

In `friction.rs` test assertions, use `hook_type::POSTTOOLUSEFAILURE` (the constant) rather than
the inline string `"PostToolUseFailure"`. This ensures that if the constant value is wrong,
the test catches it rather than silently passing by comparing literal to literal.

---

## Anti-Patterns to Avoid

- Do NOT add a `HookType` enum variant (col-023 ADR-001 prohibits enums for hook types)
- Do NOT change any existing constant values
- Do NOT add any runtime logic to the `hook_type` module
- Do NOT modify `ObservationRecord` struct fields
