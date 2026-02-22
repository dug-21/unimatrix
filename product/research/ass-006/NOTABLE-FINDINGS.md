# ASS-006: Notable Findings

Insights captured during Track 2C research that inform later tracks and design decisions.

---

## 1. MCP Server Inheritance to Custom Subagents Is Broken

The single most important finding. Custom subagents (`.claude/agents/*.md`) CANNOT reliably access project-scoped MCP servers (`.mcp.json`). There are 5+ open GitHub issues:

- **#13898**: Custom subagents can't access project-scoped MCP. Built-in subagents (general-purpose, Explore, Plan) can. Custom subagents accessing project-scoped MCP **hallucinate results** instead of failing explicitly.
- **#14496**: Task-spawned subagents fail with complex/multi-step prompts but work with simple ones.
- **#13605**: Plugin-defined subagents can't access MCP at all.
- **#13254**: Background subagents have no MCP access (intentional design).
- **#19964**: Contradictory documentation about MCP availability.

**User-scoped servers (`--scope user`) work.** This is the recommended workaround.

**Implication for Unimatrix:** Default installation must use `--scope user`. The orchestrator-passes-context pattern (Solution B) should be the primary integration path, not direct subagent MCP access.

---

## 2. Server `instructions` Field Is Massively Underutilized

The MCP `instructions` field in the initialize response is a direct behavioral injection channel — more authoritative than tool descriptions, requires zero user configuration, and persists across turns.

Despite this, almost no MCP server in the ecosystem uses it meaningfully. Most leave it empty or use a single sentence.

**For Unimatrix:** This is our primary behavioral driving channel. The instructions field should contain clear, specific guidance about when and how to use Unimatrix tools. This alone handles 70-85% of proactive usage without any CLAUDE.md changes.

---

## 3. Hooks Cannot Trigger MCP Tool Calls

Hooks execute shell commands, not Claude tool invocations. There is no bridge from hook execution to MCP tool calling. This means:

- No automated "store to memory after every edit" via hooks
- No "search memory before every file read" via hooks
- Any automation requiring MCP tool calls must be driven by instructions (CLAUDE.md, server instructions, agent defs), not hooks

**Implication:** Don't invest in hook-based automation for Unimatrix. Focus on instruction-driven behavior.

---

## 4. CLAUDE.md Is Never Compacted — But There's a Contradiction on Subagent Inheritance

Research produced conflicting findings:
- One source says subagents DO inherit parent's CLAUDE.md
- Another says subagents receive ONLY their custom system prompt, NOT parent CLAUDE.md

The likely truth: project-level CLAUDE.md is loaded for subagents (because they run in the same project directory), but it's loaded as a separate concern — not "inherited from parent." The subagent's markdown body is its primary system prompt; CLAUDE.md is supplementary.

**Needs empirical validation.** If CLAUDE.md propagates, Tier 2 configuration (5-line CLAUDE.md append) automatically benefits subagents. If not, agent definitions need their own Unimatrix instructions.

---

## 5. Tool Descriptions Drive Selection, Not Invocation Timing

A critical distinction: tool descriptions influence which tool Claude SELECTS when it decides to use a tool. They do NOT make Claude proactively invoke a tool.

`"ALWAYS search memory before starting work"` in a tool description → Claude considers memory_search more relevant when it decides to search, but doesn't make it search automatically.

`"Before starting implementation, search Unimatrix"` in CLAUDE.md → Claude actually does it ~85-90% of the time.

**Implication:** Tool descriptions are for explaining what/when/how. CLAUDE.md and server instructions are for mandating behavior.

---

## 6. The Two-Layer Reliability Model

Reliable behavioral driving requires two layers:

| Layer | Responsibility | Reliability Alone | Combined |
|-------|---------------|-------------------|----------|
| Server-side (instructions + tool descriptions) | Unimatrix team | 70-85% | — |
| Config-side (CLAUDE.md + agent defs) | User (minimal) | 85-90% | ~90%+ |

The design insight: Layer 1 (server-side) provides value with zero user effort. Layer 2 pushes reliability higher for users who want it. Users who only run `claude mcp add` still get 70-85% of the benefit.

---

## 7. The Orchestrator Pattern Bypasses All MCP Inheritance Issues

Instead of depending on subagent MCP access (which is buggy), the orchestrator searches Unimatrix and passes results in spawn prompts:

```
scrum-master → context(topic: "rust-dev", category: "duties") → includes results in spawn prompt → rust-dev receives context without needing MCP access
```

This pattern:
- Works regardless of MCP inheritance bugs
- Lets the orchestrator curate what each agent sees (role-appropriate context)
- Aligns with the multi-dimensional context model from ASS-004 (WHO × WHERE × WHAT)
- Scales to any agent topology without agent definition changes

**Implication:** The orchestrator pattern should be the PRIMARY integration path for multi-agent workflows. Direct subagent MCP access is a nice-to-have bonus.

---

## 8. SessionStart Hook with `compact` Matcher Is a Recovery Mechanism

When Claude Code auto-compacts (at ~95% context), conversation history is summarized and earlier tool results are lost. But:

```json
{
  "hooks": {
    "SessionStart": [{
      "matcher": "compact",
      "hooks": [{
        "type": "command",
        "command": "echo '{\"additionalContext\": \"Unimatrix is available. Re-search for context.\"}'"
      }]
    }]
  }
}
```

This fires AFTER compaction and can re-inject a reminder. Combined with CLAUDE.md (which never compacts), this provides a safety net.

**Implication:** Unimatrix init could generate this hook as optional Tier 2 configuration. But since CLAUDE.md already survives compaction, the marginal value is low.

---

## 9. User-Scoped vs Project-Scoped Is a Key Decision

| Aspect | User-Scoped | Project-Scoped |
|--------|-------------|----------------|
| Custom subagent access | Works | Broken (bug #13898) |
| Available in all projects | Yes | No (project-specific) |
| Setup | Once per machine | Once per project |
| Multiple projects | Same server for all | Separate config per project |

**Recommendation:** User-scoped for v0.1. Unimatrix already isolates data per-project internally (via working directory detection). Having the server available globally is a feature — users don't need to remember to configure each project.

---

## 10. Agent Definitions Can Include `mcpServers` in Frontmatter

This is a relatively new feature. Agent definitions can explicitly declare which MCP servers they need:

```yaml
---
name: ndp-rust-dev
mcpServers:
  unimatrix: {}
---
```

This is the mechanism for ensuring subagents have MCP access. But it requires modifying every agent definition file. `unimatrix init --agents` could automate this.

---

## 11. The Hallucination Risk Is Real

When custom subagents can't access a project-scoped MCP server, they don't fail with an error — they **hallucinate plausible-looking results**. The subagent acts as if it called the tool and got a response, but the response is fabricated.

**Implication:** Silent failure is worse than loud failure. If Unimatrix can't detect that a subagent is hallucinating results, bad patterns could propagate. The orchestrator pattern (Solution B) avoids this because the orchestrator makes real MCP calls and passes real results.

---

## 12. Skills Are More Reliable Than Raw Instructions for Complex Behaviors

For behaviors that require specific steps (like "store a decision with proper topic, category, and tags"), a Skill wrapping the procedure is more reliable than a CLAUDE.md instruction saying "store decisions."

Skills provide:
- Auto-invocation based on description matching
- Step-by-step procedure in the skill body
- Consistent execution (same steps every time)

**Implication:** Complex Unimatrix workflows (store, correct, compile briefing) should be Skills. Simple behaviors (search before work) can be CLAUDE.md instructions.

---

## 13. The Configuration Precedence Is Well-Defined

When mechanisms conflict:

```
Managed policy > CLAUDE.md > Agent def body > Unconditional rules >
MCP server instructions > Conditional rules > Skill content >
Tool descriptions > Conversation history
```

This is good news for Unimatrix: instructions placed in CLAUDE.md have high authority. Server instructions have medium-high authority. Tool descriptions are advisory.

**Implication:** Unimatrix should never try to override CLAUDE.md via tool descriptions. Instead, work WITH the hierarchy — server instructions for default behavior, CLAUDE.md for reinforcement.

---

## 14. Tool Search Changes the Game for Multi-Tool Servers

When MCP tools exceed 10% of context, Tool Search activates. Tool descriptions are deferred and Claude uses MCPSearch to find tools on-demand.

**For Unimatrix:** If Unimatrix has 3-5 tools, this probably won't trigger. But in a project with multiple MCP servers (GitHub, filesystem, Unimatrix, database), Tool Search may activate. When it does:

- Tool descriptions are NOT in constant context
- Claude must search for tools when it thinks one might be useful
- The tool name and description become critical for discoverability

**Implication:** Unimatrix tool names should be highly descriptive. `context` is better than `uni_ctx`. Descriptions should include the keywords Claude would search for.

---

## 15. Background Subagents Have No MCP Access (By Design)

This is intentional, not a bug. Background subagents (run_in_background: true) have no access to MCP tools. Only foreground subagents can use MCP.

**Implication:** Any Unimatrix-dependent agent MUST run in foreground. This affects workflow design — you can't fire-and-forget agents that need Unimatrix context.

---

## 16. Subagent Nesting Is Limited to 1 Level

Subagents cannot spawn other subagents. The scrum-master can spawn ndp-rust-dev, but ndp-rust-dev cannot spawn another subagent.

**Implication for context passing:** The orchestrator pattern only works one level deep. If a deeper hierarchy is needed, the orchestrator must coordinate all agents directly (hub-and-spoke, not chain).

---

## 17. `unimatrix init` Is Low-Hanging Product Surface

A CLI command that generates optimal Claude Code configuration is concrete, testable product surface:
- Appends 5 lines to CLAUDE.md
- Optionally adds mcpServers to agent defs
- Optionally creates a conventions rule file
- Non-destructive, idempotent

This bridges the gap between "add MCP server" (Tier 1) and "full multi-agent integration" (Tier 3) without requiring users to understand Claude's config hierarchy.
