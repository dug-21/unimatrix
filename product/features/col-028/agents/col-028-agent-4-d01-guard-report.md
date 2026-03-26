# Agent Report: col-028-agent-4-d01-guard

**Component**: D-01 Guard (services/usage.rs)
**Feature**: col-028 Unified Phase Signal Capture
**Date**: 2026-03-26

## Summary

Implemented the D-01 early-return guard in `record_briefing_usage` and updated the `UsageContext.current_phase` doc comment per ADR-006. Applied four compile-fix patches required to keep the workspace building as other agents' structural changes landed partially.

## Files Modified

- `crates/unimatrix-server/src/services/usage.rs` — D-01 guard, doc comment update, 5 new unit tests
- `crates/unimatrix-server/src/uds/listener.rs` — compile fix: `None` as final arg to `QueryLogRecord::new` (C-08, SR-03)
- `crates/unimatrix-server/src/mcp/tools.rs` — compile fix: `None` placeholder for phase in `context_search` call site (Agent 2 will replace with real phase)
- `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` — compile fix: `phase: None` in `make_query_log` test helper
- `crates/unimatrix-server/src/services/index_briefing.rs` — compile fix: `confirmed_entries: HashSet::new()` in `make_session_state` test helper

## Changes Made

### D-01 Guard (primary deliverable)

Added as the first statement in `record_briefing_usage`, before `filter_access`:

```rust
// D-01 guard (col-028): weight-0 is an offer-only event.
// Must appear before filter_access to avoid burning the dedup slot.
// EC-04 contract enforcement: access_count is NOT incremented for briefing.
if ctx.access_weight == 0 {
    return;
}
```

Guard is at line 322; `filter_access` call is at line 329. Correct ordering verified.

### UsageContext.current_phase doc comment

Replaced "None for all non-store operations" with accurate post-col-028 wording listing all five tools that populate the field and restricting None to mutation tools and sessionless/phaseless calls.

### Unit Tests Added (5 new tests)

| Test | AC | Result |
|------|----|--------|
| `test_d01_guard_briefing_weight_zero_does_not_consume_dedup_slot` | AC-07 positive | PASS |
| `test_d01_absent_guard_would_consume_dedup_slot_negative_arm` | AC-07 negative | PASS |
| `test_briefing_weight_zero_no_increment_for_multiple_entries` | AC-06 multi | PASS |
| `test_briefing_twice_same_entry_dedup_slot_remains_absent` | AC-06 double | PASS |
| `test_briefing_empty_entry_list_no_panic` | EC-03 | PASS |

AC-07 integration test (`test_briefing_then_get_does_not_consume_dedup_slot`) is Stage 3c work and is not included here — documented with a comment in the test module.

## Test Results

```
cargo test -p unimatrix-server services::usage
test result: ok. 25 passed; 0 failed
```

```
cargo test -p unimatrix-server
test result: ok. 2087 passed; 0 failed
```

## Compile Fixes Applied

The store schema changes (Agent 3 migration agent) and session state changes (Agent 3 session-state agent) had partially landed before my component ran. The `QueryLogRecord::new` signature gained a `phase: Option<String>` final parameter, and `SessionState` gained `confirmed_entries`. Three call sites in unimatrix-server were missing these new arguments. I applied the minimal compile fixes scoped in the IMPLEMENTATION-BRIEF.md:

- `uds/listener.rs:1324`: `None` for phase (C-08 spec-mandated, no phase semantics in UDS)
- `mcp/tools.rs` context_search site: `None` placeholder — Agent 2 (tools-read-side) will supply the real phase value
- `mcp/knowledge_reuse.rs` test helper: `phase: None`
- `services/index_briefing.rs` test helper: `confirmed_entries: HashSet::new()`

## Issues / Deviations

None. Pseudocode followed exactly. Guard position verified before and after implementation.

## Self-Check

- [x] `cargo build --workspace` passes (zero errors)
- [x] `cargo test --workspace` passes (no new failures; 2087 server tests all green)
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in non-test code
- [x] All modified files within scope defined in brief
- [x] Error handling: guard is a pure early-return with no error conditions
- [x] New tests follow naming convention `test_{fn}_{scenario}_{expected}`
- [x] Code follows validated pseudocode exactly
- [x] Test cases match component test plan expectations
- [x] No source file exceeds 500 lines after changes (usage.rs is ~1300 lines — pre-existing, not caused by this change)
- [x] Knowledge Stewardship complete

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — found entry #3510 "UsageDedup shared access_counted set across AccessSource variants — weight-0 dedup bypass required" (col-028 pre-stored ADR confirmation) and pattern #316 ServiceLayer extraction pattern
- Stored: entry #3527 "D-01 guard: early-return before filter_access in record_briefing_usage prevents dedup slot burn" via `/uni-store-pattern` — captures the load-bearing guard placement rule and the SR-07 warning about not moving it to the dispatch level without a separate ADR
