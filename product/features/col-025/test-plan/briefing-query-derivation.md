# Test Plan: briefing-query-derivation

**Crate**: `unimatrix-server`
**File modified**: `src/services/index_briefing.rs`
  (fn `synthesize_from_session`, fn `derive_briefing_query`)

**Risks covered**: R-05, R-09
**ACs covered**: AC-04, AC-05, AC-06, AC-07, AC-10

---

## Overview

Component 6 replaces the body of `synthesize_from_session` to return
`state.current_goal.clone()` directly. The old implementation synthesized a
`"{feature_cycle} {top_3_signals}"` string. All existing tests for `derive_briefing_query`
that assert the old synthesis format must be updated (R-05).

The function signature is unchanged. The shared function is used by both the
MCP `context_briefing` handler and the UDS `handle_compact_payload` path.

---

## Existing Test Updates (R-05)

The following existing tests in `src/services/index_briefing.rs` assert the
OLD step-2 synthesis format and will fail after the body change. Each must be
updated:

| Old Test Name | Old Assertion | New Assertion |
|---------------|---------------|---------------|
| `derive_briefing_query_session_signals_step_2` | `"crt-027/spec briefing hook compaction"` | Update: with `current_goal = Some("my goal")` and no topic_signals, step 2 returns `"my goal"` |
| `derive_briefing_query_fewer_than_three_signals` | `"crt-027/spec briefing"` | Update: with `current_goal = Some("my goal")`, returns `"my goal"` regardless of signals |
| `derive_briefing_query_no_feature_cycle_falls_to_topic` | Falls to step 3 (signals without feature_cycle) | Update: `current_goal = None`, no feature_cycle → step 3 returns topic (behavior unchanged) |
| `derive_briefing_query_empty_signals_fallback_to_topic` | Falls to step 3 (feature_cycle present but empty signals) | Update: `current_goal = None`, empty signals → step 3 (behavior unchanged) |

**Note**: tests that do NOT exercise step 2 (i.e., `task` is `Some`) are
unaffected by the body change. The tests for step 1 priority and step 3
fallback need no update.

The `make_session_state` helper must be extended to accept `current_goal` — see
`session-state-extension.md`.

---

## New Tests (R-05 / AC-04–AC-07)

All new tests live in `src/services/index_briefing.rs` (`#[cfg(test)] mod tests`).

### Test: `test_synthesize_from_session_returns_current_goal` (R-05)

```
#[test] fn test_synthesize_from_session_returns_current_goal()
```

Call `synthesize_from_session` with `current_goal = Some("feature goal text")`.
Assert return value `== Some("feature goal text")`.
Directly tests the new body contract.

### Test: `test_synthesize_from_session_returns_none_when_goal_absent` (R-05)

```
#[test] fn test_synthesize_from_session_returns_none_when_goal_absent()
```

Call `synthesize_from_session` with `current_goal = None`.
Assert return value `== None`.
Verifies step 3 fallback still fires when there is no goal.

### Test: `test_synthesize_from_session_ignores_topic_signals` (R-05)

```
#[test] fn test_synthesize_from_session_ignores_topic_signals()
```

Call `synthesize_from_session` with `current_goal = None` and several populated
`topic_signals`. Assert return value `== None` — signals no longer influence step 2.
Confirms the old synthesis code is fully removed.

### Test: `test_derive_briefing_query_step2_returns_current_goal` (AC-04)

```
#[test] fn test_derive_briefing_query_step2_returns_current_goal()
```

Call `derive_briefing_query(task=None, state.current_goal=Some("goal text"), topic="col-025")`.
Assert returned query `== "goal text"`.
Step 2 wins when `current_goal` is `Some`.

### Test: `test_derive_briefing_query_step1_wins_over_goal` (AC-05)

```
#[test] fn test_derive_briefing_query_step1_wins_over_goal()
```

Call `derive_briefing_query(task=Some("explicit task"), state.current_goal=Some("goal text"), topic="col-025")`.
Assert returned query `== "explicit task"`.
Step 1 (explicit task) wins unconditionally over `current_goal`.

### Test: `test_derive_briefing_query_step3_fallback_when_no_goal` (AC-06)

```
#[test] fn test_derive_briefing_query_step3_fallback_when_no_goal()
```

Call `derive_briefing_query(task=None, state.current_goal=None, topic="col-025")`.
Assert returned query `== "col-025"`.
Step 3 topic-ID fallback runs when `current_goal` is `None`.
This must behave identically to pre-col-025 behavior (NFR-02 / AC-10).

### Test: `test_derive_briefing_query_step3_no_session_state` (AC-06)

```
#[test] fn test_derive_briefing_query_step3_no_session_state()
```

Call `derive_briefing_query(task=None, session_state=None, topic="col-025")`.
Assert returned query `== "col-025"`.
Verifies the `None` session_state path (no session registered) still returns topic.

### Test: `test_derive_briefing_query_whitespace_task_falls_to_goal` (AC-04 / AC-05)

```
#[test] fn test_derive_briefing_query_whitespace_task_falls_to_goal()
```

Call `derive_briefing_query(task=Some("   "), state.current_goal=Some("goal"), topic="col-025")`.
Assert returned query `== "goal"`.
Whitespace-only task falls through to step 2 (unchanged from current behavior).

### Test: `test_derive_briefing_query_goal_with_populated_signals_returns_goal` (R-05)

```
#[test] fn test_derive_briefing_query_goal_with_populated_signals_returns_goal()
```

Call with `current_goal = Some("goal text")` AND populated `topic_signals`.
Assert returned query `== "goal text"` — signals do not affect step 2 anymore.
Explicitly confirms the old synthesis code is gone.

---

## AC-07 Coverage: CompactPayload UDS Path (AC-07)

The `derive_briefing_query` function is shared between MCP and UDS. AC-07
requires verification that the UDS `handle_compact_payload` path reaches step 2.

### Test: `test_compact_payload_uses_current_goal_as_query` (AC-07)

```
#[tokio::test] async fn test_compact_payload_uses_current_goal_as_query()
```

- Arrange: `SessionState` with `current_goal = Some("compact goal")`.
- Act: call `handle_compact_payload` (or the portion that calls `derive_briefing_query`).
- Assert: the query string passed to `IndexBriefingService::index` equals
  `"compact goal"`.
- This can be a unit test if `derive_briefing_query` is called at a seam where
  the query string is observable. If the call is buried, test via a spy or by
  extracting the query-derivation step.

If `handle_compact_payload` is not easily unit-testable in isolation, this is
covered by the infra-001 lifecycle test `test_cycle_goal_drives_briefing_query`.

---

## No-Goal Backward Compatibility (R-09 / AC-10)

### Test: `test_no_goal_briefing_behavior_unchanged` (R-09 / AC-10)

```
#[test] fn test_no_goal_briefing_behavior_unchanged()
```

With `current_goal = None` and a topic of `"legacy-feature"`:
Assert `derive_briefing_query(None, state_with_no_goal, "legacy-feature")` == `"legacy-feature"`.
Confirms the zero-goal path is identical to pre-col-025 behavior.

**Additional coverage**: the infra-001 suite `tools` contains `test_briefing_*`
tests. All must pass without modification after this change (AC-10 gate).

---

## `synthesize_from_session` Purity Contract (NFR-04)

### Test: `test_synthesize_from_session_is_sync_and_pure`

This is a code-review assertion, not a runtime test. Verify at code review:
- No `async` keyword on the function.
- No `await` inside the body.
- No `self.` receiver (it is a free function taking `&SessionState`).
- No lock acquisition inside the body.
- No I/O.

Document in code review notes that NFR-04 is satisfied.
