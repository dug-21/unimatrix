# D6c: Unimatrix Configuration Surface Design

**Track**: 2C (ASS-006)
**Date**: 2026-02-20
**Status**: Complete

---

## Design Principle

**Unimatrix should "just work" after `claude mcp add`.** Additional configuration exists for users who want higher reliability or specific behaviors, but is never required.

---

## The Three Tiers

### Tier 1: Zero Config (Just Add the Server)

**User action:**
```bash
claude mcp add --scope user --transport stdio unimatrix -- unimatrix-server
```

**What Unimatrix provides (server-side, no user effort):**

1. **Server `instructions` field** in initialize response:
```
Unimatrix is the project's context engine. Before starting implementation,
architecture, or design tasks, search for relevant patterns and conventions
using the context tool. Apply what you find. After discovering reusable
patterns or making architectural decisions, store them for future reference.
```

2. **Tool descriptions** with appropriate guidance:
```
context — Search or lookup project knowledge. Provide topic and/or category
for exact lookup. Add a query for semantic search. Use before starting
implementation work to find relevant patterns and conventions.
```

3. **Sensible defaults**: Returns small responses (<2000 tokens), front-loads best results, includes guidance footer.

**Expected reliability**: ~70-85% for proactive search. Users who want nothing more than `mcp add` get meaningful value.

**Why user-scoped:** User-scoped MCP servers (`--scope user`) work with custom subagents. Project-scoped servers (`--scope project`) have known bugs preventing access from custom agents (GitHub issues #13898, #14496).

---

### Tier 2: Recommended Config (5 Lines in CLAUDE.md)

**User action:** Add to project CLAUDE.md:
```markdown
# Unimatrix Integration
Before starting implementation or design work, search Unimatrix for relevant
patterns and conventions. Apply what you find. After making architectural
decisions or discovering patterns, store them in Unimatrix for future reference.
```

**What this adds:** Reinforces server instructions with system-prompt authority. Survives compaction. Pushes reliability from ~75% to ~90%.

**Optional: `unimatrix init` generates this.**

```bash
unimatrix init
```

Appends the 5-line block to CLAUDE.md if not already present. Non-destructive (checks for existing block first).

---

### Tier 3: Full Integration (Multi-Agent Workflows)

For teams using subagent workflows (like NDP), additional configuration enables subagent access.

**3a. Agent definitions get `mcpServers` field:**

```yaml
# .claude/agents/ndp-rust-dev.md
---
name: ndp-rust-dev
mcpServers:
  unimatrix: {}
---
```

**`unimatrix init --agents`** could scan `.claude/agents/` and add the `mcpServers` field to each agent definition. Non-destructive, additive only.

**3b. Orchestrator protocol includes Unimatrix context passing:**

The scrum-master agent's spawn prompt template includes Unimatrix context:

```
# Before spawning each agent:
1. Call context(topic: "{agent-role}", category: "duties") to get role context
2. Call context(topic: "{feature-id}", category: "convention") to get relevant conventions
3. Include results in spawn prompt
```

This is built into the orchestrator's agent definition, not user configuration. Users get this automatically when using Unimatrix-aware agent definitions.

**3c. Optional rules for code conventions:**

```yaml
# .claude/rules/unimatrix-conventions.md
---
paths:
  - "**/*.rs"
  - "**/*.ts"
---
Before modifying source code, check Unimatrix for applicable conventions
with category "convention" and a topic matching the technology being modified.
```

**`unimatrix init --rules`** generates this if the user wants file-specific convention checking.

---

## What Unimatrix Generates vs. What Users Write

| Component | Who Creates | Who Maintains | Required? |
|-----------|------------|--------------|-----------|
| Server `instructions` | Unimatrix (server code) | Unimatrix team | Yes (always present) |
| Tool descriptions | Unimatrix (server code) | Unimatrix team | Yes (always present) |
| CLAUDE.md append | `unimatrix init` or user | User | No (Tier 2, recommended) |
| Agent `mcpServers` field | `unimatrix init --agents` or user | User | No (Tier 3, for subagents) |
| Rules file | `unimatrix init --rules` or user | User | No (Tier 3, for code conventions) |
| Orchestrator protocol | Unimatrix-aware agent defs | User | No (Tier 3, for multi-agent) |
| Hooks | Not recommended | N/A | No (hooks can't trigger MCP) |

---

## The MCP Server Scope Decision

**Recommendation: User-scoped server (`--scope user`), not project-scoped.**

**Why:**
- Project-scoped MCP servers (`.mcp.json`) have known bugs preventing access from custom subagents (#13898)
- User-scoped servers work with both the primary conversation and custom subagents
- The trade-off: user-scoped means the server is available in ALL projects, not just one

**Mitigation for scope concern:**
- Unimatrix is designed for per-project knowledge isolation anyway
- The server auto-detects the project from the working directory
- Having the server available globally is a feature, not a bug — every project benefits

**When project-scope bugs are fixed:** Users can switch to project scope if they prefer. The server works identically either way.

---

## The Subagent Context Problem & Solutions

The fundamental problem: custom subagents don't reliably inherit MCP servers. Three solutions, usable independently or combined:

### Solution A: User-Scoped MCP + Agent mcpServers (Tier 3a)

Works today for user-scoped servers. Each agent definition lists `mcpServers: { unimatrix: {} }`. Subagent calls Unimatrix directly.

**Pro:** Agent is autonomous — searches and stores independently.
**Con:** Requires agent definition changes. Project-scoped servers don't work.

### Solution B: Orchestrator Passes Context (Recommended)

The orchestrator (scrum-master) calls Unimatrix, includes results in spawn prompts.

```
# Scrum master protocol:
Before spawning an agent:
  1. context(topic: "rust-dev", category: "duties") → agent duties
  2. context(topic: "{feature}", category: "convention") → conventions
  3. Include in spawn prompt as "Context from Unimatrix: ..."
```

**Pro:** Works regardless of MCP inheritance bugs. Orchestrator curates what each agent sees.
**Con:** Agents can't make ad-hoc queries during work. Context is static at spawn time.

### Solution C: Hybrid (Best of Both)

Orchestrator provides initial context (Solution B). Agent also has MCP access for ad-hoc queries (Solution A).

```yaml
---
name: ndp-rust-dev
mcpServers:
  unimatrix: {}
---
You have Unimatrix context provided in your spawn prompt (curated by the
orchestrator). For additional context during implementation, use the
context tool directly.
```

**Pro:** Rich initial context + autonomous ad-hoc search.
**Con:** Requires both orchestrator protocol changes AND agent def changes.

**Recommendation for v0.1:** Start with Solution B (orchestrator passes context). Add Solution C when MCP inheritance stabilizes.

---

## `unimatrix init` Command Design

```
unimatrix init [options]

Options:
  --project <path>    Project root (default: cwd)
  --agents            Also configure .claude/agents/*.md with mcpServers
  --rules             Also generate .claude/rules/unimatrix-conventions.md
  --dry-run           Show what would be generated without writing

What it does:
  1. Appends Unimatrix integration block to CLAUDE.md (if not present)
  2. (--agents) Adds mcpServers field to each .claude/agents/*.md (if not present)
  3. (--rules) Creates .claude/rules/unimatrix-conventions.md (if not exists)

What it does NOT do:
  - Add MCP server to Claude Code (user runs `claude mcp add` separately)
  - Modify existing CLAUDE.md instructions
  - Remove or overwrite any existing configuration
  - Create settings.json or hooks (hooks can't trigger MCP tools)
```

---

## Configuration Anti-Patterns

Things Unimatrix should NOT do:

1. **Don't generate hooks for memory operations.** Hooks can't trigger MCP tool calls. Any hook-based approach is a dead end.

2. **Don't generate .claude/rules/ for every possible file type.** One general rule is better than 10 specific ones. Keep it simple.

3. **Don't require settings.json changes.** Permissions and tool access should work with Claude Code defaults.

4. **Don't put behavioral instructions in tool descriptions only.** Tool descriptions influence selection, not invocation timing. Always pair with server instructions or CLAUDE.md.

5. **Don't assume subagents have MCP access.** Design the orchestrator pattern (Solution B) as the primary path. Treat direct subagent MCP access as a bonus.

6. **Don't fight the precedence hierarchy.** CLAUDE.md > server instructions > tool descriptions. Put the most important instructions in the highest-authority location.

---

## Validation Plan

These configurations need empirical testing:

| Test | What to Measure | Expected Result |
|------|----------------|-----------------|
| Server instructions only (Tier 1) | Does Claude search Unimatrix before starting work? | ~70-85% of the time |
| Server instructions + CLAUDE.md (Tier 2) | Does reliability improve? | ~90% |
| Agent def with mcpServers (Tier 3a) | Can subagent call Unimatrix tools? | Yes if user-scoped |
| Orchestrator context passing (Tier 3, Solution B) | Does subagent use passed context? | ~95% |
| Rules for code conventions (Tier 3) | Does Claude check conventions when editing code? | ~80% |
| Post-compaction behavior | Do CLAUDE.md instructions persist? | Yes (by design) |
| Conflicting instructions | CLAUDE.md vs server instructions vs tool description | CLAUDE.md wins |

Testing requires building a minimal Unimatrix MCP server with the designed instructions and tool descriptions, then running real tasks against it. This is Track 3 work.
