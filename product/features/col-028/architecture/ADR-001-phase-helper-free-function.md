## ADR-001: Phase Helper as Free Function Taking (&SessionRegistry, Option<&str>)

### Context

Four MCP read-side tools (`context_search`, `context_lookup`, `context_get`,
`context_briefing`) must each snapshot `SessionState.current_phase` at call time.
The canonical snapshot pattern (crt-025 ADR-001, pattern #3027) is already used in
`context_store`:

```rust
let session_state = ctx.audit_ctx.session_id.as_deref()
    .and_then(|sid| self.session_registry.get_state(sid));
let current_phase: Option<String> =
    session_state.as_ref().and_then(|s| s.current_phase.clone());
```

If this is duplicated inline at each of the four new call sites, the pattern becomes
a de-facto convention invisible to tests and future maintainers. It also makes the
`session_id: Option<&str>` → `Option<String>` chain error-prone to reproduce correctly.

### Decision

Extract the snapshot into a module-level free function in `mcp/tools.rs`:

```rust
pub(crate) fn current_phase_for_session(
    registry: &SessionRegistry,
    session_id: Option<&str>,
) -> Option<String> {
    session_id
        .and_then(|sid| registry.get_state(sid))
        .and_then(|s| s.current_phase.clone())
}
```

All four call sites call this function as the first statement in the handler body,
before any `await` (see ADR-002 for the placement constraint).

The function is `pub(crate)` rather than a method on a handler struct so it can be
called in unit tests without constructing a full `UnimatrixBackend` instance.

### Consequences

- The phase extraction logic exists in exactly one place — testable independently of
  handler construction.
- Future call sites (e.g., a fifth read tool) follow an obvious pattern: call
  `current_phase_for_session` as the first statement.
- The function signature makes the `Option<&str>` session_id contract explicit;
  callers that accidentally pass `session_id.clone()` (giving `Option<String>`) will
  get a compile error, not a subtle ownership issue.
- A unit test can verify the function returns `None` for missing sessions and the
  correct phase string for a registered session with an active phase, without mocking
  the full handler.

Related: ADR-002 (placement constraint), pattern #3027 (context_store canonical form).
