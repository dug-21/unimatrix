# vnc-013: Canonical Event Normalization for Multi-LLM Hook Providers

## Problem Statement

Unimatrix currently hardcodes two assumptions at the hook ingest boundary that make
multi-LLM participation impossible without cascading changes:

1. `source_domain = "claude-code"` is hardcoded in two places (`background.rs:1330`
   and `listener.rs:1894`) regardless of which LLM client actually fired the hook.
2. `build_request()` in `hook.rs` only handles Claude Code event names (`PreToolUse`,
   `PostToolUse`, `PostToolUseFailure`, `SubagentStart`, `SubagentStop`, `SessionStart`,
   `Stop`, `PreCompact`). Gemini CLI events (`BeforeTool`, `AfterTool`, `SessionEnd`)
   fall through to the generic `RecordEvent` wildcard arm, losing the structured
   dispatch — including the `build_cycle_event_or_fallthrough()` interception that
   writes `cycle_start`/`cycle_stop` to `cycle_events`.

The result: when Gemini CLI fires `BeforeTool` for a `context_cycle` tool call, zero
hook processing occurs — `cycle_events` is never written, `source_domain` would be
stamped "claude-code" even if it were written, and `context_cycle_review` finds nothing.

Codex CLI has an open bug (#16732) where MCP tool calls do not fire hooks at all, so
client-side hooks cannot close the Codex gap today. However, `source_domain` must still
be correct for any Codex records that do arrive via other paths.

## Goals

1. Define a canonical Unimatrix event taxonomy — Claude Code event names as canonical
   (confirmed by ASS-051: Option A) — that all downstream consumers operate on
   exclusively. Provider-specific names (e.g., Gemini `BeforeTool`) are normalized to
   these canonical names at the ingest boundary; nothing below that boundary sees
   provider-specific strings.
2. Implement a normalization layer at the ingest boundary (`hook.rs` `build_request()`)
   that translates provider-specific event names to canonical names before any
   `HookRequest` is constructed.
3. Make `source_domain` dynamic — derived from the incoming event name or a provider
   hint field, not hardcoded to `"claude-code"`.
4. Add `provider: String` to `HookInput` and `ImplantEvent` so the normalization layer
   can tag each event with its originating provider.
5. Extend `build_request()` with Gemini CLI arms (`BeforeTool` → canonical pre-tool,
   `AfterTool` → canonical post-tool, `SessionEnd` → canonical session-stop) that
   reuse existing dispatch logic, including `build_cycle_event_or_fallthrough()`.
6. Add Gemini CLI-specific `mcp_context` field handling to `HookInput` so MCP tool
   name and server identity are extractable for Gemini hook payloads.
7. Provide reference `.gemini/settings.json` configuration covering the four hook
   events that Unimatrix needs from Gemini CLI.
8. Ensure nothing below the normalization boundary — `listener.rs`,
   `extract_observation_fields()`, `background.rs`, `context_cycle_review`,
   `knowledge_reuse.rs`, `DomainPackRegistry`, `query_log.rs` SQL, `cycle_events`
   table — branches on provider-specific event names.

## Non-Goals

- Codex CLI client-side hook support. Codex bug #16732 (MCP tool calls do not fire
  hooks) is upstream. This feature does NOT wait for it. Server-side session attribution
  via `clientInfo.name` + `Mcp-Session-Id` (recommended in ASS-049) is a separate
  feature.
- Adding `source_domain` as a new column to the `observations` table. The `hook`
  column (stored canonical event name) is the correct filter key; `source_domain` is
  a runtime-derived attribute (from `DomainPackRegistry.resolve_source_domain()`) not
  a persistence concern. A schema migration is explicitly out of scope.
- Continue, Cursor, or Zed hook provider support. Primary targets are Claude Code
  and Gemini CLI. Codex is conditionally in scope (reference config only, blocked on
  #16732).
- SubagentStart/SubagentStop equivalents for Gemini or Codex. Neither client has a
  subagent hook concept that maps to Unimatrix's. Out of scope.
- Gemini `BeforeModel` hook support for context injection at LLM request level.
  Different architecture — separate spike if needed.
- Changes to `context_cycle_review` logic. The canonical event names `cycle_start`,
  `cycle_stop`, and `cycle_phase_end` are already provider-neutral. No changes needed.
- Changes to the `DomainPackRegistry` evaluation path. The registry's
  `resolve_source_domain()` already works by event type matching. Only the ingest
  hardcode must change.
- MCP schema fixes (Gemini `$defs`, union types, reserved names). Separate feature
  (vnc-012 successor or standalone). Not a hook normalization concern.
- Tool description rewrites (`context_briefing` NLI language, `context_cycle` hook path
  framing). Separate delivery task per ASS-049 recommendation.
- `max_tokens` enforcement on the MCP path. Separate delivery task.

## Background Research

### Canonical Event Names Already Exist

The cycle event names `cycle_start`, `cycle_phase_end`, and `cycle_stop` (constants in
`crates/unimatrix-server/src/infra/validation.rs`) are already provider-neutral —
they are synthetic events produced by `build_cycle_event_or_fallthrough()` regardless
of whether the trigger was Claude Code `PreToolUse` or Gemini `BeforeTool`. The cycle
pipeline is already normalized at this level. The observation event names (`PreToolUse`,
`PostToolUse`, etc.) are not yet normalized.

### hook_type Constants Are Already Strings (col-023 ADR-001)

`unimatrix-core/src/observation.rs` contains the `hook_type` module with string
constants (`PRETOOLUSE = "PreToolUse"`, `POSTTOOLUSE = "PostToolUse"`, etc.). The
`HookType` enum was replaced in col-023 with these string constants precisely to allow
arbitrary event_type strings through the pipeline without code changes. The
infrastructure is ready — only the ingest boundary needs normalization.

### Blast Radius: Event Name String Comparisons in Production Code

The following locations compare against provider-specific event name strings and
constitute the blast radius that normalization must eliminate or correctly absorb:

**hook.rs `build_request()` — the normalization site:**
- `"SessionStart"` → `HookRequest::SessionRegister`
- `"Stop" | "TaskCompleted"` → `HookRequest::SessionClose`
- `"Ping"` → `HookRequest::Ping`
- `"UserPromptSubmit"` → routes to `ContextSearch` with word-count guard
- `"PreCompact"` → `HookRequest::CompactPayload`
- `"PostToolUse"` → rework candidate dispatch with tool-name routing
- `hook_type::POSTTOOLUSEFAILURE` → explicit `RecordEvent` arm
- `"PreToolUse"` → `build_cycle_event_or_fallthrough()`
- `"SubagentStart"` → `ContextSearch` via transcript tail extraction

**hook.rs `extract_event_topic_signal()` — topic signal extraction:**
- Explicit arms for `"PreToolUse"`, `"PostToolUse"`, `POSTTOOLUSEFAILURE`,
  `"SubagentStart"`, `"UserPromptSubmit"`. All other events fall to generic stringify.

**listener.rs `extract_observation_fields()` — DB row construction:**
- Match arms for `"PreToolUse"`, `"PostToolUse" | "post_tool_use_rework_candidate"`,
  `"SubagentStart"`, `hook_type::POSTTOOLUSEFAILURE`, `"SubagentStop" | _`.
- After normalization: Gemini canonical names map to the same arms; no new arms needed.

**listener.rs source_domain hardcode (line 1894):**
- `source_domain: "claude-code".to_string()` — must become dynamic.

**background.rs `parse_observation_rows()` source_domain hardcode (line 1330):**
- `let source_domain = "claude-code".to_string();` — must become dynamic.

**background.rs test fixture (line 3521, 3537):**
- `event_type: "PreToolUse".to_string()` and `"PostToolUse".to_string()` in test data —
  these remain valid as canonical names if "PreToolUse"/"PostToolUse" are chosen as
  canonical.

**knowledge_reuse.rs (line 87):**
- `if record.event_type != "PreToolUse"` — operates on stored DB values, not live
  event names. Insulated by normalization at ingest.

**tools.rs `context_cycle_review` handler (lines 3655, 3665, 3679, 3694):**
- `o.event_type == "SubagentStart"` — agent extraction
- `o.event_type == "PreToolUse"` — tool distribution and knowledge served/stored
- These operate on `ObservationRecord` structs from DB reads, which will contain
  canonical names after normalization. Content is correct if "PreToolUse" is canonical.

**query_log.rs SQL (lines 253, 300):**
- `AND o.hook = 'PreToolUse'` — hardcoded SQL string in two functions.
- After normalization, Gemini `BeforeTool` is stored as canonical `"PreToolUse"` so
  these queries correctly include Gemini tool call observations without SQL changes.

**domain/mod.rs `builtin_claude_code_pack()` (lines 48-53):**
- `event_types: vec!["PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop"]`
- These are the filter list for `resolve_source_domain()`. If canonical names match
  these strings, no change needed.

**DomainPackRegistry.resolve_source_domain():**
- Used only for non-hook-path records (comment in source). After this feature,
  hook-path records will derive `source_domain` dynamically from the provider field
  rather than using `resolve_source_domain()`. This method remains correct as-is for
  external domain pack records.

### Provider Event Mapping (From ASS-049 FINDINGS-HOOKS.md)

| Gemini CLI Event | Claude Code Analog | Canonical Name |
|---|---|---|
| `BeforeTool` | `PreToolUse` | `PreToolUse` |
| `AfterTool` | `PostToolUse` | `PostToolUse` |
| `SessionStart` | `SessionStart` | `SessionStart` |
| `SessionEnd` | `Stop` | `Stop` (or new canonical `SessionEnd`) |
| (none) | `SubagentStart` | `SubagentStart` |
| (none) | `SubagentStop` | `SubagentStop` |
| (none) | `PreCompact` | `PreCompact` |
| (none) | `UserPromptSubmit` | `UserPromptSubmit` |
| (none) | `PostToolUseFailure` | `PostToolUseFailure` |

Codex CLI events: identical names to Claude Code for `SessionStart`, `PreToolUse`,
`PostToolUse`, `Stop` — but Codex does not fire hooks for MCP tool calls (#16732).
No normalization needed for Codex events; they pass through unchanged.

### Gemini MCP Context Field

Gemini's `BeforeTool`/`AfterTool` payloads include a structured `mcp_context` field:
```json
{
  "server_name": "unimatrix",
  "tool_name": "context_cycle",
  "url": "http://..."
}
```
The `tool_name` field in `mcp_context` is the bare tool name (without server prefix).
Claude Code sends `tool_name` at the top level of the hook payload as
`"mcp__unimatrix__context_cycle"`. `build_cycle_event_or_fallthrough()` already handles
this via substring matching. Gemini's `mcp_context.tool_name` needs to be read as the
source for Gemini arms.

### source_domain Derivation Options

Two viable approaches for making `source_domain` dynamic at the ingest boundary:

**Option A — event-name-based derivation:**
After normalization, the provider is inferred from which mapping table translated the
event: if the incoming event was `"BeforeTool"`, the provider is `"gemini-cli"`. If
it was `"PreToolUse"`, it is `"claude-code"` (or `"codex-cli"` if Codex eventually
fires hooks). This requires passing a `provider: &str` through the normalization
function into `ImplantEvent` or as a separate return value from `build_request()`.

**Option B — `provider` field in `HookInput`:**
Add `#[serde(default)] pub provider: Option<String>` to `HookInput`. The hook binary
is invoked as `unimatrix hook BeforeTool` — the event name argument IS the provider
discriminator. Populate `provider` before dispatching. This is cleaner: it threads
provider identity through `ImplantEvent` into the listener without coupling
normalization logic to the source_domain string.

Option B is recommended. It makes provider identity an explicit wire-protocol field
rather than an inference rule, which will be easier for future providers to extend.

### Canonical Name Strategy: Keep Claude Code Names

The simplest canonical strategy: use Claude Code event names as canonical Unimatrix
event names, since:
1. All existing code, tests, SQL queries, and detection rules already operate on
   Claude Code names.
2. Gemini and Codex maps trivially onto them (BeforeTool → PreToolUse, AfterTool →
   PostToolUse, SessionEnd → Stop).
3. No database migration or schema version bump required.
4. `builtin_claude_code_pack()` event_types list requires no change.

The alternative (neutral names like `tool_pre_invoke`, `session_start`, etc.) would
require changes in every downstream string comparison — a blast radius equivalent to
the col-023 HookType enum replacement, with no behavioral benefit at this time.

### DB Schema: observations.hook Column Is Already Generic

`observations` table: `hook TEXT NOT NULL`. This column stores the event_type string.
No `source_domain` column exists — `source_domain` is derived at query time in
`background.rs` and `listener.rs`. After this feature, `source_domain` will be derived
from the `provider` field on `ImplantEvent` rather than hardcoded. The DB schema does
NOT need a new column.

## Proposed Approach

### Layer 1: Wire Protocol Extension (`unimatrix-engine/src/wire.rs`)

Add `provider: Option<String>` to both `HookInput` and `ImplantEvent`:

```rust
// HookInput — what the hook binary reads from stdin
pub struct HookInput {
    // ... existing fields ...
    /// Provider hint injected by the hook binary from the event name.
    /// "claude-code" | "gemini-cli" | "codex-cli"
    #[serde(default)]
    pub provider: Option<String>,
    /// Gemini CLI mcp_context field (BeforeTool/AfterTool only).
    #[serde(default)]
    pub mcp_context: Option<serde_json::Value>,
}

// ImplantEvent — what crosses the UDS wire to the listener
pub struct ImplantEvent {
    // ... existing fields ...
    /// Originating provider. Set at normalization boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}
```

### Layer 2: Normalization (`unimatrix-server/src/uds/hook.rs`)

Add a `normalize_event_name(event: &str) -> (&str, &str)` function that returns
`(canonical_event_name, provider)`:

```
"BeforeTool"  → ("PreToolUse",  "gemini-cli")
"AfterTool"   → ("PostToolUse", "gemini-cli")
"SessionEnd"  → ("Stop",        "gemini-cli")
"SessionStart"→ ("SessionStart","claude-code")  // or provider-agnostic
"PreToolUse"  → ("PreToolUse",  "claude-code")
"PostToolUse" → ("PostToolUse", "claude-code")
"Stop"        → ("Stop",        "claude-code")
// ... all remaining Claude Code events map to themselves
_ (unknown)   → (event,         "unknown")
```

`run()` calls `normalize_event_name(event)` before `build_request()`. The canonical
name is passed to `build_request()`; the provider is propagated into each
`ImplantEvent.provider`.

Gemini `BeforeTool` Arm: After normalization to `"PreToolUse"`, the existing
`build_cycle_event_or_fallthrough()` handles `context_cycle` interception. However,
Gemini's payload structure differs: `tool_name` is in `mcp_context.tool_name` (bare
name) instead of `input.extra["tool_name"]` (prefixed name). An adapter step reads
`mcp_context.tool_name` and normalizes the payload structure before dispatching to
the shared `build_cycle_event_or_fallthrough()` function.

### Layer 3: Listener source_domain Derivation (`unimatrix-server/src/uds/listener.rs`)

Replace the two hardcoded `source_domain: "claude-code".to_string()` lines with:

```rust
source_domain: event.provider.clone().unwrap_or_else(|| "claude-code".to_string()),
```

`background.rs:1330` cannot read `ImplantEvent.provider` because it reads from the DB
(which has no `provider` column). Use `DomainPackRegistry::resolve_source_domain(event_type)`
instead of hardcoding. This is already implemented — the hardcode just needs to be
replaced with a call to the existing method.

### Layer 4: Reference Configurations

Provide `.gemini/settings.json` reference configuration for Unimatrix hook registration:

```json
{
  "hooks": {
    "BeforeTool": [{
      "matcher": "mcp_unimatrix_.*",
      "hooks": [{ "type": "command", "command": "unimatrix hook BeforeTool" }]
    }],
    "AfterTool": [{
      "matcher": "mcp_unimatrix_.*",
      "hooks": [{ "type": "command", "command": "unimatrix hook AfterTool" }]
    }],
    "SessionStart": [{
      "hooks": [{ "type": "command", "command": "unimatrix hook SessionStart" }]
    }],
    "SessionEnd": [{
      "hooks": [{ "type": "command", "command": "unimatrix hook SessionEnd" }]
    }]
  }
}
```

The matcher `mcp_unimatrix_.*` is a Gemini regex on MCP tool names, matching all
Unimatrix tools. `BeforeTool` fires for all Unimatrix tool calls; `build_request()`
only intercepts `context_cycle`; others fall through to generic `RecordEvent`.

## Acceptance Criteria

- AC-01: `normalize_event_name("BeforeTool")` returns `("PreToolUse", "gemini-cli")`.
  `normalize_event_name("AfterTool")` returns `("PostToolUse", "gemini-cli")`.
  `normalize_event_name("SessionEnd")` returns `("Stop", "gemini-cli")`.
  All Claude Code event names map to themselves with provider `"claude-code"`.
  Unknown event names map to themselves with provider `"unknown"`.

- AC-02: When Gemini fires `BeforeTool` for `context_cycle` with `type="start"`,
  `build_request()` produces `HookRequest::RecordEvent { event_type: "cycle_start" }`.
  The payload contains `feature_cycle`. `cycle_events` is written by `listener.rs`.

- AC-03: When Gemini fires `BeforeTool` for any Unimatrix tool other than
  `context_cycle`, `build_request()` produces `HookRequest::RecordEvent` with
  `event_type: "PreToolUse"` and `provider: "gemini-cli"`.

- AC-04: When Gemini fires `AfterTool`, `build_request()` produces
  `HookRequest::RecordEvent` with `event_type: "PostToolUse"` and
  `provider: "gemini-cli"`. Rework candidate logic does NOT trigger (rework detection
  is Claude Code-specific; filter by `source_domain != "gemini-cli"` or by provider
  in the rework arm).

- AC-05: `ImplantEvent.provider` is present and non-None for all events processed
  through the normalization layer.

- AC-06: In `listener.rs`, `source_domain` on the written `ObservationRecord` is
  `"gemini-cli"` for Gemini-originated events and `"claude-code"` for Claude
  Code-originated events.

- AC-07: In `background.rs` `parse_observation_rows()`, `source_domain` is derived
  via `DomainPackRegistry::resolve_source_domain(event_type)` rather than hardcoded.
  Unit test: a synthetic observation with `hook = "PreToolUse"` resolves to
  `source_domain = "claude-code"`.

- AC-08: All existing unit and integration tests pass without modification. The
  normalization layer is additive — no existing behavior changes for Claude Code events.

- AC-09: `context_cycle_review` returns correct results for a Gemini-sourced feature
  cycle: a synthetic test inserts `cycle_start`/`cycle_stop` events (written via the
  Gemini `BeforeTool` code path) and verifies that `context_cycle_review` finds and
  processes them.

- AC-10: The `.gemini/settings.json` reference configuration is written and matches
  the exact format required by Gemini CLI v0.31+. The matcher regex
  `mcp_unimatrix_.*` covers all 12 Unimatrix tools.

- AC-11: `extract_event_topic_signal()` handles the canonical event names correctly
  for Gemini-sourced events. `"PreToolUse"` (normalized from `"BeforeTool"`) extracts
  topic signal from `tool_input` — but for Gemini payloads where `tool_input` is
  nested under `mcp_context`, the adapter populates the top-level field before
  extraction.

- AC-12: Rework candidate detection (`is_rework_eligible_tool()`, `is_bash_failure()`)
  is gated to `source_domain == "claude-code"` (or equivalently `provider == "claude-code"`).
  Gemini `AfterTool` events do not enter the rework tracking path.

- AC-13: `DomainPackRegistry` builtin claude-code pack `event_types` list does NOT
  need to include Gemini event names (they are normalized before reaching the registry).
  The registry requires no changes.

- AC-14: `HookInput.mcp_context` is deserialized from Gemini `BeforeTool`/`AfterTool`
  payloads. `tool_name` is extracted from it for the `context_cycle` interception test
  in `build_cycle_event_or_fallthrough()`.

- AC-15: The `unimatrix hook` subcommand binary accepts Gemini event names without
  error. `run("BeforeTool", ...)` completes normally (exit 0) regardless of whether
  the server is running.

## Constraints

1. **No DB schema changes.** The `observations` table `hook TEXT` column is sufficient.
   `source_domain` is not persisted; it is derived at read time. A schema migration
   would require a version bump and migration test — unjustified for a derived field.

2. **No tokio runtime in hook.rs.** The hook subcommand is synchronous (ADR-002 from
   hook.rs header comment). All normalization logic must be synchronous `&str` → `&str`
   with no I/O.

3. **Rework detection must remain Claude Code-only.** The rework candidate path
   (`is_rework_eligible_tool`, `is_bash_failure`, `extract_file_path`) is specific to
   Claude Code's tool ecosystem (Bash, Edit, Write, MultiEdit). Gemini's `AfterTool`
   for MCP tools must NOT enter this path. The `provider` field is the discriminator.

4. **`build_cycle_event_or_fallthrough()` must remain the single implementation** for
   cycle event construction. The Gemini `BeforeTool` arm normalizes the payload into
   the same shape expected by `build_cycle_event_or_fallthrough()` and then calls it.
   No duplication of this function.

5. **Hook exit is always 0 (FR-03.7).** Normalization failures (unrecognized provider,
   malformed `mcp_context`) must produce no error return from `run()`. Log via
   `eprintln!` and degrade gracefully to `generic_record_event`.

6. **Gemini hook registration uses regex matching.** The `.gemini/settings.json`
   matcher must be a valid Gemini regex. The pattern `mcp_unimatrix_.*` is confirmed
   from Gemini CLI v0.31+ documentation and the ASS-049 hook findings.

7. **`HookInput` backward compatibility.** New fields (`provider`, `mcp_context`) must
   use `#[serde(default)]`. Existing Claude Code hook JSON will not contain these
   fields; missing fields must deserialize to `None` without error.

8. **Codex reference config is documentation-only.** Since Codex #16732 is open,
   the `.codex/hooks.json` reference (if included) must carry a caveat that MCP tool
   call hook support is pending upstream. No code path is added specifically for Codex.

## Open Questions

1. **Rework detection gate**: Should the Gemini `AfterTool` arm be gated by
   `provider == "gemini-cli"` (positively identified) or by the absence of
   rework-eligible tool names in Gemini's payload? The provider-based gate is cleaner
   but requires the `provider` field to be populated before `build_request()` branches.
   The tool-name gate would work incidentally (Gemini does not send `"Bash"` or `"Edit"`)
   but is fragile if Gemini ever wraps shell commands under similar names.
   **Proposed resolution**: Use provider-based gate. Spec writer should confirm.

2. ~~**Canonical name for `SessionEnd` → `Stop`**~~ **RESOLVED (ASS-051)**: Canonical
   name is `"Stop"`. `SessionEnd` (Gemini) normalizes to `"Stop"` at ingest. No new
   canonical name introduced. Existing DB content and `extract_observation_fields()`
   wildcard arm are correct as-is.

3. **UserPromptSubmit and PreCompact Gemini equivalents**: Gemini has `BeforeModel`
   which fires before each LLM inference (different semantics from UserPromptSubmit).
   No direct Gemini analog exists for `PreCompact`. Confirm these are out of scope for
   vnc-013 or explicitly defer to a future feature.

4. **`source_domain` in `background.rs` read path**: `parse_observation_rows()` reads
   from the DB which has no `provider` column. The proposed fix is to call
   `DomainPackRegistry::resolve_source_domain(event_type)`. But the registry resolves
   by matching event_type against each pack's `event_types` list. Since both
   `"PreToolUse"` (Claude Code) and normalized `"PreToolUse"` (Gemini) will resolve to
   `"claude-code"` via the builtin pack — this is semantically incorrect for Gemini
   records. The DB read path cannot distinguish Claude Code from Gemini records without
   a `source_domain` column. Two options: (a) accept the limitation (Gemini records
   read back as `source_domain = "claude-code"` from DB); (b) add `source_domain`
   column to `observations` table. **Recommendation**: Accept option (a) for now —
   the DB read path is used only for retrospective analysis where provider identity
   is less critical than event semantics. Document as a known limitation.

5. **Gemini `AfterTool` and rework detection interactions**: Gemini fires `AfterTool`
   for ALL tool calls including non-MCP tools (built-in Gemini tools). Does Unimatrix
   need to filter to `mcp_context IS NOT NULL` for `AfterTool` to avoid spurious
   observations from non-MCP tools? The `.gemini/settings.json` matcher regex already
   filters to `mcp_unimatrix_.*` patterns, so only Unimatrix tool calls fire the hook.
   Confirm this understanding is correct.

6. **Gemini `AfterTool` payload: response in which field?** Claude Code's `PostToolUse`
   payload has `tool_response` (object). Gemini's `AfterTool` payload structure for
   MCP tools is not fully specified in ASS-049. If Gemini uses a different field name
   for the tool response, `extract_response_fields()` will silently return
   `(None, None)` — `response_size` and `response_snippet` will be null. This is
   acceptable as a degraded mode but the spec writer should check Gemini CLI source
   for the actual payload shape.

## Tracking

TBD — will be updated with GH Issue link after Session 1.
