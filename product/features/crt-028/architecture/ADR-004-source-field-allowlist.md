## ADR-004: Source Field Allowlist Strategy for GH #354

### Context

`HookRequest::ContextSearch` carries an optional `source` field (added in crt-027 ADR-001):

```rust
ContextSearch {
    query: String,
    source: Option<String>,   // "SubagentStart" | None → "UserPromptSubmit"
    ...
}
```

In `listener.rs` `dispatch_request`, this field is written directly to the observations
table `hook TEXT NOT NULL` column:

```rust
hook: source.as_deref().unwrap_or("UserPromptSubmit").to_string(),
```

No length or content validation is applied before the write. The `source` field travels
over the local Unix domain socket from the hook process to the server.

GH #354 identifies this as a security gap: an unexpected or adversarially long value is
written verbatim to persistent storage. The `hook` column is used in the session
observation feed, retrospective analysis, and any future analytics on hook event types.
Writing unexpected values creates schema pollution and potential injection risk if the
column value is ever interpolated into display output.

Three strategies were considered:

**Option A: Length cap only.** Truncate to `MAX_HOOK_COLUMN_BYTES` (e.g., 64 bytes) before
writing. Rejects values that are too long but still allows novel valid-length strings.

Rejected: A length cap does not prevent schema pollution from unexpected values. A new
source type introduced by a future feature would silently appear in the observations table
under a truncated string, making observation queries unreliable. The set of valid source
values is small and known at compile time; a length cap is weaker than an allowlist.

**Option B: Allowlist with error on unknown values.** Match against known values; return
an error response for unknown `source` values.

Rejected: The `source` field is optional and defaults to `None` (backward compatible per
ADR-001 crt-027). Returning an error for unknown values would break existing hook callers
that pass a `source` value from a future version of the hook binary while the server has
not yet been updated. Hook processes are deployed separately from the server. Hard errors
on unknown fields violate the defensive wire protocol principle.

**Option C: Allowlist with fallback to default.** Match against known values;
fall back to `"UserPromptSubmit"` for any unknown/missing value.

Selected. The allowlist exhausts all valid values at the time of writing:
- `"UserPromptSubmit"` — set by the UserPromptSubmit hook arm (source: None → default)
- `"SubagentStart"` — set by the SubagentStart hook arm

Any other value (including new source types introduced by future features not yet deployed
to the server) falls back to `"UserPromptSubmit"`. This is safe because:
1. The observation is still recorded (no silent loss).
2. The hook column value is the "best known approximation" — not an error.
3. When the server is updated to include the new source type in the allowlist, subsequent
   events are tagged correctly.

**No length cap needed**: the allowlist match exhausts all valid values by string equality.
Any value not matching the allowlist falls to the default regardless of length. A
`"SuperLongAdversarialString"` matches neither "UserPromptSubmit" nor "SubagentStart" and
falls to the default. The length is irrelevant.

### Decision

Replace the inline `source.as_deref().unwrap_or("UserPromptSubmit").to_string()` with a
call to a private helper `sanitize_observation_source(source: Option<&str>) -> String`:

```rust
fn sanitize_observation_source(source: Option<&str>) -> String {
    match source {
        Some("UserPromptSubmit") => "UserPromptSubmit".to_string(),
        Some("SubagentStart")    => "SubagentStart".to_string(),
        _                        => "UserPromptSubmit".to_string(),
    }
}
```

The helper is defined in `listener.rs`. The write site becomes:

```rust
hook: sanitize_observation_source(source.as_deref()),
```

When a new source type is introduced in a future feature (e.g., "PreToolUse"), the allowlist
in this function is the single update point. Adding a new arm to the match is the complete
change.

**Test requirement (SR-05)**: A dedicated test must verify the allowlist behavior
independently of the broader transcript extraction tests:
- `Some("UserPromptSubmit")` → `"UserPromptSubmit"`
- `Some("SubagentStart")` → `"SubagentStart"`
- `None` → `"UserPromptSubmit"` (backward compat)
- `Some("unknown")` → `"UserPromptSubmit"` (fallback)
- `Some("")` → `"UserPromptSubmit"` (empty string fallback)
- `Some("UserPromptSubmitXXXXXXXXXXXXXXXXX")` → `"UserPromptSubmit"` (long string fallback)

These six cases directly correspond to AC-11.

### Consequences

- All observation records written to the `hook` column contain only known, schema-valid
  values. Retrospective queries and analytics are not polluted by unexpected strings.
- Future source types introduced before the server allowlist is updated fall back to
  `"UserPromptSubmit"` — the observation is recorded with a slightly incorrect hook label
  rather than being lost.
- The helper is a single-function change in `listener.rs`. No wire protocol changes.
- The `sanitize_observation_source` function is the sole write-gate for this column —
  documented as such to prevent future code adding a second write site that bypasses the
  allowlist.
