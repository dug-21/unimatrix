## ADR-001: PostToolUseFailure Hook Registration and Dispatch Pattern

### Context

Claude Code fires `PostToolUseFailure` (not `PostToolUse`) when a tool call fails. This event was
absent from `.claude/settings.json`, so no hook binary invocation occurred and no observation record
was produced. The `build_request()` dispatcher in `hook.rs` already has a `_` wildcard arm that
calls `generic_record_event`, meaning if the event were ever received it would produce a record with
no `tool_name` extracted — only a raw `input.extra` blob.

The `PostToolUseFailure` payload differs from `PostToolUse` in one critical way: the failure outcome
is carried in `error` (a plain string), not `tool_response` (a JSON object). The existing
`extract_event_topic_signal()` function has explicit arms for `"PreToolUse"` and `"PostToolUse"` but
no arm for `"PostToolUseFailure"`, so it would fall through to the generic stringify path if not
addressed.

Per col-023 ADR-001 (entry #2903), hook types are string constants, not enum variants. Adding a new
hook type means adding a `pub const` and new match arms — no enum change.

### Decision

1. Add `PostToolUseFailure` to `.claude/settings.json` with `matcher: "*"` and command
   `unimatrix hook PostToolUseFailure`, identical in structure to the `PreToolUse` and `PostToolUse`
   entries.

2. Add `pub const POSTTOOLUSEFAILURE: &str = "PostToolUseFailure";` to the `hook_type` module in
   `unimatrix-core/src/observation.rs`. Update the doc comment on `ObservationRecord.response_snippet`
   to list `PostToolUseFailure` alongside `PostToolUse`.

3. Add an explicit `"PostToolUseFailure"` arm in `build_request()` that:
   - Extracts `tool_name` from `input.extra["tool_name"]`
   - Computes `topic_signal` via `extract_event_topic_signal()` (from `tool_input`, same as `PostToolUse`)
   - Builds `RecordEvent { event_type: "PostToolUseFailure", payload: input.extra, topic_signal }`
   - Does NOT enter rework logic — failure events are never rework candidates
   - Does NOT call `extract_response_fields()` in this layer (the error field is handled in listener.rs)

4. Add an explicit `"PostToolUseFailure"` arm in `extract_event_topic_signal()` that extracts from
   `input.extra["tool_input"]`, identical to the `PostToolUse` arm. This ensures topic signals are
   populated for failure events without falling through to the generic stringify path.

5. The hook binary continues to always exit 0 (FR-03.7). All field accesses use `.and_then()` /
   `.unwrap_or_default()` defensive patterns — absent or malformed fields must never panic.

### Consequences

**Easier:**
- `PostToolUseFailure` events now produce properly attributed observation records with `tool_name`
  and `topic_signal` populated.
- The explicit arm prevents the wildcard fall-through that would produce records with no `tool_name`
  (SR-07 mitigated).
- Future additions of other new hook types follow the same pattern.

**Harder / Watch for:**
- The settings.json registration is live immediately. If the server is not running when a failure
  occurs, the event is queued (fire-and-forget queue path) and replayed on next connection — this is
  the existing behaviour for all registered events and requires no special handling.
- Adding the arm to `extract_event_topic_signal()` duplicates 8 lines from the `PostToolUse` arm.
  This is acceptable duplication — the two paths have the same source field but different event
  semantics, and keeping them separate preserves future divergence flexibility.
