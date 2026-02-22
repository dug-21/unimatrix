# D6b: Behavioral Driving Playbook

**Track**: 2C (ASS-006)
**Date**: 2026-02-20
**Status**: Complete

---

## Overview

For each target behavior Unimatrix needs to drive, this playbook documents: which configuration mechanism to use, exact wording, reliability assessment, and fallback options.

---

## Reliability Scale

| Rating | Meaning |
|--------|---------|
| HIGH (85-95%) | Works in nearly all scenarios. Occasional skip under heavy context load. |
| MEDIUM-HIGH (70-85%) | Works in most scenarios. May skip when task is very specific or context is dense. |
| MEDIUM (50-70%) | Works sometimes. Inconsistent. Needs reinforcement. |
| LOW (20-50%) | Unreliable. Don't depend on this alone. |
| NONE (0%) | Does not work. Technical limitation. |

---

## Behavior 1: Search Memory Before Starting Work

**Target**: Claude calls `memory_search` (or `context`) before beginning any implementation, architecture, or design task.

**Recommended approach**: MCP server `instructions` + optional CLAUDE.md reinforcement

### Primary: Server Instructions (MEDIUM-HIGH, ~70-85%)

Unimatrix MCP server returns in `initialize` response:
```json
{
  "instructions": "Unimatrix is the project's context engine. Before starting implementation, architecture, or design tasks, search for relevant patterns and conventions using the context tool. Apply what you find."
}
```

This requires ZERO user configuration beyond `claude mcp add`.

### Reinforcement: CLAUDE.md Append (HIGH, ~85-90%)

If user wants maximum reliability, append to CLAUDE.md:
```markdown
# Unimatrix Integration
Before starting implementation or design work, search Unimatrix for relevant
patterns and conventions. Apply what you find to your work.
```

Combined with server instructions, this reaches ~90% reliability.

### Why other mechanisms don't work here:
- **Tool descriptions alone**: ~50-60%. Influences selection but not invocation timing.
- **Rules**: Only fire when files are accessed, not at task start.
- **Hooks**: Cannot trigger MCP tool calls.

---

## Behavior 2: Store Key Decisions After Work

**Target**: Claude calls `memory_store` after completing significant work to save patterns, decisions, conventions.

**Recommended approach**: Skill + CLAUDE.md instruction

### Primary: Dedicated Skill (MEDIUM-HIGH, ~75%)

Create a skill that wraps memory storage:
```yaml
# .claude/skills/record-decision/SKILL.md
---
name: record-decision
description: Save a project decision, pattern, or convention to Unimatrix memory. Use after making architectural decisions or discovering reusable patterns.
---
Call the Unimatrix context tool to store the decision with appropriate topic and category.
```

Claude can self-invoke this skill when it recognizes a decision was made.

### Reinforcement: CLAUDE.md (MEDIUM, ~60-70%)

```markdown
After completing architecture decisions or discovering reusable patterns,
record them using the record-decision skill or the Unimatrix context tool.
```

### Why this is harder than search:
- End-of-task detection is ambiguous — Claude doesn't receive a "task complete" signal
- Post-completion actions compete with Claude's tendency to just respond to the user
- Hooks CAN'T trigger MCP calls, so automation isn't possible via hooks

### Alternative: Orchestrator-driven storage

In multi-agent workflows, the orchestrator (scrum-master) can explicitly store after receiving agent results. This is more reliable because the orchestrator has an explicit protocol step.

---

## Behavior 3: Check Conventions Before Writing Code

**Target**: When editing code files, Claude checks Unimatrix for applicable conventions first.

**Recommended approach**: .claude/rules/ with glob patterns

### Primary: Rules File (MEDIUM-HIGH, ~75-85%)

```yaml
# .claude/rules/code-conventions.md
---
paths:
  - "**/*.rs"
  - "**/*.ts"
  - "**/*.py"
---
Before modifying source code, check Unimatrix for applicable conventions
using the context tool with category "convention" and a topic matching
the technology or module being modified.
```

This fires specifically when code files enter Claude's context. More targeted than a global CLAUDE.md instruction.

### Why this works well:
- Scoped to relevant files (not every task)
- Loads when Claude is actively working with code
- Complements global "search before work" instruction with specific guidance

---

## Behavior 4: Subagent Uses Unimatrix

**Target**: When ndp-scrum-master spawns ndp-rust-dev via Task tool, the subagent has access to Unimatrix and uses it.

**Recommended approach**: Explicit MCP server config in agent definition + user-scoped server

### CRITICAL FINDING: MCP Inheritance Is Broken for Custom Agents

Current state (5+ open GitHub issues):
- Custom subagents CANNOT access project-scoped MCP servers (`.mcp.json`)
- Custom subagents CAN access user-scoped MCP servers (`~/.claude/.mcp.json`)
- Built-in subagents (general-purpose) CAN access all MCP servers
- Background subagents have NO MCP access at all

### Workaround A: User-Scoped MCP Server (HIGH for user-scoped, ~85%)

Configure Unimatrix at user scope:
```bash
claude mcp add --scope user --transport stdio unimatrix -- unimatrix-server
```

Then in agent definition:
```yaml
---
name: ndp-rust-dev
mcpServers:
  unimatrix: {}  # Reference user-scoped server
---
Before writing code, search Unimatrix for relevant patterns using the context tool.
```

### Workaround B: Orchestrator Passes Context (HIGH, ~90%)

The scrum-master (or parent agent) searches Unimatrix first, then includes results in the subagent's spawn prompt:

```
Task(
  subagent_type: "ndp-rust-dev",
  prompt: "Implement the auth middleware.

  Relevant patterns from Unimatrix:
  - Error handling: use thiserror for library errors, anyhow for applications
  - Async: use tokio, pin to 1.x
  - Tower middleware: follow the existing pattern in src/middleware/

  [full task description]"
)
```

This is the most reliable approach because it doesn't depend on MCP inheritance working.

### Workaround C: Hybrid — Orchestrator Searches, Agent Stores (MEDIUM-HIGH)

Orchestrator retrieves context and passes it in spawn prompt. Agent has user-scoped MCP access for storing findings after work. This gives the agent read context (via prompt) and write access (via MCP).

---

## Behavior 5: Recognize and Record Corrections

**Target**: When user says "no, do X instead of Y", Claude detects the correction pattern and stores it in Unimatrix.

**Recommended approach**: CLAUDE.md instruction + pattern detection in server logic

### CLAUDE.md Instruction (MEDIUM, ~60-70%)

```markdown
When a user corrects a previous approach ("no, use X instead" or "that's wrong,
do Y"), record the correction in Unimatrix using the context tool with
category "correction" so future work avoids the same mistake.
```

### Why only medium reliability:
- Correction detection requires understanding conversational context
- Claude doesn't always recognize implicit corrections
- Better handled server-side: Unimatrix could analyze stored context for correction patterns

### Better approach: Explicit correction tool

Provide a dedicated `record_correction` tool or skill that the user can invoke:
```
/correct "Don't use unwrap() in library code — use thiserror instead"
```

Explicit is more reliable than implicit detection.

---

## Behavior 6: Subagent Follows Protocol Steps

**Target**: When ndp-scrum-master spawns at planning wave 2, it knows to spawn pseudocode + tester agents.

**Recommended approach**: Agent definition + explicit protocol reference in spawn prompt

### Agent Definition (HIGH, ~90-95%)

The scrum-master's agent definition body IS its protocol. Instructions in the markdown body have the highest authority for that agent.

### Spawn Prompt Reinforcement (HIGH, ~95%)

The primary agent's spawn prompt should include:
```
Execute the planning protocol at `.claude/protocols/planning-protocol.md`.
You are at Wave 2. Read the protocol for Wave 2 steps.
```

This works because the scrum-master's agent definition already says "read the protocol and execute it."

### Unimatrix enhancement:

In the future, the scrum-master could call Unimatrix:
```
context(topic: "planning-protocol", category: "protocol", query: "wave 2 steps")
```

And receive just the 20 lines relevant to wave 2, instead of reading the full 300-line protocol. This is the "protocol compilation" opportunity from ASS-004 Finding #18.

---

## Behavior 7: Re-inject Context After Compaction

**Target**: When context is auto-compacted, critical Unimatrix instructions aren't lost.

**Recommended approach**: CLAUDE.md (automatic) + SessionStart hook (optional)

### CLAUDE.md (HIGH, ~95%)

CLAUDE.md is NEVER compacted. Any Unimatrix instructions in CLAUDE.md survive automatically.

### SessionStart Hook with Compact Matcher (HIGH, ~95%)

For additional context re-injection after compaction:

```json
{
  "hooks": {
    "SessionStart": [{
      "matcher": "compact",
      "hooks": [{
        "type": "command",
        "command": "echo '{\"additionalContext\": \"Unimatrix memory is available. Search for patterns before resuming work.\"}'"
      }]
    }]
  }
}
```

This fires specifically after compaction events and re-injects a reminder.

---

## Summary: The Reliability Matrix

| Behavior | Best Mechanism | Reliability | Requires User Config? |
|----------|---------------|-------------|----------------------|
| Search before work | Server instructions + CLAUDE.md | HIGH (90%) | Optional CLAUDE.md append |
| Store after work | Skill + CLAUDE.md | MEDIUM-HIGH (75%) | Skill must be defined |
| Check conventions | .claude/rules/ | MEDIUM-HIGH (80%) | Rule file needed |
| Subagent uses Unimatrix | User-scoped MCP + agent def | HIGH (85%) | User-scoped MCP setup |
| Subagent gets context | Orchestrator passes in prompt | HIGH (90%) | No (orchestrator protocol) |
| Record corrections | Explicit tool/skill | MEDIUM (65%) | Skill definition |
| Follow protocol steps | Agent def + spawn prompt | HIGH (95%) | No (built into agent defs) |
| Survive compaction | CLAUDE.md | HIGH (95%) | Instructions must be in CLAUDE.md |

---

## The Two-Layer Model

Reliable behavior driving requires two layers:

**Layer 1: Server-side (Unimatrix responsibility, zero user config)**
- `instructions` field in MCP initialize response
- Tool descriptions with appropriate guidance
- Sensible defaults in server behavior

**Layer 2: Config-side (user responsibility, minimal effort)**
- 3-5 line CLAUDE.md append for maximum reliability
- Agent `mcpServers` field for subagent access
- Optional .claude/rules/ for file-specific conventions

The goal: Layer 1 handles 70-85% of cases. Layer 2 pushes to 90%+. Users who only do `claude mcp add unimatrix` still get most of the value.
