# vnc-013: Canonical Event Normalization for Multi-LLM Hook Providers

## Problem Statement

Unimatrix currently hardcodes two assumptions at the hook ingest boundary that make
multi-LLM participation impossible without cascading changes:

1. `source_domain = "claude-code"` is hardcoded in three places:
   - `background.rs:1330` in `fetch_observation_batch()` (embeddings/metrics tick)
   - `listener.rs:1894` in the session-feature query path
   - `services/observation.rs:585` in `parse_observation_rows()` (feeds
     `context_cycle_review`, `load_feature_observations`, session parsing)
   All three hardcode `"claude-code"` regardless of which LLM client fired the hook.
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
9. Add a guard assertion in `listener.rs` `extract_observation_fields()` that prevents
   the internal `"post_tool_use_rework_candidate"` string from reaching
   `observations.hook` if the normalization match arm ever regresses. The existing
   explicit match arm `"PostToolUse" | "post_tool_use_rework_candidate"` already
   normalizes this string — the guard makes that contract enforceable and visible.
   Discovered as an out-of-scope finding in ASS-051.

## Non-Goals

- Live end-to-end Codex CLI testing. Codex bug #16732 (MCP tool calls do not fire
  hooks) is upstream. Codex reference config and code paths ARE in scope — when Codex
  fixes #16732, Unimatrix should work without further changes. Unit tests use synthetic
  Codex events. Live integration testing is out of scope until #16732 is resolved.
  Server-side session attribution via `clientInfo.name` + `Mcp-Session-Id` (recommended
  in ASS-049) is a separate feature.
- Adding `source_domain` as a new column to the `observations` table. The `hook`
  column (stored canonical event name) is the correct filter key; `source_domain` is
  a runtime-derived attribute (from `DomainPackRegistry.resolve_source_domain()`) not
  a persistence concern. A schema migration is explicitly out of scope.
- Continue, Cursor, or Zed hook provider support. Primary targets are Claude Code,
  Gemini CLI, and Codex CLI (reference config + code paths).
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

### Event Taxonomy: Two Distinct Categories

vnc-013 touches two fundamentally different event categories that must NOT be conflated:

**Category 1 — LLM Provider Hook Events** (the normalization target):
These are lifecycle events emitted by the LLM client's hook system — fired when the
client executes a tool call, starts a session, etc. They are provider-specific strings
that differ across clients:
- Claude Code: `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `SessionStart`, `Stop`,
  `TaskCompleted`, `SubagentStart`, `SubagentStop`, `PreCompact`, `UserPromptSubmit`, `Ping`
- Gemini CLI: `BeforeTool`, `AfterTool`, `SessionStart`, `SessionEnd`
- Codex CLI: `PreToolUse`, `PostToolUse`, `SessionStart`, `Stop` (identical names to Claude
  Code; provider identity is established via `--provider codex-cli` flag, not event name
  inference; live MCP hook firing is blocked by Codex bug #16732 but code paths and
  reference config are built)

Shared event names (`PreToolUse`, `PostToolUse`, `SessionStart`, `Stop`) appear in both
Claude Code and Codex CLI. The `--provider` flag on the `unimatrix hook` subcommand is
the canonical disambiguation mechanism. When the flag is absent, `"claude-code"` is the
backward-compatible default. Gemini events have unique names (`BeforeTool`, `AfterTool`,
`SessionEnd`) so inference is unambiguous for Gemini.

The normalization layer in `hook.rs` translates Category 1 events to canonical names.

**Category 2 — Unimatrix MCP Events** (already provider-neutral, NOT normalized):
These are synthetic events produced when any agent calls the `context_cycle` MCP tool.
They are MCP protocol events — not LLM client lifecycle events. They carry the same
meaning and structure regardless of which provider submits them, because `context_cycle`
is an MCP tool call with a defined schema, not a provider-specific hook:
- `cycle_start` — produced when `context_cycle(type: "start")` is called
- `cycle_stop` — produced when `context_cycle(type: "stop")` is called
- `cycle_phase_end` — produced when `context_cycle(type: "phase-end")` is called

When Gemini CLI fires `BeforeTool` for a `context_cycle` tool call, that is a Category 1
event (hook) that intercepts a Category 2 intent (MCP tool call). The normalization layer
translates `BeforeTool` → `PreToolUse` (Category 1), then `build_cycle_event_or_fallthrough()`
intercepts the canonical name and produces the Category 2 synthetic event. The Category 2
event produced is identical regardless of provider — `cycle_start` from Gemini looks the
same as `cycle_start` from Claude Code.

The normalization layer's scope is **Category 1 only**. Category 2 events are already
provider-neutral by construction and require no normalization.

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

**background.rs `fetch_observation_batch()` source_domain hardcode (line 1330):**
- `let source_domain = "claude-code".to_string();` — must become dynamic.

**services/observation.rs `parse_observation_rows()` source_domain hardcode (line 585):**
- `let source_domain: String = "claude-code".to_string();` — must become dynamic.
- This function already receives `_registry: &DomainPackRegistry` (currently unused,
  prefixed with `_`). The fix removes the underscore and calls
  `registry.resolve_source_domain(&event_type)` — no new parameter needed.
- Called from three paths: `load_feature_observations()` (used by `context_cycle_review`),
  `load_session_observations()`, and `load_windowed_observations()`. All three will
  benefit from the fix once `parse_observation_rows()` is corrected.

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
- The builtin `"claude-code"` pack only lists 4 event types:
  `["PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop"]`. Events not in
  this list (`"Stop"`, `"SessionStart"`, `"cycle_start"`, `"cycle_stop"`, etc.)
  return `"unknown"` from `resolve_source_domain()`.
- Used only for non-hook-path records (comment in source, `domain/mod.rs:176`).
- After this feature, `listener.rs:1894` derives `source_domain` from
  `ImplantEvent.provider` (live write path — correctly labels provider).
- DB-read-path fixes (`background.rs:1330`, `services/observation.rs:585`) cannot
  use `resolve_source_domain()` directly without a fallback — doing so would change
  `source_domain` from `"claude-code"` to `"unknown"` for non-listed event types.
  The spec writer must choose between a registry-with-`"claude-code"`-fallback pattern
  or a named constant `DEFAULT_HOOK_SOURCE_DOMAIN`.
- The registry itself requires no changes.

### Category 1 Provider Event Mapping (From ASS-049 FINDINGS-HOOKS.md)

These are LLM provider hook events (Category 1) only. Category 2 Unimatrix MCP events
(`cycle_start`, `cycle_stop`, `cycle_phase_end`) are not listed here — they are
provider-neutral by construction and are not subject to normalization.

| Gemini CLI Event | Claude Code Event | Codex CLI Event | Canonical Name |
|---|---|---|---|
| `BeforeTool` | `PreToolUse` | `PreToolUse` | `PreToolUse` |
| `AfterTool` | `PostToolUse` | `PostToolUse` | `PostToolUse` |
| `SessionStart` | `SessionStart` | `SessionStart` | `SessionStart` |
| `SessionEnd` | `Stop` | `Stop` | `Stop` |
| (none) | `SubagentStart` | (none) | `SubagentStart` |
| (none) | `SubagentStop` | (none) | `SubagentStop` |
| (none) | `PreCompact` | (none) | `PreCompact` |
| (none) | `UserPromptSubmit` | (none) | `UserPromptSubmit` |
| (none) | `PostToolUseFailure` | (none) | `PostToolUseFailure` |

Codex CLI Category 1 events share exact names with Claude Code for `PreToolUse`,
`PostToolUse`, `SessionStart`, and `Stop`. Provider identity for Codex events is
established via the `--provider codex-cli` flag on the `unimatrix hook` subcommand,
not by inference from the event name alone.

Note: Unimatrix agents running under Gemini or Codex submit `context_cycle` MCP tool
calls identically to Claude Code agents — these produce Category 2 events that are
already canonical. vnc-013 does not normalize Category 2 events.

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

### Layer 1: Wire Protocol Extension (`crates/unimatrix-engine/src/wire.rs`)

Add `provider: Option<String>` to both `HookInput` and `ImplantEvent`. For `mcp_context`,
note that `HookInput` already has `pub extra: serde_json::Value` with `#[serde(flatten)]`
which captures all unknown fields including Gemini's `mcp_context`. A named `mcp_context`
field is not strictly required — `hook.rs` can read it via `input.extra.get("mcp_context")`.
However, adding it as an explicit named field with `#[serde(default)]` improves
readability and avoids stringly-typed access. Either approach is acceptable; the spec
writer should decide based on the team's convention for `HookInput` extension:

```rust
// HookInput — what the hook binary reads from stdin
pub struct HookInput {
    // ... existing fields ...
    /// Provider hint injected by the hook binary from the event name.
    /// "claude-code" | "gemini-cli" | "codex-cli"
    #[serde(default)]
    pub provider: Option<String>,
    /// Gemini CLI mcp_context field (BeforeTool/AfterTool only).
    /// Also captured by `extra` flatten — explicit field for type clarity.
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

The `unimatrix hook` subcommand accepts an optional `--provider <name>` argument. When
provided, the value is stored directly into `HookInput.provider` before normalization,
threading provider identity explicitly without inference. Example invocations:

```
unimatrix hook PreToolUse --provider codex-cli
unimatrix hook PreToolUse --provider claude-code
unimatrix hook BeforeTool --provider gemini-cli  # provider also inferable from event name
```

Add a `normalize_event_name(event: &str, provider_hint: Option<&str>) -> (&str, &str)`
function that returns `(canonical_event_name, provider)`:

- When `provider_hint` is `Some(p)`: use `p` as the provider and map the event name to
  its canonical form. Shared names (`PreToolUse`, `PostToolUse`, `SessionStart`, `Stop`)
  map to themselves canonically regardless of provider.
- When `provider_hint` is `None`: infer provider from event name. Gemini events have
  unique names (`BeforeTool`, `AfterTool`, `SessionEnd`) so inference is unambiguous.
  Shared names without a hint default to `"claude-code"` (backward-compatible with
  existing deployments) — this is documented as the fallback, not the expected path for
  Codex.

```
// provider_hint = None (inference mode)
"BeforeTool"  → ("PreToolUse",  "gemini-cli")
"AfterTool"   → ("PostToolUse", "gemini-cli")
"SessionEnd"  → ("Stop",        "gemini-cli")
"SessionStart"→ ("SessionStart","claude-code")  // fallback default; use --provider for Codex
"PreToolUse"  → ("PreToolUse",  "claude-code")  // fallback default; use --provider for Codex
"PostToolUse" → ("PostToolUse", "claude-code")  // fallback default; use --provider for Codex
"Stop"        → ("Stop",        "claude-code")  // fallback default; use --provider for Codex
// ... all remaining Claude Code events map to themselves with "claude-code"
_ (unknown)   → (event,         "unknown")

// provider_hint = Some("codex-cli") (explicit flag)
"PreToolUse"  → ("PreToolUse",  "codex-cli")
"PostToolUse" → ("PostToolUse", "codex-cli")
"SessionStart"→ ("SessionStart","codex-cli")
"Stop"        → ("Stop",        "codex-cli")
```

`run()` reads `--provider` from CLI args, populates `HookInput.provider`, then calls
`normalize_event_name(event, input.provider.as_deref())` before `build_request()`. The
canonical name is passed to `build_request()`; the provider is propagated into each
`ImplantEvent.provider`.

Gemini `BeforeTool` Arm: After normalization to `"PreToolUse"`, the existing
`build_cycle_event_or_fallthrough()` handles `context_cycle` interception. However,
Gemini's payload structure differs from Claude Code's:
- `tool_name` is in `mcp_context.tool_name` (bare name, e.g., `"context_cycle"`)
  instead of `input.extra["tool_name"]` (prefixed, e.g., `"mcp__unimatrix__context_cycle"`).
- `tool_input` is at the top level of the payload (same as Claude Code — no adaptation
  needed for this field per ASS-049 FINDINGS-HOOKS.md).
`build_cycle_event_or_fallthrough()` reads `input.extra["tool_name"]` at its first
step (line 563-566 of hook.rs). For Gemini, this field is absent from `extra` — it is
in `extra["mcp_context"]["tool_name"]` instead. The adapter step must read from
`mcp_context.tool_name` (if present) and synthesize a compatible `tool_name` value,
or the function signature must be extended to accept an optional override.
The `contains("context_cycle")` + `contains("unimatrix")` OR `== "context_cycle"`
matching logic already handles bare names (`"context_cycle"` matches the second
condition). No change to the matching logic is needed — only the `tool_name` read site.

### Layer 3: source_domain Derivation — Three Sites

**`listener.rs:1894`** — reads `ImplantEvent` directly, so `provider` is available:
```rust
source_domain: event.provider.clone().unwrap_or_else(|| "claude-code".to_string()),
```

**`background.rs:1330`** and **`services/observation.rs:585`** — DB-read paths.
These cannot use `resolve_source_domain()` as-is: the builtin claude-code pack only
lists `["PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop"]` in its
`event_types`. Events like `"Stop"`, `"SessionStart"`, `"cycle_start"`, `"cycle_stop"`,
`"UserPromptSubmit"` are not in the list — `resolve_source_domain()` returns `"unknown"`
for them. Using this method on DB-read paths would change existing behavior: all
non-listed events currently get `"claude-code"` but would get `"unknown"` after the fix,
breaking downstream consumers of `source_domain`.

Two valid approaches:
- **Approach A** (recommended): Keep `"claude-code"` as the fallback for hook-path
  DB reads. Use `resolve_source_domain()` only if it returns non-`"unknown"`, otherwise
  fall back to `"claude-code"`. This preserves the current behavior for all event types
  the registry does not claim.
- **Approach B**: Accept `"unknown"` for non-listed event types in DB reads. Simple, but
  changes existing behavior and may break any consumer that checks `source_domain ==
  "claude-code"` on session or cycle events.

Approach A is recommended — it is the least disruptive change and preserves the
invariant that hook-path records are labeled with a meaningful domain. The spec writer
must choose and implement one approach.

**Note on semantics**: Both DB-read-path sites cannot distinguish Claude Code from
Gemini records after normalization (Gemini `"BeforeTool"` is stored as `"PreToolUse"`).
This is the known limitation from resolved Open Question 4. Only `listener.rs:1894`
(write path) correctly labels `"gemini-cli"` events.

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

- AC-01: `normalize_event_name("BeforeTool", None)` returns `("PreToolUse", "gemini-cli")`.
  `normalize_event_name("AfterTool", None)` returns `("PostToolUse", "gemini-cli")`.
  `normalize_event_name("SessionEnd", None)` returns `("Stop", "gemini-cli")`.
  All Claude Code event names called with `None` hint map to themselves with provider
  `"claude-code"`. Unknown event names map to themselves with provider `"unknown"`.

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

- AC-07: All three `source_domain` hardcodes are replaced:
  (a) `listener.rs:1894`: uses `event.provider.clone().unwrap_or("claude-code")` —
      correctly labels live events with their originating provider.
  (b) `background.rs:1330` in `fetch_observation_batch()`: replaces the hardcode with
      a derivation that preserves `"claude-code"` for all hook-path events. Approach A
      (registry-with-fallback) or a documented constant `DEFAULT_HOOK_DOMAIN = "claude-code"`
      — spec writer decides. The hardcode string literal must not remain.
  (c) `services/observation.rs:585` in `parse_observation_rows()`: same approach as
      (b). The `_registry` parameter is already present; use it or document why not.
  The existing tests `test_parse_rows_hook_path_always_claude_code` and
  `test_parse_rows_unknown_event_type_passthrough` must be reviewed: after the fix,
  the second test's expectation of `source_domain == "claude-code"` for an unknown
  event type is no longer guaranteed if registry-based derivation is used without
  a fallback. Spec writer must decide the contract and update the test.

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

- AC-16: `extract_observation_fields()` contains a guard (debug assertion or exhaustive
  match arm) that fires if `"post_tool_use_rework_candidate"` would reach the `hook`
  column as a raw string. The normalization to `"PostToolUse"` is structurally
  enforced, not just incidentally present.

- AC-17: `unimatrix hook PreToolUse --provider codex-cli` processes the event with
  `provider: "codex-cli"`. `normalize_event_name("PreToolUse", Some("codex-cli"))`
  returns `("PreToolUse", "codex-cli")`.

- AC-18: `unimatrix hook PreToolUse` (no `--provider` flag) processes the event with
  `provider: "claude-code"` as the backward-compatible default.
  `normalize_event_name("PreToolUse", None)` returns `("PreToolUse", "claude-code")`.

- AC-19: The Codex reference config (`~/.codex/hooks.json` or `<repo>/.codex/hooks.json`)
  is written. Config location and schema are identical to Claude Code (confirmed by
  ASS-049 FINDINGS-HOOKS.md, which read `codex-rs/core/src/hook_runtime.rs` directly).
  Each supported event invokes `unimatrix hook <event> --provider codex-cli`. The
  `--provider codex-cli` flag is **mandatory** — without it, Codex events share event
  names with Claude Code (`PreToolUse`, `PostToolUse`, `SessionStart`, `Stop`) and fall
  through to the `"claude-code"` default, producing incorrect `source_domain` attribution
  on the write path. Normalization correctness is unaffected; only provider labeling is
  wrong without the flag. The config carries a caveat that live MCP hook support is
  blocked by Codex #16732 and the config is non-functional until the upstream bug is
  resolved. Unit tests use synthetic Codex events.

- AC-20: `normalize_event_name("SessionStart", Some("claude-code"))` returns
  `("SessionStart", "claude-code")`. `normalize_event_name("SessionStart", Some("codex-cli"))`
  returns `("SessionStart", "codex-cli")`. The provider hint takes precedence over
  inference for all shared event names.

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

8. **Codex support is built but not live-tested.** Reference config (`.codex/hooks.json`
   or equivalent) ships as a real deliverable. Code paths handle Codex events with
   `--provider codex-cli`. Live end-to-end testing is blocked by Codex #16732. Unit
   tests use synthetic Codex events. The reference config must carry a caveat that live
   MCP hook support is pending upstream resolution of #16732.

9. **`services/observation.rs` tests must be reviewed and updated.**
   `test_parse_rows_hook_path_always_claude_code` asserts `source_domain == "claude-code"`
   for a `"PreToolUse"` event — this remains correct under either approach (registry
   resolves `"PreToolUse"` to `"claude-code"`, fallback also returns `"claude-code"`).
   `test_parse_rows_unknown_event_type_passthrough` asserts `source_domain == "claude-code"`
   for an `"UnknownEventType"` event — if registry-based derivation without fallback is
   used, this would return `"unknown"` (registry returns `"unknown"` for unrecognized
   types). The spec writer must decide the contract for unknown event types and update
   this test accordingly. The test comment `"FR-03.3"` must be updated to remove the
   "always claude-code" framing.

## Open Questions

1. ~~**Rework detection gate**~~ **RESOLVED**: Provider-based gate (`provider != "claude-code"`).
   The tool-name gate works today only by accident — Gemini doesn't happen to use `"Bash"`
   or `"Edit"`. That's not a contract. The `provider` field is threaded through the entire
   normalization architecture precisely to enable this discrimination. Using it here is
   consistent with the design; not using it would be an unexplained inconsistency.

2. ~~**Canonical name for `SessionEnd` → `Stop`**~~ **RESOLVED (ASS-051)**: Canonical
   name is `"Stop"`. `SessionEnd` (Gemini) normalizes to `"Stop"` at ingest. No new
   canonical name introduced. Existing DB content and `extract_observation_fields()`
   wildcard arm are correct as-is.

3. ~~**UserPromptSubmit and PreCompact Gemini equivalents**~~ **RESOLVED**: Gemini's
   `BeforeModel` fires before each LLM inference — different semantics from
   `UserPromptSubmit` (which is a user input event). No direct Gemini analog exists
   for `PreCompact`. Both are confirmed out of scope for vnc-013 per the Non-Goals
   section. The normalization table need not include entries for these — Gemini simply
   has no hooks that map to them, and the wildcard arm handles any unmapped event
   gracefully as `RecordEvent`.

4. ~~**`source_domain` in DB read path**~~ **RESOLVED**: Accept known limitation.
   All three hardcodes are replaced (see AC-07 and Layer 3 above). For the DB-read-path
   sites (`background.rs:1330`, `services/observation.rs:585`), after normalization
   Gemini records are stored as canonical Claude Code names, so
   `resolve_source_domain("PreToolUse")` still returns `"claude-code"`. This is a
   semantic imprecision on the read path only; `listener.rs:1894` (the write path)
   will correctly record `"gemini-cli"` for live events. The DB-read-path sites
   (`context_cycle_review`, metrics tick) use provider identity only for logging and
   retrospective analysis — the behavioral correctness of cycle interception and
   observation storage is unaffected. Document as a known limitation in the
   implementation brief.

5. ~~**Gemini `AfterTool` and rework detection interactions**~~ **RESOLVED**: The
   `.gemini/settings.json` matcher regex `mcp_unimatrix_.*` restricts `AfterTool`
   hooks to Unimatrix MCP tool calls only. Gemini built-in tool calls (file operations,
   shell, etc.) do not match the regex and do not fire the hook. No additional
   `mcp_context IS NOT NULL` guard is needed — the matcher is the filter.

6. ~~**Gemini `AfterTool` payload: response field name**~~ **RESOLVED (non-blocking)**:
   Degrade gracefully. If the field name differs from Claude Code's `tool_response`,
   `response_size` and `response_snippet` will be null — the observation is still recorded.
   The implementer should make one attempt to confirm from Gemini CLI source or a live
   capture during implementation. If confirmed, use it; if not, the degraded mode is
   acceptable and documented. Does not block design sign-off.

7. ~~**Codex hook config format (OQ-A)**~~ **RESOLVED (ASS-049)**: Config location is
   `~/.codex/hooks.json` or `<repo>/.codex/hooks.json`. ASS-049 FINDINGS-HOOKS.md
   explicitly states hook event names and JSON format are identical to Claude Code.
   Schema is the same structure as `.claude/settings.json`. No open question remains.

8. ~~**Codex CLI arg passthrough (OQ-B)**~~ **RESOLVED (ASS-049)**: ASS-049
   FINDINGS-HOOKS.md read `codex-rs/core/src/hook_runtime.rs` directly and confirmed
   Codex executes hook commands the same way Claude Code does — command strings are
   shelled out verbatim. `unimatrix hook PreToolUse --provider codex-cli` in a Codex
   config works identically to Claude Code. The env var fallback is unnecessary — the
   flag approach is confirmed viable.

## Tracking

https://github.com/dug-21/unimatrix/issues/567
