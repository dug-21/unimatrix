# Swarm Orchestration Protocol

Base protocol for all swarm operations. Extended by `implementation-protocol.md` (for coding) and `planning-protocol.md` (for SPARC planning).

---

## Execution Model

Swarms use **coordinator delegation**: the primary agent spawns `ndp-scrum-master` as the single coordinator, who then spawns worker agents, monitors results, detects drift, and controls flow. Use **Task tool** (spawn-and-wait) at both levels. Do NOT use TeamCreate — Teams are for long-running collaborative work requiring inter-agent messaging.

See `implementation-protocol.md` and `planning-protocol.md` for the specific delegation flows.

---

## Coordination and Persistence

Unimatrix provides a unified coordination and persistence layer for swarm operations. Rather than managing separate memory systems, agents use a single shared mechanism for:

---

## Swarm Architecture

Agents in teams or swarm require three capabilities:

1. **Spawning** — the actual Claude Code process is created via the Task tool
2. **Status tracking** — the orchestrator and other agents can check on agent state and progress

**Two required steps per agent:**
1. Register the agent — **REQUIRED** before the Task call
2. `Task()` — creates the real Claude Code process — **REQUIRED**

Without registration, status queries will fail. Registration MUST happen before the Task call.


### Agent ID = Swarm Activation

All Unimatrix agent definitions (except ndp-scrum-master) contain a `## Swarm Coordination` section. This section is **dormant** unless the agent's spawn prompt includes `Your agent ID: <id>`. When present, the agent MUST:
- Write `{task}/progress` after each major step
- Write `swarm/{id}/complete` before returning
- Read `swarm/shared/{feature}-context` for shared context

The coordinator's only job is to **pass the agent ID**. The agent definition handles the rest. This means the coordinator prompt can be minimal — no need to repeat coordination instructions.

---

## Swarm Launch: 2 Messages

When a task qualifies for swarm (see complexity detection below), execute in 2 messages. Batch aggressively — all initialization in one message, all agent spawns in the next.

### Message 1: Initialize + Register + Define (ALL batched)

All registration and TaskCreate calls go in ONE message. Batch everything:

1. **Register each agent** — register all agents that will participate in the swarm, giving each a unique ID and type.
2. **Seed shared context** — store the task description, goals, and constraints in shared state so agents can read it during execution. Use the `swarm/shared/{feature-id}-context` key convention.
3. **Define ALL tasks** — create all TaskCreate entries for the work to be done, batched together.

Set task dependencies with TaskUpdate after creation.

### Message 2: Execute (ALL agents spawned in parallel)

Spawn ALL agents in ONE message via Task tool. Every Task call runs in parallel.

Each agent prompt MUST include:
1. `Your agent ID: {feature}-agent-N-{role}` — this activates the Swarm Coordination block in agent definitions
2. The Level-1 summary (if feature work — from `/spec-compile`)
3. The task description (2-3 sentences)
4. Specific file paths

The agent ID is the critical line. All Unimatrix agent definitions (except ndp-scrum-master) contain a `## Swarm Coordination` section that activates when `Your agent ID:` is present. This section instructs agents to:
- Write `swarm/{id}/status` on start
- Write `swarm/{id}/progress` after each major step
- Write `swarm/{id}/complete` before returning
- Read `swarm/shared/{feature}-context` for shared state

The coordinator does NOT need to repeat coordination instructions in the prompt — the agent definition handles it.

Example agent prompt:
```
You are agent-N implementing {subtask} for {feature-id}.
Your agent ID: {feature-id}-agent-N-{role}

{Level-1 summary — objective, ADR list with pattern IDs, constraints, NOT in scope}

YOUR SPECIFIC TASK: {subtask description}

Files to read/modify: {paths from brief}
```

After spawning: tell the user what agents are working on, then STOP.

---

## Spawn and Wait Pattern

After spawning agents:
1. **TELL USER** what agents are working on
2. **STOP** — no more tool calls
3. **WAIT** — let agents complete
4. **SYNTHESIZE** — review results, check shared state for coordination notes

DO NOT: poll TaskOutput repeatedly, check swarm status continuously, or add more tool calls after spawning.

---

## Multi-Wave Features

For features with sequential waves (Wave 1 -> Wave 2 -> Wave 3):
- Spawn ALL agents within a wave in ONE message (parallel)
- Wait for the wave to complete
- Mark completed tasks, update TaskList
- Spawn the next wave's agents in a NEW message (parallel)
- Repeat until all waves complete

Do NOT spawn agents from different waves in the same message if Wave N+1 depends on Wave N outputs.

---

## Hooks That Fire Automatically (DO NOT duplicate)

These run via `.claude/settings.json` without agent action:

| Event | Hook | What it does |
|-------|------|-------------|
| Every user message | `route --task "$PROMPT"` | Routes to recommended agent |
| Every Task spawn | `pre-task --task-id ... --description ...` | Registers task |
| Every Task completion | `post-task --task-id ... --success ...` | Records outcome |
| Every Bash command | `pre-command` / `post-command` | Risk assessment + tracking |
| Every file edit | `pre-edit` / `post-edit` | Context + learning |
| Session start | `daemon start` + `session-restore` | Restores state |

**Do NOT manually run** `pre-task`, `route`, `pre-command`, `pre-edit`, or `session-start`. They already fire.

---

## Anti-Drift Config

Every agent gets the Level-1 summary (objective + ADR pattern IDs + constraints + NOT-in-scope) in its prompt. This is the primary anti-drift mechanism.

For small swarms (6-8 agents), use a hierarchical topology for tight control. For large swarms (10-15 agents), use a mesh topology to allow peer communication.

---

## 3-Tier Model Routing

The `model-route` hook fires automatically.

| Tier | Model | Use Cases |
|------|-------|-----------|
| 1 | Agent Booster (<1ms, $0) | Simple transforms: var-to-const, add-types, remove-console |
| 2 | Haiku (~500ms) | Simple tasks, bug fixes, low complexity |
| 3 | Sonnet/Opus (2-5s) | Architecture, security, complex reasoning |

---

## Task Complexity Detection

**USE SWARM when**: 3+ files, new feature, cross-module refactor, API changes, security changes, performance work, schema changes.

**SKIP SWARM for**: single file edits, 1-2 line fixes, documentation updates, config changes, questions/exploration.
