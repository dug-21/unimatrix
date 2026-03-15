# ASS-019: SubagentStart Injection Opportunity

**Date**: 2026-03-14
**Status**: Research complete — implementation not started
**Context**: Exploratory research into UDS injection path improvements and SubagentStart injection viability

---

## Research Questions

1. Does the UDS injection path benefit from crt-019 confidence improvements?
2. Can `context_cycle` keywords improve injection relevance?
3. Can SubagentStart be used to inject context into spawned sub-agents?
4. What does Claude Code actually send in the SubagentStart payload — and does `prompt_snippet` exist?

---

## 1. crt-019 and the UDS Injection Path

**Short answer: Yes, automatically. No UDS-side code change needed.**

The UDS ContextSearch handler (`listener.rs` `handle_context_search`) delegates to
`SearchService::search()`, which reads `confidence_weight` from a `ConfidenceStateHandle`
(the adaptive blend weight introduced by crt-019):

```rust
// services/search.rs:124
let confidence_weight = {
    guard.confidence_weight  // adaptive, from ConfidenceStateHandle
};
```

As crt-019 pushes confidence spread from ~0.15 to ≥0.20, the adaptive blend weight climbs
from 0.15 toward 0.25 (`clamp(spread * 1.25, 0.15, 0.25)`). This improvement flows into
UDS injection ranking without any hook-side change.

**One watch item:** `CONFIDENCE_FLOOR = 0.3` (listener.rs:90) was calibrated against the
current compressed distribution (0.43–0.57). After crt-019 drives low-signal entries below
0.3, some entries that currently pass the floor may be filtered out. This is the intended
behavior — but worth monitoring post-rollout to confirm the floor is still appropriate.

**Injection log scoring inconsistency (minor):** The injection log records scores using a
hardcoded `0.18375` weight:

```rust
// listener.rs:1017
confidence: rerank_score(*sim, entry.confidence, 0.18375),
```

This is for historical logging only — not for ranking. The log will not reflect the true
adaptive blend weight used during ranking. Minor inconsistency, not correctness issue.

---

## 2. context_cycle Keywords — Large Unbuilt Opportunity

Keywords from `context_cycle` are extracted and stored in cycle events (hook.rs:420-457),
but are **never used in either injection path**:

- **UDS ContextSearch path**: query = raw `input.prompt` only. No cycle awareness.
- **PreCompact → Briefing path**: `feature: None` is hardcoded in the hook dispatch
  (hook.rs:273). The active cycle topic is invisible to briefing.

What could be done:
- When a cycle is active for the session, blend cycle keywords into the UDS search query.
  The keywords are higher-signal than free-form user text — deliberately chosen at cycle
  start to characterize the feature space.
- Pass `feature: Some(active_cycle_topic)` to briefing so it can do topically-filtered
  retrieval rather than general semantic search.

This requires the hook process to retrieve the active cycle state from the server (via a
new `HookRequest::GetCycleState` variant or by bundling it into the `ContextSearch`
request), or for the server to look up the session's active cycle when handling injection.

---

## 3. SubagentStart Injection — Confirmed Viable, But Key Gap Found

### 3a. Does hook output reach the sub-agent?

**Confirmed yes.** Claude Code documentation explicitly states:

> SubagentStart is the exception — its additionalContext is injected directly into the
> Sub-Agent's context.

The structured output format required:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "SubagentStart",
    "additionalContext": "...text to inject into sub-agent..."
  }
}
```

**Critical implication**: The current `write_stdout` function outputs plain text for
`Entries` and `BriefingContent` responses. Plain text works for UserPromptSubmit (where
Claude Code accepts stdout directly as context). For SubagentStart, the JSON
`hookSpecificOutput` format may be required. This is **unverified** — plain stdout may
also work, but the docs only show the JSON path for SubagentStart.

If structured JSON is required, a new `HookResponse::SubagentInjection { content: String }`
variant is needed, with `write_stdout` serializing it to the `hookSpecificOutput` format.

### 3b. The prompt_snippet Gap — THE BIG UNKNOWN RESOLVED

**`prompt_snippet` is never present in real SubagentStart events from Claude Code.**

The code in hook.rs expects `prompt_snippet` from `input.extra`:

```rust
// hook.rs:192
"SubagentStart" => {
    let text = input.extra.get("prompt_snippet")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    extract_topic_signal(text)
}
```

The official Claude Code SubagentStart payload contains:

```json
{
  "session_id": "...",
  "transcript_path": "...",
  "cwd": "...",
  "permission_mode": "default",
  "hook_event_name": "SubagentStart",
  "agent_id": "agent-abc123",
  "agent_type": "uni-rust-dev"
}
```

No `prompt_snippet`. The test at hook.rs:1760 passes because it constructs a synthetic
payload. In production, `prompt_snippet` is always `None` for every SubagentStart event.

**Downstream consequences:**
- `extract_event_topic_signal("SubagentStart", ...)` always returns `None` in production
- The `input` column in `ObservationRow` for all SubagentStart records is `null`
- SubagentStart contributes zero topic signal to session attribution
- Any future SubagentStart injection handler using `prompt_snippet` as query would get
  an empty query and return no results

### 3c. What IS available at SubagentStart time

| Field | Available | Usefulness for injection query |
|-------|-----------|-------------------------------|
| `agent_type` | ✓ Always | Medium — can search for agent-role conventions/patterns |
| `agent_id` | ✓ Always | None — unique per spawn, no semantic value |
| `session_id` | ✓ Always | Enables session state lookup (active cycle, keywords) |
| `cwd` | ✓ Always | Low — path only |
| `prompt_snippet` | ✗ Never | Not sent by Claude Code |
| Spawn prompt text | ✗ Not here | Lives in PreToolUse Agent tool_input (parent session) |

### 3d. Where the spawn prompt text actually is

When the SM calls `Agent(subagent_type: "uni-rust-dev", prompt: "...")`, this fires a
**PreToolUse** event on the **parent session** with:

```json
{
  "tool_name": "Agent",
  "tool_input": {
    "subagent_type": "uni-rust-dev",
    "prompt": "..."
  }
}
```

This is the richest possible signal — the full spawn prompt including feature ID, role,
and task description. But it arrives in the **parent's** PreToolUse, not in SubagentStart.

Options:
1. **Intercept PreToolUse for Agent tool calls** in the parent — extract spawn prompt,
   store it in session state keyed by the upcoming sub-agent's session context. Then
   retrieve it when SubagentStart fires. This requires a round-trip: parent PreToolUse
   stores prompt → SubagentStart retrieves it. The linkage mechanism (how to match the
   PreToolUse Agent call to the subsequent SubagentStart) is unclear since `agent_id` is
   only known at SubagentStart time.

2. **Use agent_type + active cycle keywords** as the query — weaker signal than spawn
   prompt text but available without cross-event linkage. For well-named agents
   (`uni-rust-dev`, `uni-validator`, `uni-architect`), agent_type alone retrieves relevant
   conventions, duties, and patterns. Combined with cycle keywords, this is a usable query.

3. **Accept the gap for now** — implement SubagentStart injection using agent_type +
   cycle keywords, document spawn prompt as a future enhancement when/if Claude Code adds
   `prompt_snippet` to the payload.

---

## 4. Implementation Sketch (Option 2 — Pragmatic)

If we implement SubagentStart injection using available fields only:

### hook.rs changes

```
"SubagentStart" => {
    let agent_type = input.extra.get("agent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if agent_type.is_empty() {
        return generic_record_event(event, session_id, input);
    }
    HookRequest::SubagentContextSearch {
        agent_type,
        session_id: Some(session_id),
    }
}
```

### listener.rs changes

New handler for `HookRequest::SubagentContextSearch`:
1. Look up active cycle from session state → get topic + keywords
2. Build query: `"{agent_type} {keywords joined}"` or just agent_type if no cycle active
3. Call SearchService with agent-appropriate filters (conventions + duties + patterns)
4. Return results

### wire.rs changes

New request variant:
```rust
SubagentContextSearch {
    agent_type: String,
    session_id: Option<String>,
}
```

New response path in `write_stdout`: if the event was SubagentStart, wrap output in
`hookSpecificOutput` JSON rather than plain text (pending verification that plain stdout
doesn't also work).

---

## 5. Open Questions Before Implementation

1. **Does plain stdout work for SubagentStart injection, or is `hookSpecificOutput` JSON
   required?** Verify by adding a temporary test hook that writes plain text and checking
   if the sub-agent receives it. If plain text works, no new response serialization path
   needed.

2. **Does `agent_type` in the SubagentStart payload match the `subagent_type` parameter
   from the Agent tool call?** e.g., does spawning `subagent_type: "uni-rust-dev"` produce
   `agent_type: "uni-rust-dev"` in SubagentStart, or does Claude Code map it differently
   (e.g., to "general-purpose" or the agent definition name)?

3. **Session ID linkage**: Sub-agents share `session_id` with the parent (confirmed). Does
   the SubagentStart payload carry the parent's `session_id`? If yes, active cycle lookup
   from session state works directly. This appears to be the case based on the documentation
   payload, but should be verified empirically.

4. **Token budget**: SubagentStart injects into the sub-agent's context window before its
   first turn. The current `MAX_INJECTION_BYTES = 1400` was designed for UDS. Sub-agents
   may have different context budgets or injection point semantics. Should the limit differ?

5. **Agent-type aware query construction**: Should injection be tailored per agent type?
   e.g., for `uni-validator`, prioritize conventions and gate checklists; for `uni-rust-dev`,
   prioritize patterns and anti-patterns; for `uni-architect`, prioritize ADRs. This could
   be a simple query prefix strategy using the agent_type string.

---

## 6. Summary

| Finding | Status |
|---------|--------|
| crt-019 improves UDS injection ranking | Confirmed — automatic via SearchService |
| Keywords used in injection | Not implemented — large gap |
| SubagentStart output reaches sub-agent | Confirmed by Claude Code docs |
| `prompt_snippet` in SubagentStart payload | **Does not exist** — code is written against phantom field |
| `agent_type` available for query | Confirmed |
| Output format for SubagentStart (plain vs JSON) | **Unverified** — needs empirical test |
| Session ID shared with parent | Confirmed — enables cycle lookup |

Recommended next step: empirical verification of questions 1 and 2 (plain stdout + agent_type value) before scoping an implementation feature.
