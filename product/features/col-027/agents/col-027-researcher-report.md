# Researcher Agent Report — col-027

## SCOPE.md
Written to: `product/features/col-027/SCOPE.md`

## Key Findings

### 1. The gap is confirmed and precisely located

`.claude/settings.json` registers 8 hook events. `PostToolUseFailure` is absent. Claude Code fires
`PostToolUseFailure` instead of `PostToolUse` when a tool call fails — so failures have always
produced a `PreToolUse` record with no corresponding terminal record. This is not a theoretical
gap; Unimatrix lesson #3446 already documents the downstream impact.

### 2. The implementation surface is small and well-bounded

The change touches exactly 6 locations:
- `.claude/settings.json` (1 new entry)
- `unimatrix-core/src/observation.rs` (1 new constant, 1 doc update)
- `unimatrix-server/src/uds/hook.rs` `build_request()` (1 new match arm in `PostToolUse`-like
  pattern)
- `unimatrix-server/src/uds/listener.rs` `extract_observation_fields()` (1 new match arm)
- `unimatrix-observe/src/detection/friction.rs` `PermissionRetriesRule` (fix differential)
- `unimatrix-observe/src/metrics.rs` `compute_universal()` (fix `permission_friction_events`)

Plus 1 new detection rule (`ToolFailureRule`) in friction.rs.

### 3. No schema migration required

The `observations` table stores `hook TEXT` without an enum constraint. A new event_type string
flows through the existing ingest path unchanged (`let event_type: String = hook_str` — line 581
of observation.rs). This is entirely additive.

### 4. PermissionRetriesRule fix is straightforward

The rule counts `PostToolUse` in the `post_counts` map. Adding `PostToolUseFailure` to the same
map arm is a one-line change. Existing tests that verify the rule's behavior on balanced Pre/Post
pairs will continue to pass; a new test verifying that Pre+Failure pairs do not trigger the rule
must be added.

### 5. The PostToolUseFailure payload field names are an open question

The `PostToolUse` handler extracts `tool_name` and `tool_response` from `input.extra`. The
`PostToolUseFailure` payload is expected to share this structure, but this is inferred from the
hook registration research rather than from a live payload capture. The implementation should
include a defensive fallback (try `error` / `error_message` as alternative field names) or at
minimum a clear comment noting the assumption.

### 6. No stdout output path is needed

`PostToolUseFailure` is observation-only. The hook dispatcher already correctly classifies
`RecordEvent` requests as fire-and-forget (lines 136-143 of hook.rs). No response routing or
stdout writing is required.

### 7. Blast radius for detection rules is limited

Grep across all 21 detection rules confirms that only `PermissionRetriesRule` uses the Pre-Post
differential. Other rules filter on `"PreToolUse"` + specific tool combinations or `"PostToolUse"`
+ specific tool combinations, but do not use the differential and are not affected by the new
event type (they will simply never see a `PostToolUseFailure` record in their filter path).

## Proposed Scope Boundaries

**In scope**: Registration, dispatcher arm, storage arm, constant, PermissionRetriesRule fix,
permission_friction_events metric fix, new ToolFailureRule.

**Out of scope**: Retroactive retrospective correction, error classification, allowlist
recommendation text changes, rule renaming.

**Rationale**: The fix is self-contained. Detection rule naming and recommendation text are
separate concerns with their own prior tracking (col-026 AC-19 already addressed recommendation
framing; renaming PermissionRetriesRule would require broader documentation changes with no
functional benefit).

## Open Questions for Human

1. **PostToolUseFailure payload field names**: Should the implementation assume `tool_name` and
   `tool_response` (same as PostToolUse) and add a defensive fallback, or is there a known Claude
   Code changelog / spec that confirms the exact field names? This affects AC-03.

2. **ToolFailureRule threshold**: Proposed threshold is 3 failures per tool. Is this calibrated
   against any real session data, or should it start higher to avoid noise during normal
   development (where some tool failures are expected)?

3. **PermissionRetriesRule rename**: The rule name `permission_retries` is now known to be a
   misnomer (it measures tool failures + cancellations, not permission dialogs). col-027 fixes the
   computation but not the name. Should a rename be in scope here, or filed as a follow-on?

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for hook observation event_type tool failure detection rules --
  found lesson #3446 (PermissionRetriesRule misattribution confirmed), lesson #3330 (pre-post
  differential meaning), pattern #3419 (permission_friction_events is tool-cancellation proxy),
  ADR #2903 (col-023 string-based event type model).
- Stored: entry #3471 "Adding a new Claude Code hook event type: registration + dispatcher +
  storage + detection" via `/uni-store-pattern`
