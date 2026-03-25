# Test Plan: core-constants (observation.rs)

**File:** `crates/unimatrix-core/src/observation.rs`
**Risks covered:** R-11 (constant misspelled), partially R-01 and R-03 (constant used in rules)

---

## Unit Test Expectations

### Test module location

In-module under `#[cfg(test)] mod tests` in `observation.rs`. The existing test block already
contains `test_hook_type_constants_are_str`. New tests are added to that same block.

---

### T-CC-01: `test_posttoolusefailure_constant_value`
**AC:** AC-02
**Risk:** R-11

Arrange: none (compile-time constant)
Act: evaluate `hook_type::POSTTOOLUSEFAILURE`
Assert:
- `assert_eq!(hook_type::POSTTOOLUSEFAILURE, "PostToolUseFailure")`
- The type must be `&str` (same pattern as existing `test_hook_type_constants_are_str`):
  `let _: &str = hook_type::POSTTOOLUSEFAILURE;`

**Why this exact assertion matters:** R-11 — a constant with value `"postToolUseFailure"` or
`"Post_Tool_Use_Failure"` would compile but silently break all string comparisons in rules and
listener. The exact-equality check is the only gate.

---

### T-CC-02: `test_hook_type_constants_are_str` — extend existing test
**AC:** AC-02 (extension)
**Risk:** R-11

The existing test in `observation.rs` currently asserts:
- `hook_type::PRETOOLUSE == "PreToolUse"`
- `hook_type::POSTTOOLUSE == "PostToolUse"`
- `hook_type::SUBAGENTSTART == "SubagentStart"`
- `hook_type::SUBAGENTSTOPPED == "SubagentStop"`

Extend this test (or verify via T-CC-01) to include `POSTTOOLUSEFAILURE`. This is consistent with
NFR-06 (string constant discipline per col-023 ADR-001). Do not add an enum variant — add only the
`&str` constant.

---

### T-CC-03: Constant used in assertions (not inline string literals)
**AC:** AC-02, AC-03, AC-04
**Risk:** R-11 (secondary)

This is a test-writing discipline check, not a separate test. All tests in `listener.rs`,
`friction.rs`, and `metrics.rs` that compare against `"PostToolUseFailure"` must use
`hook_type::POSTTOOLUSEFAILURE` (imported from `unimatrix_core::observation::hook_type`), not the
raw string literal. Using the constant means a spelling mistake in the constant definition is
caught at every test site, not just at the T-CC-01 site.

**Verification:** Code inspection during Stage 3c — `grep '"PostToolUseFailure"' crates/` must
return only the constant definition itself. All other comparison sites must reference the constant.

---

## Integration Test Expectations

This component has no MCP-visible behaviour. The constant is a compile-time `&str`. No
infra-001 integration test is needed.

---

## Edge Cases

- The constant must not include any trailing whitespace, embedded newline, or Unicode normalization
  variation. `assert_eq!` on a raw string literal is sufficient.
- The constant must be `pub` and in the `hook_type` module (same visibility as `POSTTOOLUSE`) so it
  is accessible from `unimatrix-server` and `unimatrix-observe` without re-exporting.

---

## Notes

- No struct changes are required (AC-03 already uses the existing `response_snippet: Option<String>`
  field on `ObservationRecord`).
- The doc comment update on `ObservationRecord::response_snippet` (mentioning `PostToolUseFailure`
  as a source) is part of this component but is verified by reading the doc, not by a test. Stage
  3c verifier should visually confirm the comment was updated.
