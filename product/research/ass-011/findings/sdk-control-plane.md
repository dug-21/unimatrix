# Research Findings: Claude Agent SDK as Unimatrix Control Plane

**Date:** 2026-02-26
**Spike:** ASS-011 (Hook-Driven Orchestration)
**Context:** Evaluating an alternative architecture where Unimatrix (Rust) uses the Claude Agent SDK to launch/manage LLM agents as execution units, instead of (or alongside) hooks as middleware.

---

## 1. Claude Agent SDK Capabilities

### What It Is

The Claude Agent SDK (formerly Claude Code SDK, renamed September 2025) is Anthropic's official library for building AI agents programmatically. It provides the same tools, agent loop, and context management that power Claude Code, available as a library in Python and TypeScript.

**Key architectural fact:** The SDK is a subprocess wrapper around the Claude Code CLI. It spawns `claude` as a child process and communicates via stdin/stdout using JSONL. The SDK does NOT call the Anthropic Messages API directly — the CLI handles API communication, tool execution, context management, and compaction internally.

### Official SDKs

| Language | Package | Version (Feb 2026) | Status |
|----------|---------|---------------------|--------|
| TypeScript | `@anthropic-ai/claude-agent-sdk` | v0.2.59 | Official, production |
| Python | `claude-agent-sdk` | v0.1.34 | Official, production |
| **Rust** | **None (official)** | N/A | **Not available** |

### Community Rust SDKs

Two community Rust wrappers exist, both wrapping the Claude Code CLI as a subprocess:

| Crate | Approach | Stars | Maturity |
|-------|----------|-------|----------|
| `claude_agent_sdk_rust` (Wally869) | Subprocess wrapper around CLI v2.0.0+ | ~20 | Early-stage, 8 commits |
| `claude-agent-sdk` (louloulin) | Subprocess wrapper | Minimal | Experimental |

**Neither community SDK is production-ready.** Both replicate the Python/TS pattern of spawning the CLI as a subprocess.

### Programmatic Capabilities (Documented)

| Capability | Supported | How |
|-----------|-----------|-----|
| Launch agents with specific system prompts | **YES** | `system_prompt` option (string or preset) |
| Control available tools | **YES** | `allowed_tools` / `disallowed_tools` lists |
| Intercept/filter tool calls before execution | **YES** | `PreToolUse` hooks with programmatic callbacks |
| Read agent output programmatically | **YES** | Async iterator yields all messages as structured objects |
| Set token budgets | **YES** | `max_budget_usd` (dollar budget) |
| Set turn limits | **YES** | `max_turns` (integer) |
| Run multiple agents concurrently | **YES** | Multiple `query()` calls or `ClaudeSDKClient` instances |
| Define subagents with scoped tools | **YES** | `agents` option with `AgentDefinition` |
| Session persistence/resume | **YES** | `resume` (session ID) or `continue_conversation` |
| Connect MCP servers | **YES** | `mcp_servers` option (external or in-process) |
| Define custom tools | **YES** | In-process MCP servers via `createSdkMcpServer` / `create_sdk_mcp_server` |
| Structured output | **YES** | `output_format` with JSON Schema |
| Modify tool inputs | **YES** | `PreToolUse` hook returns `updatedInput` |
| Block tool execution | **YES** | `PreToolUse` hook returns `permissionDecision: "deny"` |
| Inject context mid-session | **YES** | Hook callbacks return `additionalContext` or `systemMessage` |
| Interrupt running agent | **YES** | `ClaudeSDKClient.interrupt()` |
| Model selection | **YES** | `model` option (string) |
| Extended thinking | **YES** | `thinking` config with budget tokens |
| Permission control | **YES** | `permission_mode` (default, acceptEdits, plan, bypassPermissions) + `can_use_tool` callback |
| File checkpointing/rewind | **YES** | `enable_file_checkpointing` + `rewind_files()` |
| Sandbox configuration | **YES** | `sandbox` option |
| Working directory | **YES** | `cwd` option |
| Environment variables | **YES** | `env` option |

### Full ClaudeAgentOptions Interface (Python)

```python
@dataclass
class ClaudeAgentOptions:
    tools: list[str] | ToolsPreset | None = None
    allowed_tools: list[str] = field(default_factory=list)
    system_prompt: str | SystemPromptPreset | None = None
    mcp_servers: dict[str, McpServerConfig] | str | Path = field(default_factory=dict)
    permission_mode: PermissionMode | None = None
    continue_conversation: bool = False
    resume: str | None = None
    max_turns: int | None = None
    max_budget_usd: float | None = None
    disallowed_tools: list[str] = field(default_factory=list)
    model: str | None = None
    fallback_model: str | None = None
    betas: list[SdkBeta] = field(default_factory=list)
    output_format: dict[str, Any] | None = None
    cwd: str | Path | None = None
    cli_path: str | Path | None = None
    env: dict[str, str] = field(default_factory=dict)
    can_use_tool: CanUseTool | None = None
    hooks: dict[HookEvent, list[HookMatcher]] | None = None
    agents: dict[str, AgentDefinition] | None = None
    sandbox: SandboxSettings | None = None
    thinking: ThinkingConfig | None = None
    effort: Literal["low", "medium", "high", "max"] | None = None
    enable_file_checkpointing: bool = False
    # ... additional fields for plugins, settings, etc.
```

### SDK Hook Events (Programmatic Callbacks)

| Hook Event | Python | TypeScript | Trigger |
|------------|--------|------------|---------|
| `PreToolUse` | YES | YES | Before tool execution (can block/modify) |
| `PostToolUse` | YES | YES | After tool execution (can add context) |
| `PostToolUseFailure` | NO | YES | Tool execution failure |
| `UserPromptSubmit` | YES | YES | User prompt submission |
| `Stop` | YES | YES | Agent execution stop |
| `SubagentStart` | NO | YES | Subagent initialization |
| `SubagentStop` | YES | YES | Subagent completion |
| `PreCompact` | YES | YES | Conversation compaction |
| `PermissionRequest` | NO | YES | Permission dialog |
| `SessionStart` | NO | YES | Session initialization |
| `SessionEnd` | NO | YES | Session termination |
| `Notification` | NO | YES | Agent status messages |

**Important:** The TypeScript SDK has significantly more hook event coverage than the Python SDK. SubagentStart, SessionStart, SessionEnd, PostToolUseFailure, PermissionRequest, and Notification hooks are TypeScript-only.

---

## 2. Claude API tool_use (Raw Agentic Loop)

### The Pattern

The Anthropic Messages API supports an agentic loop pattern without any SDK. The caller:

1. Sends a `messages.create()` request with `tools` definitions
2. Receives a response. If `stop_reason == "tool_use"`, the response contains `tool_use` content blocks
3. The caller executes the tool themselves and constructs `tool_result` content blocks
4. Sends a new request with the accumulated message history
5. Repeats until `stop_reason == "end_turn"`

```
Orchestrator                    Anthropic API
    │                               │
    ├──messages.create(tools=[...])──►
    │◄──── stop_reason: tool_use ────┤
    │      tool_use: {name, input}   │
    │                                │
    │ [execute tool locally]         │
    │                                │
    ├──messages.create(tool_result)──►
    │◄──── stop_reason: end_turn ────┤
    │      text response             │
```

### What the Raw API Provides

| Capability | Supported | Notes |
|-----------|-----------|-------|
| Define available tools per request | **YES** | `tools` parameter, JSON Schema per tool |
| Intercept tool_use before execution | **YES** | The orchestrator IS the executor — nothing runs unless the orchestrator runs it |
| Feed tool results back | **YES** | `tool_result` content blocks |
| System prompt control | **YES** | `system` parameter |
| Multiple tool calls per turn | **YES** | Response can contain multiple `tool_use` blocks |
| Mixed text + tool calls | **YES** | Response can mix text and tool_use |
| Token limits | **YES** | `max_tokens` parameter |
| Streaming | **YES** | Server-sent events |
| Extended thinking | **YES** | `thinking` parameter |
| Prompt caching | **YES** | Cache control headers, up to 90% input cost reduction |

### Rust Libraries for the Messages API

| Library | Stars | Features |
|---------|-------|----------|
| `anthropic-tools` | Low | Messages API, tool calling, streaming, vision |
| `anthropic-rs` | Moderate | Typed requests, streaming |
| `anthropic-api` | Low | Async, messages, tool use |
| `misanthropy` | Moderate | Full API bindings |

Direct HTTP via `reqwest` is also straightforward — the API is a single `POST /v1/messages` endpoint with JSON payloads.

### Key Difference: Raw API vs Agent SDK

| Aspect | Raw API | Agent SDK |
|--------|---------|-----------|
| Tool execution | **You implement everything** | Built-in: Bash, Read, Write, Edit, Glob, Grep, etc. |
| Context management | **You manage the message array** | Automatic compaction when nearing limits |
| File operations | **You implement file I/O tools** | Built-in with permission controls |
| Shell execution | **You implement shell tools** | Built-in Bash tool with sandboxing |
| Conversation state | **You store/load message history** | Automatic session persistence |
| Tool definitions | **JSON Schema per request** | Pre-defined + custom MCP tools |
| MCP integration | **You implement MCP client** | Built-in MCP server connections |

---

## 3. Control Plane Architecture Analysis

### Unimatrix as Control Plane via Agent SDK

If Unimatrix (Rust) runs the orchestration layer using the Agent SDK:

| Control Plane Function | SDK Mechanism | Strength |
|-----------------------|---------------|----------|
| Define system prompt (role, phase, conventions) | `system_prompt` option | **Full control** — set per agent, per phase |
| Define available tools (scoped per phase) | `allowed_tools` / `disallowed_tools` | **Full control** — deterministic, per-agent tool sets |
| Intercept every tool call | `PreToolUse` hooks (programmatic callbacks) | **Full control** — in-process function, not subprocess |
| Manage conversation state | `ClaudeSDKClient` with `resume` / session IDs | **Full control** — persist, fork, or discard |
| Control agent start/stop | `query()` lifecycle + `max_turns` + `max_budget_usd` | **Full control** — deterministic gates |
| Inject mid-session context | Hook callbacks return `additionalContext` / `systemMessage` | **Full control** — context injection at any hook point |
| Launch subagents with scoped capabilities | `agents` option with `AgentDefinition` | **Full control** — tool + model + prompt per subagent |
| Connect to Unimatrix knowledge base | `mcp_servers` option | **Direct** — register Unimatrix as MCP server |
| Observe all tool calls | `PostToolUse` hooks | **Complete visibility** — every tool call and result |
| Budget enforcement | `max_budget_usd` | **Deterministic** — hard cap |

### Unimatrix as Control Plane via Raw API

If Unimatrix implements its own agentic loop using the Anthropic Messages API directly:

| Control Plane Function | Raw API Mechanism | Strength |
|-----------------------|-------------------|----------|
| Define system prompt | `system` parameter | **Full control** |
| Define available tools | `tools` parameter per request | **Full control** — can change tools between turns |
| Intercept every tool call | **Intrinsic** — orchestrator IS the tool executor | **Maximum control** — no tool runs without explicit orchestrator action |
| Manage conversation state | Orchestrator manages message array | **Full control** — can edit, rewrite, or truncate history |
| Control agent start/stop | Orchestrator decides when to call API | **Full control** |
| Inject mid-session context | Modify message array before next API call | **Full control** — can inject system messages, modify history |
| Launch parallel agents | Multiple concurrent API sessions | **Full control** — completely independent |
| Connect to Unimatrix knowledge base | Implement as a tool the orchestrator handles | **Direct** — in-process, no MCP protocol needed |
| Observe all tool calls | **Intrinsic** — orchestrator sees everything | **Complete visibility** |
| Budget enforcement | Count tokens from API responses | **Deterministic** |

### Key Architectural Differences

| Dimension | Hook Approach | SDK Approach | Raw API Approach |
|-----------|---------------|-------------|-----------------|
| Who controls execution | LLM (Claude Code) | LLM (Claude Code, via subprocess) | **Orchestrator** |
| Tool execution | Claude Code built-in | Claude Code built-in (via subprocess) | **Orchestrator implements tools** |
| Identity guarantee | Hook-injected (validated by ASS-011 Phase 1) | Programmatic (SDK sets system prompt) | **Intrinsic** — orchestrator assigns identity |
| Scope enforcement | Hook denies out-of-scope calls | `allowed_tools` list + hook denial | **Intrinsic** — orchestrator only executes in-scope tools |
| Context management | Claude Code manages | Claude Code manages (via subprocess) | **Orchestrator manages** |
| File system access | Claude Code sandboxing | Claude Code sandboxing | **Orchestrator implements** (or delegates) |
| Shell access | Claude Code Bash tool | Claude Code Bash tool | **Orchestrator implements** (or delegates) |
| MCP integration | Claude Code MCP client | Claude Code MCP client | **Orchestrator implements MCP client** |
| Latency per tool call | Hook overhead (~80ms per hook, from Phase 1) | In-process callbacks (<1ms) + subprocess overhead | **Zero overhead** for interception |
| Implementation effort | Moderate (hooks are scripts/binaries) | Moderate (TS/Python wrapper around Rust core) | **High** (implement all tools, context mgmt, compaction) |

---

## 4. Comparison: Hooks vs SDK vs Raw API

### Guarantee Strength

| Guarantee | Hooks | SDK | Raw API |
|-----------|-------|-----|---------|
| LLM cannot choose its own identity | **Strong** — PreToolUse injects identity transparently (validated) | **Strong** — system_prompt set by orchestrator | **Maximum** — identity is never in LLM's context as modifiable |
| LLM cannot access unauthorized tools | **Moderate** — hook can deny, but LLM sees tool definitions | **Strong** — `allowed_tools` filters before LLM sees them | **Maximum** — unauthorized tools never exist in the API call |
| LLM cannot bypass scope | **Moderate** — hook can deny post-hoc | **Strong** — combination of allowed_tools + hooks | **Maximum** — orchestrator never executes out-of-scope actions |
| LLM cannot manipulate conversation state | **Weak** — LLM controls the conversation; hooks only intercept tool calls | **Moderate** — orchestrator can manage sessions but Claude Code manages internally | **Maximum** — orchestrator owns the message array |
| LLM cannot exceed budget | **Weak** — no budget control via hooks | **Strong** — `max_budget_usd` | **Maximum** — orchestrator controls when to stop calling API |
| LLM cannot ignore injected context | **Moderate** — system-reminder has high priority but is advisory | **Moderate** — same mechanism via hooks | **Maximum** — system messages are part of the prompt architecture |

### Implementation Complexity

| Task | Hooks | SDK | Raw API |
|------|-------|-----|---------|
| Reading files | Free (Claude Code) | Free (Claude Code) | **Must implement** |
| Running shell commands | Free (Claude Code) | Free (Claude Code) | **Must implement** (process spawn, output capture, sandboxing) |
| Editing files | Free (Claude Code) | Free (Claude Code) | **Must implement** (diff-based editing) |
| Web search | Free (Claude Code) | Free (Claude Code) | **Must implement** or use MCP |
| Context window management | Free (Claude Code compaction) | Free (Claude Code compaction) | **Must implement** (summarization, history truncation) |
| Tool definitions | N/A (Claude Code defines them) | Pre-defined + custom | **Must define all tool schemas** |
| Session persistence | Free (Claude Code) | Free (Claude Code) | **Must implement** |
| Error recovery | Free (Claude Code) | Free (Claude Code) | **Must implement** |

### Latency Characteristics

| Path | Hooks | SDK | Raw API |
|------|-------|-----|---------|
| Tool interception | +80ms per hook (subprocess) | <1ms (in-process callback) | 0ms (intrinsic) |
| Subprocess overhead | N/A | ~50-200ms per query (CLI spawn) | 0 (direct API calls) |
| API call | Handled by Claude Code | Handled by Claude Code via subprocess | Direct HTTP call |
| Total per-agent-turn | Fast (Claude Code is fast, hooks add ~80ms) | Moderate (subprocess JSON-RPC overhead) | Fastest (minimal overhead, but must implement everything) |

---

## 5. SDK Language Support & Integration Options

### Official: No Rust SDK

Anthropic does not provide an official Rust SDK for the Agent SDK. The official offerings are:

- **TypeScript:** `@anthropic-ai/claude-agent-sdk` — most complete (all 12 hook events)
- **Python:** `claude-agent-sdk` — functional but fewer hook events (no SessionStart/End, SubagentStart, etc.)

### Integration Options for a Rust Control Plane

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| **A. Raw API via `reqwest`** | Implement agentic loop directly against Anthropic Messages API | Maximum control, pure Rust, no subprocess dependencies | Must implement all tools (file I/O, shell, editing, search, compaction) |
| **B. Subprocess wrapper around CLI** | Rust program spawns `claude` CLI, communicates via JSONL | Access to all Claude Code tools, same as official SDKs | Dependency on Claude Code CLI installation, subprocess overhead, parsing JSONL |
| **C. TS/Python SDK as subprocess** | Rust program spawns Node.js/Python with SDK script | Access to full SDK API surface including all hooks | Two subprocess layers (Rust→Node→CLI), complex error handling |
| **D. FFI to TS/Python** | Rust calls into Node.js/Python runtime via FFI | In-process access to SDK | Extremely complex, fragile, not recommended |
| **E. Hybrid: Rust raw API + Claude Code for tools** | Rust orchestrator for control flow, delegate tool execution to Claude Code instances | Control + convenience | Architecture complexity, two execution paths |

### Recommended for Unimatrix

**Option B (subprocess wrapper around CLI)** is the most practical path. Rationale:
- All official SDKs (Python, TypeScript, Java, Go community ports) use this pattern
- The Rust community SDKs already demonstrate feasibility
- Gets all Claude Code built-in tools for free
- Hook system available for interception
- No need to implement file I/O, shell execution, code editing, web search, or context compaction
- The CLI is installed as part of Claude Code, which Unimatrix already depends on

**Option A (raw API)** is the most architecturally pure for a control plane but requires implementing ~10,000+ lines of tool execution code that Claude Code already handles.

---

## 6. Practical Constraints

### Authentication & Cost

| Aspect | Hooks (Current) | Agent SDK | Raw API |
|--------|-----------------|-----------|---------|
| Authentication | Uses Claude Code session (user's subscription or API key) | **Requires API key** — `ANTHROPIC_API_KEY` env var | **Requires API key** |
| Subscription compatibility | Works with Pro/Max/Team plans | **API key only** (no Max plan billing as of Feb 2026) — [open issue #559](https://github.com/anthropics/claude-agent-sdk-python/issues/559) | API key only |
| Cost model | Included in Claude Code subscription usage | Per-token API pricing | Per-token API pricing |
| Bedrock/Vertex support | N/A | YES (via env vars) | YES (different endpoints) |

**Critical constraint:** The Agent SDK currently requires API keys and does NOT support Max plan billing. Issue #559 on the Python SDK repo tracks this. For users who rely on Max plan subscriptions for Claude Code usage, the SDK approach would be an **additional cost** on top of their subscription.

### API Pricing (Feb 2026)

| Model | Input (per MTok) | Output (per MTok) |
|-------|------------------|--------------------|
| Claude Opus 4.6 | $5 | $25 |
| Claude Sonnet 4.5 | $3 | $15 |
| Claude Haiku 4.5 | $1 | $5 |

Prompt caching: up to 90% reduction on cached input tokens.
Batch API: 50% discount for async processing (24-hour window).

### Tool Execution

| Question | Answer |
|----------|--------|
| Can SDK agents access local files? | **YES** — Built-in Read/Write/Edit tools (via Claude Code subprocess) |
| Can SDK agents run shell commands? | **YES** — Built-in Bash tool (via Claude Code subprocess) |
| Does the orchestrator implement tools? | **NO** (for SDK) — Claude Code handles all built-in tool execution. **YES** (for raw API) — orchestrator must implement everything |
| MCP server integration? | **YES** — `mcp_servers` option connects to MCP servers, including Unimatrix |

### SDK Requirement: Claude Code CLI

The Agent SDK requires the Claude Code CLI (`claude`) to be installed:
- `cli_path` option allows specifying a custom CLI path
- The CLI must be v2.0.0+ for the SDK
- The CLI handles API communication, tool execution, context management, and compaction

---

## 7. Hybrid Possibility

### The Case for Hybrid

Both approaches serve different orchestration weight classes:

| Orchestration Weight | Best Approach | Why |
|---------------------|---------------|-----|
| **Lightweight** (context injection, identity, scope enforcement) | **Hooks** | Already validated (Phase 1), minimal overhead, works within existing Claude Code sessions, no API key cost |
| **Heavyweight** (launching specialized agents, deterministic gate enforcement, budget-controlled phases) | **Agent SDK** | Full lifecycle control, tool scoping, budget limits, session management |
| **Maximum control** (custom agentic loops, non-Claude-Code environments) | **Raw API** | Complete orchestrator ownership, but high implementation cost |

### Hybrid Architecture

```
Human uses Claude Code (existing workflow, subscription-based)
    │
    ├── Hooks (lightweight orchestration)
    │   ├── SubagentStart: inject role×phase context (VALIDATED)
    │   ├── PreToolUse: inject identity, enforce scope (VALIDATED)
    │   ├── PostToolUse: observe outcomes, enrich briefings (VALIDATED)
    │   └── Workflow state read/write via CLI binary (<100ms, to validate)
    │
    └── Unimatrix SDK Orchestrator (heavyweight orchestration)
        ├── Launches targeted agents for specific phases
        │   └── query(prompt, options={
        │         system_prompt: compiled_briefing,
        │         allowed_tools: phase_scoped_tools,
        │         max_turns: 50,
        │         max_budget_usd: 2.0,
        │         mcp_servers: { unimatrix: ... },
        │         hooks: { PreToolUse: [scope_enforcer] }
        │       })
        ├── Manages gate transitions (stop agent, evaluate, start next)
        ├── Runs parallel agents for independent work
        └── Requires API key (separate from user's subscription)
```

### Boundary Definition

| Trigger | Orchestration Mode |
|---------|-------------------|
| User starts Claude Code session manually | **Hooks** — lightweight context injection, identity, scope |
| Unimatrix needs to launch an autonomous phase (e.g., automated testing, code review) | **SDK** — Unimatrix launches an agent with full control |
| CI/CD pipeline triggers agent work | **SDK** — No human session, Unimatrix orchestrates |
| Human wants to override or inspect agent mid-task | **Hooks** — Human's Claude Code session, hooks provide visibility |

### Progressive Adoption Path

1. **Phase 1 (now):** Hooks only. Validate control plane within existing Claude Code sessions. No API key needed. (Already in progress: ASS-011.)

2. **Phase 2 (near-term):** Add SDK orchestrator for automated/CI workflows. Unimatrix can launch agents for gate evaluation, automated review, or parallel implementation. Requires API key.

3. **Phase 3 (future):** Raw API for specialized scenarios where Claude Code's tool set is not needed (e.g., pure reasoning tasks, or connecting to non-Claude-Code execution environments).

### What Coexistence Requires

| Requirement | Implementation |
|-------------|---------------|
| Shared workflow state | Both hooks and SDK read/write the same redb database (or state file) |
| Shared identity system | Both inject identity from the same source (workflow state) |
| Unified observation | Both feed PostToolUse data to the same observation pipeline |
| Single knowledge base | Both connect to the same Unimatrix MCP server (or redb directly) |
| Configuration | Hooks configured in `.claude/settings.json`, SDK configured programmatically |

---

## Findings Summary

### What Documentation Says

1. The Claude Agent SDK is a subprocess wrapper around the Claude Code CLI, available in Python and TypeScript.
2. It provides full programmatic control over system prompts, tools, hooks, subagents, MCP servers, sessions, budgets, and turn limits.
3. The raw Anthropic Messages API supports tool_use with a well-documented agentic loop pattern.
4. Multiple Rust libraries exist for the Messages API (reqwest-based).
5. Community Rust wrappers exist for the Agent SDK (subprocess-based, early-stage).
6. SDK hooks are programmatic callbacks (not subprocess-based like Claude Code hooks), enabling lower-latency interception.
7. The SDK requires an API key; Max plan billing is not supported.

### What Can Be Inferred

1. **The SDK approach gives stronger guarantees** than hooks alone because the orchestrator controls agent lifecycle, not just individual tool calls. The LLM cannot start itself, extend its own turn limit, or choose its own tools.
2. **The raw API approach gives maximum guarantees** but at prohibitive implementation cost — reimplementing Claude Code's entire tool suite, context management, and compaction is not practical for a single project.
3. **The subprocess architecture of the SDK** means latency characteristics are similar to hooks (both spawn Claude Code as a subprocess), but SDK hooks are in-process callbacks rather than external script invocations, which is faster.
4. **Hybrid is the pragmatic path** — hooks for lightweight orchestration within user sessions, SDK for heavyweight autonomous orchestration. Both share the same Unimatrix state and knowledge base.

### What Remains Unknown

1. **SDK hook latency in practice.** SDK hooks are documented as in-process callbacks, but the SDK itself communicates with the CLI via subprocess. Whether hook callbacks experience the same ~80ms overhead as Claude Code file-based hooks, or are faster (in-process), needs measurement.
2. **Concurrent SDK agents sharing redb.** Can multiple SDK-launched agents connect to the same Unimatrix MCP server simultaneously? redb supports concurrent readers but single writer — MCP server holds the writer. Multiple agent sessions each connecting to their own MCP server instance would each try to open the database.
3. **SDK stability for long-running orchestration.** The SDK is 5 months old (released September 2025). Long-running orchestration (hours-long feature implementation) may encounter edge cases in subprocess management, memory, or session state.
4. **Max plan billing timeline.** Issue #559 tracks SDK support for Max plan billing. If this ships, the cost model changes significantly — SDK usage could be covered by existing subscriptions.
5. **Custom Transport for Rust.** The Python SDK exposes a `Transport` abstract class. Could a Rust program implement a custom transport that communicates with the CLI directly, bypassing Python/TS? The Transport API is marked as "low-level internal API" and may change.
6. **SubagentStart hook in Python SDK.** SubagentStart is TypeScript-only. If using Python SDK as the wrapper, Unimatrix cannot intercept subagent creation — a significant limitation for identity injection.

---

## Recommendation for ASS-011

### Do Not Pivot Away from Hooks

The hook approach validated in Phase 1 is the right near-term architecture:
- All five mechanisms validated
- Works within existing user sessions (no API key cost)
- Latency is within thresholds
- Compatible with future SDK adoption

### Document SDK as Phase 2+ Option

The SDK provides a stronger control plane for autonomous orchestration. It should be documented as a future capability, not a replacement for hooks. The hybrid model is the end state.

### Avoid Raw API for Now

Implementing the full agentic loop in Rust (file I/O, shell execution, code editing, context compaction) would consume months of development for capabilities Claude Code already provides. Only revisit if Unimatrix needs to orchestrate non-Claude-Code agents.

### If/When SDK Is Adopted

1. Use **Option B (subprocess wrapper around CLI)** — build a minimal Rust wrapper that spawns `claude` and communicates via JSONL
2. Use **TypeScript SDK as reference** — it has the most complete hook coverage
3. Share state via **redb** (same as hooks approach)
4. Use SDK for **CI/CD pipelines, automated phases, and gate evaluation** where no human session exists

---

## Sources

- [Agent SDK Overview — Claude API Docs](https://platform.claude.com/docs/en/agent-sdk/overview)
- [Agent SDK Hooks — Claude API Docs](https://platform.claude.com/docs/en/agent-sdk/hooks)
- [Agent SDK Python Reference — Claude API Docs](https://platform.claude.com/docs/en/agent-sdk/python)
- [Agent SDK Custom Tools — Claude API Docs](https://platform.claude.com/docs/en/agent-sdk/custom-tools)
- [How to Implement Tool Use — Claude API Docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/implement-tool-use)
- [Building Agents with the Claude Agent SDK — Claude Blog](https://claude.com/blog/building-agents-with-the-claude-agent-sdk)
- [claude-agent-sdk-typescript — GitHub](https://github.com/anthropics/claude-agent-sdk-typescript)
- [claude_agent_sdk_rust (community) — GitHub](https://github.com/Wally869/claude_agent_sdk_rust)
- [Agent SDK Max Plan Billing Issue #559 — GitHub](https://github.com/anthropics/claude-agent-sdk-python/issues/559)
- [Anthropic API Pricing — Claude API Docs](https://platform.claude.com/docs/en/about-claude/pricing)
- [misanthropy (Rust API bindings) — GitHub](https://github.com/cortesi/misanthropy)
- [anthropic-tools (Rust) — lib.rs](https://lib.rs/crates/anthropic-tools)
- [Laminar: Instrumenting Claude Agent SDK with Rust proxy](https://laminar.sh/blog/2025-12-03-claude-agent-sdk-instrumentation)
