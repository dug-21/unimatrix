# Agent Report: col-027-agent-1-pseudocode

## Summary

Produced per-component pseudocode for all six components of col-027. All files are in
`product/features/col-027/pseudocode/`.

## Files Produced

| File | Component | Wave |
|------|-----------|------|
| `pseudocode/OVERVIEW.md` | Cross-cutting: data flow, shared types, wave dependency graph | — |
| `pseudocode/core-constants.md` | `unimatrix-core/src/observation.rs` — POSTTOOLUSEFAILURE constant | 1 |
| `pseudocode/hook-registration.md` | `.claude/settings.json` — PostToolUseFailure hook entry | 1 |
| `pseudocode/hook-dispatcher.md` | `unimatrix-server/src/uds/hook.rs` — build_request() + extract_event_topic_signal() | 2 |
| `pseudocode/observation-storage.md` | `unimatrix-server/src/uds/listener.rs` — extract_observation_fields() arm + extract_error_field() | 2 |
| `pseudocode/friction-metrics.md` | friction.rs + mod.rs + metrics.rs — atomic three-file commit | 2 |

Note: The IMPLEMENTATION-BRIEF.md component map listed `pre-post-differential-fix` and
`tool-failure-rule` as separate pseudocode files. Per the wave plan in the spawn prompt, these
are combined into `friction-metrics.md` because they share friction.rs and must ship atomically
(ADR-004). The pseudocode file covers all three source files (friction.rs, mod.rs, metrics.rs).

## Components Covered

1. core-constants — `hook_type::POSTTOOLUSEFAILURE` constant + doc comment update
2. hook-registration — settings.json PostToolUseFailure entry
3. hook-dispatcher — explicit `build_request()` arm + `extract_event_topic_signal()` arm
4. observation-storage — `extract_observation_fields()` PostToolUseFailure arm + new `extract_error_field()`
5. friction-metrics — `PermissionRetriesRule` terminal_counts rename + `ToolFailureRule` + `compute_universal()` fix + `default_rules()` registration

## Key Design Decisions Encoded

1. `extract_error_field()` is a NEW sibling function — pseudocode explicitly prohibits calling `extract_response_fields()` in the PostToolUseFailure arm. The negative test (T-OS-06) is included to document why.
2. `"PostToolUseFailure"` match arm in `build_request()` is explicit — placed before the `_` wildcard arm, not extending it.
3. `hook = "PostToolUseFailure"` stored verbatim — normalization block in `extract_observation_fields()` is left unchanged; PostToolUseFailure is not added to it.
4. `ToolFailureRule` threshold is `count > TOOL_FAILURE_THRESHOLD` (strictly greater than 3) — boundary test T-FM-05 explicitly checks count == 3 produces no finding.
5. `terminal_counts` rename is internal to `PermissionRetriesRule` — no external interface changes.
6. Three-file atomic commit constraint encoded in friction-metrics.md Purpose and Anti-Patterns sections, and in T-FM-07 (the cross-site coherence test).

## Codebase Patterns Verified

Read the following existing source before writing pseudocode:
- `observation.rs` — confirmed four existing constants; POSTTOOLUSEFAILURE placement is after POSTTOOLUSE
- `hook.rs` `extract_event_topic_signal()` — confirmed exact existing arms (PreToolUse, PostToolUse, SubagentStart, UserPromptSubmit, wildcard); new arm mirrors PostToolUse exactly
- `hook.rs` `build_request()` — confirmed PostToolUse arm structure with rework logic; PostToolUseFailure arm must NOT include rework logic
- `listener.rs` `extract_observation_fields()` — confirmed existing arms and normalization block; PostToolUseFailure arm added before `"SubagentStop" | _`
- `listener.rs` `extract_response_fields()` — confirmed it reads `payload["tool_response"]`; confirmed the wrong-function risk
- `listener.rs` `truncate_at_utf8_boundary()` — confirmed signature `fn truncate_at_utf8_boundary(s: &str, max_bytes: usize) -> String`
- `friction.rs` — confirmed existing `make_pre`/`make_post` helper structure; `post_counts` variable name; rule structure
- `metrics.rs` — confirmed existing `post_counts`/`pre_counts` variable structure in `compute_universal()`; `hook_type` import present
- `detection/mod.rs` — confirmed `default_rules()` structure; friction group has 4 rules
- `settings.json` — confirmed hook entry structure; `matcher: "*"` pattern for PreToolUse/PostToolUse

## Open Questions

None. All architectural questions were resolved in the source documents. Specifically:
- Q4 from SCOPE.md (topic_signal for PostToolUseFailure) resolved: use `tool_input` field, same as PostToolUse
- Rule name `"tool_failure_hotspot"` confirmed authoritative (SPECIFICATION.md over SCOPE.md/ARCHITECTURE.md's `"tool_failures"`)
- `make_failure` helper does not need `response_snippet` set by default (follows make_pre/make_post pattern)
- `terminal_counts` rename scope: internal to PermissionRetriesRule only; variable in metrics.rs also renamed for clarity but is independent

## Knowledge Stewardship

- Queried: /uni-query-patterns for PostToolUseFailure hook event type dispatch patterns -- MCP parameter error on k parameter; no results retrieved. Proceeded from ADR files.
- Queried: /uni-query-patterns for col-027 architectural decisions -- same MCP error. Proceeded from ADR files in `product/features/col-027/architecture/`.
- Deviations from established patterns: none. All pseudocode follows:
  - String constants over enum (col-023 ADR-001)
  - Explicit match arms before wildcard (established hook.rs pattern)
  - Fire-and-forget RecordEvent routing (established observation path)
  - Sibling function extractor pattern (extract_response_fields model)
  - make_pre/make_post test helper pattern (friction.rs model)
  - source_domain == "claude-code" pre-filter (all friction rules)
