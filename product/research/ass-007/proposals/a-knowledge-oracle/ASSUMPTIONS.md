# Proposal A: Knowledge Oracle -- Assumptions

## Core Strategic Assumption

Unimatrix is a knowledge store. The `.claude/` control plane is static, human-maintained, and the sole source of truth for process and structure. Unimatrix enhances what agents **know**, never how they **work**.

## Ownership Boundaries

| Concern | Owner | Rationale |
|---------|-------|-----------|
| Process flow (wave ordering, gates) | `.claude/protocols/*.md` | Human-authored, human-edited. Agents execute, never modify. |
| Role definitions (scope, self-checks) | `.claude/agents/ndp/*.md` | Human-authored. Roles are static per project lifecycle. |
| Routing rules (task -> team shape) | `.claude/protocols/agent-routing.md` | Dispatch table changes only when humans rethink team topology. |
| Constitutional rules | `CLAUDE.md` | Sacred. Never generated. |
| Contextual rules | `.claude/rules/*.md` | File-pattern triggered. Human-maintained. |
| Skills | `.claude/skills/*/SKILL.md` | Stateless procedures. Human-authored. |
| Domain knowledge (conventions, patterns) | Unimatrix DB | Accumulated by agents, curated over time. |
| Architectural decisions (ADRs) | Unimatrix DB + feature dirs | Stored by architect agent, retrievable by all. |
| Lessons learned (retrospective output) | Unimatrix DB | What went wrong, what worked, why. |
| Corrections (wrong knowledge fixed) | Unimatrix DB | Correction chains with audit trail. |

## The Learning Loop

```
Agents work -> discover patterns -> context_store() -> knowledge accumulates
                                                            |
Agents search -> context_search() -> apply knowledge -> better work
                                                            |
Humans review retrospectives -> read context_status() -> manually edit .claude/ files
```

The loop is **open** on the process side. Unimatrix never closes the loop by writing to `.claude/`. A human reads the lessons, decides what to change, and edits the files.

## What's Explicitly OUT of Scope

- **Protocol generation or modification.** Unimatrix never writes `.claude/protocols/*.md`.
- **Agent definition evolution.** No auto-updating role boundaries or self-check gates.
- **Routing table updates.** The dispatch table is human-curated.
- **Workflow state tracking.** Wave progress, agent status, completion gates -- all stay in protocols executed by the scrum-master.
- **File generation of any kind.** Unimatrix is a database with an MCP interface, not a file emitter.

## Tradeoffs (Honest Assessment)

**Strengths:**
- Minimal blast radius. A bug in Unimatrix can return bad knowledge but cannot corrupt the control plane.
- Clear debugging. If an agent misbehaves, check `.claude/` files (process) vs. Unimatrix results (knowledge) independently.
- Human retains full control over how teams work.

**Weaknesses:**
- Process improvement is slow. The 12-release retrospective produces lessons-learned in Unimatrix, but a human must read them and edit protocols. If the human doesn't act, the same process failures repeat.
- Knowledge about process (e.g., "wave 2 should have a smaller scope") sits in a database but cannot self-actuate. It's advice, not automation.
- New project setup requires manually authoring 20+ `.claude/` files. Unimatrix provides knowledge to inform this, but cannot scaffold it.
