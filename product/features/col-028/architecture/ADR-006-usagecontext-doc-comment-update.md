## ADR-006: UsageContext.current_phase Doc Comment Is a Required Deliverable

### Context

`UsageContext.current_phase` (services/usage.rs, field defined around line 71) carries
this doc comment:

```rust
/// Workflow phase active at the moment `context_store` was called (ADR-001 crt-025).
///
/// Snapshotted from `SessionState.current_phase` at call time — never re-read from
/// live state during drain or spawn. `None` for all non-store operations (search,
/// lookup, get, correct, deprecate, etc.) and for store calls with no active phase.
```

After col-028, this comment is materially wrong. Read-side tools (`context_search`,
`context_lookup`, `context_get`, `context_briefing`) now populate `current_phase`.
The sentence "None for all non-store operations (search, lookup, get, ...)" becomes
a direct falsehood and will mislead any implementor reading this file.

### Decision

Update the `UsageContext.current_phase` doc comment as part of the col-028 deliverable.
The new comment must:

1. Not reference `context_store` as the sole source of phase data.
2. State that read-side tools (search, lookup, get, briefing) also populate this field
   as of col-028.
3. Retain the "snapshotted before any await" race-condition guarantee.
4. Correctly enumerate the tools that pass `None` (correct, deprecate, quarantine —
   mutation tools with no phase-learning semantics).

Example:

```rust
/// Workflow phase active at the moment the MCP tool was called (ADR-001 crt-025,
/// col-028 ADR-002).
///
/// Snapshotted from `SessionState.current_phase` at call time — never re-read from
/// live state during drain or spawn. Populated for all read-side tools (context_search,
/// context_lookup, context_get, context_briefing) as of col-028. `None` for mutation
/// tools (context_correct, context_deprecate, context_quarantine) and for any call
/// with no active session or no active phase.
```

### Consequences

- Future implementors reading `usage.rs` get an accurate description of which tools
  populate the field.
- If `current_phase` is later extended to mutation tools, this doc comment becomes the
  prompt to update the ADR, not just to update code.
- The doc comment update is mandatory — shipping it stale would be an active source of
  implementor confusion on the next feature that touches `UsageContext`.

Related: ADR-002 (placement constraint), ADR-001 crt-025 (#2998).
