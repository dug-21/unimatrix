# D4: MCP Integration Guide

**Deliverable**: MCP Integration Guide (Track 2A)
**Date**: 2026-02-20
**Status**: Complete
**Used By**: Track 2B (Context Injection), Track 2C (Config Audit), Track 3 (Interface Spec)

---

## 1. Protocol Overview

MCP (Model Context Protocol) is a JSON-RPC 2.0 based protocol for connecting AI assistants to external tools, resources, and data. Current spec version: **2025-11-25**.

### Three Primitives

| Primitive | Control Model | Purpose | Unimatrix Use |
|-----------|--------------|---------|---------------|
| **Tools** | Model-controlled (Claude decides when to invoke) | Active operations with side effects | Primary interface — `memory_search`, `memory_store`, etc. |
| **Resources** | Application-driven (Claude Code controls) | Passive, read-only context | Potential for project metadata, convention files |
| **Prompts** | User-controlled (explicit invocation) | Templated message sequences | Potential for `/remember`, `/search` slash commands |

---

## 2. JSON-RPC Lifecycle

### Phase 1: Initialization (Mandatory)

```
CLIENT                                        SERVER
  |                                              |
  |--- initialize request ---------------------->|
  |    { protocolVersion, capabilities,          |
  |      clientInfo }                            |
  |                                              |
  |<-- initialize response ----------------------|
  |    { protocolVersion, capabilities,          |
  |      serverInfo, instructions }              |
  |                                              |
  |--- notifications/initialized --------------->|
  |    (no params — signals ready)               |
```

**Key fields in server response:**
- `capabilities.tools` — declares tool support (with optional `listChanged: true`)
- `capabilities.resources` — declares resource support (with optional `subscribe: true`, `listChanged: true`)
- `capabilities.prompts` — declares prompt support
- `capabilities.logging` — declares structured logging
- **`instructions`** — free-form string guidance for the LLM about how to use this server

**Critical**: The `instructions` field in the initialize response is a direct channel to influence Claude's behavior. This is separate from individual tool descriptions and applies server-wide.

### Phase 2: Discovery

After initialization, Claude Code fetches available primitives:

```
CLIENT                                        SERVER
  |--- tools/list ------------------------------>|
  |<-- { tools: [...] } -------------------------|
  |                                              |
  |--- resources/list --------------------------->|
  |<-- { resources: [...] } ---------------------|
  |                                              |
  |--- prompts/list ----------------------------->|
  |<-- { prompts: [...] } -----------------------|
```

Tool discovery happens **once at connection**, not per-conversation. Servers can push updates via `notifications/tools/list_changed`.

### Phase 3: Operation

```
CLIENT                                        SERVER
  |                                              |
  | [Claude decides to use a tool]               |
  |--- tools/call { name, arguments } ---------->|
  |<-- { content: [...], isError?: bool } -------|
  |                                              |
  | [Server needs LLM help — sampling]           |
  |<-- sampling/createMessage -------------------|
  |--- sampling response ----------------------->|
  |                                              |
  | [Server needs user input — elicitation]      |
  |<-- elicitation/create ---------------------->|
  |--- elicitation response -------------------->|
```

### Phase 4: Shutdown

Transport-dependent:
- **stdio**: Client closes stdin, waits for exit, SIGTERM, then SIGKILL
- **HTTP**: Client sends HTTP DELETE with session ID

---

## 3. Transport: stdio (Our Target)

For Unimatrix, **stdio is the primary transport**. Claude Code launches the MCP server as a subprocess.

### How It Works
- Server reads JSON-RPC messages from **stdin** (newline-delimited)
- Server writes responses to **stdout** (newline-delimited)
- Messages MUST NOT contain embedded newlines
- Server MAY write to **stderr** for logging (not protocol messages)

### Configuration in Claude Code

```bash
# Add a stdio MCP server
claude mcp add --transport stdio unimatrix -- /path/to/unimatrix-server

# Or with environment variables
claude mcp add --transport stdio unimatrix --env UNIMATRIX_PROJECT=/path/to/project -- unimatrix-server
```

### Tradeoffs
- Simplest transport; no networking needed
- One client per server process (acceptable for local-first)
- Optimal performance (no HTTP overhead)
- Claude Code recommends stdio for local tools

### Other Transports (Future)

| Transport | Use Case | Status |
|-----------|----------|--------|
| Streamable HTTP | Remote/cloud deployment | Available, not needed for v0.1-v0.3 |
| SSE | Legacy | Deprecated, don't implement |

---

## 4. Tool Definition Schema

### Complete Tool Structure

```json
{
  "name": "memory_search",
  "title": "Search Memory",
  "description": "Search project memory for relevant knowledge. Use this before starting tasks to find existing conventions, patterns, decisions, and solutions. Returns entries ranked by relevance.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "Natural language search query describing what you're looking for"
      },
      "k": {
        "type": "integer",
        "description": "Maximum results to return (default: 10)"
      }
    },
    "required": ["query"]
  },
  "annotations": {
    "readOnlyHint": true,
    "destructiveHint": false,
    "openWorldHint": false
  }
}
```

### Tool Annotations (Behavioral Hints)

| Annotation | Default | Meaning | Unimatrix Recommendation |
|------------|---------|---------|--------------------------|
| `readOnlyHint` | `false` | Tool doesn't modify environment | `true` for search/list/get; `false` for store/delete/correct |
| `destructiveHint` | `true` | Tool may destructively modify | `false` for store; `true` for delete/correct |
| `idempotentHint` | `false` | Repeated calls same result | `true` for search/get; `false` for store |
| `openWorldHint` | `true` | Interacts with external world | `false` for all Unimatrix tools (closed memory system) |

**Why annotations matter**: Claude Code uses these to determine permission prompting behavior. A `readOnlyHint: true` tool may be auto-approved; a `destructiveHint: true` tool may require user confirmation.

### Tool Response Format

Responses contain a `content` array with typed entries:

```json
{
  "content": [
    {
      "type": "text",
      "text": "## Search Results\n\n### 1. Database Connection Pattern (similarity: 0.92)\n..."
    }
  ],
  "isError": false
}
```

**Supported content types**: `text`, `image`, `audio`, `resource_link`, `resource` (embedded)

**Structured output** (optional, via `outputSchema`):
```json
{
  "content": [{ "type": "text", "text": "Found 3 results..." }],
  "structuredContent": {
    "results": [...],
    "total_found": 3
  }
}
```

---

## 5. Token Cost and Tool Search

### The Problem

Every tool description consumes tokens in Claude's context window. With many tools, this competes for context budget.

### Claude Code's Solution: MCP Tool Search

When tool descriptions exceed **10% of context window**, Claude Code automatically activates Tool Search:
- Tools are **deferred** instead of preloaded
- Claude uses a search tool to discover relevant tools on-demand
- Only tools Claude actually needs are loaded into context

**Configuration:**
```bash
ENABLE_TOOL_SEARCH=auto          # Default: auto-enable at 10% threshold
ENABLE_TOOL_SEARCH=auto:5        # Custom 5% threshold
ENABLE_TOOL_SEARCH=true          # Always enabled
ENABLE_TOOL_SEARCH=false         # Disabled
```

### Implication for Unimatrix

With ~5-15 tools planned (v0.1 through v0.5), Unimatrix alone won't trigger Tool Search. But users may have multiple MCP servers. Design tool descriptions to be:
- **Concise** — minimize token cost
- **Distinctive** — clearly differentiate each tool's purpose
- **Self-documenting** — the description IS the prompt engineering for when Claude invokes the tool

### Response Size Limits

| Threshold | Behavior |
|-----------|----------|
| 10,000 tokens | Warning displayed |
| 25,000 tokens (default max) | Truncated |
| Configurable via `MAX_MCP_OUTPUT_TOKENS` | User can increase |

**Design implication**: `memory_search` responses should stay under 10K tokens. Use `max_tokens` parameter to let Claude control response budget.

---

## 6. Resource Support in Claude Code

### How Resources Work

Resources are **passive context** — Claude Code (not Claude) decides how to use them.

**User references resources via `@` mentions:**
```
Can you check @unimatrix:memory://project/conventions and follow them?
```

Resources appear in autocomplete when typing `@`.

### Resource Definition

```json
{
  "uri": "memory://project/conventions",
  "name": "Project Conventions",
  "description": "Active coding conventions and patterns for this project",
  "mimeType": "text/markdown"
}
```

### Resource Templates

Dynamic resources using URI templates (RFC 6570):
```json
{
  "uriTemplate": "memory://search/{query}",
  "name": "Memory Search",
  "description": "Search project memory"
}
```

### Subscriptions

If server declares `subscribe: true`, clients can subscribe to resource changes:
- Client sends `resources/subscribe` with URI
- Server sends `notifications/resources/updated` when resource changes
- Client re-reads the resource

### Unimatrix Resource Opportunities

| Resource | URI Pattern | Use Case |
|----------|-------------|----------|
| Project conventions | `memory://conventions` | Passive context injection of active conventions |
| Recent decisions | `memory://recent` | Auto-include recent architectural decisions |
| Project status | `memory://status` | Summary of project learning state |

**Key question for Track 2B**: Do resources provide better context injection than tool responses? Resources are application-driven (Claude Code decides to include them), while tools are model-driven (Claude decides to call them).

---

## 7. Error Handling Design

### Two Error Levels

**Protocol errors** (JSON-RPC level — structural failures):
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "error": {
    "code": -32602,
    "message": "Unknown tool: memory_search_typo"
  }
}
```

**Tool execution errors** (application level — actionable failures):
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "No project initialized. Run memory_init first to set up project memory."
      }
    ],
    "isError": true
  }
}
```

### Key Distinction

| Error Type | When | Claude Can Self-Correct? |
|------------|------|--------------------------|
| Protocol Error | Malformed request, unknown tool | No (structural) |
| Tool Execution Error | Bad input, missing project, rate limit | **Yes** (actionable feedback) |

**Design principle**: Always return tool execution errors (not protocol errors) for application-level failures. Include actionable guidance in the error message so Claude can self-correct.

### Error Codes

| Code | Meaning |
|------|---------|
| `-32700` | Parse error (invalid JSON) |
| `-32600` | Invalid request |
| `-32601` | Method not found |
| `-32602` | Invalid params / unknown tool |
| `-32603` | Internal error |

### Claude Code Retry Behavior

- **Claude Code does NOT automatically retry failed tool calls**
- Tool execution errors are passed to Claude as context
- Claude decides whether to retry based on the error message
- Design error messages to guide Claude's next action

---

## 8. Proactive Tool Use

### The Core Challenge

Claude uses tools when it decides to, based on tool descriptions and conversation context. There is no MCP mechanism to force proactive tool invocation.

### What Works

| Mechanism | Reliability | Where |
|-----------|-------------|-------|
| **CLAUDE.md instructions** | High (always loaded) | Project root |
| **Server `instructions` field** | Medium (loaded at init) | Initialize response |
| **Tool descriptions** | Medium (influences selection, not timing) | Tool definition |
| **MCP Prompts** | High but user-invoked | Prompt templates |
| **.claude/rules/** | Medium (conditional on file context) | Glob-matched rules |

### What Does NOT Work

- Tool description text like "Always use this tool first" is **not guaranteed** to be followed
- Tool annotations/descriptions are treated as untrusted hints
- No MCP mechanism exists to inject tools into the conversation flow without Claude's decision

### Recommended Strategy (To Be Validated in Track 2B/2C)

1. **CLAUDE.md append** (5-10 lines): "Before starting any task, search Unimatrix memory for relevant context"
2. **Server `instructions`**: Reinforce the search-first pattern
3. **Tool descriptions**: Make it clear when each tool should be used
4. **MCP Prompts**: Provide `/remember` and `/recall` for explicit user invocation

---

## 9. Rust MCP SDK Evaluation

### Recommendation: Use `rmcp` (Official SDK)

| Attribute | Value |
|-----------|-------|
| Crate | `rmcp` |
| Version | 0.16.0 (Feb 2026) |
| Downloads | ~1.14M/month |
| Contributors | 139+ |
| Stars | ~3,000 |
| License | Apache-2.0 |
| Async runtime | Tokio 1.x |

### Why `rmcp` Over Alternatives

| Option | Verdict | Reason |
|--------|---------|--------|
| **`rmcp`** | **Recommended** | Official, 100x adoption of nearest competitor, Tokio-native, proc macros |
| `rust-mcp-sdk` | Not recommended | 11.5K dl/mo, "use at own risk", heavier deps |
| `mcpkit` | Not recommended | 2 stars, single contributor, zero production validation |
| DIY (~750 LoC) | Not recommended | Must track evolving spec manually, no proc macros |

### Minimal Cargo.toml

```toml
[dependencies]
rmcp = { version = "0.16", features = ["server", "transport-io", "macros"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "1.0"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
```

**Feature flags**: `server` + `transport-io` + `macros` gives us stdio transport and tool definition macros without pulling in HTTP clients, OAuth, or web frameworks.

### Minimal Server Example

```rust
use rmcp::{tool, ServerHandler, ServiceExt, transport::stdio};
use serde::Deserialize;
use schemars::JsonSchema;

#[derive(Clone)]
struct UnimatrixServer;

#[derive(Deserialize, JsonSchema)]
struct SearchArgs {
    query: String,
    k: Option<i64>,
}

#[tool_router]
impl UnimatrixServer {
    #[tool(description = "Search project memory for relevant knowledge")]
    async fn memory_search(
        &self,
        #[tool(aggr)] args: SearchArgs,
    ) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::text(format!("Results for: {}", args.query)))
    }
}

#[tool_handler]
impl ServerHandler for UnimatrixServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            name: "unimatrix".into(),
            version: "0.1.0".into(),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = UnimatrixServer.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

### Risk Mitigation

| Risk | Mitigation |
|------|------------|
| 15 breaking changes in 56 releases | Pin to exact version (`rmcp = "=0.16.0"`) during dev |
| 35% doc coverage | Examples directory is comprehensive (10+ server examples) |
| Evolving protocol | SDK tracks spec changes; easier than DIY tracking |

---

## 10. MCP Sampling and Elicitation

### Sampling (Server-Initiated LLM Requests)

Servers can request Claude to generate text via `sampling/createMessage`. This enables agentic behaviors within server operations.

**Flow:**
```
Server → Claude Code: sampling/createMessage request
Claude Code → User: Present for approval
User → Claude Code: Approve
Claude Code → Claude: Generate
Claude → Claude Code: Response
Claude Code → Server: Return result
```

**Unimatrix potential**: A future version could use sampling to have Claude summarize or categorize stored memories, generate corrections, or produce session summaries — all server-side without requiring tool calls.

**Constraint**: Human-in-the-loop approval required. Not suitable for automated/silent operations.

### Elicitation (Server-Initiated User Input)

New in 2025-11-25. Servers can request user input via forms or URL redirects.

**Unimatrix potential**: Could use form-mode elicitation for project setup (select project, configure settings) or URL-mode for dashboard links.

---

## 11. Multiple MCP Servers

Claude Code supports multiple simultaneous MCP servers. Each gets a dedicated client connection.

```
Claude Code (MCP Host)
├── MCP Client 1 → Unimatrix (memory)
├── MCP Client 2 → GitHub (issues, PRs)
├── MCP Client 3 → Sentry (error tracking)
└── MCP Client 4 → Database (queries)
```

**Tool naming**: Tools are namespaced by server. In Claude's context, they appear as `mcp__unimatrix__memory_search`. No conflicts across servers.

**Implication**: Unimatrix tool names should be clear and distinctive. Prefix with `memory_` to avoid confusion with tools from other servers.

---

## 12. Key Design Decisions for Track 3

Based on this research, these decisions should inform the interface specification:

### Confirmed

1. **Use `rmcp` SDK** — official, mature, Tokio-native
2. **stdio transport** for v0.1-v0.3 — simplest, no networking needed
3. **Tool execution errors** (not protocol errors) for application failures — enables Claude self-correction
4. **Tool annotations** on every tool — drives permission behavior in Claude Code
5. **`instructions` field** in initialize response — free channel to influence Claude's behavior
6. **Response budget** via `max_tokens` parameter — keep under 10K tokens

### Open Questions for Track 2B

1. Does markdown or JSON response format get better utilization from Claude?
2. How many results should `memory_search` return by default?
3. Should responses include "guidance" text (e.g., "Based on these conventions, you should...") or just raw data?
4. Do resources work better than tools for passive context injection?

### Open Questions for Track 2C

1. Can the server `instructions` field alone drive "search memory before starting work" behavior?
2. What CLAUDE.md text most reliably drives proactive memory usage?
3. Do `.claude/rules/` files fire reliably for memory-before-code patterns?
4. Can a `unimatrix init` command generate sufficient config for reliable behavior?

---

## Sources

- [MCP Specification 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25)
- [MCP Lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle)
- [MCP Transports](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
- [MCP Tools](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
- [MCP Resources](https://modelcontextprotocol.io/specification/2025-11-25/server/resources)
- [MCP Prompts](https://modelcontextprotocol.io/specification/2025-11-25/server/prompts)
- [MCP Sampling](https://modelcontextprotocol.io/specification/2025-11-25/client/sampling)
- [MCP Elicitation](https://modelcontextprotocol.io/specification/2025-11-25/client/elicitation)
- [Claude Code MCP Documentation](https://code.claude.com/docs/en/mcp)
- [Official Rust SDK (rmcp)](https://github.com/modelcontextprotocol/rust-sdk)
- [rmcp on docs.rs](https://docs.rs/rmcp/latest/rmcp/)
- [MCP TypeScript Schema](https://github.com/modelcontextprotocol/specification/blob/main/schema/2025-11-25/schema.ts)
