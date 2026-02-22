# Proposal A: Knowledge Oracle -- Control Structure

## What Stays in .claude/ Files

| File Category | Examples | Why It Stays |
|---------------|----------|-------------|
| Constitutional rules | `CLAUDE.md` | Absolute constraints. Never overridden, never compacted. Must be static text Claude reads at session start. |
| Protocols | `protocols/planning-protocol.md` | Step-by-step execution sequences. The scrum-master reads and follows these literally. A database entry cannot replace "read this file and do steps 1-7." |
| Agent definitions | `agents/ndp/*.md` | Cognitive priming for spawned subagents. Pasted into spawn prompts as literal text. Must be static, auditable, version-controlled. |
| Routing tables | `protocols/agent-routing.md` | Dispatch logic. The primary agent reads this to decide which swarm shape to spawn. Must be deterministic and human-reviewable. |
| Contextual rules | `rules/rust-workspace.md` | File-pattern triggers (e.g., "when editing `*.rs`, apply these constraints"). Claude Code loads these automatically. Cannot be replaced by database queries. |

**The core reason:** These files are the **execution substrate**. Claude reads them as instructions, not as reference material. They define behavior, not knowledge.

## What Goes in Unimatrix Entries

| Knowledge Type | Example | Why Unimatrix |
|---------------|---------|---------------|
| Conventions | "Use `anyhow` for app errors, `thiserror` for libraries" | Accumulated through work, evolves, needs semantic search. |
| Architectural decisions | ADR-003: "Use DistDot with pre-normalized vectors" | Created per-feature, needs cross-feature retrieval. |
| Lessons learned | "Feature nxs-002 took 12 releases because scope was too broad" | Retrospective output. Needs to surface when similar situations arise. |
| Corrections | "SQLx, not raw SQL" superseding earlier wrong entry | Correction chains with audit trail. |
| Domain facts | "EPA AQI breakpoints: 0-50 Good, 51-100 Moderate..." | Reference data agents need during implementation. |
| Pattern catalog | "Channel-based flow pattern: spawn producer, consumer, join handle" | Reusable implementation patterns discovered during work. |

**The core reason:** This is **reference material**, not execution instructions. Agents query it when they need context, not as a program to execute.

## Runtime Interaction Model

```
Session Start:
  1. Claude reads CLAUDE.md (always, never compacted)
  2. Claude reads applicable .claude/rules/ (file-pattern triggered)
  3. Server `instructions` field tells Claude: "search Unimatrix before starting work"

Agent Spawned by Scrum-Master:
  1. Spawn prompt includes: role definition (from .claude/agents/), task, file paths
  2. Orchestrator optionally calls context_briefing() and pastes result into spawn prompt
  3. Subagent works with static instructions + injected knowledge

Knowledge Storage:
  1. Agent discovers convention/pattern/decision during work
  2. Agent calls context_store() (driven by server instructions)
  3. Entry persisted in redb + embedded in hnsw_rs

Knowledge Retrieval:
  1. Agent needs context -> context_search() or context_lookup()
  2. Server returns compact markdown (< 2000 tokens)
  3. Agent applies knowledge to current task
```

The key: `.claude/` files flow into agents at spawn time (static). Unimatrix entries flow into agents at query time (dynamic). They never write to each other.

## Human Management of the Control Plane

Humans edit `.claude/` files using standard tools (editor, git). Unimatrix informs these edits but never performs them.

**Workflow for process improvement:**
1. After a feature completes, human (or scrum-master at learning gate) stores lessons via `context_store(category: "lesson-learned")`
2. Human queries `context_lookup(category: "lesson-learned", topic: "planning")` to review accumulated lessons
3. Human reads the lessons and decides: "Wave 2 needs a scope-check gate"
4. Human edits `.claude/protocols/implementation-protocol.md` to add the gate
5. Next feature execution uses the updated protocol

**Workflow for new project setup:**
1. Human creates `.claude/` directory structure manually (or copies from a template repo)
2. Human runs `unimatrix init` to add the 5-line CLAUDE.md append (Tier 2 config)
3. Agents begin accumulating knowledge through normal work
4. No Unimatrix involvement in `.claude/` file creation

## How the System Evolves Over Time

| Dimension | Evolution Mechanism | Speed |
|-----------|-------------------|-------|
| Knowledge quality | Confidence decay, corrections, dedup | Automatic, continuous |
| Knowledge breadth | Agents store as they work | Automatic, continuous |
| Process quality | Human reads lessons, edits files | Manual, periodic |
| Role definitions | Human observes agent performance, edits files | Manual, rare |
| Routing rules | Human adds new swarm shapes as needed | Manual, rare |

The knowledge layer gets smarter continuously. The process layer gets smarter only when a human acts. This is the fundamental tradeoff of Proposal A.
