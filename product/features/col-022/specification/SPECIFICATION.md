# Specification: col-022 Explicit Feature Cycle Lifecycle

## Objective

Provide an explicit, authoritative mechanism for SM/coordinator agents to declare which feature cycle a session belongs to, replacing reliance on heuristic attribution (eager voting, majority vote) that fails for worktree-isolated subagents, single-spawn sessions, and mixed-signal sessions. The explicit declaration takes priority over heuristics via first-writer-wins semantics, while preserving backward compatibility when no explicit declaration is made.

## Functional Requirements

### MCP Tool Registration

- FR-01: A new MCP tool `context_cycle` is registered on `UnimatrixServer` following the established 6-step handler pipeline convention (#318).
- FR-02: The tool accepts three parameters: `type` (required, enum: "start" or "stop"), `topic` (required, string), and `keywords` (optional, array of strings).
- FR-03: The `type` parameter only accepts the literal values "start" and "stop". Any other value returns a validation error with a descriptive message.
- FR-04: The `topic` parameter is validated using `sanitize_metadata_field()` -- non-empty, max 128 characters, no control characters.
- FR-05: The `keywords` parameter accepts an array of up to 5 strings, each max 64 characters. If more than 5 are provided, the array is silently truncated to the first 5. Empty arrays and omitted keywords are both valid.
- FR-06: Individual keyword strings exceeding 64 characters are truncated to 64 characters (not rejected).
- FR-07: The MCP tool itself is lightweight -- it validates parameters and returns an acknowledgment response. It does not perform session attribution directly (the MCP server has no session identity).

### Hook-Side Attribution (PreToolUse Interception)

- FR-08: The hook handler detects `context_cycle` in the PreToolUse event's `tool_name` field (matching by MCP tool name, which includes the server prefix).
- FR-09: For `type: "start"`: the hook handler extracts `topic` from `tool_input`, constructs an ImplantEvent with a designated `event_type` (e.g., "cycle_begin"), and sends it via `RecordEvent` over UDS. The server-side handler calls `set_feature_if_absent()` on `SessionRegistry` and `update_session_feature_cycle()` on the SQLite sessions table.
- FR-10: For `type: "stop"`: the hook handler constructs an ImplantEvent with event_type "cycle_end" and sends it via `RecordEvent`. The server records this as an observation event marking the boundary, but does not clear or modify the session's feature_cycle.
- FR-11: Keywords from a `type: "start"` call are included in the ImplantEvent payload and persisted to the session record (see FR-17).
- FR-12: The `set_feature_if_absent` semantic is preserved: if the session already has a non-NULL `feature_cycle` (set by SessionStart, eager attribution, or a prior cycle_begin), the new value is silently rejected. First writer wins.
- FR-13: After `context_cycle(type: "start")` sets the feature_cycle, subsequent eager attribution attempts are no-ops (the value is already present, so `set_feature_if_absent` returns false).

### Wire Protocol

- FR-14: Cycle lifecycle events use the existing `RecordEvent` wire variant with `ImplantEvent`. The `event_type` field distinguishes cycle events: "cycle_begin" and "cycle_end".
- FR-15: The server-side `RecordEvent` handler recognizes "cycle_begin" and "cycle_end" event types and routes them to the appropriate attribution/observation logic (analogous to the bugfix-198 pattern for extracting feature_cycle from RecordEvent payloads).
- FR-16: No new `HookRequest` enum variants are added. This is the resolved decision (Option B) from SCOPE.md.

### Keywords Storage

- FR-17: Keywords are persisted alongside the session record when `context_cycle(type: "start")` is processed. The storage mechanism (new column vs. separate table) is an architect decision.
- FR-18: Stored keywords are retrievable from the session record for future use by the context injection pipeline. The retrieval path does not need to be implemented in col-022; storage and schema must support it.

### Response Format

- FR-19: For `type: "start"`, the MCP tool response indicates success and includes a `was_set` boolean field (true if this call actually set the feature_cycle, false if it was already set by a prior mechanism).
- FR-20: For `type: "stop"`, the response indicates success and confirms the boundary event was recorded.
- FR-21: Validation errors (invalid type, empty topic, etc.) return standard MCP error responses with descriptive messages.

### Backward Compatibility

- FR-22: When `context_cycle` is never called in a session, the existing attribution pipeline (SessionStart extraction, eager voting, majority vote on close) operates identically to current behavior. No code paths for heuristic attribution are modified.
- FR-23: Old hook binaries encountering unknown `event_type` values in `RecordEvent` safely ignore them (existing deserialization handles unknown event types without crashing).

## Non-Functional Requirements

- NFR-01: **Hook latency**: The PreToolUse interception for `context_cycle` must add less than 5ms marginal latency to the hook handler, staying within the 50ms total budget (40ms transport + 10ms startup). Verification: latency measurement in integration tests.
- NFR-02: **Fire-and-forget persistence**: Session attribution writes from the hook path use `spawn_blocking` fire-and-forget, consistent with existing session write patterns. Attribution may be lost if the server is unavailable (accepted risk per ADR-003 col-012 #384).
- NFR-03: **Wire backward compatibility**: The `RecordEvent`-based approach does not add new `HookRequest` variants, so old binaries continue to work. Unknown `event_type` values in `ImplantEvent` are ignored by older server versions.
- NFR-04: **Tool count**: Total MCP tool count goes from 11 to 12. Single tool with type discriminator (not two separate tools).
- NFR-05: **Validation consistency** (SR-07 mitigation): A shared validation function for cycle parameters must be callable by both the MCP tool handler and the hook handler to prevent validation divergence between the two code paths.

## Acceptance Criteria

All AC-IDs carried forward from SCOPE.md:

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `context_cycle` MCP tool registered with params: `type` (required, "start"/"stop"), `topic` (required), `keywords` (optional, up to 5 strings) | Unit test: tool schema introspection; integration test: tool call roundtrip |
| AC-02 | `context_cycle(type: "start", topic: "X")` causes session's `feature_cycle` to be set to "X" in SessionRegistry and SQLite sessions table | Integration test: call tool, verify session record in DB |
| AC-03 | If session already has non-NULL `feature_cycle`, `context_cycle(type: "start", topic: "Y")` does NOT overwrite (first-writer-wins) | Integration test: set feature via SessionStart, then call cycle start, verify original value retained |
| AC-04 | `context_cycle(type: "stop", topic: "X")` records a cycle-end observation event with session_id and topic | Integration test: call tool, query observations table for cycle_end event |
| AC-05 | Response indicates whether feature_cycle was set (start) or boundary was recorded (stop) | Unit test: parse response JSON, check `was_set` field for start; check acknowledgment for stop |
| AC-06 | `topic` validated: non-empty, max 128 chars, no control characters (via `sanitize_metadata_field`) | Unit test: empty string rejected, 129-char string rejected, control chars rejected |
| AC-07 | `type` only accepts "start" or "stop"; other values return validation error | Unit test: "pause", "restart", "", null all return errors |
| AC-08 | Heuristic attribution (eager voting, majority vote) unchanged when `context_cycle` not called | Integration test: session without cycle call, verify eager attribution still resolves |
| AC-09 | After `context_cycle(start)`, eager attribution skips (`set_feature_if_absent` returns false) | Integration test: call cycle start, then send topic signals, verify no overwrite |
| AC-10 | Hook handler processes `context_cycle` PreToolUse within 50ms budget | Integration test: measure end-to-end hook latency, assert < 50ms |
| AC-11 | `context_retrospective(feature_cycle: "X")` finds observations for sessions where `context_cycle(start, "X")` was called | Integration test: create session, call cycle start, record observations, run retrospective, verify data returned |
| AC-12 | Wire protocol backward compatible: old hook binaries do not crash on new event types | Unit test: deserialize unknown event_type into ImplantEvent, verify no panic |
| AC-13 | `keywords` accepts array of up to 5 strings, each max 64 chars; excess entries truncated to 5 | Unit test: 7-element array truncated to 5; 65-char string truncated to 64 |
| AC-14 | Keywords stored in session record (SQLite), retrievable for future injection use | Integration test: call cycle start with keywords, query session record, verify keywords present |
| AC-15 | Keywords pass through hook/UDS path and persist via fire-and-forget pattern | Integration test: full hook-to-persistence roundtrip with keywords in payload |

## Domain Models

### Key Terms

| Term | Definition |
|------|-----------|
| **Feature Cycle** | A string identifier (e.g., "col-022") associating a session with a specific feature's work. One session maps to at most one feature cycle. Stored as `feature_cycle` on `SessionRecord`. |
| **Session** | A Claude Code interaction identified by `session_id`. Tracked in `SessionRegistry` (in-memory) and `sessions` table (SQLite). Has lifecycle: Active -> Completed/TimedOut. |
| **SessionRegistry** | In-memory registry of active sessions. Provides `set_feature_if_absent()` for first-writer-wins attribution and topic signal accumulation for eager voting. |
| **Eager Attribution** | Streaming heuristic: as events arrive, `TopicTally` accumulates topic signals. When a topic reaches count >= 3 and > 60% share, it wins via `set_feature_if_absent`. (bugfix-198) |
| **Majority Vote** | Last-chance attribution on SessionClose: resolves remaining topic signals to pick a winner. |
| **First-Writer-Wins** | The invariant that once a session's `feature_cycle` is set (by any mechanism), it cannot be overwritten. Enforced by `set_feature_if_absent()` returning false on subsequent attempts. Supports one-session-one-feature constraint (#1067). |
| **Cycle Boundary Event** | An observation event of type "cycle_end" recorded when `context_cycle(type: "stop")` is called. Marks a logical boundary within a session for retrospective analysis. Does not change session state. |
| **ImplantEvent** | Wire protocol struct carrying observation data: `event_type`, `session_id`, `timestamp`, `payload`, optional `topic_signal`. Used for all hook-to-server event transmission. |
| **Keywords** | Up to 5 semantic strings describing what a feature is about. Stored with the session, intended for future context injection. Not used for attribution. |

### Entity Relationships

```
context_cycle(MCP tool)
    |
    v
PreToolUse hook intercept
    |
    v
ImplantEvent (event_type: "cycle_begin" | "cycle_end")
    |
    v [UDS RecordEvent]
Server-side RecordEvent handler
    |
    +---> cycle_begin: SessionRegistry.set_feature_if_absent()
    |                   + update_session_feature_cycle() [SQLite]
    |                   + persist keywords to session record
    |
    +---> cycle_end:   Insert observation row (boundary marker)
```

## User Workflows

### Workflow 1: SM Agent Declares Feature Cycle at Session Start

1. SM agent spawns and begins working on feature col-022.
2. SM calls `context_cycle(type: "start", topic: "col-022", keywords: ["observation pipeline", "feature attribution"])`.
3. Claude Code fires PreToolUse hook with `tool_name` containing "context_cycle" and `tool_input` with the params.
4. Hook handler extracts topic, builds ImplantEvent(event_type: "cycle_begin"), sends via UDS.
5. Server calls `set_feature_if_absent("col-022")` -- succeeds (first writer).
6. Server persists feature_cycle and keywords to SQLite.
7. MCP tool returns `{ "status": "ok", "was_set": true }`.
8. All subsequent observations in this session are attributed to col-022.
9. Eager attribution signals arrive but `set_feature_if_absent` returns false (already set).

### Workflow 2: Feature Cycle Already Set (SessionStart Path)

1. A session starts with `feature_cycle` set via `input.extra["feature_cycle"]` in SessionStart.
2. SM calls `context_cycle(type: "start", topic: "col-022")`.
3. Hook sends cycle_begin event. Server calls `set_feature_if_absent` -- returns false (already set).
4. MCP tool returns `{ "status": "ok", "was_set": false }`.
5. Session retains the original feature_cycle value.

### Workflow 3: SM Marks End of Feature Work

1. SM finishes feature work and calls `context_cycle(type: "stop", topic: "col-022")`.
2. Hook sends cycle_end event. Server records observation boundary.
3. MCP tool returns `{ "status": "ok", "boundary_recorded": true }`.
4. Session remains attributed to col-022. Post-feature activity in the session is still attributed to col-022.

### Workflow 4: No Explicit Declaration (Backward Compat)

1. Session starts without `feature_cycle` in extra, and no `context_cycle` call is made.
2. Events arrive, topic signals accumulate.
3. Eager attribution resolves at 3+ signals / >60% share, or majority vote resolves on SessionClose.
4. Behavior is identical to pre-col-022.

## Constraints

1. **Hook latency budget (50ms)**: PreToolUse interception must not add meaningful overhead. The cycle_begin/cycle_end UDS message is a single fire-and-forget write, no response wait. Target: <5ms marginal cost (SR-02).
2. **First-writer-wins invariant (#1067)**: `set_feature_if_absent` semantics are non-negotiable. Explicit declaration does not get "override" semantics -- it wins by being first, not by being privileged. SM agents must call `context_cycle(start)` before any file-touching tool calls to avoid losing the race to eager attribution (SR-01 mitigation).
3. **No MCP server session state**: The MCP server does not know which session is calling. All session-aware logic runs on the hook/UDS path. The MCP tool is a trigger, not an executor.
4. **RecordEvent reuse (resolved decision)**: Wire protocol uses existing `RecordEvent { event: ImplantEvent }` with event_type "cycle_begin"/"cycle_end". No new `HookRequest` variants.
5. **Shared validation (SR-07 mitigation)**: A single `validate_cycle_params()` function must be used by both MCP tool and hook handler. Validation logic must not diverge between the two paths.
6. **UDS capabilities**: Hook connections require `SessionWrite` capability, which is already granted to hook connections.
7. **Fire-and-forget pattern**: Session writes follow `spawn_blocking` fire-and-forget. Attribution writes may be lost under server unavailability (accepted risk).
8. **Ordering constraint (SR-01)**: `context_cycle(start)` must be called before other tool calls in a session for guaranteed attribution. If eager attribution resolves first, the explicit declaration is a no-op. This is by design (first-writer-wins), but protocol integration (follow-up) must enforce call ordering.

## Dependencies

### Crates (Existing)

| Crate | Role in col-022 |
|-------|----------------|
| `unimatrix-server` | MCP tool registration (`context_cycle`), UDS listener RecordEvent handler extension |
| `unimatrix-engine` | `ImplantEvent` struct (wire.rs), `SessionRegistry` (set_feature_if_absent) |
| `unimatrix-store` | `SessionRecord` (sessions.rs), SQLite session persistence |
| `unimatrix-observe` | `is_valid_feature_id()` validation (if used), topic signal extraction |

### Existing Components

| Component | Dependency |
|-----------|-----------|
| `set_feature_if_absent()` | Core attribution primitive. Must not be modified. |
| `update_session_feature_cycle()` | SQLite persistence for feature_cycle. Extended to also persist keywords. |
| `sanitize_metadata_field()` | Input validation for topic string. Reused as-is. |
| `extract_event_topic_signal()` | Hook-side topic extraction. Extended to handle context_cycle tool_input. |
| `RecordEvent` handler (UDS listener) | Server-side event routing. Extended with cycle_begin/cycle_end handling. |
| PreToolUse hook handler | Hook-side event dispatch. Extended to detect context_cycle calls. |

### External

- `rmcp` 0.16.0 -- MCP SDK for tool registration (existing dependency).
- `serde_json` -- ImplantEvent payload serialization (existing dependency).

## NOT in Scope

1. **Protocol/agent file updates** -- Updating SM agent definitions and design/delivery/bugfix protocols to call `context_cycle` is integration work tracked as a follow-up GH issue. col-022 ships the tool; integration ships separately (SCOPE Non-Goal 5, SR-04).
2. **Keyword-driven context injection** -- Keywords are stored but not used for semantic search or injection. Follow-up GH issue (SCOPE Non-Goal 6).
3. **Multi-feature sessions** -- One session = one feature constraint (#1067) is maintained. No support for attributing a single session to multiple features (SCOPE Non-Goal 1).
4. **SubagentStart signal weighting** -- Separate enhancement to heuristic pipeline (#214 discussion). Not part of col-022 (SCOPE Non-Goal 2).
5. **Cross-session feature lifecycle** -- cycle_end does not implement feature-level lifecycle spanning multiple sessions (SCOPE Non-Goal 3).
6. **MCP server session state** -- The MCP server remains session-unaware (SCOPE Non-Goal 4).
7. **Override/force-set semantics** -- `context_cycle(start)` does not override existing attribution. First-writer-wins only. If an agent needs a different feature_cycle, it must start a new session.
8. **Retrospective pipeline changes** -- The retrospective pipeline must already handle sessions with explicit attribution. If cycle_end boundary events need special retrospective interpretation, that is a separate enhancement (SR-06 noted).

## Follow-Up Deliverables

Per SR-04 recommendation, the following GH issues must be created as part of col-022 delivery (definition of done):

1. **Protocol integration issue**: Update SM agent definitions and design/delivery/bugfix protocols to call `context_cycle(type: "start", topic: "{feature-id}")` as the first action after session start. Acceptance: all three protocol types include the call; SM agent definition documents the requirement.
2. **Keyword injection issue**: Implement context injection on `context_cycle(start)` using stored keywords for semantic search. Acceptance: keywords trigger search, results injected via hook stdout.

## Knowledge Stewardship

- Queried: /query-patterns for feature cycle attribution, MCP tool validation, hook wire protocol -- found #981/#756 (NULL feature_cycle lesson), #1067 (one-session-one-feature constraint), #318/#234 (MCP tool pipeline conventions), #763 (server-side observation intercept pattern), #246 (wire protocol ADR). These informed domain model definitions and constraint formulation.
