# ASS-002: Notable Findings

Raw insights and observations captured during Track 2A research that may inform later tracks or design decisions.

---

## 1. Server Instructions — An Underappreciated Channel

The MCP `initialize` response includes an `instructions` field: a free-form string that the client/LLM receives at connection time. This is distinct from individual tool descriptions and could serve as Unimatrix's primary behavioral injection point.

**Implication**: Before resorting to CLAUDE.md modifications or .claude/rules/, test whether `instructions` alone can drive "search memory before starting work" behavior. If it can, the setup story becomes literally just `claude mcp add unimatrix`.

**Needs validation in Track 2B/2C.**

---

## 2. Tool Annotations Drive Permission Behavior

Claude Code uses tool annotations (`readOnlyHint`, `destructiveHint`, etc.) to determine whether to auto-approve tool calls or prompt the user. This is not just metadata — it directly affects UX.

**Design decisions:**
- `memory_search`, `memory_get`, `memory_list`: `readOnlyHint: true` → auto-approved (no friction for reads)
- `memory_store`: `readOnlyHint: false, destructiveHint: false` → prompt but non-scary
- `memory_delete`: `readOnlyHint: false, destructiveHint: true` → prompt with warning
- All Unimatrix tools: `openWorldHint: false` → signals closed system (no external API calls)

**This annotation strategy is part of the interface design, not an afterthought.**

---

## 3. Structured Output (`outputSchema`) Is New

The 2025-11-25 spec added `outputSchema` and `structuredContent` to tool definitions. This allows tools to return both human-readable text content AND machine-parseable structured data.

```json
{
  "content": [{ "type": "text", "text": "Found 3 relevant memories..." }],
  "structuredContent": {
    "results": [
      { "id": "abc", "similarity": 0.95, "content": "..." }
    ],
    "total_found": 3
  }
}
```

**Implication**: Unimatrix tools can return formatted markdown for Claude's consumption AND structured JSON for programmatic use. This is the best of both worlds — Claude gets readable context while any future UI/client gets typed data.

**Need to verify**: Does `rmcp` 0.16 support `outputSchema`? The feature is recent.

---

## 4. MCP Tool Search Changes the Game

When tool descriptions exceed 10% of context window, Claude Code defers tools and uses search to find them on-demand. This means:

- Unimatrix tool descriptions **must be search-friendly** (clear names and descriptions that match natural queries)
- Server-level `instructions` become more important for discoverability
- If a user has many MCP servers, our tools might not be visible until searched for

**Counter-consideration**: With ~5-15 Unimatrix tools, we likely won't trigger Tool Search alone. But combined with other MCP servers, we might.

---

## 5. Resources Could Enable Passive Context Injection

Resources are application-driven (Claude Code decides to include them). A resource like `memory://conventions` could be auto-included in every conversation via Claude Code's `@` mention system.

**However**: Resources are NOT auto-loaded. A user would need to type `@unimatrix:memory://conventions` each time, which is worse UX than a proactive tool call.

**Alternative**: Use `resources/subscribe` to push convention updates, but this still requires Claude Code to decide what to do with updates.

**Verdict**: Resources are supplementary, not primary. Tools remain the main interface for memory operations. Resources might work for a "project dashboard" or "convention summary" that users can optionally reference.

---

## 6. Sampling Enables Server-Side Intelligence

MCP sampling lets the server ask Claude to generate text. Future Unimatrix uses:

- **Automatic categorization**: Store a memory, server asks Claude to categorize it
- **Summarization**: Server asks Claude to summarize a session's memories
- **Conflict detection**: Server asks Claude to compare new memory against existing ones

**Constraint**: Requires human-in-the-loop approval per the spec. Not suitable for silent background operations unless Claude Code relaxes this requirement.

**Not for v0.1-v0.3, but architecturally significant.**

---

## 7. rmcp Version Stability Concern

15 breaking changes across 56 releases is a lot. The crate is still pre-1.0 (0.16.0). However:

- The core server API has stabilized around `ServerHandler` + `#[tool]` macros
- Breaking changes are mostly additive (new protocol features), not destructive
- Pinning to `"=0.16.0"` during development mitigates churn
- The 1.14M downloads/month and 139 contributors provide confidence in continued maintenance

**Risk**: If `rmcp` makes a breaking change to the `ServerHandler` trait or `#[tool]` macro during our development, we'd need to adapt. Pin the version and update deliberately.

---

## 8. Claude Code Namespaces Tools by Server

In Claude's context, tools appear as `mcp__<server>__<tool>`. Example: `mcp__unimatrix__memory_search`.

**Implications:**
- Tool names don't need global uniqueness — just uniqueness within our server
- The `memory_` prefix in tool names is redundant with the server name but adds clarity
- Users will see `mcp__unimatrix__memory_search` in their conversations

---

## 9. Elicitation for Project Setup

The 2025-11-25 spec added server-initiated user prompting via forms. This could enable:

```
Server: "Which project should Unimatrix use?"
Form: [dropdown of detected projects]
User: selects "my-web-app"
Server: initializes for that project
```

**This could replace `unimatrix init` CLI commands** — the server detects it's not configured and uses elicitation to set up. But this depends on Claude Code's implementation of elicitation.

**Needs validation**: Does Claude Code actually support elicitation? The MCP spec defines it but client support varies.

---

## 10. Error Messages as Behavioral Guidance

Since Claude sees tool execution errors and can self-correct, error messages become a prompt engineering surface:

**Bad**: `"Error: project not found"`
**Good**: `"No project initialized at /path/to/project. Call memory_init with the project path first, then retry your search."`

Claude will read this and take the suggested corrective action. Design every error message as guidance for Claude's next action.

---

## 11. Tasks Feature (2025-11-25) for Long-Running Operations

The spec now supports task-augmented requests for long-running operations. Tasks return immediately with a reference, which can be polled for status.

**Unimatrix potential**: A future `memory_import` tool that ingests a large codebase could use task support to avoid blocking Claude's conversation while the import runs.

**Not needed for v0.1 but worth designing `memory_import` with this in mind.**

---

## 12. Protocol Version Negotiation

The client sends its supported version, the server responds with what it supports. If they don't match, the client should disconnect.

**Unimatrix should**:
- Always respond with the latest version it supports
- Fail gracefully if the client requests an older version we don't support
- The `rmcp` SDK handles this automatically

---

## 13. No WebSocket Transport in MCP

Despite being a real-time protocol, MCP does not define a WebSocket transport. It uses stdio (local) or HTTP+SSE (remote).

**Implication**: No WebSocket complexity to worry about. stdio for local, streamable HTTP for future remote deployment.

---

## 14. MCP Prompts as Slash Commands

MCP prompts appear as slash commands in Claude Code. A prompt named `search_memory` would be invocable as `/mcp__unimatrix__search_memory`.

**Potential Unimatrix prompts:**
- `/remember` — guided memory storage with categorization
- `/recall` — structured memory search with filters
- `/forget` — guided memory cleanup/deprecation
- `/status` — project memory dashboard

These are user-controlled (explicit invocation) vs. tools (model-controlled). Both have roles in the UX.
