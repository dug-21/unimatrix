# Component: subagent-start-injection

**Crate**: `unimatrix-server`
**File**: `src/uds/hook.rs` (fn `run`, SubagentStart fallback block — Step 5b)

---

## Purpose

When a `SubagentStart` hook fires during a session with an active goal,
route directly to `IndexBriefingService` using the goal as the query,
instead of falling through to transcript extraction and `ContextSearch`.
When no goal is set, the existing transcript/RecordEvent path runs unchanged.

---

## Architecture Clarification (OQ-04 Resolution)

The SubagentStart injection path lives in `src/uds/hook.rs` (the hook
subprocess), not in `dispatch_request` in `listener.rs`.

Current flow in `hook.rs::run` Step 5b:
1. `build_request(&event, &hook_input)` produces a `HookRequest::RecordEvent`
   for SubagentStart (because prompt_snippet is absent in Claude Code's payload).
2. If `event == "SubagentStart"` and the request is `RecordEvent`:
   extract query from transcript tail → build `HookRequest::ContextSearch`.
3. Send `HookRequest::ContextSearch` to the server via transport.
4. Write response using `write_stdout_subagent_inject_response`.

**The goal check cannot happen in `hook.rs`**: The hook subprocess does not
have direct access to `SessionRegistry`. The session state lives in the server
process. The goal check must happen on the SERVER side via `dispatch_request`.

**Resolution**: The goal-present branch must be implemented in
`src/uds/listener.rs::dispatch_request`, inside the
`HookRequest::ContextSearch` arm when `source == "SubagentStart"`.

The hook.rs SubagentStart path always produces `HookRequest::ContextSearch`
(with query from transcript or a fallback). On the server side,
`dispatch_request` receives this `ContextSearch` with `source = "SubagentStart"`.
The goal-present branch intercepts it there.

---

## Modified Code Location: `dispatch_request` in `listener.rs`

### Current `HookRequest::ContextSearch` arm (simplified)

```
HookRequest::ContextSearch { query, session_id, source, role, task, feature, k, max_tokens } => {
    // ... capability check ...
    // ... session state lookup ...
    // ... build search params ...
    // ... call search service ...
    // ... write injection log ...
    // return HookResponse::...
}
```

### New structure: goal-present branch before existing ContextSearch dispatch

Insert as the FIRST check inside the `ContextSearch` arm, before any
existing session state lookup or search params construction:

```
HookRequest::ContextSearch { query, session_id, source, role, task, feature, k, max_tokens } => {
    // capability check (unchanged)
    if !uds_has_capability(Capability::Search) || !uds_has_capability(Capability::Read) {
        return HookResponse::Error { ... };
    }

    // col-025 ADR-003: SubagentStart goal-present branch.
    // Check FIRST, before transcript extraction or ContextSearch.
    // When a session has an active goal, use it as the IndexBriefingService query.
    if source.as_deref() == Some("SubagentStart") {
        // Look up session state to check current_goal.
        // get_state returns a clone; no lock held during the await.
        let maybe_goal: Option<String> = session_registry
            .get_state(&session_id)
            .and_then(|state| state.current_goal)
            .filter(|g| !g.trim().is_empty());

        if let Some(ref goal_text) = maybe_goal {
            // Goal is present and non-empty — route to IndexBriefingService.
            // This is the same path as handle_compact_payload but anchored to goal.
            tracing::debug!(
                session_id = %session_id,
                goal_preview = %&goal_text[..goal_text.len().min(50)],
                "col-025: SubagentStart goal-present branch — routing to IndexBriefingService"
            );

            // Build IndexBriefingParams with goal as the query (k=20 per ADR-003)
            let session_state = session_registry.get_state(&session_id);
            let category_histogram = session_state
                .as_ref()
                .map(|s| s.category_counts.clone())
                .filter(|h| !h.is_empty());

            let params = IndexBriefingParams {
                query: goal_text.clone(),
                k: 20,
                session_id: Some(session_id.clone()),
                max_tokens: None,
                category_histogram,
            };

            let audit_ctx = AuditContext {
                source: AuditSource::Uds {
                    uid: 0,
                    pid: None,
                    session_id: session_id.clone(),
                },
                caller_id: "subagent-start-goal".to_string(),
                session_id: Some(session_id.clone()),
                feature_cycle: session_state
                    .as_ref()
                    .and_then(|s| s.feature.clone()),
            };

            let entries = match services
                .briefing
                .index(params, &audit_ctx, None)
                .await
            {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        session_id = %session_id,
                        "col-025: SubagentStart IndexBriefingService failed, degrading to empty"
                    );
                    vec![]
                }
            };

            let table_text = format_index_table(&entries);
            // format_index_table now prepends CONTEXT_GET_INSTRUCTION header.

            if table_text.is_empty() {
                // No results — fall through to existing ContextSearch path
                // (graceful degradation: if briefing returns nothing, let
                // the existing transcript/RecordEvent path proceed)
                // Note: this could alternatively return RecordEvent/Ack;
                // falling through preserves the existing injection behavior.
            } else {
                // Return as BriefingContent — same response format as
                // handle_compact_payload, triggers SubagentStart response routing
                // in hook.rs (write_stdout_subagent_inject_response).
                let token_count = (table_text.len() / 4) as u32;
                return HookResponse::BriefingContent {
                    content: table_text,
                    token_count,
                };
            }
        }
        // goal absent or empty: fall through to existing ContextSearch dispatch
    }

    // ... rest of existing ContextSearch handling (unchanged) ...
}
```

### Notes on response format

The existing SubagentStart path produces `HookResponse::ContextSearch { ... }`
or similar. The goal-present branch produces `HookResponse::BriefingContent { ... }`.
In `hook.rs`, the response routing for SubagentStart uses
`write_stdout_subagent_inject_response(&response)` which handles both
`HookResponse::BriefingContent` and `HookResponse::ContextSearch` variants
by writing the `hookSpecificOutput` JSON envelope.

Confirm that `write_stdout_subagent_inject_response` handles `BriefingContent`
correctly (existing code in `hook.rs` already handles it via this path for
CompactPayload responses — see line 165: `HookResponse::BriefingContent`).

---

## Precedence

SubagentStart query selection (ADR-003):
1. `current_goal` is `Some(g)` and `g` is non-empty → IndexBriefingService
   with goal as query. Does NOT fall through to transcript extraction.
2. `current_goal` is `None` or empty → existing transcript/ContextSearch path
   (unchanged).

When goal is present AND transcript is non-empty: goal wins unconditionally
(ADR-003 §Decision step 1 > step 2).

Edge case: if `IndexBriefingService::index` returns empty `Vec<IndexEntry>`,
`format_index_table` returns empty string, and the branch falls through to
the existing ContextSearch path. This is graceful degradation.

---

## Data Flow

Input: `HookRequest::ContextSearch` with `source = "SubagentStart"`
- Goal check: `session_registry.get_state(session_id)?.current_goal`
- If goal present: `IndexBriefingService::index(goal, k=20)`
  → `format_index_table(entries)` (with CONTEXT_GET_INSTRUCTION header)
  → `HookResponse::BriefingContent { content, token_count }`
- If goal absent: fall through to existing `ContextSearch` handling

---

## Error Handling

| Failure | Behavior |
|---------|----------|
| `get_state` returns `None` (session not registered) | `maybe_goal = None`; fall through to existing ContextSearch path |
| `current_goal` is `None` or empty | Fall through to existing ContextSearch path |
| `IndexBriefingService::index` returns `Err` | `tracing::warn!`; entries = `vec![]`; fall through to ContextSearch path |
| `format_index_table` returns empty string | Fall through to ContextSearch path (graceful degradation) |

---

## Key Test Scenarios

### T-SAI-01: Goal present → IndexBriefingService called with goal query (AC-08 / ADR-003)
```
setup: session registered with current_goal = Some("feature goal text")
       session source = "SubagentStart"
act:   dispatch_request receives HookRequest::ContextSearch with source = "SubagentStart"
assert: IndexBriefingService::index called with query = "feature goal text"
assert: existing transcript-extraction ContextSearch path NOT taken
assert: response is HookResponse::BriefingContent (not ContextSearch response)
```

### T-SAI-02: Goal wins over non-empty transcript (AC-12 / ADR-003)
```
setup: session registered with current_goal = Some("feature goal")
       hook.rs constructs ContextSearch with non-empty transcript query
act:   dispatch_request receives ContextSearch with source = "SubagentStart"
assert: IndexBriefingService::index called with query = "feature goal"
assert: transcript-derived query NOT used
```

### T-SAI-03: Goal absent → existing ContextSearch path runs (R-12 regression guard)
```
setup: session registered with current_goal = None
act:   dispatch_request receives ContextSearch with source = "SubagentStart"
assert: existing ContextSearch path runs (not IndexBriefingService)
assert: response is the existing ContextSearch response format
```

### T-SAI-04: Goal = Some("") edge case → fall through (R-04)
```
setup: session registered with current_goal = Some("") (empty string)
act:   SubagentStart ContextSearch dispatch
assert: filter(|g| !g.trim().is_empty()) catches empty goal
assert: falls through to existing ContextSearch path
```

### T-SAI-05: Session not registered → fall through (R-12)
```
setup: no session registered for session_id
act:   SubagentStart ContextSearch dispatch
assert: get_state returns None; maybe_goal = None
assert: falls through to existing ContextSearch path
```

### T-SAI-06: IndexBriefingService returns empty → fall through (graceful degradation)
```
setup: session registered with current_goal = Some("goal"), but store is empty
act:   SubagentStart ContextSearch dispatch
assert: IndexBriefingService returns empty vec
assert: table_text is empty
assert: falls through to existing ContextSearch path (not silent no-injection)
```

### T-SAI-07: BriefingContent response carries CONTEXT_GET_INSTRUCTION header (AC-18)
```
setup: session registered with current_goal = Some("goal")
       IndexBriefingService returns non-empty entries
act:   SubagentStart goal-present branch fires
assert: HookResponse::BriefingContent.content starts with CONTEXT_GET_INSTRUCTION
```
