# FINDINGS-EXTERNAL: Unimatrix as Session Host — Programmatic Session Interfaces

**Spike**: ass-066
**Date**: 2026-05-30
**Approach**: investigation + evaluation (exhaustive breadth)
**Confidence**: directional
**Track**: External (ecosystem investigation)

---

## Findings

### Q: What programmatic session interfaces exist for target clients? (Claude Code, Codex CLI, Gemini CLI)

The answer is structured per-client, with exhaustive depth on each.

---

## 1. Claude Code

### 1.1 CLI Print Mode (`claude -p`)

The foundational programmatic interface. Key capabilities:

**Invocation**: `claude -p "prompt"` runs non-interactively, prints response, exits.

**Stdin piping**: Non-interactive mode reads stdin. `cat file | claude -p "analyze"` works. Stdin capped at 10MB as of v2.1.128.

**Output formats**:
- `--output-format text` (default): plain text
- `--output-format json`: structured JSON with `result`, `session_id`, `total_cost_usd`, per-model cost breakdown
- `--output-format stream-json`: newline-delimited JSON events in real-time

**Session management**:
- `--continue` / `-c`: resume most recent conversation
- `--resume <session-id>` / `-r`: resume specific session by ID or name
- `--session-id <uuid>`: use a specific session UUID
- `--fork-session`: create new session ID when resuming (branch a conversation)
- `--no-session-persistence`: ephemeral sessions with no disk persistence
- Session IDs returned in JSON output, capturable for multi-step pipelines:
  ```bash
  session_id=$(claude -p "Start review" --output-format json | jq -r '.session_id')
  claude -p "Continue review" --resume "$session_id"
  ```

**System prompt control**:
- `--system-prompt "text"`: replace the entire default system prompt
- `--system-prompt-file path`: replace from file
- `--append-system-prompt "text"`: append to default prompt
- `--append-system-prompt-file path`: append from file
- All four work in both interactive and non-interactive modes

**MCP server configuration from CLI**:
- `--mcp-config <file-or-json>`: load MCP servers
- `--strict-mcp-config`: only use servers from `--mcp-config`, ignore all other MCP config

**Agent/subagent configuration from CLI**:
- `--agents <json>`: define custom subagents dynamically via JSON
- `--agent <name>`: specify agent for current session

**Permission control**:
- `--allowedTools "Bash,Read,Edit"`: auto-approve specific tools
- `--permission-mode <mode>`: `default`, `acceptEdits`, `plan`, `auto`, `dontAsk`, `bypassPermissions`
- `--permission-prompt-tool <mcp-tool>`: delegate permission decisions to an MCP tool in non-interactive mode
- `--tools "Bash,Edit,Read"`: restrict which tools are available at all
- `--disallowedTools "Bash(rm *)"`: deny specific tool patterns

**Budget and turn limits**:
- `--max-budget-usd 5.00`: spending cap
- `--max-turns 3`: agentic turn limit

**Bare mode** (`--bare`): skip auto-discovery of hooks, skills, plugins, MCP servers, auto memory, CLAUDE.md. Fastest startup. Recommended for scripted calls and will become the default for `-p` in a future release.

**Settings override**: `--settings <file-or-json>` for per-invocation settings.

### 1.2 Streaming JSON Events (`--output-format stream-json`)

The stream-json output is the primary observation channel for programmatic hosts.

**Event types emitted**:

| Event type | Subtype | When it fires | Key fields |
|---|---|---|---|
| `system` | `init` | Session initialization | `session_id`, `tools[]`, `mcp_servers[]`, `model`, `cwd`, `permissionMode`, `apiKeySource` |
| `system` | `api_retry` | API request retry | `attempt`, `max_retries`, `retry_delay_ms`, `error_status`, `error` |
| `system` | `plugin_install` | Plugin install progress | `status`, `name`, `error` |
| `system` | `compact_boundary` | Context compaction occurred | Indicates working memory reset |
| `assistant` | - | Claude generates response | `message` with `content[]` (text blocks, tool_use blocks), `usage`, optional `parent_tool_use_id` |
| `user` | - | Tool results submitted | `message` with `tool_result` content blocks, optional `parent_tool_use_id` |
| `stream_event` | (varies) | Token-by-token streaming (requires `--include-partial-messages`) | `event` field with raw Claude API events: `message_start`, `content_block_start`, `content_block_delta`, `content_block_stop`, `message_delta`, `message_stop` |
| `result` | `success` / `error` | Session completion | `session_id`, `total_cost_usd`, `is_error`, `duration_ms`, `num_turns`, `result`, `usage`, `permission_denials[]` |

**Tool call observation**: Every tool call appears as a `tool_use` content block within `assistant` messages, containing `id`, `name`, and `input`. Tool results appear as `tool_result` content blocks in `user` messages. This provides complete tool call observability.

**Subagent observation**: Messages from within a subagent's context include `parent_tool_use_id`, allowing the host to track which messages belong to which subagent.

**Compaction observation**: `compact_boundary` system messages indicate when context was compacted.

**Flags for maximum observation**:
```bash
claude -p "task" \
  --output-format stream-json \
  --verbose \
  --include-partial-messages \
  --include-hook-events
```

### 1.3 Bidirectional Streaming (`--input-format stream-json`)

The `--input-format stream-json` flag enables bidirectional multi-turn communication over stdin/stdout in a single process invocation. This is the most powerful CLI interface for session hosting.

**Capabilities**:
- Send multiple user messages as NDJSON on stdin
- Receive all events as NDJSON on stdout
- Maintains a persistent session without process restart between turns
- Combined with `--output-format stream-json` for full bidirectional protocol

**Maturity**: Documented only in the CLI flags table. An open issue (anthropics/claude-code#24594) notes usage is "undocumented beyond the CLI flags table." A known bug (anthropics/claude-code#5034) causes duplicate entries in session JSONL files during multi-turn stream-json input. Community wrappers have reverse-engineered parts of the protocol.

**Significance for session hosting**: This is the closest thing to a "session socket" that Claude Code offers at the CLI level. A host process could maintain a long-running `claude -p --input-format stream-json --output-format stream-json --verbose --include-partial-messages` subprocess and inject prompts / observe responses indefinitely.

### 1.4 Claude Agent SDK (Python and TypeScript)

The Agent SDK is the official library for programmatic control. Renamed from "Claude Code SDK" in September 2025, reflecting general-purpose agentic use beyond coding.

**Installation**:
- TypeScript: `npm install @anthropic-ai/claude-agent-sdk` (bundles native Claude Code binary)
- Python: `pip install claude-agent-sdk` (requires Python 3.10+)

**Core API**: The `query()` function is the primary interface, returning an async iterable of messages.

```python
from claude_agent_sdk import query, ClaudeAgentOptions

async for message in query(
    prompt="Find and fix the bug in auth.py",
    options=ClaudeAgentOptions(allowed_tools=["Read", "Edit", "Bash"]),
):
    print(message)
```

**Session management via SDK**:
- Capture `session_id` from `SystemMessage` with `subtype == "init"`
- Resume sessions: `ClaudeAgentOptions(resume=session_id)`
- Fork sessions for branching exploration
- Sessions persist as JSONL on the filesystem

**Tool call observation**: Complete. Every tool call streams through the message iterable:
- `AssistantMessage` contains `content` blocks including `ToolUseBlock` with `name` and `input`
- Tool results flow as `UserMessage` blocks
- With `include_partial_messages=True`, streaming events reveal tool calls in real-time:
  - `content_block_start` with `type: "tool_use"` shows tool name
  - `content_block_delta` with `input_json_delta` streams tool arguments
  - `content_block_stop` signals tool call completion

**Subagent spawning and observation**:
- Define subagents in code via `agents` parameter in `ClaudeAgentOptions`
- `AgentDefinition` fields: `description`, `prompt`, `tools`, `disallowedTools`, `model`, `skills`, `memory`, `mcpServers`, `maxTurns`, `background`, `effort`, `permissionMode`
- Subagents invoked via `Agent` tool (auto-approve with `Agent` in `allowedTools`)
- Messages from subagents include `parent_tool_use_id` for tracking
- Subagents can be resumed by capturing their `agentId` from tool results
- Subagent transcripts persist independently; unaffected by main conversation compaction
- Limitation: subagents cannot spawn their own subagents (single nesting level)
- `background` flag allows non-blocking subagent execution

**Hook system (programmatic callbacks)**:
The SDK provides programmatic hooks as native callback functions (vs. shell commands in settings files):

| Hook Event | Python | TypeScript | What it intercepts |
|---|---|---|---|
| `PreToolUse` | Yes | Yes | Tool calls (can block, modify input, approve) |
| `PostToolUse` | Yes | Yes | Tool results (can inject context, replace output) |
| `PostToolUseFailure` | Yes | Yes | Tool execution failures |
| `PostToolBatch` | No | Yes | Full batch of tool calls resolved |
| `UserPromptSubmit` | Yes | Yes | User prompt before processing |
| `MessageDisplay` | No | Yes | Assistant message text after generation |
| `Stop` | Yes | Yes | Agent stopping |
| `SubagentStart` | Yes | Yes | Subagent initialization |
| `SubagentStop` | Yes | Yes | Subagent completion |
| `PreCompact` | Yes | Yes | Context compaction request |
| `PermissionRequest` | Yes | Yes | Permission dialog |
| `SessionStart` | No | Yes | Session initialization |
| `SessionEnd` | No | Yes | Session termination |
| `Notification` | Yes | Yes | Agent status messages |
| `Setup` | No | Yes | Session setup/maintenance |
| `TeammateIdle` | No | Yes | Teammate becomes idle |
| `TaskCompleted` | No | Yes | Background task completes |
| `ConfigChange` | No | Yes | Config file changes |
| `WorktreeCreate` | No | Yes | Git worktree created |
| `WorktreeRemove` | No | Yes | Git worktree removed |

Hook callbacks receive: `input_data` (typed per event), `tool_use_id` (correlates pre/post), and `context` (includes `AbortSignal` in TypeScript).

Hook outputs can: block operations (`permissionDecision: "deny"`), modify tool inputs (`updatedInput`), inject context (`additionalContext`), replace tool output (`updatedToolOutput`), approve operations (`permissionDecision: "allow"`), or run asynchronously for side-effects (`async: true`).

**Critical finding**: The hooks system provides exactly the interception and control surface a session host needs. A host process using the SDK can observe every tool call (pre and post), every subagent lifecycle event, every compaction event, every session start/stop, and can inject context or block operations at any point.

**MCP server configuration from SDK**:
```python
ClaudeAgentOptions(
    mcp_servers={
        "unimatrix": {"command": "unimatrix-server", "args": ["--mode", "hosted"]}
    }
)
```

**System prompt control from SDK**:
- `systemPrompt`: replace entire system prompt
- `appendSystemPrompt`: append to default prompt
- Loaded from `settingSources` for CLAUDE.md integration

**Additional SDK capabilities**:
- `Workflow` tool (TypeScript SDK v0.3.149+) for orchestrating dozens/hundreds of agents from a script
- `json_schema` for structured output enforcement
- Dynamic agent configuration via factory functions
- Agent teams and teammate coordination
- Plugins support (skills, agents, hooks, MCP servers as packages)

### 1.5 Authentication and Billing

**Authentication methods**:
- `ANTHROPIC_API_KEY` environment variable (primary for SDK/programmatic use)
- Claude subscription (OAuth via browser -- not suitable for headless)
- `claude setup-token`: generates long-lived OAuth token for CI/scripts (requires subscription)
- Amazon Bedrock: `CLAUDE_CODE_USE_BEDROCK=1` + AWS credentials
- Claude Platform on AWS: `CLAUDE_CODE_USE_ANTHROPIC_AWS=1` + workspace ID + AWS credentials
- Google Vertex AI: `CLAUDE_CODE_USE_VERTEX=1` + Google Cloud credentials
- Microsoft Azure: `CLAUDE_CODE_USE_FOUNDRY=1` + Azure credentials
- `apiKeyHelper` in settings for dynamic key injection

**Billing model**:
- API key: standard Anthropic API pricing (pay-per-token)
- Subscription: draws from interactive usage limits
- As of June 15, 2026: Agent SDK and `claude -p` usage on subscription plans draws from a **separate** monthly Agent SDK credit allocation
- `--max-budget-usd` flag for per-invocation spending caps
- `total_cost_usd` in JSON output for cost tracking

**Bare mode authentication**: skips OAuth and keychain reads. Must use `ANTHROPIC_API_KEY` or `apiKeyHelper`.

### 1.6 Maturity Assessment

| Interface | Stability | Documentation | Production-ready |
|---|---|---|---|
| `claude -p` (print mode) | Stable | Comprehensive | Yes |
| `--output-format stream-json` | Stable | Good (event types partially documented) | Yes |
| `--input-format stream-json` | Experimental | Minimal (undocumented beyond flags table) | No -- known bugs |
| Agent SDK (TypeScript) | Stable | Comprehensive | Yes |
| Agent SDK (Python) | Stable, some gaps vs TS | Good | Yes (missing SessionStart/SessionEnd hooks) |
| Hooks (programmatic) | Stable | Comprehensive | Yes |
| Subagent system | Stable | Comprehensive | Yes |

**Key limitation**: the Python SDK does not support `SessionStart` or `SessionEnd` as programmatic callback hooks -- only as shell command hooks loaded from settings files.

---

## 2. Codex CLI (OpenAI)

### 2.1 CLI Non-Interactive Mode (`codex exec`)

**Invocation**: `codex exec "task"` runs non-interactively. Progress streams to stderr; final message to stdout.

**Stdin piping**:
- `curl ... | codex exec "format into markdown table"`: pipe content as context
- `cat prompt.txt | codex exec -`: read entire prompt from stdin
- If both prompt arg and stdin exist, arg becomes instruction, piped content adds context

**Output formats**:
- Default: final message to stdout, progress to stderr
- `--json`: newline-delimited JSON events (one per state change)
- `--output-schema <path>`: structured JSON output conforming to JSON Schema
- `-o / --output-last-message <path>`: write final message to file

**Session management**:
- `codex exec resume --last "new task"`: continue previous run
- `codex exec resume <SESSION_ID>`: resume specific session
- Sessions persist as JSONL rollout files in `~/.codex/sessions`
- `--ephemeral`: skip persisting session files to disk

**Sandbox/permission modes**:
- `--sandbox read-only` (default): no destructive changes
- `--sandbox workspace-write`: write access to workspace
- `--sandbox danger-full-access`: full access (network, cross-machine)
- `--ask-for-approval untrusted|on-request|never`

### 2.2 JSON Event Stream (`--json`)

The `--json` flag provides the observation channel. Event types:

| Event | Description |
|---|---|
| `thread.started` | Session begins |
| `thread.archived` / `thread.unarchived` | Session archival |
| `thread.closed` | Session terminates |
| `turn.started` | Turn begins |
| `turn.completed` | Turn ends (includes `tokenUsage`) |
| `turn.failed` | Turn failed |
| `item.started` | Item processing begins |
| `item.completed` | Item processing ends |
| `item.agentMessage/delta` | Streaming text chunks |
| `item.commandExecution/outputDelta` | Command output streaming |
| `item.thinkingBlock/delta` | Reasoning output |
| `approval/requested` | Tool needs user authorization |
| `mcpServer/startupStatus/updated` | MCP server lifecycle events |

**Item types**: `userMessage`, `agentMessage`, `commandExecution`, `fileEdit`, `functionCall`, `toolResult`, `contextCompaction`, `enteredReviewMode`, `exitedReviewMode`.

**Tool call observation**: Tool calls appear as distinct items (`commandExecution`, `fileEdit`, `functionCall`) in the event stream. Each follows `item.started` -> delta events -> `item.completed` lifecycle.

### 2.3 App-Server Architecture (JSON-RPC 2.0)

Codex's most powerful programmatic interface is the app-server, which uses a JSON-RPC 2.0 bidirectional protocol over stdio, WebSocket, or Unix socket. This is the same protocol the Codex CLI, IDE extensions, and web interface all use to communicate with the agent backend.

**Transport options**:
- stdio (default): newline-delimited JSON
- WebSocket: `--listen ws://IP:PORT` (experimental)
- Unix socket: websocket over control socket

**Handshake**: Client sends `initialize` request, receives capabilities, sends `initialized` notification.

**Thread lifecycle control**:
- `thread/start`: create fresh conversation
- `thread/resume`: reopen by ID
- `thread/fork`: clone history into new thread (supports ephemeral in-memory forks)
- `thread/list`: paginate stored threads with filters
- `thread/compact/start`: trigger manual context compaction

**Turn control**:
- `turn/start`: send user input (text, images, skills, mentions)
- `turn/steer`: append input to active turn
- `turn/interrupt`: cancel in-flight turn
- Per-turn settings overrides: `model`, `effort`, `cwd`, `permissions`, `approvalPolicy`, `outputSchema`

**Approval system**:
- `approval/requested` notifications with `approvalId`, `kind`, `displayText`, `riskLevel`
- Client responds via `approval/respond` with `approved: true|false`
- Supports auto-review mode via subagent-mediated review

**MCP integration**:
- `mcpServer/tool/call`: invoke MCP tools with thread context
- `config/mcpServer/reload`: reload MCP config
- `mcpServerStatus/list`: enumerate configured servers with tools and auth status
- `mcpServer/oauth/login`: OAuth flow for MCP servers

**Process execution**:
- `command/exec`: sandboxed command execution with PTY support
- `process/spawn` (experimental): unsandboxed process on host

**Significance for session hosting**: The app-server protocol is the richest programmatic interface of any agentic CLI. A host process could implement the full JSON-RPC client protocol and have complete control over threads, turns, approvals, MCP servers, and process execution. However, this is an internal protocol -- while documented, it is designed for first-party clients (CLI, IDE extensions), not external hosting.

### 2.4 Codex TypeScript SDK (`@openai/codex-sdk`)

**Installation**: `npm install @openai/codex-sdk` (requires Node.js 18+)

**Core API**:
```typescript
import Codex from "@openai/codex-sdk";
const codex = new Codex();
const thread = codex.startThread();
const result = await thread.run("Fix the CI failures");
```

**Streaming**: `runStreamed()` returns async generator of structured events:
- `item.completed`: processed items (tool calls, messages, file edits)
- `turn.completed`: turn completion with usage data

**Session management**: Threads persist in `~/.codex/sessions`. Resume with `codex.resumeThread(threadId)`.

**Configuration**: JSON objects flattened to dotted paths, serialized as TOML for CLI config override.

### 2.5 Codex as MCP Server

Codex can run as an MCP server: `codex mcp-server`. This exposes two tools to MCP clients:
- `codex`: start a new session with `prompt`, `approval-policy`, `sandbox`, `model`, `cwd`
- `codex-reply`: continue existing session with `prompt` and `threadId`

Designed for integration with the OpenAI Agents SDK, enabling multi-agent orchestration where Codex is a tool invoked by other agents.

### 2.6 Hooks System

Codex CLI has a hooks system with 5 event types (fewer than Claude Code):

| Event | Trigger | Can block? |
|---|---|---|
| `SessionStart` | Session lifecycle | Yes (halt session) |
| `UserPromptSubmit` | User prompt before model | Yes (block prompt) |
| `PreToolUse` | Before tool execution | Yes (deny only, cannot approve) |
| `PostToolUse` | After tool completion | No (can inject context) |
| `Stop` | Agent signals completion | Yes (force continuation) |

Hooks are shell commands configured in `hooks.json` files. JSON wire protocol on stdin/stdout. Exit code 2 signals block/feedback. Fail-open: crashed hooks never silently block.

**Key differences from Claude Code**: fewer events (no `PostToolBatch`, `SubagentStart/Stop`, `PreCompact`, `SessionEnd`), no programmatic callback hooks (shell commands only), `PreToolUse` can only deny (not approve or modify input).

### 2.7 Authentication and Billing

**Authentication methods**:
- ChatGPT sign-in: browser-based OAuth flow
- `CODEX_API_KEY`: API key from OpenAI dashboard (recommended for CI)
- Access tokens: for enterprise CI/CD (ChatGPT Enterprise)
- `codex login --with-api-key`: read API key from stdin
- `codex login --device-auth`: device code flow for remote environments

**Billing**:
- ChatGPT sign-in: uses subscription credits
- API key: standard OpenAI API pricing (pay-as-you-go)
- Access tokens: use ChatGPT Enterprise entitlements

### 2.8 Maturity Assessment

| Interface | Stability | Documentation | Production-ready |
|---|---|---|---|
| `codex exec` (non-interactive) | Stable | Comprehensive | Yes |
| `--json` event stream | Stable | Good | Yes |
| App-server JSON-RPC | Internal/experimental | Detailed README | Not for external use |
| TypeScript SDK | Stable | Minimal | Yes |
| Python SDK | Early | Minimal | Limited |
| MCP server mode | Stable | Good | Yes |
| Hooks | Stable | Good | Yes |

---

## 3. Gemini CLI (Google)

### 3.1 Headless Mode (`gemini -p`)

**Invocation**: `gemini -p "query"` or `gemini --prompt "query"`. Headless mode also activates automatically in non-TTY environments.

**Stdin piping**: `cat file.md | gemini -p "analyze"` works with standard Unix piping.

**Output formats**:
- `--output-format text` (default): plain text response
- `--output-format json`: single JSON object with `response`, `stats`, optional `error`
- `--output-format stream-json` (JSONL): real-time streaming events

**Model selection**: `-m / --model <model>` (e.g., `gemini-2.5-flash`)

**Approval mode**: `-y / --yolo` auto-approves all actions; `--approval-mode auto_edit` for selective auto-approval.

**Exit codes**: 0 (success), 1 (error), 42 (input error), 53 (turn limit exceeded).

### 3.2 Streaming JSON Events

The `--output-format stream-json` flag provides JSONL event streaming:

| Event type | Description |
|---|---|
| `init` | Session metadata: `session_id`, `model` |
| `message` | User and assistant messages with content |
| `tool_use` | Tool call requests with `tool_name`, `tool_id`, parameters |
| `tool_result` | Tool results with status, output, `tool_id` |
| `error` | Non-fatal warnings and system errors |
| `result` | Final outcome with stats: `total_tokens`, `input_tokens`, `output_tokens`, `duration_ms`, `tool_calls` |

**Tool call observation**: `tool_use` and `tool_result` events provide visibility into tool execution. Stats section provides aggregate and per-tool metrics including call counts, success/failure rates, duration, and decision types.

### 3.3 Hooks System

Gemini CLI added hooks in v0.26.0 with 11 lifecycle events -- the most comprehensive event set among CLI agents:

| Event | Description | Can block? |
|---|---|---|
| `SessionStart` | Session begins | Yes |
| `SessionEnd` | Session ends | No (advisory) |
| `BeforeAgent` | After user input, before planning | Yes |
| `AfterAgent` | Agent loop completes | Yes (force retry) |
| `BeforeModel` | Before LLM request | Yes (can mock responses) |
| `AfterModel` | LLM response received | No (filtering/redaction) |
| `BeforeToolSelection` | Before tool selection by LLM | Can filter tools |
| `BeforeTool` | Before tool execution | Yes (validate/block) |
| `AfterTool` | After tool completion | Can hide results |
| `PreCompress` | Before context compression | No (advisory) |
| `Notification` | System notifications | No |

**Unique capabilities**: `BeforeModel` (prompt modification, model swapping, response mocking) and `BeforeToolSelection` (tool filtering) are capabilities neither Claude Code nor Codex offer at the hook level.

**Configuration**: JSON in `settings.json` at project (`.gemini/settings.json`), user (`~/.gemini/settings.json`), or system (`/etc/gemini-cli/settings.json`) levels.

**Environment variables injected**: `GEMINI_PROJECT_DIR`, `GEMINI_SESSION_ID`, `GEMINI_CWD`, `GEMINI_PLANS_DIR`, plus `CLAUDE_PROJECT_DIR` (compatibility alias).

**Matchers**: regex patterns for tool events, exact string for lifecycle events.

**Security**: project-level hooks are fingerprinted; changed hooks treated as untrusted.

### 3.4 SDK and Programmatic API

**Official packages**:
- `@google/gemini-cli`: main CLI distribution (bundled executable)
- `@google/gemini-cli-core`: core API logic (can be used as standalone Node.js module in external projects)
- `@google/gemini-cli-a2a-server`: A2A protocol server for remote programmatic control

**No official SDK for embedding**: Unlike Claude Code's Agent SDK, there is no documented `@google/gemini-cli-sdk` package from Google. The `@google/gemini-cli-core` package contains the `AgentSession`, `GeminiClient`, and tool systems but its API surface is not documented for external use.

**Third-party SDK**: `@ketd/gemini-cli-sdk` (community package) provides a `GeminiClient` for spawning and interacting with Gemini CLI as a subprocess.

**A2A Server**: The `@google/gemini-cli-a2a-server` enables remote programmatic interaction via the Agent-to-Agent (A2A) protocol. This allows external control of an active Gemini session, supporting multi-agent workflows with remote subagents.

**Core internals** (from `@google/gemini-cli-core`):
- `LegacyAgentProtocol` manages the execution loop
- `AgentSession` provides a streamable `AsyncGenerator` interface
- Real-time event translation: raw server events -> standardized `AgentEvent` streams
- Abort support via `AbortController`
- `LocalAgentExecutor` creates isolated registries for sub-agents
- Dynamic MCP server discovery

### 3.5 Session Management

**Session resumption**: Not supported in headless mode. Gemini CLI headless mode is single-shot -- no `--resume` or `--continue` equivalent documented for headless use.

**Interactive sessions**: support `/resume` and session history in interactive mode only.

### 3.6 Authentication and Billing

**Authentication methods**:
- Google account sign-in (browser-based, credentials cached locally)
- `GEMINI_API_KEY` environment variable (best for headless/CI)
- Vertex AI: `GOOGLE_CLOUD_PROJECT` + `GOOGLE_CLOUD_LOCATION` with ADC, service account, or API key
- Google Cloud environments (Cloud Shell, Compute Engine): automatic ADC

**Billing tiers**:
- Free tier: 60 requests/min, 1,000 requests/day with personal Google account (Gemini 2.5 Pro, 1M token context)
- Free API key tier: 15 requests/min (Flash model only), no credit card required
- Paid: standard Gemini API pricing when billing enabled
- **Critical caveat**: enabling billing eliminates free tier on that project -- all calls become billable

**Data privacy**: free tier may use prompts for model training; paid tier governed by enterprise privacy terms.

### 3.7 Maturity Assessment

| Interface | Stability | Documentation | Production-ready |
|---|---|---|---|
| `gemini -p` (headless) | Stable | Good | Yes |
| `--output-format json` | Stable | Good | Yes |
| `--output-format stream-json` | Recent (2026) | Minimal | Early |
| Hooks system | Stable (v0.26.0+) | Good | Yes |
| `@google/gemini-cli-core` (embedding) | Internal | Not documented for external use | No |
| A2A server | Early | Minimal | Experimental |
| Session resumption (headless) | Not available | N/A | No |

---

## Comparative Analysis

### Capability Matrix

| Capability | Claude Code | Codex CLI | Gemini CLI |
|---|---|---|---|
| **Non-interactive mode** | `claude -p` | `codex exec` | `gemini -p` |
| **Streaming events** | `stream-json` (NDJSON) | `--json` (NDJSON) | `stream-json` (NDJSON) |
| **Bidirectional streaming** | `--input-format stream-json` (experimental) | App-server JSON-RPC (internal) | Not available |
| **Official SDK** | Python + TypeScript | TypeScript + Python (early) | Not available (core internals only) |
| **Session resumption (headless)** | Yes (`--resume`, `--continue`) | Yes (`resume --last`, `resume <id>`) | No |
| **Tool call observation** | Complete (stream + SDK hooks) | Complete (JSON stream + app-server) | Complete (stream-json events) |
| **Tool call interception** | Yes (PreToolUse hooks: block, modify, approve) | Partial (PreToolUse: deny only) | Yes (BeforeTool: block, validate) |
| **Tool input modification** | Yes (updatedInput in hooks) | No | Limited |
| **System prompt injection** | Full (replace, append, from file) | Not documented for `exec` | Not documented for headless |
| **MCP server config from CLI** | `--mcp-config` | `codex mcp add` (persistent config) | Not documented for headless |
| **Subagent spawning** | Yes (SDK agents param) | MCP server mode for Agents SDK | A2A protocol (remote subagents) |
| **Subagent observation** | Yes (parent_tool_use_id, SubagentStart/Stop) | Via event stream items | Limited |
| **Context compaction observation** | Yes (compact_boundary event, PreCompact hook) | Yes (contextCompaction item type) | Yes (PreCompress hook) |
| **Session lifecycle hooks** | SessionStart, SessionEnd (TS only) | SessionStart | SessionStart, SessionEnd |
| **Budget/cost control** | `--max-budget-usd`, `total_cost_usd` in output | Not documented | Not documented |
| **Hook event count** | 21 events (TS), 13 events (Python) | 5 events | 11 events |
| **Programmatic hooks (callbacks)** | Yes (Python + TypeScript) | No (shell commands only) | No (shell commands only) |

### Interface Richness Ranking

1. **Claude Code**: The most complete programmatic interface by a significant margin. The Agent SDK provides native Python/TypeScript APIs with typed message objects, programmatic hook callbacks, subagent orchestration, session management with resume, MCP configuration, and streaming. The CLI interface covers the same ground with flags. Bidirectional streaming (`--input-format stream-json`) exists but is experimental.

2. **Codex CLI**: The app-server JSON-RPC protocol is architecturally the richest wire protocol (threads, turns, items, approvals, MCP tool calls, process spawning). However, it is an internal protocol not designed for external hosting. The TypeScript SDK and `codex exec --json` provide the external-facing surface, which is capable but less rich than Claude Code's SDK. Hooks are limited (5 events, deny-only for PreToolUse, no programmatic callbacks).

3. **Gemini CLI**: Good headless mode and hook system (11 events with unique capabilities like BeforeModel). However, the lack of an official embedding SDK, the absence of session resumption in headless mode, and the early-stage streaming JSON support make it the least suitable for session hosting. The A2A server is the most interesting unique capability but is experimental.

---

## Unanswered Questions

1. **`--input-format stream-json` protocol specification**: The bidirectional streaming protocol is not documented. The exact NDJSON message format for sending user messages, the session lifecycle behavior, and error handling are unclear. This matters for session hosting via the CLI path.

2. **Claude Agent SDK compaction callbacks**: while `PreCompact` hooks exist and `compact_boundary` events are emitted, it is unclear whether a host process can intercept and customize compaction behavior (e.g., to inject Unimatrix-summarized context). An open feature request (anthropics/claude-agent-sdk-python#772) suggests this is an area of active development.

3. **Gemini CLI session resumption roadmap**: whether Google plans to add `--resume` for headless mode is unclear. Without it, session hosting requires a long-lived process rather than process-per-turn architecture.

4. **Codex app-server stability contract**: the JSON-RPC protocol is well-documented but marked as internal. Whether OpenAI will stabilize it as a public API is unknown.

---

## Out-of-Scope Discoveries

1. **Claude Code Workflow tool** (TypeScript SDK v0.3.149+): orchestrates many subagents from a script for jobs too large for one conversation. This could be relevant to how Unimatrix orchestrates multi-agent delivery -- warrants investigation if session hosting is pursued.

2. **Codex Cloud** (`codex cloud exec`): OpenAI's managed execution environment for Codex. Runs in cloud with up to 4 attempts. Potentially relevant as a comparison point for Unimatrix-as-runtime.

3. **Claude Code Agent View** (`claude agents`): built-in UI for monitoring and dispatching parallel background sessions. `--json` flag prints live sessions as JSON array. `claude attach <id>` connects to background sessions. This is essentially a lightweight session management UI that already exists within Claude Code -- relevant to understanding what session hosting would replicate vs. complement.

4. **Claude Code Remote Control**: `claude remote-control` starts a server controllable from claude.ai or the Claude app. This is a hosting-adjacent capability where Claude Code itself becomes a remote-controllable agent. Relevant to understanding Anthropic's own vision for hosted agents.

5. **Gemini CLI A2A protocol**: the Agent-to-Agent protocol could allow Unimatrix to act as a remote subagent to Gemini CLI sessions, rather than hosting them. Different architectural direction worth noting.

---

## Recommendations Summary

- **Q1 (Programmatic interfaces)**: Claude Code's Agent SDK (TypeScript/Python) is the clear winner for session hosting. It provides complete tool call observation, programmatic hook callbacks for interception/injection, native subagent orchestration, session management with resume, MCP server configuration, and system prompt control -- all via typed APIs. Codex CLI's app-server JSON-RPC is architecturally powerful but internal. Gemini CLI lacks an official SDK and headless session resumption, making it unsuitable for hosting without significant workaround.
- **Recommended hosting interface**: Use the Claude Agent SDK (TypeScript or Python) as the primary session hosting interface, not `claude -p`. The SDK provides structured message types, programmatic hook callbacks, and native subagent control that the CLI path requires reverse-engineering. For non-Claude clients, fall back to CLI wrapping (`codex exec --json`, `gemini -p --output-format stream-json`) with reduced observation fidelity.
- **Model-agnostic feasibility**: hosting Claude Code sessions is deeply feasible; hosting Codex sessions is architecturally possible but with reduced control; hosting Gemini CLI sessions is feasible for single-shot but not for stateful multi-turn workflows. Full model-agnosticism at the session hosting level is not achievable today without significant abstraction work.
