## ADR-005: Goal Byte-Length Guard Enforced at the MCP Tool Handler Layer

### Context

SCOPE.md §Constraints states that goal text is stored as-is with no truncation
or validation at the storage layer. SR-02 flags that a pathological caller
providing a multi-megabyte goal string would store it in `cycle_events` and
subsequently load it into `SessionState` in-memory on every session using that
feature cycle.

`SessionState` is held in a `Mutex<HashMap<String, SessionState>>` in the
`SessionRegistry`. A multi-megabyte `current_goal` cloned into each `get_state`
call would substantially degrade performance on the hot path.

Three placement options were considered:

**Option A**: Truncate at the storage layer (inside `insert_cycle_event`). This
silently corrupts the goal text — the caller would believe the full goal was stored
but would get a truncated version on resume. This violates the principle of
least surprise.

**Option B**: Return a validation error from the storage layer when goal exceeds
the limit. This couples input validation to the persistence layer, which is not
appropriate — the store should be a thin persistence layer.

**Option C**: Enforce a maximum byte length check in `CycleParams` processing at
the MCP tool handler layer (`context_cycle` in `tools.rs`), before the value is
passed to any persistence or in-memory path. This is consistent with how the
existing MCP handlers validate `topic`, `phase`, and `outcome` lengths via
`validate_cycle_params` (already called in the handler).

Option C was chosen. SCOPE.md suggests this as the correct placement ("can be
enforced at the tool layer with a max-byte check if desired").

The guard also applies to the UDS path: `handle_cycle_event` in `listener.rs`
extracts goal from the `ImplantEvent` payload; a max-byte clamp or rejection at
that layer is also appropriate as defence-in-depth.

### Decision

**MCP tool layer**: In `context_cycle` handler, after extracting
`params.goal`, check `goal.len() <= MAX_GOAL_BYTES`. If exceeded, return an
error `CallToolResult::error(...)` with a clear message.

**UDS listener layer**: In `handle_cycle_event`, after extracting `goal` from the
payload, apply the same bound. If exceeded, log a warning and truncate to
`MAX_GOAL_BYTES` (truncate, not reject, since the hook path is fire-and-forget
and returning an error to the hook client is not possible for this code path).
The truncation must be UTF-8 char-boundary safe.

`MAX_GOAL_BYTES = 4096` (approximately 1000 words). This is generous for a
1–2 sentence goal and strictly prevents the multi-megabyte case.

This limit is intentionally not exposed to callers as a parameter — it is a
safety bound, not a configuration knob.

### Consequences

- Well-behaved callers (1–2 sentence goal) are unaffected.
- Pathological callers receive an explicit error from the MCP tool layer.
- The UDS listener truncates silently with a warn log (fire-and-forget constraint).
- `SNIPPET_CHARS` (used in IndexEntry) is 200 chars; a 4096-byte goal is still
  much shorter than content fields that can reach thousands of bytes. Memory
  impact per session is bounded and acceptable.
- The constant `MAX_GOAL_BYTES` should be defined adjacent to `MAX_INJECTION_BYTES`
  and `MAX_PRECOMPACT_BYTES` in `listener.rs` (or in a shared constants location)
  for discoverability.
