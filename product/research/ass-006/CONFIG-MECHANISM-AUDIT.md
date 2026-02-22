# D6a: Claude Code Configuration Mechanism Audit

**Track**: 2C (ASS-006)
**Date**: 2026-02-20
**Status**: Complete

---

## Overview

Claude Code has 9 distinct configuration mechanisms that inject context or drive behavior. Each has different scope, loading rules, persistence, and precedence. This audit maps them all.

---

## 1. CLAUDE.md Files

**What**: Free-form markdown injected into Claude's system prompt. The primary behavioral instruction mechanism.

**Locations (precedence order, highest first):**

| Location | Scope | Persistence | Sharing |
|----------|-------|-------------|---------|
| Managed policy (`/etc/claude-code/CLAUDE.md`) | Organization | Always | Admin-controlled |
| `.claude/CLAUDE.local.md` | Project, personal | Always | Gitignored |
| `./CLAUDE.local.md` | Project root, personal | Always | Gitignored |
| `.claude/CLAUDE.md` | Project | Always | Git-tracked |
| `./CLAUDE.md` | Project root | Always | Git-tracked |
| `~/.claude/CLAUDE.md` | User, all projects | Always | Personal |
| Subdirectory `CLAUDE.md` | Subtree | On-demand | Git-tracked |

**Loading rules:**
- All files at or above cwd loaded at session start, IN FULL
- Subdirectory files loaded on-demand when Claude reads files in that subtree
- `@path/to/file` import syntax supported (max 5-hop depth, no circular imports)
- Never compacted — always reloaded fresh after compaction
- No token budget limit (but contributes to overall context usage)

**Key properties:**
- Highest behavioral authority (except managed policy)
- Content treated as system prompt (direct behavioral instruction)
- Supports markdown formatting
- Multiple CLAUDE.md files merge (they don't override each other — all are visible)

---

## 2. Agent Definitions (.claude/agents/*.md)

**What**: YAML frontmatter + markdown body defining specialized subagents spawned via the Task tool.

**Format:**
```yaml
---
name: agent-name
description: When to use this agent
tools: Read, Grep, Glob          # allowlist (omit = inherit all)
disallowedTools: Write, Edit      # denylist
model: sonnet | opus | haiku | inherit
permissionMode: default | acceptEdits | dontAsk | plan
maxTurns: 50
skills: [skill-name-1]           # preloaded into agent context
mcpServers: {name: config}       # MCP servers for this agent
hooks: {...}                     # agent-scoped hooks
memory: user | project | local   # persistent cross-session memory
background: true | false
isolation: worktree
---

Agent system prompt (markdown body)
```

**Discovery locations (precedence order):**
1. CLI-defined (`--agents` JSON flag) — session only
2. Project (`.claude/agents/*.md`) — git-tracked
3. User (`~/.claude/agents/*.md`) — personal
4. Plugin-provided — lowest priority

**Selection mechanisms:**
- `subagent_type` parameter in Task tool call
- Automatic delegation based on description
- Explicit user instruction ("use the X agent")

**Context inheritance (CRITICAL for Unimatrix):**

| Inherits? | Item | Notes |
|-----------|------|-------|
| YES | CLAUDE.md | Project-level CLAUDE.md loads in subagent context |
| PARTIAL | MCP servers | Must be explicitly listed in `mcpServers` field. User-scoped servers may work; project-scoped servers DO NOT for custom agents (bug #13898) |
| YES | Permissions | Parent permissions inherited, agent's `permissionMode` overrides |
| YES | Tools | Built-in tools inherited if `tools` field omitted |
| NO | Conversation history | Fresh context window per spawn |
| NO | Skills | Must be explicit in `skills` field |
| NO | Rules | Not inherited; agent must read matching files to trigger |

**CLAUDE.md inheritance note:** There is conflicting information in documentation about whether subagents see parent CLAUDE.md. Project-level CLAUDE.md likely loads (it's project-scoped), but this needs empirical validation. Subagent's markdown body is the primary system prompt.

---

## 3. Rules (.claude/rules/*.md)

**What**: Contextual instructions that fire when Claude works with files matching glob patterns.

**Format:**
```yaml
---
paths:
  - "**/*.rs"
  - "src/**/*.{ts,tsx}"
---

# Rule content (markdown)
```

**Glob semantics:**
- Standard fnmatch patterns: `*`, `**`, `?`, `{a,b}` brace expansion
- Paths relative to project root
- Rule activates if ANY pattern matches current file
- No `paths` field = unconditional (always loaded)

**Locations:**
- `.claude/rules/*.md` — project-level, higher priority
- `~/.claude/rules/*.md` — user-level, lower priority
- Discovered recursively in subdirectories

**Loading:**
- Unconditional rules: loaded at session start, survive compaction
- Conditional rules: loaded on-demand when Claude reads matching files
- Multiple rules can fire simultaneously (no mutual exclusion)
- Not inherited by subagents

---

## 4. Hooks (settings.json)

**What**: Lifecycle event handlers that run shell commands, LLM prompts, or agent logic at specific points during Claude's execution.

**Configuration locations (precedence order):**
1. Managed policy (system-wide, immutable)
2. `.claude/settings.local.json` (project-personal, gitignored)
3. `.claude/settings.json` (project-shared)
4. `~/.claude/settings.json` (user-level)
5. Agent/Skill frontmatter (scoped to component)

**Key hook events:**

| Event | When | Can Block? | Receives |
|-------|------|------------|----------|
| SessionStart | Session begins/resumes/compacts | No | startup/resume/clear/compact matcher |
| UserPromptSubmit | User sends prompt | Yes (exit 2) | Prompt text |
| PreToolUse | Before tool execution | Yes (exit 2) | Tool name + input |
| PostToolUse | After tool succeeds | No | Tool name + output |
| SubagentStart | Subagent spawned | No | Agent type |
| SubagentStop | Subagent finishes | Yes (exit 2) | Agent type + result |
| Stop | Claude finishes response | Yes (exit 2) | Response |
| PreCompact | Before compaction | No | manual/auto |
| SessionEnd | Session terminates | No | Reason |

**Hook types:**
- `command`: Shell command, receives JSON on stdin, returns exit code
- `prompt`: Single LLM call for yes/no decisions
- `agent`: Multi-turn subagent verification (50 turns, 60s timeout)

**Critical limitation: Hooks CANNOT trigger MCP tool calls.** They run shell commands, not Claude tool invocations. A PostToolUse hook on "Write" can run a shell script but cannot call `memory_store`.

**Recovery mechanism:** SessionStart hook with `compact` matcher fires after auto-compaction. Can re-inject context that was lost during compaction.

---

## 5. MCP Tool Descriptions

**What**: JSON schema + description text for each MCP tool, loaded at session start from all configured servers.

**Loading:**
- All tool descriptions loaded at session start via `tools/list`
- Token cost: ~500-1000 tokens per server depending on tool count
- Tool Search activates when total MCP tool definitions exceed 10% of context window
- When Tool Search active: descriptions deferred, Claude uses MCPSearch to find tools on-demand

**Behavioral impact:**
- Descriptions influence tool SELECTION (Claude considers relevance)
- Descriptions do NOT reliably drive tool INVOCATION TIMING
- `ALWAYS`/`NEVER` directives are soft guidance, not hard constraints
- Best for: explaining what a tool does and when it's appropriate
- Poor for: mandating "call this tool before every task"

---

## 6. MCP Server Instructions

**What**: Free-form string in the MCP `initialize` response. Behavioral guidance scoped to that server.

**Loading:**
- Loaded at connection time during session initialization
- Persists across turns (not ephemeral)
- Treated as behavioral guidance similar to skill descriptions

**Behavioral impact:**
- More authoritative than tool descriptions for driving behavior
- Less authoritative than CLAUDE.md
- Effective for: "Before starting implementation, search memory for patterns"
- Underutilized and underdocumented in the ecosystem

---

## 7. Skills (.claude/skills/\<name\>/SKILL.md)

**What**: Reusable prompt templates with metadata, invokable by Claude or user.

**Format**: YAML frontmatter (`name`, `description`, `disable-model-invocation`, `user-invocable`, `allowed-tools`, `model`, `context`, `agent`, `hooks`) + markdown body.

**Loading:**
- Descriptions loaded at session start (low token cost)
- Full content loaded on-demand when skill is invoked
- Claude can self-invoke based on description matching task
- Supporting files loaded on-demand when referenced

---

## 8. Commands (.claude/commands/*.md)

**What**: Predecessor to Skills. Markdown prompt templates, one per file. Filename becomes slash command. Still functional but superseded by Skills.

---

## 9. settings.json

**What**: JSON configuration for permissions, hooks, environment, and other session settings.

**Key settings:**
- `permissions.allow[]` / `permissions.deny[]`: Tool allowlist/denylist
- `hooks`: Lifecycle event handlers (see #4)
- `env`: Environment variables for Bash commands
- `model`: Default model override
- MCP server configs: Separate file `.mcp.json`

**Persistence:** Settings NOT reloaded mid-session (except hooks via `/hooks` menu).

---

## System Prompt Composition Order

Claude's context is composed in this order (approximate, from base to highest priority):

1. Claude Code base system prompt (fixed)
2. MCP tool definitions (or Tool Search deferred list)
3. MCP server instructions (from `initialize`)
4. Skill descriptions (summaries only)
5. Available agent descriptions
6. Environment context (cwd, git branch, platform)
7. CLAUDE.md files (all loaded variants — highest behavioral authority)
8. Auto memory MEMORY.md (first 200 lines)
9. Unconditional .claude/rules/ content
10. Conversation history (messages + tool results)
11. Conditional .claude/rules/ (loaded on-demand per file access)
12. Active skill content (when invoked)

---

## Compaction Behavior

**When triggered:** ~95% context capacity (configurable via `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`)

**Survives compaction:**
- CLAUDE.md (always reloaded fresh)
- Unconditional rules (always reloaded)
- Base system prompt (always preserved)
- MCP tool definitions (reloaded from servers)
- MCP server instructions (reloaded)

**Summarized/lost:**
- Conversation history (compressed)
- Tool results (older ones cleared, 3 most recent kept)
- Earlier instructions NOT in CLAUDE.md
- Explored dead-ends

**Recovery:** SessionStart hook with `compact` matcher for re-injection.

---

## Precedence Summary (Conflict Resolution)

When mechanisms provide conflicting guidance:

1. **Managed policy** — immutable, always wins
2. **CLAUDE.md** — system prompt authority, never compacted
3. **Agent definition body** — highest for that subagent's context
4. **Unconditional rules** — persistent, scoped to project
5. **MCP server instructions** — connection-time guidance
6. **Conditional rules** — on-demand, file-scoped
7. **Skill content** — on-demand, task-scoped
8. **Tool descriptions** — advisory, influences selection not mandate
9. **Conversation history** — ephemeral, subject to compaction
