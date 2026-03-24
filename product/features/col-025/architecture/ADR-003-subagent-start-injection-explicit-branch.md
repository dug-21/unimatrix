## ADR-003: SubagentStart Injection Routes to IndexBriefingService When Goal Is Present

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

**Background on prompt_snippet**: On SubagentStart, `prompt_snippet` carries the
spawning prompt — typically spawn boilerplate (protocol role assignments, agent
identity text). It is noisy and not semantically representative of the feature's
intent. Col-024 confirmed that UDS `topic_signal` enrichment (`enrich_topic_signal`
in `listener.rs`) shipped for retrospective attribution; that is a separate concern
from injection quality. Goal is a deliberate, concise statement of feature intent
and is the stronger signal.

**Revised precedence** (settled design decision, superseding original SCOPE.md §Goals-5):

```
SubagentStart injection precedence:
  goal (Some, non-empty)  →  prompt_snippet / transcript (non-empty)  →  RecordEvent/topic
```

Goal wins over prompt_snippet. When goal is present, the SubagentStart path routes
to `IndexBriefingService` (not `ContextSearch`), returning the full k=20 ranked
index injected before the subagent's first token. Agents receive relevant context
without needing to call `context_briefing` explicitly.

When goal is absent, the path falls through to the existing prompt_snippet →
transcript → RecordEvent logic unchanged.

Three options for implementing the goal-present branch:

**Option A**: Route SubagentStart through `handle_compact_payload` (CompactPayload
path). This would unify the two paths but changes response format in all cases
including when goal is absent, alters observed injection structure, and risks
breaking AC-10 (existing tests must pass unmodified).

**Option B**: Add an explicit fallback step after transcript extraction: when
transcript is empty or None, check `current_goal` before RecordEvent. This was
the original design but inverts the goal/prompt_snippet priority relative to the
settled decision — prompt_snippet is spawn boilerplate, not semantic intent.

**Option C (chosen)**: When goal is `Some(g)` and non-empty, immediately route to
`IndexBriefingService::index(query: g, session_state, k: 20)`. This produces a full
ranked-index injection (the same payload as `handle_compact_payload`) anchored to
the feature goal. Only when goal is absent does the path fall through to the
existing transcript → RecordEvent logic.

Option C was chosen because goal is a deliberate statement of intent whereas
prompt_snippet is boilerplate. Routing to `IndexBriefingService` rather than
`ContextSearch` gives the injected content the same quality as a full briefing call.

### Decision

In the SubagentStart arm of `dispatch_request` in `src/uds/listener.rs`:

1. Check `session_registry.get_state(session_id)?.current_goal`.
2. If `current_goal` is `Some(g)` and `g` is non-empty:
   - Call `IndexBriefingService::index(query: &g, session_state, k: 20)`.
   - Inject the resulting ranked index payload (same format as `handle_compact_payload`).
   - **Do not fall through to transcript extraction or RecordEvent.**
3. If `current_goal` is `None` or empty:
   - Proceed with the existing `extract_transcript_block` path (prompt_snippet / transcript → RecordEvent/topic), unchanged.

The `session_id` is already available in the SubagentStart arm (`hook_input.session_id`).
The `session_registry` is passed to `dispatch_request`. No additional parameters
are needed to implement step 1.

This is structurally parallel to `handle_compact_payload` for the goal-present branch
but remains an explicit branch so existing SubagentStart behavior is not disturbed
when goal is absent.

### Consequences

- SubagentStart injection for goal-present sessions is now as rich as a full
  `context_briefing` call — k=20 ranked index injected before the agent's first
  token, anchored to the feature goal.
- Agents starting work on a feature cycle do not need to call `context_briefing`
  explicitly; relevant context arrives via injection.
- Precedence is goal → prompt_snippet (not prompt_snippet → goal). Spec writer
  must add:
  - Test: goal present → IndexBriefingService called, transcript path skipped (AC-08 revised).
  - Test: goal absent → existing transcript/RecordEvent path runs unchanged (SR-03 guard).
  - Test: RecordEvent fallback when both goal and transcript are absent.
- The existing `ContextSearch` path for SubagentStart (when goal is absent) is
  not modified. If future features want to further unify injection paths, ADR-002
  and ADR-003 together define the current contract boundary.
- AC-08 must be revised by the spec writer: "SubagentStart uses goal as the
  IndexBriefingService query when goal is set, regardless of prompt_snippet."
- SR-03 guard test must verify goal wins over prompt_snippet (not the reverse).
