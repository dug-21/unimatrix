# vnc-013: Canonical Event Normalization for Multi-LLM Hook Providers
## SPECIFICATION.md

---

## Objective

Unimatrix's hook ingest boundary currently hardcodes `source_domain = "claude-code"` in three production files and only handles Claude Code event names in `hook.rs build_request()`. This makes multi-LLM participation impossible: Gemini CLI fires `BeforeTool` where Claude Code fires `PreToolUse`, causing the `build_cycle_event_or_fallthrough()` interception — and therefore all `cycle_events` writes — to silently fail for Gemini-sourced sessions.

vnc-013 introduces a normalization layer at the ingest boundary that translates provider-specific event names to canonical Unimatrix names (Claude Code event names as canonical, per ASS-051), threads a `provider` field through `HookInput` and `ImplantEvent` so `source_domain` can be derived dynamically, and extends `build_request()` with Gemini CLI dispatch arms. Codex CLI code paths and reference configuration are built for forward-compatibility; live end-to-end testing is blocked by Codex upstream bug #16732 and is explicitly out of scope.

---

## Functional Requirements

### FR-01: Normalization Function

**FR-01.1** A pure synchronous function `normalize_event_name(event: &str, provider_hint: Option<&str>) -> (&str, &str)` shall be implemented in `hook.rs`. The return type is `(canonical_event_name, provider)`.

**FR-01.2** When `provider_hint` is `Some(p)`, the returned provider shall equal `p`. The canonical event name shall be the standard Unimatrix canonical name for that event regardless of provider.

**FR-01.3** When `provider_hint` is `None`, provider shall be inferred from the event name using the mapping table in the Domain Models section. Gemini-unique names (`BeforeTool`, `AfterTool`, `SessionEnd`) infer `"gemini-cli"` unambiguously. Shared names (`PreToolUse`, `PostToolUse`, `SessionStart`, `Stop`, and all other Claude Code events) default to `"claude-code"` as the backward-compatible fallback.

**FR-01.4** Unknown event names (not in the mapping table and not inferrable) shall map to `(event, "unknown")` — the event name passes through unchanged and provider is `"unknown"`.

**FR-01.5** The function shall contain no I/O, no `async`, and no allocations beyond static string returns. It operates on `&str` inputs and returns `&'static str` pairs or borrows from input.

**FR-01.6** `normalize_event_name` shall be called in `build_request()` before any `HookRequest` variant is constructed. The canonical name is used throughout `build_request()`; the provider is propagated into each `ImplantEvent.provider`.

### FR-02: Wire Protocol Extension

**FR-02.1** `HookInput` in `wire.rs` shall gain two new fields, both `#[serde(default)]`:
- `pub provider: Option<String>` — the originating provider string, populated before normalization
- `pub mcp_context: Option<serde_json::Value>` — captures Gemini's structured MCP context from `BeforeTool`/`AfterTool` payloads

**FR-02.2** `ImplantEvent` in `wire.rs` shall gain one new field:
- `#[serde(default, skip_serializing_if = "Option::is_none")] pub provider: Option<String>`

**FR-02.3** Existing Claude Code hook JSON that omits these fields shall deserialize without error. Missing fields shall produce `None` in all cases.

**FR-02.4** The `ImplantEvent.provider` field shall be non-`None` for all events processed through the normalization layer.

### FR-03: CLI Provider Flag

**FR-03.1** The `unimatrix hook` subcommand shall accept an optional `--provider <name>` argument.

**FR-03.2** When `--provider` is supplied, its value shall be stored into `HookInput.provider` before normalization is called. The provider_hint passed to `normalize_event_name` shall be `input.provider.as_deref()`.

**FR-03.3** When `--provider` is absent, `HookInput.provider` shall be `None` and inference rules in `normalize_event_name` apply.

**FR-03.4** The `unimatrix hook` binary shall accept all Gemini CLI event names (`BeforeTool`, `AfterTool`, `SessionStart`, `SessionEnd`) without error. Exit code shall be 0 regardless of whether the server is running.

**FR-03.5** Normalization failures (unrecognized provider value, malformed `mcp_context` structure) shall log via `eprintln!` and degrade gracefully to `generic_record_event`. They shall not cause `run()` to return an error code.

### FR-04: Gemini CLI Dispatch Arms

**FR-04.1** `build_request()` shall add match arms for `"BeforeTool"`, `"AfterTool"`, and `"SessionEnd"` that are reached only if normalization has not already translated them. After normalization these strings will never reach the match in normal flow; the arms exist as a defense-in-depth guard.

**FR-04.2** Gemini `BeforeTool` (normalized to `"PreToolUse"`) shall route through `build_cycle_event_or_fallthrough()` using the same dispatch path as Claude Code `PreToolUse`. No duplication of `build_cycle_event_or_fallthrough()`.

**FR-04.3** Before calling `build_cycle_event_or_fallthrough()` for a Gemini `BeforeTool` event, the adapter step shall:
- Read `mcp_context.tool_name` from `HookInput.mcp_context` if present
- Promote it to the top-level `tool_name` position expected by `build_cycle_event_or_fallthrough()` (which reads `input.extra["tool_name"]`)
- The bare tool name `"context_cycle"` is sufficient: the existing `contains("context_cycle")` match condition handles it without the `"mcp__unimatrix__"` prefix

**FR-04.4** Gemini `AfterTool` (normalized to `"PostToolUse"`) shall produce `HookRequest::RecordEvent { event_type: "PostToolUse", provider: "gemini-cli" }`. Rework candidate logic shall NOT be entered.

**FR-04.5** Gemini `SessionEnd` (normalized to `"Stop"`) shall route to `HookRequest::SessionClose` using the same path as Claude Code `Stop`.

**FR-04.6** Gemini `SessionStart` (shares the name with Claude Code; no normalization needed) shall route to `HookRequest::SessionRegister` exactly as before, with `provider` set from inference or flag.

**FR-04.7** `extract_event_topic_signal()` shall handle normalized canonical event names correctly. For Gemini `BeforeTool` (arrives normalized as `"PreToolUse"`), `tool_input` at the top-level payload is the extraction source per ASS-049 FINDINGS-HOOKS.md confirmation. No special Gemini arm is required in `extract_event_topic_signal()` if `tool_input` is already at top-level.

### FR-05: Rework Detection Gate

**FR-05.1** The rework candidate path — `is_rework_eligible_tool()`, `is_bash_failure()`, `extract_file_path()` — shall be gated on `provider == "claude-code"`. Events where provider is `"gemini-cli"`, `"codex-cli"`, or `"unknown"` shall not enter this path.

**FR-05.2** The gate shall use the `provider` field threaded from normalization, not a tool-name inference heuristic. Relying on Gemini's MCP-only tool names to accidentally avoid the rework path is not a contract.

### FR-06: source_domain Derivation — Three Sites

All three existing `source_domain` hardcodes shall be replaced. The string literal `"claude-code"` shall not appear as a hardcoded `source_domain` assignment in any of these three locations after this feature.

**FR-06.1** `listener.rs:1894` (write path, `ImplantEvent` available):
```rust
source_domain: event.provider.clone().unwrap_or_else(|| "claude-code".to_string()),
```
This is the only site that correctly labels live `"gemini-cli"` events.

**FR-06.2** `background.rs:1330` in `fetch_observation_batch()` (DB-read path):
Replace the hardcode using Approach A: call `registry.resolve_source_domain(&event_type)` and fall back to `"claude-code"` if the registry returns `"unknown"`. The fallback preserves existing behavior for all event types not in the builtin claude-code pack (`Stop`, `SessionStart`, `cycle_start`, `cycle_stop`, `UserPromptSubmit`).

**FR-06.3** `services/observation.rs:585` in `parse_observation_rows()` (DB-read path):
Apply the same Approach A derivation. The `_registry` parameter is already present on this function; the underscore prefix shall be removed and the registry shall be used. The fallback to `"claude-code"` applies identically.

**FR-06.4** Approach A is the required contract: `resolve_source_domain(event_type)` → if result is `"unknown"` → use `"claude-code"`. This is the DB-read-path invariant. The known limitation — that Gemini records stored as canonical `"PreToolUse"` will return `"claude-code"` on the DB read path rather than `"gemini-cli"` — is accepted and shall be documented in the implementation brief.

**FR-06.5** The `DomainPackRegistry` itself requires no changes. The builtin claude-code pack's 4-event `event_types` list is not modified.

### FR-07: Test Updates for Existing Test Suite

**FR-07.1** `test_parse_rows_hook_path_always_claude_code` in `services/observation.rs`: this test asserts `source_domain == "claude-code"` for a `"PreToolUse"` event. Under Approach A, `resolve_source_domain("PreToolUse")` returns `"claude-code"` (it is in the builtin pack). The test remains valid and shall not be changed.

**FR-07.2** `test_parse_rows_unknown_event_type_passthrough` in `services/observation.rs`: this test asserts `source_domain == "claude-code"` for an `"UnknownEventType"` event. Under Approach A, `resolve_source_domain("UnknownEventType")` returns `"unknown"` and the fallback restores `"claude-code"`. The assertion remains correct under Approach A. The test comment referencing "always claude-code" shall be updated to state the Approach A contract: "registry returns unknown for unregistered types, fallback to claude-code preserves hook-path invariant." The `"FR-03.3"` reference in the comment shall be updated to reference the new FR-06 numbering.

**FR-07.3** All existing unit and integration tests shall pass without behavioral change. The normalization layer is additive for all Claude Code events.

### FR-08: Observation Guard

**FR-08.1** `extract_observation_fields()` in `listener.rs` shall contain a debug assertion or exhaustive guard that prevents the internal string `"post_tool_use_rework_candidate"` from reaching the `hook` column in the `observations` table. The normalization to `"PostToolUse"` is structurally enforced, not incidental. The guard makes the contract visible and enforceable.

**FR-08.2** The guard applies to the rework candidate string only. The `PostToolUseFailure` arm in `extract_observation_fields()` is untouched. Per ADR-003 col-027 (entry #3475), `PostToolUseFailure` is intentionally NOT normalized to `PostToolUse` — the signal distinction is preserved.

### FR-09: Reference Configurations

**FR-09.1** A `.gemini/settings.json` reference configuration shall be written covering the four Gemini CLI hook events Unimatrix requires: `BeforeTool`, `AfterTool`, `SessionStart`, `SessionEnd`. The matcher regex `mcp_unimatrix_.*` shall be used for tool-scoped events. The format shall be valid for Gemini CLI v0.31+.

**FR-09.2** A `.codex/hooks.json` (or `~/.codex/hooks.json` — same schema as `.claude/settings.json` per ASS-049 FINDINGS-HOOKS.md) reference configuration shall be written covering `PreToolUse`, `PostToolUse`, `SessionStart`, `Stop`. Each event invocation shall include the `--provider codex-cli` flag. The config shall carry an inline comment or accompanying README note stating that live MCP hook support is blocked by Codex upstream bug #16732 and the configuration is non-functional until that is resolved. Unit tests use synthetic Codex events.

**FR-09.3** The `--provider codex-cli` flag in the Codex reference config is mandatory. Without it, Codex events share names with Claude Code and fall through to the `"claude-code"` default, producing incorrect `source_domain` attribution on the write path. This must be documented in the config.

### FR-10: Gemini cycle_start/cycle_stop Interception

**FR-10.1** When Gemini fires `BeforeTool` for a `context_cycle` tool call with `type="start"`, the normalized `build_request()` shall produce `HookRequest::RecordEvent { event_type: "cycle_start" }` with the `feature_cycle` field populated. `listener.rs` shall write to `cycle_events`.

**FR-10.2** The `context_cycle_review` tool shall return correct results for a Gemini-sourced feature cycle. This is verified by AC-09.

---

## Non-Functional Requirements

**NFR-01: Synchronous hook.rs** — All normalization logic in `hook.rs` shall be synchronous. No tokio runtime, no async functions, no I/O in `normalize_event_name`. This is an existing architectural constraint (ADR-002 from hook.rs header).

**NFR-02: Zero regression on existing events** — The normalization layer is purely additive for Claude Code events. Every Claude Code event name maps to itself with provider `"claude-code"`. No existing behavior changes.

**NFR-03: No DB schema changes** — The `observations` table `hook TEXT` column stores canonical event names only. `source_domain` is not persisted. No schema migration, no schema version bump.

**NFR-04: Graceful degradation** — Unrecognized event names pass through unchanged with provider `"unknown"`. Malformed `mcp_context` structures (missing `tool_name`, wrong type) do not cause panics or error returns. Log and fall through to `generic_record_event`.

**NFR-05: Backward-compatible deserialization** — Existing Claude Code hook JSON, which does not contain `provider` or `mcp_context` fields, must deserialize `HookInput` without error. `#[serde(default)]` is required on both new fields.

**NFR-06: Hook exit is always 0** — Normalization failures, unrecognized providers, and missing server connections all produce exit code 0 from `run()`. This matches the existing hook binary contract.

**NFR-07: No new crate dependencies** — All normalization logic uses existing Rust standard library and serde_json. No new Cargo dependencies.

**NFR-08: `build_cycle_event_or_fallthrough()` remains the single implementation** — The Gemini adapter normalizes the payload into the shape expected by the existing function and calls it. No duplication.

---

## Acceptance Criteria

All 20 ACs from SCOPE.md are incorporated and expanded here with verification method.

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| AC-01 | `normalize_event_name("BeforeTool", None)` returns `("PreToolUse", "gemini-cli")`. `normalize_event_name("AfterTool", None)` returns `("PostToolUse", "gemini-cli")`. `normalize_event_name("SessionEnd", None)` returns `("Stop", "gemini-cli")`. All Claude Code event names with `None` hint map to themselves with `"claude-code"`. Unknown names map to themselves with `"unknown"`. | Unit tests in `hook.rs` covering each mapping table row and the unknown fallback. |
| AC-02 | When Gemini fires `BeforeTool` for `context_cycle` with `type="start"`, `build_request()` produces `HookRequest::RecordEvent { event_type: "cycle_start" }`. Payload contains `feature_cycle`. `cycle_events` is written by `listener.rs`. | Integration test: inject synthetic Gemini `BeforeTool`+`context_cycle` event; assert `cycle_events` row exists with `event_type = "cycle_start"`. |
| AC-03 | When Gemini fires `BeforeTool` for any Unimatrix tool other than `context_cycle`, `build_request()` produces `HookRequest::RecordEvent` with `event_type: "PreToolUse"` and `provider: "gemini-cli"`. | Unit test with synthetic `BeforeTool` payload where `mcp_context.tool_name` is not `context_cycle`. |
| AC-04 | When Gemini fires `AfterTool`, `build_request()` produces `HookRequest::RecordEvent` with `event_type: "PostToolUse"` and `provider: "gemini-cli"`. Rework candidate logic does NOT trigger (gated by `provider != "claude-code"`). | Unit test: verify `HookRequest` variant and that rework path is not entered; assert `is_rework_eligible_tool` is never called for provider `"gemini-cli"`. |
| AC-05 | `ImplantEvent.provider` is non-`None` for all events processed through the normalization layer. | Unit test: for each canonical event name, construct an `ImplantEvent` via normalization and assert `provider.is_some()`. |
| AC-06 | `source_domain` on the written `ObservationRecord` is `"gemini-cli"` for Gemini-originated events and `"claude-code"` for Claude Code-originated events (write path: `listener.rs:1894`). | Integration test: inject events with `provider: "gemini-cli"` and `provider: "claude-code"`; assert resulting `ObservationRecord.source_domain` values. |
| AC-07 | All three `source_domain` hardcodes are replaced: (a) `listener.rs:1894` uses `event.provider.clone().unwrap_or("claude-code")`; (b) `background.rs:1330` uses Approach A registry-with-fallback; (c) `services/observation.rs:585` uses the same Approach A pattern via the existing `_registry` parameter (underscore removed). The string literal `"claude-code"` does not appear as a hardcoded assignment in any of these three locations. | Code review: grep for literal `"claude-code"` in the three files verifies removal. Unit tests for each path confirm correct derivation. |
| AC-08 | All existing unit and integration tests pass without modification. Normalization is additive — no existing Claude Code behavior changes. | `cargo test --workspace` passes. |
| AC-09 | `context_cycle_review` returns correct results for a Gemini-sourced feature cycle: a test inserts `cycle_start`/`cycle_stop` events via the Gemini `BeforeTool` code path and verifies `context_cycle_review` finds and processes them. This is a mandatory (not stretch) AC. | Integration test in `services/` or `tools.rs` using synthetic Gemini events through the full hook-to-listener path. |
| AC-10 | `.gemini/settings.json` reference configuration is written and matches the format required by Gemini CLI v0.31+. The matcher regex `mcp_unimatrix_.*` covers all 12 Unimatrix tools. | File exists at `.gemini/settings.json`; format review against Gemini CLI v0.31+ documentation. |
| AC-11 | `extract_event_topic_signal()` handles canonical event names correctly for Gemini-sourced events. `"PreToolUse"` (normalized from `"BeforeTool"`) extracts topic signal from `tool_input` at the top-level payload position. | Unit test: synthetic Gemini `BeforeTool` payload with `tool_input` at top level; assert topic signal is extracted (non-empty result). |
| AC-12 | Rework candidate detection (`is_rework_eligible_tool()`, `is_bash_failure()`) is gated to `provider == "claude-code"`. Gemini `AfterTool` events do not enter the rework tracking path. The gate uses the `provider` field, not tool-name inference. | Unit test: assert rework path is never entered when `provider` is `"gemini-cli"`. Code review: gate expression references `provider` field explicitly. |
| AC-13 | `DomainPackRegistry` builtin claude-code pack `event_types` list is unchanged. The registry requires no changes. Gemini event names never reach the registry (normalized at ingest). | Code review: no diff in `domain/mod.rs` `builtin_claude_code_pack()`. |
| AC-14 | `HookInput.mcp_context` is deserialized from Gemini `BeforeTool`/`AfterTool` payloads. `tool_name` is extracted from it for the `context_cycle` interception test in `build_cycle_event_or_fallthrough()`. The bare name `"context_cycle"` satisfies the `contains("context_cycle")` match. | Unit test: deserialize a synthetic Gemini `BeforeTool` JSON with `mcp_context.tool_name = "context_cycle"`; assert `build_request()` produces `cycle_start` event. |
| AC-15 | The `unimatrix hook` binary accepts Gemini event names without error. `run("BeforeTool", ...)` completes normally (exit 0) regardless of whether the server is running. | Integration test or manual verification: run `unimatrix hook BeforeTool` with no server; verify exit code 0. |
| AC-16 | `extract_observation_fields()` contains a guard (debug assertion or exhaustive match arm) that fires if `"post_tool_use_rework_candidate"` would reach the `hook` column as a raw string. The guard is scoped to rework-candidate strings only. The `PostToolUseFailure` arm is untouched. | Code review: guard present in `listener.rs extract_observation_fields()`; `PostToolUseFailure` arm unchanged. Unit test: attempt to produce a `"post_tool_use_rework_candidate"` observation and assert the guard fires in test mode. |
| AC-17 | `unimatrix hook PreToolUse --provider codex-cli` processes the event with `provider: "codex-cli"`. `normalize_event_name("PreToolUse", Some("codex-cli"))` returns `("PreToolUse", "codex-cli")`. | Unit test for `normalize_event_name` with `Some("codex-cli")`; integration test for CLI flag passthrough. |
| AC-18 | `unimatrix hook PreToolUse` (no `--provider` flag) processes the event with `provider: "claude-code"` as the backward-compatible default. `normalize_event_name("PreToolUse", None)` returns `("PreToolUse", "claude-code")`. | Unit test: `normalize_event_name("PreToolUse", None)` asserts `("PreToolUse", "claude-code")`. |
| AC-19 | The Codex reference config (`.codex/hooks.json`) is written. Each event invokes `unimatrix hook <event> --provider codex-cli`. The config carries an explicit caveat that live MCP hook support is blocked by Codex bug #16732. Unit tests use synthetic Codex events verifying normalization produces `provider: "codex-cli"`. | File exists at `.codex/hooks.json`; caveat text present; unit test with synthetic Codex event and `--provider codex-cli` flag. |
| AC-20 | `normalize_event_name("SessionStart", Some("claude-code"))` returns `("SessionStart", "claude-code")`. `normalize_event_name("SessionStart", Some("codex-cli"))` returns `("SessionStart", "codex-cli")`. Provider hint takes precedence over inference for all shared event names. | Unit tests for each case. |

---

## Domain Models

### Ubiquitous Language

| Term | Definition |
|------|------------|
| **Canonical Event Name** | The Unimatrix-internal string for a hook lifecycle event. Claude Code event names are the canonical names (e.g., `"PreToolUse"`, `"PostToolUse"`, `"Stop"`). All downstream code operates on canonical names only. |
| **Provider-Specific Event Name** | The string an LLM client uses in its hook protocol that may differ from canonical (e.g., Gemini's `"BeforeTool"`, `"SessionEnd"`). Exists only at the ingest boundary. |
| **Provider** | The identity of the LLM client that fired the hook: `"claude-code"`, `"gemini-cli"`, `"codex-cli"`, or `"unknown"`. Carried in `ImplantEvent.provider`. |
| **source_domain** | Runtime-derived string labeling which provider originated an observation. Derived at write time from `ImplantEvent.provider` (live path); derived at DB-read time via Approach A registry-with-fallback. Never persisted as a column. |
| **Normalization Layer** | The `normalize_event_name()` function in `hook.rs` that translates provider-specific event names to canonical names and infers/confirms the provider. Called before any `HookRequest` is constructed. |
| **Ingest Boundary** | The point in `hook.rs build_request()` where raw CLI arguments are converted into `HookRequest` variants. Normalization happens here exclusively. Nothing below this boundary branches on provider-specific names. |
| **Category 1 Event** | LLM provider hook lifecycle events (`PreToolUse`, `BeforeTool`, etc.). Subject to normalization. |
| **Category 2 Event** | Unimatrix MCP synthetic events (`cycle_start`, `cycle_stop`, `cycle_phase_end`). Already provider-neutral by construction. Not subject to normalization. |
| **Approach A** | DB-read-path `source_domain` derivation contract: call `registry.resolve_source_domain(event_type)`; if result is `"unknown"`, fall back to `"claude-code"`. Preserves existing behavior for all event types not in the builtin pack. |
| **`mcp_context`** | Gemini CLI field in `BeforeTool`/`AfterTool` payloads containing `{ server_name, tool_name, url }`. `tool_name` is the bare MCP tool name (e.g., `"context_cycle"` not `"mcp__unimatrix__context_cycle"`). |
| **Provider Hint** | The `Option<&str>` passed to `normalize_event_name`. Comes from `--provider` CLI flag when present; `None` when absent, triggering inference. |
| **Rework Candidate** | Internal routing label (never stored) for `PostToolUse` events that meet Claude Code-specific criteria. Gated to `provider == "claude-code"`. |

### Entity Relationships

```
HookInput (wire.rs)
  ├── event_type: raw CLI argument (e.g., "BeforeTool")
  ├── provider: Option<String>   [NEW — from --provider flag]
  ├── mcp_context: Option<Value> [NEW — Gemini structured MCP field]
  └── extra: Value               [serde flatten, catches other unknown fields]
        │
        ▼ normalize_event_name(event_type, provider_hint)
        │
        ▼ returns (canonical_name: &str, provider: &str)
        │
ImplantEvent (wire.rs)
  ├── event_type: canonical name (e.g., "PreToolUse")
  ├── provider: Option<String>   [NEW — "gemini-cli", "claude-code", etc.]
  └── [existing fields unchanged]
        │
        ▼ listener.rs write path
        │
ObservationRecord
  ├── event_type: String (canonical, stored in observations.hook column)
  └── source_domain: String      [derived from ImplantEvent.provider at write time]
```

### Event Mapping Table (Category 1 Only)

| Gemini CLI Event | Claude Code Event | Codex CLI Event | Canonical Name | Provider (inferred) |
|---|---|---|---|---|
| `BeforeTool` | `PreToolUse` | `PreToolUse` | `PreToolUse` | `gemini-cli` / `claude-code` / via `--provider` |
| `AfterTool` | `PostToolUse` | `PostToolUse` | `PostToolUse` | `gemini-cli` / `claude-code` / via `--provider` |
| `SessionStart` | `SessionStart` | `SessionStart` | `SessionStart` | `claude-code` (fallback) / via `--provider` |
| `SessionEnd` | `Stop` | `Stop` | `Stop` | `gemini-cli` / `claude-code` (fallback) / via `--provider` |
| (none) | `SubagentStart` | (none) | `SubagentStart` | `claude-code` |
| (none) | `SubagentStop` | (none) | `SubagentStop` | `claude-code` |
| (none) | `PreCompact` | (none) | `PreCompact` | `claude-code` |
| (none) | `UserPromptSubmit` | (none) | `UserPromptSubmit` | `claude-code` |
| (none) | `PostToolUseFailure` | (none) | `PostToolUseFailure` | `claude-code` |
| (none) | `TaskCompleted` | (none) | `TaskCompleted` | `claude-code` |
| (none) | `Ping` | (none) | `Ping` | `claude-code` |

Note: Codex CLI provider identity is established exclusively via `--provider codex-cli` flag. Codex shares all event names with Claude Code; inference without the flag defaults to `"claude-code"`.

---

## User Workflows

### Workflow 1: Gemini CLI Agent Using context_cycle

1. Gemini CLI agent calls `context_cycle(type="start", topic="vnc-013")` as an MCP tool call.
2. Gemini CLI fires `BeforeTool` hook to `unimatrix hook BeforeTool`.
3. `run()` reads stdin JSON; `HookInput.mcp_context.tool_name` contains `"context_cycle"`.
4. `normalize_event_name("BeforeTool", None)` returns `("PreToolUse", "gemini-cli")`.
5. `build_request()` is called with canonical name `"PreToolUse"`.
6. Adapter step promotes `mcp_context.tool_name` to the expected position.
7. `build_cycle_event_or_fallthrough()` intercepts `"context_cycle"`, reads `type="start"`, produces `HookRequest::RecordEvent { event_type: "cycle_start" }`.
8. `ImplantEvent` is sent over UDS with `provider: "gemini-cli"`.
9. `listener.rs` writes the observation; `cycle_events` row is written; `source_domain = "gemini-cli"`.
10. `context_cycle_review` later finds the `cycle_start`/`cycle_stop` events and returns correct results.

### Workflow 2: Gemini CLI Agent Tool Call (Non-cycle)

1. Gemini CLI agent calls `context_search(query="...")`.
2. Gemini fires `BeforeTool` with `mcp_context.tool_name = "context_search"`.
3. Normalization: `("PreToolUse", "gemini-cli")`.
4. `build_cycle_event_or_fallthrough()` does not intercept (tool_name is not `context_cycle`).
5. Falls through to `HookRequest::RecordEvent { event_type: "PreToolUse", provider: "gemini-cli" }`.
6. Observation written with `source_domain = "gemini-cli"`.

### Workflow 3: Codex CLI Agent (Forward-Compatibility)

1. Codex config: `unimatrix hook PreToolUse --provider codex-cli` in `.codex/hooks.json`.
2. When Codex bug #16732 is resolved, `run()` receives `provider = Some("codex-cli")`.
3. `normalize_event_name("PreToolUse", Some("codex-cli"))` returns `("PreToolUse", "codex-cli")`.
4. Observation written with `source_domain = "codex-cli"`.
5. Live MCP hook testing remains blocked until #16732 resolved; this workflow is verified by synthetic unit tests only.

### Workflow 4: Claude Code Agent (Unchanged)

1. Claude Code fires `PreToolUse`.
2. `--provider` flag absent; `HookInput.provider = None`.
3. `normalize_event_name("PreToolUse", None)` returns `("PreToolUse", "claude-code")`.
4. Existing dispatch path runs unchanged.
5. All existing behavior preserved.

---

## Constraints

**C-01: No DB schema changes.** The `observations` table `hook TEXT` column is sufficient. `source_domain` is not persisted. A schema migration would require a version bump and migration test — unjustified for a derived field.

**C-02: No tokio in hook.rs.** The hook subcommand is synchronous (existing ADR). All normalization logic must be synchronous `&str` → `&str` with no I/O.

**C-03: Rework detection is Claude Code-only.** `is_rework_eligible_tool()`, `is_bash_failure()`, `extract_file_path()` are specific to Claude Code's tool ecosystem. Gemini's `AfterTool` events (normalized to `PostToolUse`) must not enter this path. The `provider` field is the gate.

**C-04: `build_cycle_event_or_fallthrough()` is the single implementation.** No duplication. The Gemini adapter normalizes the payload shape and calls the existing function.

**C-05: Hook exit is always 0.** Normalization failures degrade to `generic_record_event`. `run()` never returns an error from normalization failures.

**C-06: Gemini hook registration uses regex.** The `.gemini/settings.json` matcher `mcp_unimatrix_.*` is confirmed valid for Gemini CLI v0.31+. Gemini built-in tools (file operations, shell) do not match this regex and do not fire the Unimatrix hook.

**C-07: `HookInput` backward compatibility.** New fields (`provider`, `mcp_context`) use `#[serde(default)]`. Existing Claude Code hook JSON deserializes without error.

**C-08: Codex is built, not live-tested.** Reference config ships. Code paths handle Codex events with `--provider codex-cli`. Live end-to-end testing is blocked by Codex bug #16732.

**C-09: Approach A is the required DB-read-path contract.** `resolve_source_domain(event_type)` with `"claude-code"` fallback when registry returns `"unknown"`. Approach B (accepting `"unknown"` for non-listed events) is rejected: it changes existing behavior and may break consumers checking `source_domain == "claude-code"` on session or cycle events.

**C-10: `PostToolUseFailure` path is untouched.** Per ADR-003 col-027 (entry #3475), `PostToolUseFailure` is intentionally not normalized to `PostToolUse`. The signal distinction is preserved. AC-16's guard targets the rework candidate string only.

**C-11: Blast-radius file enumeration.** The following six files constitute the complete blast radius. Every file must have at least one AC that validates its change:

| File | Change Required | Validated By |
|------|----------------|--------------|
| `crates/unimatrix-engine/src/wire.rs` | Add `provider: Option<String>` to `HookInput` and `ImplantEvent`; add `mcp_context: Option<Value>` to `HookInput` | AC-05, AC-07 |
| `crates/unimatrix-server/src/uds/hook.rs` | Add `normalize_event_name()`; add `--provider` CLI arg; add Gemini dispatch arms; gate rework path on provider; promote `mcp_context.tool_name` | AC-01, AC-02, AC-03, AC-04, AC-12, AC-14, AC-15, AC-17, AC-18 |
| `crates/unimatrix-server/src/uds/listener.rs` | Replace `source_domain` hardcode at line 1894; add AC-16 rework candidate guard in `extract_observation_fields()` | AC-06, AC-07(a), AC-16 |
| `crates/unimatrix-server/src/background.rs` | Replace `source_domain` hardcode at line 1330 with Approach A derivation | AC-07(b) |
| `crates/unimatrix-server/src/services/observation.rs` | Replace `source_domain` hardcode at line 585 with Approach A derivation; update `test_parse_rows_unknown_event_type_passthrough` comment | AC-07(c), AC-08 |
| `crates/unimatrix-server/src/domain/mod.rs` | No changes required — registry is insulated by normalization | AC-13 |

**C-12: `mcp_context.tool_name` promotion is the highest-risk integration point (SR-08).** The adapter step in FR-04.3 must have a dedicated unit test (covered by AC-14) before any other Gemini `BeforeTool` AC is attempted during implementation.

---

## Dependencies

### Existing Crates (No New Dependencies)

| Crate | Usage |
|-------|-------|
| `serde_json` | `mcp_context: Option<serde_json::Value>` deserialization; `tool_name` extraction |
| `serde` | `#[serde(default)]` on new `HookInput`/`ImplantEvent` fields |
| `clap` (or existing CLI arg parsing) | `--provider` optional argument on `unimatrix hook` subcommand |

### Internal Components

| Component | Dependency Type |
|-----------|----------------|
| `unimatrix-engine::wire::HookInput` | Modified — new fields |
| `unimatrix-engine::wire::ImplantEvent` | Modified — new field |
| `unimatrix-server::uds::hook::build_request()` | Modified — normalization + Gemini arms |
| `unimatrix-server::uds::hook::build_cycle_event_or_fallthrough()` | Reused unchanged — called after normalization |
| `unimatrix-server::uds::listener::extract_observation_fields()` | Modified — guard added |
| `unimatrix-server::uds::listener` (line 1894) | Modified — source_domain derivation |
| `unimatrix-server::background` (line 1330) | Modified — source_domain derivation |
| `unimatrix-server::services::observation` (line 585) | Modified — source_domain derivation |
| `unimatrix-core::observation::hook_type` | Read-only — string constants remain valid as canonical names |
| `DomainPackRegistry::resolve_source_domain()` | Used for Approach A — no changes to registry |

### External

| Dependency | Notes |
|------------|-------|
| Gemini CLI v0.31+ | Reference config target. `mcp_unimatrix_.*` matcher syntax confirmed for this version. |
| Codex CLI | Reference config target. Live testing blocked by upstream bug #16732. Schema confirmed identical to Claude Code by ASS-049. |

---

## NOT In Scope

The following are explicitly excluded to prevent scope creep:

- **Live Codex CLI end-to-end testing.** Blocked by Codex bug #16732. Out of scope until upstream resolves.
- **`source_domain` as a persisted column.** No schema migration. Derived at runtime only.
- **Continue, Cursor, or Zed hook provider support.** Primary targets are Claude Code, Gemini CLI, Codex CLI only.
- **SubagentStart/SubagentStop for Gemini or Codex.** Neither client has a subagent hook concept mapping to Unimatrix's.
- **Gemini `BeforeModel` hook.** Different architecture — separate spike if needed.
- **Changes to `context_cycle_review` logic.** Canonical event names (`cycle_start`, `cycle_stop`, `cycle_phase_end`) are already provider-neutral.
- **Changes to `DomainPackRegistry`.** The registry requires no changes; it is insulated by normalization.
- **MCP schema fixes** (Gemini `$defs`, union types, reserved names). Separate feature.
- **Tool description rewrites** (`context_briefing` NLI language, `context_cycle` hook path framing). Separate delivery task.
- **`max_tokens` enforcement on the MCP path.** Separate delivery task.
- **Server-side session attribution** via `clientInfo.name` + `Mcp-Session-Id`. Recommended in ASS-049 as a separate feature.
- **PostToolUseFailure normalization to PostToolUse.** Intentionally excluded per ADR-003 col-027 (entry #3475); the signal distinction is preserved.
- **Any change to the `DomainPackRegistry` builtin pack's `event_types` list.**

---

## Open Questions

None. All open questions from SCOPE.md are resolved:

1. **Rework detection gate** — RESOLVED: `provider != "claude-code"` gate (FR-05).
2. **Canonical name for `SessionEnd`** — RESOLVED (ASS-051): `"Stop"`.
3. **UserPromptSubmit / PreCompact Gemini equivalents** — RESOLVED: no Gemini analogs; out of scope.
4. **`source_domain` DB-read-path** — RESOLVED: Approach A required (FR-06.2, FR-06.3, C-09).
5. **Gemini `AfterTool` and rework detection** — RESOLVED: matcher regex is the filter; no additional guard needed.
6. **Gemini `AfterTool` response field** — RESOLVED (non-blocking): degrade gracefully; implementer confirms during implementation. `response_size` and `response_snippet` will be null if field name differs (SR-02).
7. **Codex hook config format** — RESOLVED (ASS-049): identical to Claude Code schema.
8. **Codex CLI arg passthrough** — RESOLVED (ASS-049): flag approach confirmed viable.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 20 results; entries #4298 (hook-normalization-boundary pattern), #2903 (col-023 ADR-001 HookType→string), #2906 (col-023 ADR-004 wave-based blast-radius plan) were directly applicable. Entry #4298 confirms the normalization-at-boundary pattern and the mcp_context promotion requirement. ADR-004 informs C-11's per-file blast-radius enumeration requirement.
