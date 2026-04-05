# Gate Bugfix Report: bugfix-519

> Gate: Bug Fix Validation
> Date: 2026-04-04
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | Pre-registers evicted session before set_feature_force — breaks the None-arm no-op |
| No placeholders/stubs | PASS | No todo!(), unimplemented!(), TODO, FIXME in changed code |
| Tests pass | PASS | 2734 unit + smoke 22/22 + lifecycle 6/6 — 0 failures |
| No new clippy warnings | PASS | All clippy errors are in pre-existing crates (unimatrix-observe, patches/anndists, unimatrix-engine) — none in changed files |
| No unsafe code | PASS | No unsafe blocks added |
| Fix is minimal | PASS | 2 files changed: listener.rs (core fix + session_id guards) and session.rs (doc comment only) |
| New test catches original bug | PASS | cycle_start_on_evicted_session_re_registers_and_attributes_observations covers full causal chain |
| Integration smoke gate | PASS | 22/22 passed |
| xfail markers | PASS | No xfail markers added; none needed |
| Knowledge stewardship (rust-dev) | PASS | 519-agent-1-fix-report.md has Queried + Stored entries |
| Knowledge stewardship (verifier) | PASS | 519-agent-2-verify-report.md has Queried + "nothing novel" with reason |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

**Evidence**: The root cause is that `drain_and_signal_session` removes the session from the registry before `context_cycle(start)` arrives, leaving `set_feature_force` to hit the `None` arm (silent no-op). `enrich_topic_signal` subsequently calls `get_state()` → `None` and stores `topic_signal = NULL` for all observations.

The fix in `handle_cycle_event` (listener.rs:2369–2392) inserts a pre-registration step:

```
if lifecycle == CycleLifecycle::Start
    && !feature_cycle.is_empty()
    && session_registry.get_state(&event.session_id).is_none()
{
    session_registry.register_session(&event.session_id, None, Some(feature_cycle.clone()));
}
```

This guard fires only when the session is absent — live sessions are not touched. After re-registration, `set_feature_force` finds the session and returns `AlreadyMatches` (since `register_session` already set `feature`). `set_current_phase` then sets `current_phase` from the `next_phase` payload field. All subsequent observations are enriched with the correct `topic_signal`.

The change to `session.rs` is documentation only — the comment on the `None` arm of `set_feature_force` is clarified to warn callers that `Set` is indistinguishable from a no-op, with a pointer to GH #519's fix.

### No Placeholders or Stubs

**Status**: PASS

**Evidence**: `git diff main...HEAD` scanned for `todo!`, `unimplemented!`, `TODO`, `FIXME`, `unsafe` — zero matches in changed lines.

### Tests Pass

**Status**: PASS

**Evidence**:
- Regression test `cycle_start_on_evicted_session_re_registers_and_attributes_observations`: 1 passed
- Full workspace `cargo test --workspace`: 2734 passed, 0 failed (confirmed independently)
- Integration smoke gate: 22 passed, 0 failed (from 519-agent-2-verify-report.md)
- Targeted lifecycle tests: 6 passed, 0 failed

### No New Clippy Warnings

**Status**: PASS

**Evidence**: `cargo clippy --workspace -- -D warnings` errors are all in `unimatrix-observe`, `patches/anndists`, and `unimatrix-engine/src/auth` and `event_queue`. None of the errors map to `uds/listener.rs` or `infra/session.rs`. Pre-existing debt confirmed against main branch (identical error locations).

### No Unsafe Code

**Status**: PASS

**Evidence**: Diff contains no `unsafe` blocks.

### Fix Is Minimal

**Status**: PASS

**Evidence**: `git diff main...HEAD --name-only` shows exactly 2 changed files:
- `crates/unimatrix-server/src/uds/listener.rs` — core fix (Step 1b block, session_id guards on RecordEvent/RecordEvents arms, new test)
- `crates/unimatrix-server/src/infra/session.rs` — comment update on set_feature_force None arm only

The `sanitize_session_id` guards added to `RecordEvent` and `RecordEvents` arms are directly load-bearing for the fix: without input validation the new pre-registration path could be exploited to inject arbitrary session_ids into the registry. These are security complements to the core fix, not unrelated changes.

### New Test Catches Original Bug

**Status**: PASS

**Evidence**: `cycle_start_on_evicted_session_re_registers_and_attributes_observations` exercises the complete regression scenario:
1. Register session, evict via `drain_and_signal_session`
2. Assert session absent from registry
3. Dispatch `cycle_start` for evicted session
4. Assert session re-registered with `feature = "col-999"`
5. Assert `current_phase = "discovery"` from payload
6. Dispatch `PreToolUse` with no explicit `topic_signal`
7. Wait for spawn_blocking insert
8. Assert DB row has `topic_signal = "col-999"`

Step 8 is the exact assertion that would have caught the original bug (would have seen `topic_signal = NULL`).

### Integration Smoke Gate

**Status**: PASS

**Evidence**: 22/22 smoke tests passed per 519-agent-2-verify-report.md. Note: UDS listener dispatch path is not covered by the stdio-transport integration harness; the regression is fully covered by the unit test which exercises the UDS path directly.

### xfail Markers

**Status**: PASS

**Evidence**: No xfail markers were added. None are needed — all tests pass unconditionally.

### File Size Limits

**Status**: WARN (pre-existing)

`listener.rs` is 7560 lines (main: 7375 lines). `session.rs` is 2030 lines (main: 2026 lines). Both exceed the 500-line limit, but this is pre-existing debt not introduced by this PR. The fix adds 185 lines (core logic + test) to a file already 7375 lines over limit. Not a blocker for this bugfix.

### Knowledge Stewardship

**Status**: PASS

**Evidence**:
- `519-agent-1-fix-report.md` (rust-dev): Queried `context_briefing` (entries #4135, #3382, #3374). Stored entry #4136 "Pre-register absent sessions in handle_cycle_event before set_feature_force on cycle_start".
- `519-agent-2-verify-report.md` (verifier): Queried `context_briefing` (entries #4135, #4136). Stored: "nothing novel to store — the key lesson (#4135) and fix pattern (#4136) were already stored during the earlier analysis phase."

Both reports present `## Knowledge Stewardship` sections with substantive Queried and Stored/declined entries.

Note: The rust-dev report (`519-agent-1-fix-report.md`) exists in the worktree at `.claude/worktrees/agent-a500f42b/product/features/bugfix-519/agents/` but is not committed to the main working tree. It should be present in the PR commit. The content was verified and is compliant.

## Rework Required

None.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` for patterns relevant to evicted session handling and set_feature_force behavior — entries #4135 and #4136 confirmed the fix approach is aligned with stored patterns.
- Stored: nothing novel to store — this gate validation produced no patterns beyond what was already captured in #4135 and #4136 during the bugfix session.
