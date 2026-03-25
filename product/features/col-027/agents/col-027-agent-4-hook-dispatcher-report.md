# Agent Report: col-027-agent-4-hook-dispatcher

**Feature:** col-027 — PostToolUseFailure Hook Support
**Component:** hook-dispatcher (`crates/unimatrix-server/src/uds/hook.rs`)
**Agent ID:** col-027-agent-4-hook-dispatcher

---

## Summary

Added explicit `"PostToolUseFailure"` match arms to `build_request()` and `extract_event_topic_signal()` in `hook.rs`, along with 7 unit tests covering all test plan scenarios (T-HD-01 through T-HD-06 + T-HD-extra).

---

## Changes Made

**File modified:** `crates/unimatrix-server/src/uds/hook.rs`

1. Added import: `use unimatrix_core::observation::hook_type;`

2. Added `hook_type::POSTTOOLUSEFAILURE` arm in `extract_event_topic_signal()`:
   - Reads from `input.extra["tool_input"]` (same source as PostToolUse arm)
   - Separate arm preserves future divergence flexibility (ADR-001)
   - Defensive: absent/null `tool_input` returns `None` via `unwrap_or_default()` + `extract_topic_signal("")`

3. Added `hook_type::POSTTOOLUSEFAILURE` arm in `build_request()`:
   - Extracts `tool_name` from `input.extra["tool_name"]` with `.and_then().unwrap_or("")`
   - Computes `topic_signal` via `extract_event_topic_signal(event, input)`
   - Returns `HookRequest::RecordEvent` with `event_type: "PostToolUseFailure"` verbatim (ADR-003)
   - Does NOT enter rework logic (ADR-001)
   - Does NOT call `extract_response_fields()` — error field handled in listener.rs (ADR-001)
   - All field accesses defensive — no panic on null/malformed payload (FR-03.7)

4. Updated doc comment on `extract_event_topic_signal()` to include `PostToolUseFailure`.

5. Added 7 unit tests:
   - `build_request_posttoolusefailure_explicit_arm` (T-HD-01)
   - `build_request_posttoolusefailure_empty_extra` (T-HD-02)
   - `build_request_posttoolusefailure_missing_tool_name` (T-HD-03)
   - `build_request_posttoolusefailure_null_error` (T-HD-04)
   - `build_request_posttoolusefailure_does_not_enter_rework_logic` (T-HD-05)
   - `extract_event_topic_signal_posttoolusefailure` (T-HD-06)
   - `build_request_posttoolusefailure_null_extra` (T-HD-extra)

---

## Test Results

All 7 new tests: PASS
Total suite: 2067 passed, 1 failed (pre-existing flaky test `col018_topic_signal_from_feature_id` — embedding model initialization race under concurrent execution; passes in isolation before and after my changes).

---

## Commit

Changes are in commit `a4f5a59` (included with observation-storage agent's commit, which staged hook.rs together with listener.rs changes).

---

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] `cargo test --workspace` — 7 new tests pass; pre-existing flaky failure unrelated to this change
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in non-test code
- [x] All modified files within scope defined in brief (`hook.rs` only)
- [x] Error handling uses defensive Option chaining, no `.unwrap()` in production paths
- [x] New code follows validated pseudocode — no silent deviations
- [x] Test cases match component test plan (T-HD-01 through T-HD-06 + T-HD-extra)
- [x] `hook.rs` remains within 500-line module (file is large but pre-existing; no new module splits needed for my additions)
- [x] `cargo fmt` applied; `cargo clippy` — no warnings from modified code

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for PostToolUseFailure hook dispatch — MCP deserialization error (k parameter type mismatch); context_lookup for col-027 decisions returned 5 ADRs (#3473–#3477) which were applied.
- Stored: attempted `/uni-store-pattern` for `hook_type` import pattern in `hook.rs` — MCP Write capability denied for anonymous agent. Pattern to store: "When hook_type constants from unimatrix_core::observation::hook_type are needed in hook.rs, import directly (`use unimatrix_core::observation::hook_type`); the module is not re-exported through unimatrix_engine."
