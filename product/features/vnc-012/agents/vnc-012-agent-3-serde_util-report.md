# Agent Report: vnc-012-agent-3-serde_util

## Task
Implement `mcp/serde_util.rs` (new) and add `mod serde_util;` to `mcp/mod.rs`.

## Files Modified

- `crates/unimatrix-server/src/mcp/serde_util.rs` — CREATED
- `crates/unimatrix-server/src/mcp/mod.rs` — MODIFIED (added `mod serde_util;`)

## Implementation Summary

### serde_util.rs

Three `pub(crate)` deserializer functions implemented exactly per pseudocode:

- `deserialize_i64_or_string` — `I64OrStringVisitor` with `deserialize_any`; implements `visit_i64`, `visit_u64`, `visit_str`, `visit_string`, `visit_f64` (reject), `visit_f32` (reject).
- `deserialize_opt_i64_or_string` — `OptI64OrStringVisitor` with `deserialize_option`; implements `visit_none`, `visit_unit`, `visit_some` (delegates to `deserialize_i64_or_string`).
- `deserialize_opt_usize_or_string` — `OptUsizeOrStringVisitor` with `deserialize_option`; inner `UsizeOrStringVisitor` with `deserialize_any`; parses strings via `u64` first; uses `usize::try_from` (never `as usize`, C-06).

Float JSON Numbers rejected via `visit_f64`/`visit_f32` returning `de::Error::invalid_type` per FR-13.

### mod.rs

Single line added: `mod serde_util;` (private, between `response` and `tools` in alphabetical order).

## Test Results

**33 tests pass, 0 fail.**

```
mcp::serde_util::tests::test_deserialize_i64_integer_input       ok
mcp::serde_util::tests::test_deserialize_i64_string_input        ok
mcp::serde_util::tests::test_deserialize_i64_negative_string     ok
mcp::serde_util::tests::test_deserialize_i64_zero_string         ok
mcp::serde_util::tests::test_deserialize_i64_max_string          ok
mcp::serde_util::tests::test_deserialize_i64_min_string          ok
mcp::serde_util::tests::test_deserialize_i64_overflow_string     ok
mcp::serde_util::tests::test_deserialize_i64_nonnumeric_string   ok
mcp::serde_util::tests::test_deserialize_i64_empty_string        ok
mcp::serde_util::tests::test_deserialize_i64_float_string        ok
mcp::serde_util::tests::test_deserialize_i64_whitespace_string   ok
mcp::serde_util::tests::test_deserialize_i64_float_number        ok
mcp::serde_util::tests::test_deserialize_i64_bool_input          ok
mcp::serde_util::tests::test_deserialize_i64_array_input         ok
mcp::serde_util::tests::test_deserialize_opt_i64_integer_input   ok
mcp::serde_util::tests::test_deserialize_opt_i64_string_input    ok
mcp::serde_util::tests::test_deserialize_opt_i64_null_input      ok
mcp::serde_util::tests::test_deserialize_opt_i64_absent_field    ok
mcp::serde_util::tests::test_deserialize_opt_i64_nonnumeric_string ok
mcp::serde_util::tests::test_deserialize_opt_i64_float_string    ok
mcp::serde_util::tests::test_deserialize_opt_i64_float_number    ok
mcp::serde_util::tests::test_deserialize_opt_i64_negative_string ok
mcp::serde_util::tests::test_deserialize_opt_usize_integer_input ok
mcp::serde_util::tests::test_deserialize_opt_usize_string_input  ok
mcp::serde_util::tests::test_deserialize_opt_usize_zero_string   ok
mcp::serde_util::tests::test_deserialize_opt_usize_null_input    ok
mcp::serde_util::tests::test_deserialize_opt_usize_absent_field  ok
mcp::serde_util::tests::test_deserialize_opt_usize_negative_string ok
mcp::serde_util::tests::test_deserialize_opt_usize_u64_overflow_string ok
mcp::serde_util::tests::test_deserialize_opt_usize_nonnumeric_string ok
mcp::serde_util::tests::test_deserialize_opt_usize_float_number  ok
mcp::serde_util::tests::test_deserialize_opt_usize_float_string  ok
mcp::serde_util::tests::test_deserialize_opt_usize_negative_five_string ok
```

Covers all 25+ scenarios specified in the test plan (14 for i64, 8 for opt_i64, 11 for opt_usize). Exceeds test plan minimum.

## Build Status

- `cargo build -p unimatrix-server` — PASS (zero errors)
- `cargo fmt` — PASS (no changes after run)
- `cargo clippy` — no issues in `serde_util.rs` (pre-existing warnings in `unimatrix-engine` unrelated to this feature)

## Issues / Blockers

None.

## Commit

`impl(serde_util): add deserializer helpers for string-encoded integer MCP params (#448)`
Commit: `15ea297`

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned ADRs #3787–#3790 (all vnc-012 decisions) and pattern #3784 (serde+schemars override). All relevant and applied.
- Stored: entry #3791 "Use deserialize_option (not deserialize_any) for Option<T> serde visitor helpers" via `/uni-store-pattern` — the key structural insight is that `deserialize_option` must be used for optional fields (not `deserialize_any`), because only `deserialize_option` routes null to `visit_none`. Nothing in source code makes this visible; it only manifests as a silent wrong result at runtime.
