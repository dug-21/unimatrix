# Test Plan: subagent-start-injection

**Crate**: `unimatrix-server`
**File modified**: `src/uds/listener.rs` (SubagentStart arm of `dispatch_request`)

**Risks covered**: R-04, R-12
**ACs covered**: AC-08, AC-10, AC-12

---

## Overview

Component 7 introduces an explicit goal-first branch in the SubagentStart hook arm:

```rust
if let Some(g) = session_registry
        .get_state(session_id)?
        .current_goal
        .as_deref()
        .filter(|g| !g.is_empty())
{
    // Goal wins — route to IndexBriefingService
    let payload = index_briefing_service.index(&g, session_state, 20).await?;
    return Ok(inject(payload));
}
// else: fall through to existing transcript / prompt_snippet / RecordEvent path
```

Five distinct precedence branches must all be tested (R-04). The goal-present
→ IndexBriefingService case is the non-negotiable safety net (lesson #2758).

---

## Precedence Branch Tests

All tests in this section live in `src/uds/listener.rs` (`#[cfg(test)] mod tests`).

### Test: `test_subagent_start_goal_present_routes_to_index_briefing` (R-04 / AC-08 / Gate 3c scenario 2)

```
#[tokio::test] async fn test_subagent_start_goal_present_routes_to_index_briefing()
```

This is the **non-negotiable Gate 3c scenario 2**.

Arrange:
- `SessionRegistry` with `current_goal = Some("feature goal text")`.
- A spy/mock for `IndexBriefingService` that records whether `index` was called
  and with what query.
- `prompt_snippet = "some non-empty prompt content"` (distracting value).

Act: dispatch a `SubagentStart` hook.

Assert:
- `IndexBriefingService::index` was called with `query = "feature goal text"`.
- The response is a ranked-index payload (not an empty response or a `ContextSearch` response).
- The transcript extraction path was NOT taken (the function returned early).

### Test: `test_subagent_start_goal_wins_over_nonempty_prompt_snippet` (R-04 / AC-12 / Gate 3c scenario 3)

```
#[tokio::test] async fn test_subagent_start_goal_wins_over_nonempty_prompt_snippet()
```

This is the **non-negotiable Gate 3c scenario 3**.

Arrange:
- `current_goal = Some("feature goal text")`.
- `prompt_snippet = "non-empty spawn boilerplate"` (explicitly non-empty).

Act: dispatch `SubagentStart`.

Assert:
- `IndexBriefingService::index` was called with `query = "feature goal text"`.
- `prompt_snippet` text was NOT used as the query.
- The injection payload contains a ranked index, not a context search result.

This is the SR-03 inversion guard from lesson #2758.

### Test: `test_subagent_start_goal_absent_uses_existing_transcript_path` (R-12 / AC-10 / Gate 3c scenario 4)

```
#[tokio::test] async fn test_subagent_start_goal_absent_uses_existing_transcript_path()
```

This is the **non-negotiable Gate 3c scenario 4**.

Arrange:
- `current_goal = None`.
- `prompt_snippet = "non-empty prompt"` (existing path trigger).

Act: dispatch `SubagentStart`.

Assert:
- `IndexBriefingService::index` was NOT called.
- The existing transcript/prompt_snippet → RecordEvent/topic path ran unchanged.
- No behavior change from pre-col-025 for this case.

### Test: `test_subagent_start_goal_absent_no_prompt_falls_to_topic` (R-04)

```
#[tokio::test] async fn test_subagent_start_goal_absent_no_prompt_falls_to_topic()
```

Arrange:
- `current_goal = None`.
- `prompt_snippet = ""` (empty).

Act: dispatch `SubagentStart`.

Assert:
- Falls through to `RecordEvent` or topic-ID fallback (unchanged).
- `IndexBriefingService::index` was NOT called.

### Test: `test_subagent_start_goal_empty_string_falls_through` (R-04 / edge case)

```
#[tokio::test] async fn test_subagent_start_goal_empty_string_falls_through()
```

Arrange:
- `current_goal = Some("")` (empty string stored — edge case if normalization
  was skipped on the UDS path, per RISK-TEST-STRATEGY.md §Edge Cases).

Act: dispatch `SubagentStart`.

Assert:
- The non-empty check `.filter(|g| !g.is_empty())` prevents routing to
  `IndexBriefingService`.
- Falls through to existing transcript path (goal-absent branch).
- `IndexBriefingService::index` was NOT called.

---

## Integration Wiring Tests (R-12)

Unit tests can verify the branch logic but cannot confirm that
`IndexBriefingService` is actually reachable from the SubagentStart arm in the
running server. That wiring test must be at the infra-001 level.

### infra-001: `test_cycle_start_with_goal_subagent_receives_index_briefing`

**Suite**: `test_lifecycle.py`
**Fixture**: `shared_server`

Sequence:
1. `context_cycle(action="start", feature_cycle="col-025-test", goal="test lifecycle goal")`.
2. Fire a `SubagentStart` UDS hook for a new session associated with `col-025-test`.
3. Assert the injection response is a ranked-index table (contains a Markdown
   table with `context_get` instruction header).
4. Assert the response is NOT a `ContextSearch`-style response (which would have
   different structure than a full ranked-index payload).

This test confirms:
- `IndexBriefingService` is wired into the SubagentStart arm.
- The `session_registry.get_state(session_id)` call succeeds in the SubagentStart arm.
- OQ-03 is resolved: session_id is available in the SubagentStart arm after
  cycle start has processed.

---

## OQ-03 Resolution Note

OQ-03 (ARCHITECTURE.md): "Delivery must confirm that `session_id` is reliably
populated in the SubagentStart hook payload before a CYCLE_START_EVENT has been
processed for that session."

The infra-001 integration test sequence above (step 1: cycle start; step 2:
SubagentStart) establishes the expected ordering. If the `session_id` is not
available in the SubagentStart arm, the integration test will fail with a lookup
error or empty response.

If the ordering constraint cannot be guaranteed (session_id may fire before
cycle_start on some code paths), the branch must degrade gracefully to the
goal-absent path (same as `get_state` returning `None`). Document the resolution
at code review.

---

## Backward Compatibility Guard (AC-10)

All existing `SubagentStart` unit tests and integration tests must pass without
modification. The goal-absent branch is unchanged; the only new code path is
the goal-present branch.

Check at implementation time: run existing `SubagentStart`-related tests and
confirm zero failures before and after the branch addition.
