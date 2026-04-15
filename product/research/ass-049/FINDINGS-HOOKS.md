# FINDINGS: Hook System Compatibility & Multi-LLM Co-operation — Extended Investigation

**Spike**: ass-049 addendum
**Date**: 2026-04-11
**Approach**: dual-track (internal code read + external ecosystem research)
**Confidence**: high — source code confirmed internally; external findings from primary source code + open issue reports

---

## Internal: cycle_start Event Origin (Resolved Open Question)

**Answer**: `cycle_start` events are produced by the **`PreToolUse` hook**, not PostToolUse and not the MCP handler. The `context_cycle` MCP handler is stateless — it writes nothing to `cycle_events`. The hook path intercepts the tool call before it executes.

**Evidence (`crates/unimatrix-server/src/uds/hook.rs`):**

```
"PreToolUse" => build_cycle_event_or_fallthrough(event, session_id, input),
```

`build_cycle_event_or_fallthrough()` (lines 552–630):
1. Reads `tool_name` from `input.extra["tool_name"]`
2. Matches `tool_name.contains("context_cycle")` AND (`tool_name.contains("unimatrix")` OR `tool_name == "context_cycle"`)
3. Reads `tool_input["type"]` — "start" / "stop" / "phase_end"
4. Validates via `validate_cycle_params()`
5. Emits `HookRequest::RecordEvent` with `event_type = CYCLE_START_EVENT | CYCLE_PHASE_END_EVENT | CYCLE_STOP_EVENT`
6. Payload carries `feature_cycle`, `phase`, `outcome`, `goal` (Start only)

**Tool name format**: Claude Code sends `"mcp__unimatrix__context_cycle"` (server prefix + tool name). The hook matches by substring. A tool from a different MCP server named `context_cycle` is rejected unless its prefix contains `"unimatrix"` (R-09 mitigation).

**Critical implication for multi-LLM**: This mechanism is entirely client-side. A client must fire `PreToolUse` hooks for MCP tool calls for cycle attribution to work.

---

## Q1: Codex CLI Hook System

**Answer**: Yes, Codex has hooks. Same event types, same JSON stdin format. **MCP tool calls do NOT fire hooks** — confirmed open bug.

**Hook events**: `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`
**Config**: `~/.codex/hooks.json` or `<repo>/.codex/hooks.json`
**Format**: JSON stdin — `session_id`, `hook_event_name`, `cwd`, `transcript_path`, `model`, `permission_mode`, plus turn-scoped fields (`turn_id`, `tool_name`, `tool_input`, `tool_response`)

**Critical gap**: `codex-rs/core/src/hook_runtime.rs` hardcodes `tool_name: "Bash".to_string()` in both `run_pre_tool_use_hooks` and `run_post_tool_use_hooks`. MCP tool dispatch (`codex-rs/core/src/tools/handlers/mcp.rs`) calls `handle_mcp_tool_call` with **no hook invocation**.

Confirmed by:
- openai/codex issue #16732 (opened 2026-04-03, labeled `bug`+`hooks`): "Hooks only fire for Bash tool"
- Official Codex docs: "hooks...doesn't intercept MCP, Write, WebSearch, or other non-shell tool calls"
- openai/codex #16226: cannot distinguish subagent from main agent in hooks

**Impact on Unimatrix**: When a Codex agent calls any Unimatrix MCP tool (`context_cycle`, `context_store`, etc.), **zero hook events fire**. `cycle_events` is never written. Observation records are never produced. Attribution is nil unless server-side tracking is used.

**Timeline**: Bug #16732 has no milestone or PR as of 2026-04-03. Fix timeline unknown.

---

## Q2: Gemini CLI Hook System

**Answer**: Yes, Gemini has 11 hook events. `BeforeTool`/`AfterTool` explicitly support MCP tool calls with an `mcp_context` field. Full observation coverage achievable.

**Hook events**:
| Event | MCP Coverage | Blocking |
|-------|-------------|----------|
| `SessionStart` | n/a | No |
| `SessionEnd` | n/a | No |
| `BeforeAgent` | n/a | Yes |
| `AfterAgent` | n/a | Yes (retry) |
| `BeforeModel` | n/a | Yes |
| `AfterModel` | n/a | No (per-chunk) |
| `BeforeToolSelection` | n/a | Tool filter |
| `BeforeTool` | **Yes** | Yes |
| `AfterTool` | **Yes** | No |
| `PreCompress` | n/a | Async |
| `Notification` | n/a | No |

**MCP context schema** (in `BeforeTool`/`AfterTool` payloads):
```typescript
interface McpToolContext {
  server_name: string;    // "unimatrix"
  tool_name: string;      // "context_cycle"
  url?: string;           // Streamable HTTP endpoint URL
  command?: string;       // stdio: executable path (if stdio)
}
```

When Gemini calls `context_cycle` via Unimatrix's Streamable HTTP endpoint:
- `BeforeTool` fires with `mcp_context.server_name = "unimatrix"`, `tool_name = "context_cycle"`, `tool_input` = all parameters
- This maps **directly** to what Unimatrix's `build_cycle_event_or_fallthrough()` needs
- But Gemini calls this `"BeforeTool"`, not `"PreToolUse"` — Unimatrix's `hook.rs` currently only handles `"PreToolUse"`

**Config** (`.gemini/settings.json`):
```json
{
  "hooks": {
    "BeforeTool": [{ "matcher": "mcp_unimatrix_.*", "hooks": [{ "type": "command", "command": "unimatrix hook BeforeTool" }] }],
    "AfterTool":  [{ "matcher": "mcp_unimatrix_.*", "hooks": [{ "type": "command", "command": "unimatrix hook AfterTool" }] }],
    "SessionStart": [{ "hooks": [{ "type": "command", "command": "unimatrix hook SessionStart" }] }],
    "SessionEnd":   [{ "hooks": [{ "type": "command", "command": "unimatrix hook SessionEnd" }] }]
  }
}
```

The `matcher` is a regex on the MCP tool name. MCP tools follow the pattern `mcp_<server_name>_<tool_name>` in Gemini's matcher namespace.

**Open issues**: Only minor hook-related issues; no blocking bugs against BeforeTool/AfterTool or MCP coverage (v0.31.0+ stable).

**Impact on Unimatrix**: Achievable with three changes:
1. Add `"BeforeTool"` arm to `build_request()` in `hook.rs` — maps directly to current `"PreToolUse"` arm (same `build_cycle_event_or_fallthrough` logic)
2. Add `"AfterTool"` arm — maps to current `"PostToolUse"` arm
3. Add `"SessionEnd"` arm — maps to current `"Stop"` arm
4. Handle `mcp_context` field in `HookInput` (or read `tool_name` from it — the Gemini payload puts tool params in `tool_input` same as Claude Code)

---

## Q3: Alternative Observation Paths (No-Hook Fallback)

**Answer**: MCP server-side session tracking closes the Codex gap completely without client-side changes.

### Option A: MCP Transport-Level Attribution (Recommended)

On every Streamable HTTP request to Unimatrix, the spec requires the `Mcp-Session-Id` header. The server assigns it during initialization via `initialize` request, which also carries:

```json
{
  "params": {
    "clientInfo": {
      "name": "codex-mcp-client",       // Codex
      "name": "gemini-cli-mcp-client",   // Gemini
      "version": "..."
    }
  }
}
```

Confirmed client names from primary source:
- Codex: `"codex-mcp-client"` (from `codex-rs/codex-mcp/src/mcp_connection_manager.rs`)
- Gemini: `"gemini-cli-mcp-client"` (from `packages/core/src/tools/mcp-client.ts`)
- Claude Code: TBD (rmcp clientInfo string not confirmed from primary source — needs live capture)

**Implementation**: Log `clientInfo.name` at initialize, bind to `Mcp-Session-Id`, tag all subsequent tool call records with `client_type`. This gives per-request attribution for all providers with zero client configuration.

**Limitation**: Tool call payloads at the MCP layer contain all arguments but no tool-result (response) at the request boundary — response only available at the response boundary.

### Option B: MCP Proxy/Middleware

Intercepts all JSON-RPC traffic before forwarding. Adds latency and deployment complexity. Not recommended over Option A.

---

## Q4: Multi-Provider Session Identity (MCP Spec)

**Answer**: `clientInfo.name` + `Mcp-Session-Id` provides reliable per-request attribution for concurrent Claude + Codex + Gemini sessions. No MCP extension required.

**Mechanism**:
1. Three agents connect simultaneously → three `Mcp-Session-Id` values (server-assigned UUIDs, distinct)
2. Each agent's `clientInfo.name` is distinct at initialize
3. Every subsequent tool call request includes `Mcp-Session-Id` header
4. Unimatrix binds `clientInfo.name` to session ID → all tool calls tagged with client type

**Enterprise tier**: OAuth `sub` claim in `Authorization: Bearer` header is cryptographically bound identity per provider. Supersedes self-reported `agent_id` parameter.

**No standard MCP per-request agent identity extension exists** in the 2025-03-26 spec. `clientInfo` is the closest standard mechanism.

---

## Summary: Hook Coverage by Provider

| Feature | Claude Code | Codex CLI | Gemini CLI |
|---------|-------------|-----------|------------|
| Hook system exists | Yes | Yes (v0.117+) | Yes (v0.31+) |
| PreToolUse / BeforeTool | Yes | Yes | Yes |
| PostToolUse / AfterTool | Yes | Yes | Yes |
| SessionStart | Yes | Yes | Yes |
| SessionEnd / Stop | Yes | Yes | Yes |
| **MCP tool calls fire hooks** | **Yes — all tools** | **No (bug #16732)** | **Yes (mcp_context)** |
| MCP server identity in hook | Yes (`tool_name`) | N/A | Yes (`mcp_context.server_name`) |
| `context_cycle` interception | Yes (`build_cycle_event_or_fallthrough`) | No | Yes (BeforeTool, same logic) |
| Hook data format | JSON stdin | JSON stdin | JSON stdin |
| Config location | `.claude/settings.json` | `~/.codex/hooks.json` | `.gemini/settings.json` |
| Can block tool calls | Yes (exit 2) | Yes | Yes (`decision: deny`) |
| SubagentStart/Stop | Yes | No | No |
| LLM request interception | No | No | Yes (`BeforeModel`) |

---

## Gap Analysis: Per-Provider Observation Coverage

### Claude Code — No Gap
Full coverage. All 12 MCP tools observed. `cycle_events` written via PreToolUse interception. `source_domain = "claude-code"` correct.

### Codex CLI — Blocked on Bug #16732
**Gaps**:
- Zero MCP observation from client-side hooks
- `cycle_events` never written (context_cycle calls unobserved)
- `source_domain` hardcoded `"claude-code"` in `background.rs` — wrong for Codex records

**Mitigations**:
1. (Server-side, now): Log `clientInfo.name = "codex-mcp-client"` from initialize, bind to Mcp-Session-Id, record all tool calls at transport layer. Closes coverage gap completely.
2. (Client-side, blocked): Once #16732 fixed, hooks work with no Unimatrix changes — JSON format is identical to Claude Code.
3. (Unblockable): No SubagentStart/Stop equivalent — Codex has no subagent hook concept.

### Gemini CLI — Achievable with Small Extension
**Gaps**:
- `hook.rs` only handles Claude Code event names (`PreToolUse`, `PostToolUse`, `Stop`, `SessionStart`)
- `BeforeTool`, `AfterTool`, `SessionEnd` not handled → routes to unknown event → no observation
- `source_domain` hardcoded `"claude-code"` in `background.rs` — wrong for Gemini records

**Mitigations** (all small, well-defined):
1. Add `"BeforeTool"` → `build_cycle_event_or_fallthrough()` arm in `hook.rs` (identical logic)
2. Add `"AfterTool"` → PostToolUse arm in `hook.rs` (identical logic; `mcp_context` field is additive, can be ignored or used for server attribution)
3. Add `"SessionEnd"` → SessionClose arm in `hook.rs`
4. Make `source_domain` dynamic in `background.rs` — derive from a config or hook event header rather than hardcode
5. Provide `.gemini/settings.json` reference config in docs

---

## Blocking Architectural Issues for Multi-LLM Co-operation

### 1. `source_domain` Hardcoded to `"claude-code"` (`background.rs:1330`)

All hook-path observation records get `source_domain = "claude-code"` regardless of which client fired the hook. When Gemini hooks are wired up, their records will be mislabeled. DomainPackRegistry is config-defined and structurally supports multiple providers, but the actual `source_domain` value is hardcoded in `background.rs` at the point where `ObservationRecord` is built.

**Fix**: Pass the `source_domain` through `ImplantEvent` or derive it from the event name prefix (`BeforeTool` → `"gemini-cli"`; `PreToolUse` → `"claude-code"`). Alternatively, make `source_domain` a field in `HookInput` and populate it in `hook.rs`.

### 2. UDS Transport is Claude Code-Specific

The UDS socket path is `~/.unimatrix/{project_hash}/unimatrix.sock`. Unimatrix's `hook` subcommand binary is the UDS client. Gemini and Codex hooks can invoke `unimatrix hook <EventName>` — the same binary — which then connects to the same UDS socket. This is actually **not a blocker**: the transport is process-agnostic. Any process can open the socket.

The hook binary just needs to handle the new event names. The socket protocol is shared.

### 3. `context_cycle_review` Finds Nothing for Non-Claude Clients (Until cycle_events Written)

If neither client-side hooks nor server-side transport logging writes `cycle_events` rows, `context_cycle_review` returns an empty result for Codex/Gemini feature cycles. This is expected behavior and not a bug — but it means the retrospective pipeline is Claude-only until Gemini hooks land and Codex #16732 is resolved.

---

## Recommendations

**Immediate (before Wave 2 launch):**
1. **Server-side session attribution** (Option A, Q3): Log `clientInfo.name` at initialize, bind to `Mcp-Session-Id`, propagate as `client_type` on all tool call audit records. Closes the Codex observation gap with no client changes. Blocks nothing.
2. **Document `agent_id` is the only attribution signal for non-Claude clients** in `context_cycle` tool description (reinforces FINDINGS.md recommendation §Q1).

**Near-term (Wave 2 delivery):**
3. **Gemini hook handler additions** (`hook.rs`): Add `"BeforeTool"`, `"AfterTool"`, `"SessionEnd"` arms — 3 match arms, each mapping to existing logic. Enables full Gemini observation parity.
4. **Dynamic `source_domain`** (`background.rs`): Derive from event name or a config field rather than hardcoding `"claude-code"`. Required for correct DomainPack routing when multiple providers are active.
5. **Reference `.gemini/settings.json`** in onboarding documentation (5 lines, regex matcher `mcp_unimatrix_.*`).

**Deferred:**
6. **Codex client-side hooks**: Track openai/codex #16732. When fixed, add `~/.codex/hooks.json` to Unimatrix documentation. No code changes required — hook event names and JSON format are identical to Claude Code.
7. **Gemini `BeforeModel` hook**: Enables context injection at the LLM request level (not just session start). Different architecture than current approach. Worth a dedicated spike if session-start injection quality is insufficient for Gemini.

---

## Unanswered Questions

1. **Claude Code `clientInfo.name`**: Exact string not confirmed from primary source. Needed for server-side attribution to distinguish Claude from Codex/Gemini. Requires live capture or `anthropics/claude-code` source read.
2. **Codex exec mode `clientInfo.name`**: Whether the non-TUI `exec` mode sends `"codex-mcp-client"` or a different string.
3. **Gemini `AfterTool` payload size limit**: Whether Gemini imposes a stdin size limit for hook commands. Unimatrix briefing responses can be large. Needs live test.
4. **Codex #16732 fix timeline**: No milestone or PR as of 2026-04-03.
