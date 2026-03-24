## ADR-003: SubagentStart Injection Uses an Explicit Goal Branch

### Context

This ADR addresses SR-03 (SubagentStart precedence rule is new and not tested
in isolation).

The SubagentStart hook arm in `src/uds/listener.rs` (step 5b / `dispatch_request`)
does not call `handle_compact_payload` or `derive_briefing_query`. It constructs a
`HookRequest::ContextSearch` directly, using either:
- The transcript block extracted by `extract_transcript_block(transcript_path)` as
  the query (when non-empty), or
- A generic `RecordEvent` fallback (when transcript extraction fails).

This path is architecturally separate from the `CompactPayload` path, which is
why ADR-002 (making `derive_briefing_query` step 2 return `current_goal`) does
NOT automatically cover SubagentStart injection.

Three options for introducing goal on the SubagentStart path:

**Option A**: Route SubagentStart through `handle_compact_payload` instead of
`handle_context_search`. This would unify the two paths but changes response format
(BriefingContent vs SearchResult), alters the observed injection structure, and
risks breaking AC-10 (existing tests must pass unmodified).

**Option B**: Call `derive_briefing_query` from the SubagentStart arm with
`task = transcript_query, session_state = ...`. This would work but requires
materializing `session_state` in the SubagentStart arm and changing the query
construction logic significantly. It also conflates a transcript-derived query with
a task-style query, which has different semantics.

**Option C**: Add an explicit fallback step inside the existing SubagentStart
transcript-query path: when `transcript_query` is empty or `None`, check
`session_registry.get_state(session_id)?.current_goal` before falling back to
`RecordEvent`. This preserves the existing path, changes only the fallback
behavior, and is directly testable in isolation.

Option C was chosen. It matches the precedence specified in SCOPE.md:

```
SubagentStart injection precedence:
  prompt_snippet / transcript query (non-empty)  →  current_goal  →  RecordEvent/topic
```

The explicit branch makes the precedence rule visible in code and directly
testable with a unit test that verifies goal is NOT used when transcript is
non-empty (SR-03 requirement: missed or inverted precedence degrades quality
silently).

### Decision

In the SubagentStart arm of `dispatch_request` in `src/uds/listener.rs`, after
the existing `extract_transcript_block` call:

1. If transcript query is `Some(q)` and non-empty → use as `ContextSearch` query
   (unchanged behavior).
2. If transcript query is `None` or empty → look up `session_id` in
   `session_registry`; if the session has `current_goal = Some(g)` → use `g` as
   the `ContextSearch` query.
3. Otherwise → fall through to `RecordEvent` (unchanged behavior).

The `session_id` is already available in the SubagentStart arm (`hook_input.session_id`).
The `session_registry` is passed to `dispatch_request`. No additional parameters
are needed.

The new branch generates a `HookRequest::ContextSearch` with:
- `query = current_goal`
- `session_id = hook_input.session_id`
- `source = Some("SubagentStart".to_string())`
- `role = agent_type` (same as existing path)

This is structurally identical to what the existing transcript-derived path
produces, just with a different query string source.

### Consequences

- SubagentStart injection now produces semantically anchored content for sessions
  that have a feature goal and no transcript content available.
- Precedence is explicit and testable. Spec writer must add:
  - Test: goal used when transcript empty + goal set (AC-08).
  - Test: goal NOT used when transcript non-empty + goal set (SR-03 guard).
  - Test: RecordEvent fallback when both transcript and goal are absent.
- The path remains separate from `derive_briefing_query`. If future features want
  to unify the SubagentStart path with the CompactPayload path, ADR-002 and
  ADR-003 together define the current contract boundary.
- `handle_context_search` is unchanged. The goal-derived query goes through
  the same SearchService pipeline as any other ContextSearch query.
