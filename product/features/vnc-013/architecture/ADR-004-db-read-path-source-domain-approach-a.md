## ADR-004: Registry-with-Fallback for DB Read Path `source_domain` (Approach A)

### Context

Three sites hardcode `source_domain = "claude-code"`. Site A (`listener.rs:1894`)
is the live write path and has `ImplantEvent.provider` directly available — the fix
is straightforward. Sites B (`background.rs:1330`) and C (`services/observation.rs:585`)
are DB read paths that have the stored event_type string but no provider field (there
is no `source_domain` column in `observations` — it is derived at query time).

For Sites B and C, `DomainPackRegistry.resolve_source_domain()` is available. However,
the builtin claude-code pack only lists 4 event types in its `event_types` filter:
`["PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop"]`. Events outside this
list — `"Stop"`, `"SessionStart"`, `"cycle_start"`, `"cycle_stop"`, `"cycle_phase_end"`,
`"UserPromptSubmit"`, `"PreCompact"`, `"PostToolUseFailure"` — resolve to `"unknown"`,
not `"claude-code"`. See entry #4304 for the prior analysis.

Two approaches for Sites B and C:

**Approach A — Registry-with-fallback**: Call `resolve_source_domain(event_type)`;
if the result is `"unknown"`, fall back to `DEFAULT_HOOK_SOURCE_DOMAIN = "claude-code"`.
This preserves existing behavior for all non-listed event types while correctly
resolving the 4 listed types through the registry. The named constant replaces the
string literal, making the fallback explicit and reviewable.

**Approach B — Accept "unknown"**: Accept that DB read path returns `"unknown"` for
non-listed event types. Simple, no fallback logic. Breaks any consumer that checks
`source_domain == "claude-code"` for session or cycle events. Changes existing
behavior for `"Stop"`, `"cycle_start"`, etc. — requires auditing all consumers of
`source_domain` on DB-read paths. The existing tests `test_parse_rows_unknown_event_type_passthrough`
and `test_parse_rows_hook_path_always_claude_code` would need contract changes.

SR-03 from the risk assessment explicitly recommends Approach A as the less disruptive
path. The DB read path limitation (cannot distinguish Gemini from Claude Code records
without a `source_domain` column) is a documented known limitation, not a bug.

### Decision

Use Approach A (registry-with-fallback) for both Sites B and C.

Define `DEFAULT_HOOK_SOURCE_DOMAIN: &str = "claude-code"` as a named constant.
The constant placement is an open question for the spec writer (see ARCHITECTURE.md
OQ-A). Both sites use the same pattern:

```rust
let source_domain = {
    let resolved = registry.resolve_source_domain(&event_type);
    if resolved != "unknown" {
        resolved
    } else {
        DEFAULT_HOOK_SOURCE_DOMAIN.to_string()
    }
};
```

For `services/observation.rs`: remove the `_` prefix from `_registry` parameter.
The parameter was already present but unused (comment says "available for future
non-hook ingress paths"). This feature is that future path.

Test `test_parse_rows_hook_path_always_claude_code` (asserts `source_domain ==
"claude-code"` for `"PreToolUse"`) remains valid: registry resolves `"PreToolUse"`
to `"claude-code"` → result unchanged.

Test `test_parse_rows_unknown_event_type_passthrough` (asserts `source_domain ==
"claude-code"` for `"UnknownEventType"`) remains valid: registry resolves
`"UnknownEventType"` to `"unknown"` → fallback returns `"claude-code"` → result
unchanged. Update the test comment to remove the "always claude-code" framing and
reference `DEFAULT_HOOK_SOURCE_DOMAIN` instead.

### Consequences

Easier: existing behavior preserved for all event types; no consumer auditing required;
tests require only comment updates; the string literal `"claude-code"` is replaced by
a named constant at both sites simultaneously.

Harder: DB read paths still cannot distinguish Claude Code from Gemini records for
stored canonical event types — this is the known limitation of not persisting
`source_domain`. Documenting it explicitly (and accepting it) is the correct
engineering choice rather than adding a schema migration for a derived field.
