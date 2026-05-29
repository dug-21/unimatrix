# FINDINGS: Multi-LLM Routing & Workflow Engine Landscape (R4)

**Spike**: ass-063
**Date**: 2026-05-29
**Approach**: investigation
**Confidence**: validated

---

## Findings

### Q: RQ-5 — Multi-LLM Routing: Technical feasibility of dispatching workflow steps to different LLM providers. What does the interface look like per provider? How does artifact and context state transfer between providers? What's the minimum viable routing model?

**Answer**: Multi-LLM routing is technically feasible today using existing provider tooling. All three major CLI agents (Claude Code, Codex CLI, Gemini CLI) support programmatic non-interactive invocation with JSON output, working directory specification, and MCP server connections. The minimum viable routing model is **passive guidance with filesystem-mediated state** -- Unimatrix does not need to initiate LLM sessions directly; it provides workflow state via MCP tools that the current LLM reads, and a thin coordinator script dispatches the next step to whichever provider is appropriate.

**Evidence**: Primary source documentation fetched and analyzed 2026-05-29.

---

#### Provider Interface Survey

##### Claude Code / Claude Agent SDK

| Capability | Interface |
|---|---|
| Non-interactive CLI | `claude -p "task" --allowedTools "Read,Edit,Bash"` |
| JSON output | `--output-format json` (includes `session_id`, `total_cost_usd`) |
| Streaming | `--output-format stream-json --verbose --include-partial-messages` |
| Working directory | Runs in cwd; `--bare` skips all auto-discovery |
| MCP servers | `--mcp-config <file-or-json>` in bare mode |
| Session resume | `--continue` or `--resume <session_id>` |
| System prompt control | `--append-system-prompt`, `--system-prompt` |
| SDK (Python/TypeScript) | `claude_agent_sdk.query(prompt=..., options=...)` -- async iterator |
| Subagents | `AgentDefinition` with per-agent model, tools, MCP servers, maxTurns |
| Structured output | `--json-schema` for typed responses |

**Key constraints**: Subagents cannot spawn their own subagents (depth = 1 from SDK). Bare mode is recommended for scripted/SDK calls.

**MCP integration**: First-class. MCP servers can be passed per-agent via `mcpServers` in `AgentDefinition`. A Unimatrix MCP server can be connected to any Claude Code invocation.

##### OpenAI Codex CLI

| Capability | Interface |
|---|---|
| Non-interactive CLI | `codex exec "task"` (aliased as `codex e`) |
| JSON output | `--json` -- newline-delimited JSON events |
| Working directory | `--cd <path>` or `-C <path>` |
| MCP servers | `codex mcp add <name>` or `[mcp_servers.<name>]` in `config.toml` |
| Session resume | `codex exec resume [SESSION_ID]` with optional follow-up |
| Sandbox modes | `--sandbox read-only|workspace-write|danger-full-access` |
| Approval control | `--ask-for-approval never` for fully automated runs |
| SDK (TypeScript) | Production-ready; `thread.run("task")` |
| App server | `codex app-server --listen stdio://` for JSONL-over-stdio or WebSocket |
| MCP server mode | `codex mcp-server` -- Codex itself as an MCP server over stdio |

**MCP support**: Full. STDIO and Streamable HTTP transports. Per-server environment variables, OAuth, bearer token auth. Tool-level approval modes.

**Notable capability**: `codex mcp-server` allows Codex to run as an MCP server itself, enabling agent-to-agent integration.

##### Google Gemini CLI

| Capability | Interface |
|---|---|
| Non-interactive CLI | `gemini -p "task"` |
| JSON output | `--output-format json` (single JSON object with response + stats) |
| Streaming | `--output-format stream-json` (JSONL events) |
| Working directory | Runs in cwd; `--include-directories ../lib,../docs` for multi-dir |
| MCP servers | `~/.gemini/settings.json` mcpServers config |
| Model selection | `-m gemini-2.5-flash` |

**Current state**: v0.44.1 (May 28, 2026). Active development, 105k GitHub stars.

**Key limitation**: No programmatic SDK equivalent. Programmatic control is CLI-only (pipe in/pipe out). No session resume capability documented.

##### Other Notable Tools

| Tool | MCP Support | Programmatic Dispatch | Notes |
|---|---|---|---|
| Cursor | Yes | CLI for agent runs | Agent Tabs for parallel dispatch |
| Aider | No native MCP | CLI-based; connects to any LLM | Multi-LLM support |
| Continue | Yes (MCP client) | IDE extension only | Open-source; any model provider |

---

#### Context Transfer Between Providers

| State Type | Transfer Mechanism | Feasibility |
|---|---|---|
| Files changed | Git working tree (filesystem) | Trivial -- all CLIs operate on filesystem |
| Decisions made | Unimatrix graph (via MCP) | Feasible -- store as workflow step completion entries |
| Artifacts produced | Filesystem + Unimatrix metadata | Feasible -- files on disk, metadata via MCP |
| Conversation history | Not transferable | Cannot transfer between providers |
| In-memory reasoning | Not transferable | Lost at session boundary |

**What works today**:

1. **Filesystem as shared state**: All three CLIs operate on the same filesystem (git working tree). File changes, test results, and build artifacts transfer naturally.

2. **MCP as context provider**: A Unimatrix MCP server connected to any CLI agent can expose current workflow step and requirements, results from previous steps, and project knowledge.

3. **Structured handoff files**: The industry pattern is "living specifications" -- persistent, machine-readable files that survive context window boundaries. Research shows blackboard architectures (shared state objects) show "13% to 57% improvement in end-to-end task success" vs alternatives.

**What does NOT transfer**: Conversation history (provider-specific), in-memory reasoning (lost at session boundary), tool execution state (per-session).

---

#### MCP as Coordination Layer

**Current MCP specification status**: Version 2025-11-25 (stable). Release candidate 2026-07-28 locked May 21, 2026.

**Critical finding -- Sampling is deprecated**: MCP Sampling (the mechanism by which an MCP server could request the connected LLM to generate a completion) is deprecated in the 2026-07-28 release candidate (SEP-2577). Rationale: "low adoption relative to implementation complexity." Implications:

- An MCP server **cannot** ask the LLM to do work. It can only respond to requests from the LLM.
- Unimatrix as MCP server cannot initiate work. It can only provide information when the LLM asks.
- The "active dispatch" model (Unimatrix tells LLM what to do) is **not supported** through MCP alone.

**What MCP CAN do for coordination**:

| MCP Primitive | Coordination Use | Status |
|---|---|---|
| Tools | `workflow_next`, `workflow_complete_step`, `workflow_status` | Stable |
| Resources | Expose workflow definitions, step requirements, prior step results | Stable |
| Elicitation | Server asks user for structured input -- usable for approval gates | Restructured in 2026-07-28 RC |
| Sampling | Server requests LLM completion | **Deprecated** |
| MCP Apps | Server-rendered interactive HTML UIs in sandboxed iframes | New in 2026-07-28 RC |

**Server-initiated request constraint**: "Server-initiated requests may now only be issued while the server is actively processing a client request." Unimatrix cannot push notifications to the LLM out-of-band.

---

#### Minimum Viable Routing Model

**Recommended: Passive Guidance + Thin Coordinator**

Three components:

1. **Unimatrix as MCP server** (existing role, extended): Exposes workflow state via tools. Any LLM connected via MCP can query for its current assignment and context.

2. **Thin coordinator script** (new, minimal): A shell script or small Rust binary (~100-200 lines) that reads the workflow definition from Unimatrix, for each pending step invokes the appropriate CLI agent (`claude -p`, `codex exec`, or `gemini -p`), passes the Unimatrix MCP server connection to each invocation, captures JSON output and exit codes, and reports step completion back to Unimatrix.

3. **System prompt per step** (generated by Unimatrix): Each dispatched agent receives: "You are executing workflow step N. Call `workflow_next` to get your task. When done, call `workflow_complete_step` with your results."

| Concern | How addressed |
|---|---|
| Does Unimatrix initiate LLM sessions? | No. The thin coordinator does. Unimatrix provides data. |
| Does it require MCP Sampling? | No. Uses only tools (request-response from LLM). |
| Does it work with all three providers? | Yes. All support `-p` + MCP config or equivalent. |
| How does context transfer? | Filesystem (git) + Unimatrix MCP tools. |
| How are gates enforced? | Unimatrix `workflow_next` refuses to advance until gate conditions met. |
| What about failures? | Coordinator checks exit code; Unimatrix records failure; step can be retried. |

---

### Q: How do existing workflow engines and agentic frameworks handle step dispatch, gating, and state management?

**Answer**: Five distinct architecture families exist, each with patterns directly relevant to Unimatrix. The most applicable are Temporal's durable execution model (for reliability guarantees) and the living-specification pattern from multi-agent orchestration research (for context transfer).

#### Framework-by-Framework Analysis

##### LangGraph (LangChain)
- **Architecture**: Python framework using directed graphs. Nodes are functions; edges define transitions.
- **State management**: Explicit, reducer-driven state schemas. Built-in checkpointing for persistence and time-travel.
- **Relevance**: Closest analog to what Unimatrix's typed graph could become. Key difference: LangGraph graphs defined in code, Unimatrix would define in knowledge graph.

##### CrewAI
- **Architecture**: Hierarchical delegation. Manager agent plans; worker agents execute. Two tiers: Crews + Flows.
- **Relevance**: Crew/Flow two-tier model maps to workflow/step hierarchy. Hub-and-spoke pattern matches passive-guidance model.

##### Microsoft AutoGen / AG2
- **Architecture**: Event-driven core. GroupChat for coordination.
- **Relevance**: Limited. AutoGen in maintenance mode. Tightly coupled to Microsoft ecosystem.

##### Temporal.io
- **Architecture**: Durable workflow execution. Workflows = deterministic orchestration code. Activities = non-deterministic work units.
- **State management**: Event History records all decisions and results. On failure, replays workflow to last checkpoint.
- **Relevance**: Gold standard for reliable workflow execution. "Deterministic orchestrator + non-deterministic activities" maps directly to "Unimatrix workflow graph + LLM step execution."
- **Key lesson**: Activities must be idempotent.

##### Prefect
- **Architecture**: Python-native. Follows Python control flow. No precompiled DAGs.
- **Gating**: First-party human-in-the-loop. Type-safe human input through auto-generated UI forms.
- **Relevance**: Human-in-the-loop approach directly applicable to workflow gates.

##### n8n
- **Architecture**: Visual workflow automation. Self-hostable.
- **MCP integration**: Native via MCP Server Trigger and MCP Client Tool. Validates "expose workflow tools via MCP" approach.

#### Cross-Framework Pattern Synthesis

**Patterns applicable to Unimatrix**:

1. **Deterministic orchestrator + non-deterministic workers** (Temporal): Unimatrix workflow graph = orchestrator. LLM agents = workers.
2. **Living specification as coordination artifact**: Maintain shared state (graph) that each agent reads and updates.
3. **Pull-based task assignment** (MCP-compatible): Agent asks "what's next?" rather than being pushed.
4. **State as blackboard**: Shared state object that agents read/write. 13-57% improvement in task success vs alternatives.
5. **Wave-based parallel execution**: Tasks at same dependency level run simultaneously.
6. **Schema validation gates**: Enforce step outputs match expected structure before downstream steps proceed.
7. **Human-in-the-loop as first-class primitive**: Gates that pause execution for human approval.

**Anti-patterns to avoid**:
1. **Precompiled rigid DAGs**: Agentic workflows need dynamic routing. Support conditional edges.
2. **Conversation-based state transfer**: Fragile, provider-specific, token-expensive.
3. **Peer-to-peer agent communication**: "Coordination failures account for 36.94% of all failures across AutoGen, CrewAI, and LangGraph."

---

## Unanswered Questions

1. **Claude Agent SDK Dispatch**: Detailed API documentation for queue-based dispatch not fully available.
2. **Codex CLI MCP server mode**: Exact tool surface exposed not fully documented.
3. **Cross-provider token cost comparison**: Needed for intelligent routing but requires pricing analysis outside this scope.
4. **Gemini CLI session persistence**: No session resume capability documented.

---

## Out-of-Scope Discoveries

1. **MCP Apps (2026-07-28 RC)**: Servers can ship interactive HTML UIs in sandboxed iframes. Directly relevant to RQ-6 — Unimatrix could serve workflow visualization as an MCP App rather than bundling a standalone web UI.
2. **Anthropic Programmatic Tool Calling**: Claude writes Python code that calls tools within sandboxed containers, reducing API round-trips. Relevant to token reduction and subagent bypass.
3. **Anthropic Managed Agents**: Hosted REST API. Alternative to CLI dispatch for Claude steps.
4. **n8n bidirectional MCP integration**: Could serve as visual workflow editor dispatching to Unimatrix-managed agents.

---

## Recommendations Summary

- **RQ-5 (Multi-LLM Routing)**: Feasible today. Use passive guidance model -- Unimatrix provides workflow state via MCP tools; thin coordinator script (~200 lines) dispatches steps to `claude -p`, `codex exec`, or `gemini -p` with MCP server connection. Filesystem + Unimatrix graph entries handle context transfer. Do not depend on MCP Sampling (deprecated). Do not build active dispatch initially.
- **RQ-5 (Minimum Viable Model)**: Passive guidance + thin coordinator. Unimatrix does NOT initiate LLM sessions. Unimatrix informs; coordinator dispatches; LLM executes. Zero custom provider integration required.
- **Workflow Engine Patterns**: Adopt Temporal's "deterministic orchestrator + non-deterministic workers" pattern. Use pull-based task assignment. Implement schema validation gates. Support dynamic routing. Make human-in-the-loop gates first-class. Avoid peer-to-peer agent communication.
