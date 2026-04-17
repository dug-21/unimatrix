# Agent Report: vnc-013-agent-2-spec

## Output

`product/features/vnc-013/specification/SPECIFICATION.md`

## Key Decisions Made

**D-01: Approach A is the required DB-read-path contract** (C-09, FR-06.2, FR-06.3).
Approach B (accepting `"unknown"` for non-listed events) was rejected. `resolve_source_domain()` returns `"unknown"` for `"Stop"`, `"SessionStart"`, `"cycle_start"`, `"cycle_stop"` — using it without a fallback changes existing behavior and may break `source_domain == "claude-code"` checks in downstream consumers. Approach A (registry-with-fallback to `"claude-code"`) preserves the hook-path invariant and is the mandatory implementation.

**D-02: `test_parse_rows_unknown_event_type_passthrough` remains correct under Approach A** (FR-07.2).
Under Approach A, `resolve_source_domain("UnknownEventType")` → `"unknown"` → fallback → `"claude-code"`. The assertion passes. The test comment must be updated to state the Approach A contract, but the test logic is unchanged.

**D-03: `mcp_context` as an explicit named field on `HookInput`** (FR-02.1).
The SCOPE.md noted either named field or `extra.get("mcp_context")` was acceptable. Named field chosen for type clarity and to avoid stringly-typed access in the adapter step (FR-04.3). Annotated `#[serde(default)]` for backward compatibility.

**D-04: AC-16 guard scoped to rework-candidate string only** (FR-08.2).
SR-04 risk: the guard must not touch the `PostToolUseFailure` arm. Per ADR-003 col-027 (entry #3475), `PostToolUseFailure` is intentionally not normalized to `PostToolUse`. Spec explicitly isolates the guard's scope to `"post_tool_use_rework_candidate"` only.

**D-05: Blast-radius file enumeration is a C-11 constraint** (from SR-07).
All 6 files are enumerated in a table with specific change required and AC coverage per file. `domain/mod.rs` is included explicitly as "no changes required" to close the SR-07 risk that implicit no-change decisions get missed.

**D-06: AC-09 is mandatory, not stretch** (from SR-06).
SCOPE.md already stated this; spec reinforces it with the word "mandatory" in the AC table and in FR-10.

## Constraints That Could Conflict with SCOPE.md

None identified. All constraints in the specification are drawn directly from SCOPE.md constraints, resolved open questions, and the SCOPE-RISK-ASSESSMENT.md recommendations. No new constraints introduced beyond what the scope materials mandate.

## Known Limitations (Documented in Spec)

- DB-read-path sites (`background.rs:1330`, `services/observation.rs:585`) cannot distinguish Claude Code from Gemini records after normalization. Gemini `"BeforeTool"` is stored as `"PreToolUse"` — `resolve_source_domain("PreToolUse")` returns `"claude-code"`. Only `listener.rs:1894` (write path) correctly labels `"gemini-cli"`. Accepted per resolved Open Question 4.
- Gemini `AfterTool` `response_size`/`response_snippet` will be null if the response field name differs from Claude Code's `tool_response`. Degrade gracefully (SR-02).
- Codex live end-to-end testing blocked by upstream bug #16732.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entry #4298 (hook-normalization-boundary pattern) confirmed normalization-at-boundary approach and mcp_context promotion requirement; entries #2903 and #2906 (col-023 ADRs) confirmed blast-radius enumeration discipline and wave-based coverage patterns.
