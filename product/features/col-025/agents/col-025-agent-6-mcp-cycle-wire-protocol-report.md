# Agent Report: col-025-agent-6-mcp-cycle-wire-protocol

**Agent ID**: col-025-agent-6-mcp-cycle-wire-protocol
**Component**: mcp-cycle-wire-protocol
**Wave**: Wave 2

---

## Summary

Implemented the MCP wire protocol extension for the `context_cycle` tool as specified by the pseudocode and ADR-005.

---

## Files Modified

- `crates/unimatrix-server/src/uds/hook.rs` — Added `pub(crate) const MAX_GOAL_BYTES: usize = 1024` adjacent to `MAX_INJECTION_BYTES` and `MAX_PRECOMPACT_BYTES`
- `crates/unimatrix-server/src/mcp/tools.rs` — Added `goal: Option<String>` field to `CycleParams`, validation block in `context_cycle` handler, updated response text and audit detail, added 10 new test cases

---

## What Was Implemented

### `hook.rs`
- Added `pub(crate) const MAX_GOAL_BYTES: usize = 1024` with ADR-005 attribution comment, adjacent to the existing byte-budget constants. Marked `pub(crate)` so `tools.rs` can import it without duplicating the value.

### `tools.rs`
1. Import: `use crate::uds::hook::MAX_GOAL_BYTES;`
2. `CycleParams` struct: added `pub goal: Option<String>` field after `next_phase`, before `agent_id`, with doc comment explaining FR-11/ADR-005 semantics.
3. Goal validation block (step 3b) inserted between the existing `validate_cycle_params` call and the `action` string match:
   - Only runs when `validated.cycle_type == CycleType::Start`; yields `None` for PhaseEnd and Stop
   - Trims whitespace: `g.trim().to_owned()`
   - Normalizes empty/whitespace-only to `None` before byte check
   - Byte check: `if trimmed.len() > MAX_GOAL_BYTES` → returns `CallToolResult::error(...)` immediately with message quoting both the limit and the actual byte count
   - Accepted goals returned as `Some(trimmed)`
4. Response text: updated to a two-branch `if let Some(ref g) = validated_goal` format — acknowledges goal in text when present.
5. Audit detail: appends `" goal=present"` when `validated_goal.is_some()`.

### Tests (10 new)
| Test | AC/FR |
|------|-------|
| `test_cycle_params_goal_field_present` | T-MCP-01 |
| `test_cycle_params_goal_field_absent` | T-MCP-02 / AC-02 |
| `test_cycle_params_goal_null` | deserialization |
| `test_cycle_start_goal_exceeds_max_bytes_rejected` | AC-13a |
| `test_cycle_start_goal_at_exact_max_bytes_accepted` | AC-13a / R-07 boundary |
| `test_cycle_start_empty_goal_normalized_to_none` | AC-17 |
| `test_cycle_start_whitespace_only_goal_normalized_to_none` | AC-17 |
| `test_cycle_start_whitespace_trimmed_goal_within_limit_accepted` | AC-17 |
| `test_cycle_phase_end_with_goal_ignores_goal` | FR-01 |
| `test_cycle_stop_with_goal_ignores_goal` | FR-01 |

A private `validate_goal_mcp` helper in the test module mirrors the handler's validation block for isolated unit testing of the normalization/byte-check logic.

---

## Design Decisions

### `session_registry.set_current_goal` not called from `tools.rs`
The spawn prompt mentioned calling `session_registry.set_current_goal(session_id, goal.clone())`. The pseudocode makes clear that:
1. `CycleParams` has no `session_id` field — session_id is not available in the MCP handler context
2. The pseudocode explicitly states: "No direct ImplantEvent emission needed from tools.rs"
3. `set_current_goal` is called from the UDS listener in `handle_cycle_event` (Wave 3 / cycle-event-handler component), which operates in a session-aware context

The MCP handler's responsibility is validation and error return only. This is consistent with the "MCP server is session-unaware" comment already in the code (line 1800).

### Constant placement
`MAX_GOAL_BYTES` is defined in `hook.rs` as `pub(crate)` and imported in `tools.rs`. This keeps both paths (MCP hard-reject, UDS truncate) using the same constant value without duplication. If it were defined in `tools.rs` it would not be accessible to the UDS listener in `listener.rs`.

---

## Test Results

```
cargo test -p unimatrix-server -- tools
88 passed; 0 failed
```

Full workspace: all tests pass (1950 server tests, no new failures). One pre-existing intermittent failure (`col018_topic_signal_from_file_path`) observed once due to embedding model initialization timing under parallel test load — passes when run in isolation, unrelated to this component.

---

## Commit

`933e2fa impl(mcp-cycle-wire-protocol): add goal field + validation to context_cycle (#374)`

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `MCP tool handler validation goal byte-length guard` (pattern) and `col-025 architectural decisions` (decision) — found ADR-005 entry #3405 confirming one-constant approach; found pattern #317 (ToolContext pre-validated handler) as relevant convention
- Stored: entry #3408 "MCP tool handler byte-length guard: define pub(crate) constant in hook.rs, import in tools.rs" via `/uni-store-pattern` — captures the non-obvious placement decision (constant lives in hook.rs not tools.rs) and the reason it must be `pub(crate)`
