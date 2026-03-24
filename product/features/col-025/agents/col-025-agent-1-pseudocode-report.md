# Agent Report: col-025-agent-1-pseudocode

**Agent ID**: col-025-agent-1-pseudocode
**Session**: Session 2, Stage 3a
**Date**: 2026-03-24

---

## Work Completed

Produced pseudocode files for all 8 components of col-025 (Feature Goal Signal).

### Files Created

- `product/features/col-025/pseudocode/OVERVIEW.md`
- `product/features/col-025/pseudocode/schema-migration-v16.md`
- `product/features/col-025/pseudocode/session-state-extension.md`
- `product/features/col-025/pseudocode/cycle-event-handler.md`
- `product/features/col-025/pseudocode/mcp-cycle-wire-protocol.md`
- `product/features/col-025/pseudocode/session-resume.md`
- `product/features/col-025/pseudocode/briefing-query-derivation.md`
- `product/features/col-025/pseudocode/subagent-start-injection.md`
- `product/features/col-025/pseudocode/format-index-table-header.md`

---

## Open Questions Resolved

### OQ-01: insert_cycle_event call site count
Verified by code inspection: exactly ONE call site at `listener.rs:2302`
inside the `tokio::spawn` in `handle_cycle_event` Step 5. No other callers.
Safe to change the signature.

### OQ-02: tracing::warn! severity for resume errors
Confirmed: `listener.rs` uses `tracing::warn!` for all non-fatal DB failures
in session paths (e.g., line 2268: "col-022: feature_cycle persist failed").
`tracing::warn!` is the correct severity for the goal resume path.

### OQ-03: CONTEXT_GET_INSTRUCTION exact wording
Settled per ADR-006 §Decision:
`"Use context_get with the entry ID for full content when relevant."`

### OQ-04: session_id availability in SubagentStart before CYCLE_START_EVENT
Resolved with an architectural clarification: The SubagentStart injection goal
check must be implemented in `dispatch_request` in `listener.rs` (server side),
inside the `HookRequest::ContextSearch` arm when `source == "SubagentStart"`.
It cannot be in `hook.rs` because the hook subprocess has no access to
`SessionRegistry`. If the session is not registered, `get_state` returns `None`,
the goal branch is skipped, and the existing path runs. See
`subagent-start-injection.md` for the full pseudocode.

---

## Key Architecture Findings from Code Inspection

1. **`format_index_table` lives in `src/mcp/response/briefing.rs`**, not in
   `index_briefing.rs`. The CONTEXT_GET_INSTRUCTION constant belongs in
   `briefing.rs` alongside the function it annotates.

2. **`context_cycle` in `tools.rs` does NOT emit ImplantEvent directly.**
   The MCP tool validates and returns acknowledgment; the hook process
   (`hook.rs`) constructs and sends the `ImplantEvent` payload. Goal
   validation in `tools.rs` is a guard only. The goal reaches
   `handle_cycle_event` via `ImplantEvent.payload["goal"]` populated by
   Claude Code's hook machinery from the tool call parameters.

3. **SubagentStart injection is in `hook.rs`, not `listener.rs`**. The hook
   subprocess builds `HookRequest::ContextSearch` and sends it to the server.
   The goal-present branch must intercept this in `dispatch_request` on the
   `ContextSearch` arm when `source == "SubagentStart"`. This is different
   from what the architecture document implies ("SubagentStart arm in
   dispatch_request") — the dispatch_request arm that receives the SubagentStart
   event is `ContextSearch` (not a dedicated arm).

4. **`MAX_GOAL_BYTES` placement**: `MAX_INJECTION_BYTES` and `MAX_PRECOMPACT_BYTES`
   are defined in `hook.rs` (not listener.rs). `MAX_GOAL_BYTES` should be
   defined adjacent to them in `hook.rs` and imported with `pub(crate)` into
   `tools.rs` and `listener.rs`.

5. **`synthesize_from_session` is currently inside `derive_briefing_query`
   body** (not a separate function in the current code). The ADR references it
   as a callable function, but the current implementation inlines the logic.
   The implementation agent must extract it as a named function `synthesize_from_session`
   returning `state.current_goal.clone()`, then call it from `derive_briefing_query`.
   Alternatively, inline the change directly into `derive_briefing_query` step 2.

---

## Deviations from Established Patterns

- None. All pseudocode follows established patterns:
  - Pattern #1264 (idempotent ALTER TABLE guard) applied to v15→v16 migration.
  - Pattern #2933 (schema version cascade) documented with explicit audit list.
  - Pattern #3000 (direct write pool) preserved in insert_cycle_event spawn.
  - Pattern #3301 (graceful degradation) applied to session resume.
  - Pattern #3180 (SessionState field additions) documented with make_session_state
    audit requirement.
  - Pattern #3383 (cycle_events-first lookup) used for get_cycle_start_goal.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for schema migration cycle_events session state
  patterns — found patterns #1264, #2933, #3000, #3325, #3383, #3398, #3400,
  #3246, #3301, #3180 (all provided via knowledge package in task prompt).
- Queried: context_search for col-025 architectural decisions via knowledge
  package — ADR files read directly from filesystem.
- Deviations from established patterns: none.
