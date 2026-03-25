# Agent Report: col-027-agent-3-core-constants

## Task
Add `hook_type::POSTTOOLUSEFAILURE` constant to `unimatrix-core/src/observation.rs`, update the `response_snippet` doc comment, and add test `test_posttoolusefailure_constant_value` (AC-02).

## Files Modified
- `crates/unimatrix-core/src/observation.rs`

## Changes Made

1. Added `pub const POSTTOOLUSEFAILURE: &str = "PostToolUseFailure";` to the `hook_type` module after `POSTTOOLUSE`, maintaining lifecycle order. Annotated with `// col-027`.
2. Updated `response_snippet` doc comment from `"First 500 chars of response (PostToolUse only)."` to `"First 500 chars of response. Populated for PostToolUse (from tool_response object) and PostToolUseFailure (from error string). None for all other event types."`
3. Added test `test_posttoolusefailure_constant_value` — asserts exact string equality `"PostToolUseFailure"` and compile-time `&str` type annotation. Located in existing `#[cfg(test)] mod tests` block, consistent with `test_hook_type_constants_are_str` pattern.

No enum created or modified. No struct fields changed. No runtime logic added.

## Tests

- 17 passed, 0 failed (`cargo test -p unimatrix-core`)
- New test: `observation::tests::test_posttoolusefailure_constant_value` — passes

## Issues / Blockers

None.

## Knowledge Stewardship
- Queried: /uni-query-patterns for `hook_type string constants observation record` (category: pattern) -- found entry #3471 "Adding a new Claude Code hook event type: registration + dispatcher + storage + detection" already covers the col-027 hook addition pattern. Also found ADR #2903 (col-023 ADR-001: string constants over enum) confirming the constraint. No stale entries encountered.
- Stored: nothing novel to store -- the constant-addition pattern (add `pub const` to `hook_type` module, add exact-equality test) is already captured by #3471 and the existing col-023 ADR-001. This component followed the established pattern without deviation.
