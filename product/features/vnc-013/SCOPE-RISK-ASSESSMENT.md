# Scope Risk Assessment: vnc-013

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `normalize_event_name()` fallback to `"claude-code"` for shared event names (PreToolUse, PostToolUse, SessionStart, Stop) without `--provider` flag will silently mislabel Codex events as Claude Code events on the write path | High | High | Architect must decide whether backward-compatible fallback is acceptable or whether `--provider` should be required for Codex configs — document the known semantic imprecision explicitly |
| SR-02 | Gemini `AfterTool` response field name diverges from Claude Code's `tool_response` — `response_size` and `response_snippet` will silently be null for all Gemini PostToolUse observations | Med | Med | Architect should confirm Gemini payload structure from source or a live capture; define explicit degraded-mode contract if unconfirmable |
| SR-03 | `DomainPackRegistry.resolve_source_domain()` returns `"unknown"` for `"Stop"`, `"SessionStart"`, `"cycle_start"`, `"cycle_stop"` — the DB-read-path fix (background.rs:1330, services/observation.rs:585) could change existing `source_domain` from `"claude-code"` to `"unknown"` for these events if registry-based derivation is used without a fallback | High | Med | Spec writer must define an explicit registry-with-fallback contract (Approach A) and update `test_parse_rows_unknown_event_type_passthrough` accordingly |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | PostToolUseFailure normalization precedent (ADR-003 col-027, entry #3475): that ADR explicitly decided NOT to normalize PostToolUseFailure to PostToolUse to preserve signal. The scope's AC-16 guard for `post_tool_use_rework_candidate` must not inadvertently re-open this boundary | Med | Low | Spec writer must ensure AC-16 guard is scoped to rework-candidate strings only; PostToolUseFailure path is untouched |
| SR-05 | Codex reference config ships but is non-functional (blocked by Codex bug #16732). A future fix to #16732 may reveal integration gaps not covered by synthetic unit tests | Med | Med | Architect should design the Codex code path for testability from day one; synthetic tests must be structurally identical to what live Codex would produce |
| SR-06 | Scope excludes `context_cycle_review` logic changes but Gemini-sourced `cycle_start`/`cycle_stop` events must be found by it (AC-09). The assumption that canonical name parity alone is sufficient is untested | Med | Med | AC-09 integration test is the proof; spec writer must include it as a mandatory (not stretch) acceptance criterion |

## Integration Risks

| Risk ID | Risk | Likelihood | Severity | Recommendation |
|---------|------|------------|----------|----------------|
| SR-07 | Blast radius spans 6+ files across 3 crates (hook.rs, wire.rs, listener.rs, background.rs, services/observation.rs, domain/mod.rs). Past wave-based HookType refactor (ADR-004 col-023, entry #2906) showed that blast-radius-wide changes accumulate rework if any site is missed | High | High | Architect must enumerate all blast-radius sites explicitly in the architecture doc and assign AC coverage to each site; no site can be implicit |
| SR-08 | `build_cycle_event_or_fallthrough()` reads `tool_name` from `input.extra["tool_name"]` (top-level) but Gemini puts it in `mcp_context.tool_name` — the adapter must promote this field before calling the function; if missed, `context_cycle` interception silently falls through to generic RecordEvent | High | High | Architect must specify the exact promotion step and confirm `contains("context_cycle")` matching handles bare names without the `"mcp__unimatrix__"` prefix |
| SR-09 | `extract_event_topic_signal()` reads `tool_input` for PreToolUse topic signal; for Gemini payloads, the scope states `tool_input` is at top-level (same as Claude Code) — if this assumption is wrong, topic signal extraction silently degrades to generic stringify | Med | Low | Spec writer should add an explicit AC or unit test covering Gemini BeforeTool topic signal extraction |

## Assumptions

- **Goals §3 / Proposed Approach Layer 3**: Assumes `resolve_source_domain()` is sufficient for DB-read-path source_domain derivation with a `"claude-code"` fallback. This assumption holds only if the builtin claude-code pack's 4-event `event_types` list is never expanded (expansion would change fallback behavior).
- **Goals §6 / Gemini MCP Context**: Assumes `mcp_context.tool_name` is the bare tool name for all Gemini BeforeTool/AfterTool events. If Gemini prefixes tool names differently across versions, normalization silently fails.
- **Non-Goals / Codex**: Assumes Codex bug #16732 will eventually be fixed and that the code paths built here will remain compatible with whatever hook mechanism Codex ships.
- **Background Research / Gemini regex**: Assumes `mcp_unimatrix_.*` is valid Gemini regex syntax for v0.31+. Not verified against a live Gemini CLI instance.

## Design Recommendations

- **SR-01 + SR-07**: Architect should produce an explicit blast-radius table in ARCHITECTURE.md mapping each changed file to the AC that validates it. Entry #2906 (HookType refactor) shows this class of change accumulates rework without per-site coverage ownership.
- **SR-03**: Spec writer must choose Approach A (registry-with-fallback) or document why Approach B is acceptable; test `test_parse_rows_unknown_event_type_passthrough` must be updated to reflect the chosen contract — not left as a spec writer TODO.
- **SR-08**: The `mcp_context.tool_name` → top-level `tool_name` promotion step is the single highest-risk integration point; it must have a dedicated unit test before any other Gemini BeforeTool AC is attempted.
- **SR-04**: Architect should confirm the `post_tool_use_rework_candidate` guard (AC-16) is structurally isolated from the PostToolUseFailure arm — referencing ADR-003 (entry #3475) explicitly in the spec prevents silent scope creep.
