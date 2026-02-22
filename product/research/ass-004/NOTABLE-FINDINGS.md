# ASS-004: Notable Findings

Insights captured during Track 2B research that may inform later tracks or design decisions.

---

## 1. Tool Results Are User-Role Messages — Not Special

The single most important architectural fact: MCP tool results arrive as `user`-role content blocks (`tool_result`). They are processed through the same transformer attention as any other user message. There is no privileged "tool channel" that gets special treatment.

**Implication**: Everything we know about how Claude processes user messages applies to tool responses. Formatting, length, position — all standard attention dynamics apply.

---

## 2. The "Lost in the Middle" Effect Has Concrete Implications

Liu et al. (2023) showed that LLMs recall information at the beginning and end of context much better than the middle. Anthropic confirmed this affects Claude and addressed it in Claude 2.1 by prefacing answers with "This is the most relevant sentence in context:" — improving accuracy from 27% to 98%.

**For Unimatrix**:
- Put the highest-similarity result FIRST in search responses
- Put the guidance footer LAST
- Middle results will get less attention — this is fine if they're lower-relevance

---

## 3. Programmatic Tool Calling Changes the Economics

Anthropic's new "programmatic tool calling" feature lets Claude write Python code that calls tools in a sandbox. **Tool results from programmatic calls don't enter Claude's context** — only the final output does. This achieved 37% token reduction on complex research tasks.

**For Unimatrix**: If Claude Code supports programmatic tool calling with MCP tools, a complex research flow like "search memory for X, Y, and Z, then compile a summary" could execute in the sandbox without consuming context for intermediate results. This is worth monitoring but not designing for in v0.1.

---

## 4. Tool Result Clearing Is Automatic

Claude Code automatically clears old tool results when context exceeds ~100K tokens. It keeps the 3 most recent tool use/result pairs. Cleared results are replaced with placeholder text so Claude knows something was removed.

**For Unimatrix**: Don't worry about accumulating too many search results in a long conversation. Claude Code handles cleanup. But DO keep individual responses small — each one consumes context until cleared.

**Important nuance**: Specific tools can be excluded from clearing. If Unimatrix search results are more valuable than, say, file reads, we could potentially configure this. But this is a Claude Code feature, not an MCP server feature.

---

## 5. Content Annotations Exist But Client Support Is Uncertain

MCP 2025-11-25 added `annotations.audience` to content blocks, allowing `["assistant"]`-only content (Claude sees it, user doesn't) and `["user"]`-only content (user sees it, Claude doesn't).

**Potential use**: Send debugging metadata (similarity scores, vector distances, query timing) as `["assistant"]`-only content so Claude can reason about relevance without cluttering the user's view.

**Risk**: Claude Code may not implement this yet. Needs validation before relying on it.

---

## 6. Tool Description Length Matters More Than Expected

Anthropic's documentation states that detailed tool descriptions are **"by far the most important factor in tool performance."** They recommend 3-4+ sentences covering what the tool does, when to use it, parameter details, caveats, and return format.

Three tiers observed in the wild:
- **Minimal** (~10 words): GitHub MCP Go server. Just says what the tool does.
- **Medium** (~40 words): Filesystem server. What + How + When.
- **Aggressive** (~100+ words): Claude Code's own tools. Uses CAPS, ALWAYS/NEVER directives, explicit scope boundaries, anti-patterns.

**The aggressive tier is the Claude Code house style.** Look at how Claude Code describes its own tools (Read, Edit, Bash) — they're 100+ word descriptions with behavioral directives. This is the template for Unimatrix tool descriptions.

---

## 7. The Fetch Server's Pagination Pattern

The official MCP Fetch server handles large web pages by returning a chunk with inline pagination guidance:

```
Contents of https://example.com:
[first N characters of content]

<error>Content truncated. Call the fetch tool with a start_index of 5000 to get more content.</error>
```

The `<error>` tag is used specifically because Claude treats error-tagged content with higher attention — it's a hack to ensure Claude sees the pagination instruction.

**For Unimatrix**: If `memory_search` results are truncated, include a similar pagination hint. But prefer keeping responses under the truncation threshold in the first place.

---

## 8. structuredContent Enables Future Programmatic Use

The dual content pattern (`content` for Claude + `structuredContent` for machines) means Unimatrix can serve both AI consumption and programmatic consumption from the same tool call. Future uses:

- A dashboard UI that reads `structuredContent` to display memory statistics
- An export tool that reads `structuredContent` to produce structured data files
- Another agent that calls Unimatrix tools and parses `structuredContent` instead of the markdown

**Design now, use later.** Define `structuredContent` schemas for every tool even if nothing reads them yet.

---

## 9. Confirmation Responses Should Be Minimal

Every official MCP server follows the same pattern for mutation operations: return a brief confirmation, not a full echo of the stored/modified data.

**Good**: `"Successfully wrote to /path/to/file"`
**Bad**: `"Successfully wrote to /path/to/file. Contents:\n[entire file contents echoed back]"`

**For Unimatrix**: `memory_store` returns `"Stored memory entry {id}. Tags: X, Y. Category: Z."` — not the full content back. If Claude needs to verify, it can call `memory_get`.

---

## 10. PostToolUse Injection Scanning

Claude Code runs a security scanner on MCP tool outputs that detects:
- Instruction override attempts ("ignore previous instructions")
- Role-playing/DAN prompts
- Encoding/obfuscation
- Context manipulation
- Instruction smuggling in HTML/code comments

**For Unimatrix**: Our guidance footers must avoid patterns that trigger this scanner. The recommended approach (grounded guidance: "Apply these conventions to your code") should be safe because it's contextual, not overriding. But avoid phrases like "IGNORE," "OVERRIDE," or "ALWAYS DO."

---

## 11. schemars 1.0 Is the JSON Schema Standard

The `rmcp` SDK uses `schemars` 1.0 for auto-generating JSON Schema from Rust types. This is a breaking change from `schemars` 0.8 (different derive macros, different output format).

**For Unimatrix**: All tool parameter types should derive `schemars::JsonSchema`. This auto-generates the `inputSchema` that Claude sees. The schema descriptions become parameter-level documentation.

```rust
#[derive(Deserialize, JsonSchema)]
struct SearchArgs {
    /// Natural language search query describing what you're looking for
    query: String,
    /// Maximum results to return (default: 10)
    k: Option<i64>,
    /// Maximum tokens in response (default: 2000)
    max_tokens: Option<i64>,
}
```

The `///` doc comments become the parameter descriptions in the JSON Schema.

---

## 12. Context Compaction Preserves CLAUDE.md But Not Tool Results

When Claude Code approaches context limits, it compacts:
1. First: old tool results are cleared
2. Then: conversation is summarized
3. Never compacted: CLAUDE.md (system prompt), user's current request

**For Unimatrix**: Memories retrieved early in a long session will eventually be compacted away. If Claude needs a convention throughout a session, it should either:
- Re-search memory when starting a new subtask
- Or rely on CLAUDE.md for standing instructions (which persist)

This reinforces the CLAUDE.md/tool complementary model: CLAUDE.md holds "always do X" rules, tool responses hold "here's the specific pattern for X."

---

## 13. The Flat Memory Model Is Wrong — Unimatrix Is Multi-Dimensional

The initial playbook assumed a flat model: one Claude instance searches one memory store. The real system has 17 agent types, multi-wave workflows with dependencies, and each agent at each step needs different context.

`memory_search("error handling")` should return DIFFERENT results depending on:
- **WHO asks**: architect gets ADRs + tradeoffs; rust-dev gets code patterns + conventions; tester gets test patterns + acceptance criteria
- **WHERE in workflow**: planning-wave-1 gets existing decisions; implementation gets concrete patterns; validation gets expected behaviors
- **WHAT task**: implementing auth middleware gets auth-specific context; fixing a bug gets debugging context

This means the tool interface needs dimensional query parameters (role, phase, workflow_step) and the server needs role-aware response assembly, not just vector similarity ranking.

---

## 14. Agent Definitions Can Potentially Shrink by 10x

Current agent definitions are ~150 lines each because they bake in all conventions, patterns, and procedures statically. If Unimatrix serves this context dynamically via `context_briefing`, definitions shrink to ~20 lines: role identity + "ask Unimatrix for your context."

**Massive implication**: Changes to conventions, patterns, or procedures propagate instantly through Unimatrix, not through editing 17 agent files. This is the difference between "maintaining a fleet of agents" and "maintaining a knowledge base that serves agents."

**Risk**: Need to validate in Track 2C whether thin agent definitions produce reliable role-specific behavior, or whether Claude needs the full definition in its system prompt to anchor its persona.

---

## 15. Subagent MCP Inheritance Is the Critical Unknown

When `ndp-scrum-master` spawns `ndp-rust-dev` via the Task tool, does the subagent automatically have access to the parent's MCP servers?

- **If yes**: The config story is simple. Connect Unimatrix once, every agent inherits it.
- **If no**: Every agent would need its own MCP connection. The spawn prompt would need to include Unimatrix connection setup. This might not even be possible for stdio servers.

**This is the single most important question for Track 2C.** The entire config simplification story depends on the answer.

---

## 16. `context_briefing` Is the Key Tool, Not `memory_search`

The highest-value tool isn't semantic search — it's **compiled context assembly**. When an agent spawns, it needs an orientation briefing: "here's who you are, here's your task, here's everything you need to know." This is `context_briefing`, not `memory_search`.

`memory_search` is still useful for ad-hoc queries during work. But the agent's first interaction with Unimatrix should be a briefing that replaces reading 5 files.

---

## 17. Cross-Agent Context Routing Is a First-Class Feature

What the architect decides → what the developer implements → what the tester validates → what the validator checks. This is a PIPELINE, not isolated memory queries.

Unimatrix needs to route context across agents: when the architect stores an ADR, it should be immediately available to the developer who queries for relevant constraints. When the developer discovers a pattern, the tester should see it when querying for test expectations.

This isn't just "store and search." It's **knowledge routing across a team.**

---

## 18. The Protocol Compilation Opportunity

Current protocols (planning-protocol.md, implementation-protocol.md) are ~300 lines each. An agent at step 3c doesn't need the full protocol — just the 20 lines relevant to step 3c.

Unimatrix can store protocols and compile excerpts: "You're at planning wave 2 start. Here's what you do: spawn pseudocode + tester agents in ONE message, include Wave 1 artifact paths, wait for completion."

This is protocol-as-a-service, not protocol-as-a-file.

---

## 19. Hard Constraint: No Hardcoded Agent Roles in Unimatrix Code

Unimatrix must NEVER contain code awareness of specific roles. No `match role { "architect" => ... }`. Agent roles, duties, and responsibilities are DATA stored in Unimatrix, not logic. This means:

- Any agent topology works without code changes (NDP agents, DevOps teams, data science teams)
- Role definitions are entries in the data store, queryable like any other entry
- No role enumeration or role registry in code — any string is a valid role
- The NDP agent definitions are seed data, not hardcoded behavior
- The `context_briefing` tool is generic: it composes lookups by metadata, not by role-specific logic

This is the difference between "a tool for NDP agents" and "a generic context engine that NDP agents happen to use."

---

## 20. Hard Constraint: Deterministic vs. Semantic Retrieval Are Distinct

Two fundamentally different retrieval modes:

- **Deterministic**: "What are my duties as scrum-master?" → exact metadata match, always returns same data. This is a database query.
- **Semantic**: "What patterns are relevant to auth middleware?" → vector similarity search, results may vary as knowledge grows. This is an AI operation.

Claude must decompose requests into the appropriate mode. A query like "I'm a scrum master starting planning wave 2, what do I need?" should become:
1. `context_lookup(role: "scrum-master", category: "duties")` — deterministic
2. `context_lookup(phase: "planning-wave-2", category: "protocol")` — deterministic
3. `context_search(query: "planning coordination patterns")` — semantic

Separate tools (`context_lookup` vs `context_search`) guide Claude to decompose correctly, rather than a single ambiguous tool that tries to guess the mode.

---

## 21. Categories Are Data Conventions, Not Code Enums

The category taxonomy (duties, protocol, rules, convention, pattern, decision, correction, context, knowledge) is a CONVENTION in stored data, not an enum in Unimatrix's code. Users can invent new categories without code changes. The system just filters strings.

This is important because different teams may organize their knowledge differently. A DevOps team might use "runbook" as a category. A data science team might use "experiment" or "hypothesis." Unimatrix shouldn't care.

---

## 22. The Seed Data Story

Current agent definitions (17 .md files) + protocols (4 .md files) + CLAUDE.md conventions = the SEED DATA for a Unimatrix deployment. `unimatrix init` could parse these files and load them as entries with appropriate metadata tags. After seeding, the static files shrink to thin shells. Knowledge lives in Unimatrix, not in scattered markdown files.

---

## 23. The Query Model Simplifies to { topic, category, query }

The parameter-heavy design (role, phase, workflow_step, feature, tags...) was over-fitted to NDP. When you strip away the domain assumptions, the query model is just three fields:

- **topic**: freeform subject area (a role name, technology, feature ID, domain — whatever)
- **category**: freeform knowledge type (duties, convention, protocol, pattern, decision — whatever)
- **query**: optional natural language — presence triggers semantic search, absence means deterministic lookup

This is the simplest model that can express everything. A scrum master looking up duties: `topic: "scrum-master", category: "duties"`. A DevOps engineer finding runbooks: `topic: "kubernetes", category: "runbook"`. A developer searching for patterns: `topic: "auth", category: "pattern", query: "JWT validation in Tower middleware"`.

No code changes between use cases. The model is domain-agnostic.

---

## 24. Practical Hybrid: Static Anchors + Dynamic Knowledge

Not everything should move into Unimatrix. Agent definitions keep their core identity ("You are a Rust developer"), hard behavioral rules, and self-check procedures — the stuff that must be in the system prompt, never compacted, always present. Unimatrix provides the dynamic layer: detailed patterns, workflow protocols, cross-agent context, learned corrections.

Where the line falls is a practical choice per deployment. Unimatrix is designed so it COULD serve everything. Whether it does is up to the user. Some teams will keep thick agent definitions. Others will go thin. Both are valid.

---

## 25. One Tool May Be Enough

The `{ topic, category, query }` model might not even need two separate tools. One `context` tool that switches mode based on whether `query` is present:
- `topic + category` → deterministic lookup (no query)
- `topic + category + query` → semantic search within scope
- `query` alone → broad semantic search

Whether one tool or two works better is an empirical question for Track 2C testing. The data model is the same either way.
