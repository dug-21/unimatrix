## ADR-001: Optional `source` Field on `HookRequest::ContextSearch`

### Context

`dispatch_request` in `listener.rs` hardcodes `hook: "UserPromptSubmit".to_string()` when
inserting an `ObservationRow` for a `HookRequest::ContextSearch` event. This was correct
when `ContextSearch` was exclusively produced by `UserPromptSubmit` hook events.

crt-027 routes `SubagentStart` hook events to `ContextSearch`. If the hardcoded literal is
not replaced, observations from SubagentStart-sourced requests will be tagged
`"UserPromptSubmit"` — misidentifying the hook source in the observations table. This
corrupts retrospective data and any future analysis that segments observations by hook type.

Three options were considered:
1. Add a new `HookRequest` variant `SubagentSearch` that mirrors `ContextSearch` but carries
   a different tag. Rejected: requires separate dispatch arm, duplicates all handler logic,
   violates DRY.
2. Infer source from `role` or `feature` fields in the existing ContextSearch payload.
   Rejected: those fields are `None` for SubagentStart-sourced requests, making inference
   ambiguous. Not robust as more sources are added.
3. Add an optional `source` field with a backward-compatible default. Selected.

Wire backward compatibility is required: existing JSON payloads that omit `source` must
continue to deserialize without error. This rules out a required field.

### Decision

Add `#[serde(default)] source: Option<String>` to `HookRequest::ContextSearch` in
`unimatrix-engine/src/wire.rs`. The field defaults to `None` on deserialization when absent.

In `dispatch_request`, replace the hardcoded `"UserPromptSubmit"` literal with:
```rust
hook: source.as_deref().unwrap_or("UserPromptSubmit").to_string(),
```

In `build_request` (`hook.rs`), the `SubagentStart` arm sets `source: Some("SubagentStart".to_string())`.
The `UserPromptSubmit` arm sets `source: None` (backward compat — defaults to
`"UserPromptSubmit"` at the server).

Any future hook type routed to `ContextSearch` sets the appropriate `source` value.

### Consequences

- Backward compatible: all existing JSON payloads that omit `source` continue to work.
  The server correctly tags them as `"UserPromptSubmit"`.
- Forward extensible: any new hook type routed to `ContextSearch` (e.g., `PreToolUse`
  in a future feature) adds a new source string without changing the wire schema.
- Existing tests constructing `HookRequest::ContextSearch` via struct literal must be
  updated to add `source: None` (or use `..` pattern with a struct-update syntax). This
  is a compile-time-visible breakage, not a silent regression.
- Round-trip tests in `wire.rs` that construct `ContextSearch` must add `source: None`
  or use `..` spread. The test coverage change is minimal.
