## ADR-005: Goal Byte-Length Guard — One Constant, Path-Specific Violation Behavior

### Context

SCOPE.md §Constraints states that goal text is stored as-is with no truncation
or validation at the storage layer. SR-02 flags that a pathological caller
providing a multi-megabyte goal string would store it in `cycle_events` and
subsequently load it into `SessionState` in-memory on every session using that
feature cycle.

`SessionState` is held in a `Mutex<HashMap<String, SessionState>>` in the
`SessionRegistry`. A multi-megabyte `current_goal` cloned into each `get_state`
call would substantially degrade performance on the hot path.

Three placement options were considered for the guard:

**Option A**: Truncate at the storage layer (inside `insert_cycle_event`). This
silently corrupts the goal text — the caller would believe the full goal was stored
but would get a truncated version on resume. Violates the principle of least surprise.

**Option B**: Return a validation error from the storage layer when goal exceeds
the limit. This couples input validation to the persistence layer, which is not
appropriate — the store should be a thin persistence layer.

**Option C (chosen)**: Enforce a maximum byte length check in `CycleParams`
processing at the MCP tool handler layer (`context_cycle` in `tools.rs`), before
the value is passed to any persistence or in-memory path. This is consistent with
how existing MCP handlers validate `topic`, `phase`, and `outcome` lengths via
`validate_cycle_params` (already called in the handler). On the UDS path (which
is fire-and-forget and cannot return errors), apply a truncate-at-char-boundary
strategy instead.

**Earlier design used two different constants** (`MCP_MAX_GOAL_BYTES = 2048` and
`UDS_MAX_GOAL_BYTES = 4096`). The settled design uses **one constant** with
path-specific violation behavior. Using the same limit on both paths is consistent
and prevents a class of subtle bugs: if the MCP path rejects at a lower limit than
the UDS path truncates at, an agent's corrected retry (after a rejection) would
write a new UDS entry at the full MCP-accepted length, which then overwrites the
earlier truncated version cleanly. One constant makes this reasoning trivial.

**Empty/whitespace normalization** (no separate ADR warranted): An empty string
or whitespace-only goal provides no signal and must not be stored as a blank string.
Normalize at the MCP handler before the byte check: `goal.trim()` → if empty, treat
as `None`. This is part of the same validation pass as the byte check.

### Decision

**One constant**:

```rust
pub const MAX_GOAL_BYTES: usize = 1024;
```

Defined adjacent to `MAX_INJECTION_BYTES` and `MAX_PRECOMPACT_BYTES` in the
constants location (e.g., `listener.rs` or a shared `constants.rs`) for
discoverability.

**MCP tool layer** (in `context_cycle` handler, `tools.rs`):

1. Trim whitespace: `let goal = params.goal.map(|g| g.trim().to_owned())`.
2. Normalize empty: if trimmed value is `""`, treat as `None`.
3. Byte check: if `goal.len() > MAX_GOAL_BYTES`, return `CallToolResult::error`
   with a descriptive message that tells the agent exactly what to fix, e.g.:
   `"goal exceeds MAX_GOAL_BYTES bytes; shorten and retry"`.
4. Pass the normalized `Option<String>` to the persistence and session paths.

Agents can correct the oversized goal and retry; the MCP error is actionable.

**UDS listener layer** (in `handle_cycle_event`, `listener.rs`):

1. After extracting `goal` from the `ImplantEvent` payload, check `goal.len() > MAX_GOAL_BYTES`.
2. If exceeded: truncate to the largest UTF-8 char boundary ≤ `MAX_GOAL_BYTES`,
   log `tracing::warn!`, and write the truncated value (last-writer-wins semantics).
3. Rationale for truncate-not-reject: the hook path is fire-and-forget; returning
   an error is not possible. If the MCP path rejects first, the agent's corrected
   retry fires a new UDS write that overwrites the earlier truncated version — no
   failure, no skip, correct final state.

The `MAX_GOAL_BYTES` limit is intentionally not a configuration knob — it is a
safety bound.

### Consequences

- Well-behaved callers (1–2 sentence goal, ≤ 1024 bytes) are unaffected.
- Pathological MCP callers receive an explicit, actionable error; they can shorten
  and retry.
- UDS path truncates with a warn log; agent's corrected retry overwrites cleanly.
- Whitespace-only or empty goals are normalized to `None` at the MCP layer; blank
  strings are never stored.
- One constant `MAX_GOAL_BYTES` means MCP and UDS operate at the same limit, removing
  the two-constant discrepancy from the earlier design.
- `SNIPPET_CHARS` (IndexEntry) is 200 chars; a 1024-byte goal is bounded and
  acceptable for in-memory clone cost per session.
