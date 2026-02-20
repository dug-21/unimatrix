# Agent Orchestration Database: From Static Markdown to Adaptive Control Plane

## Problem Statement

The agent framework encodes its orchestration logic — gates, validations, context routing, behavioral reminders — across **28+ markdown files** in 6 different directories. Every control is statically authored, manually maintained, and duplicated across layers. When an agent drifts or a gate proves insufficient, the fix is a human editing prose in the right file and hoping all agents read it.

This works. But it doesn't learn.

---

## Current Control Inventory

Reading through the full NDP stack, here is every distinct control/reminder and where it lives:

### 1. Mandatory Workflow Gates

| Control | What It Enforces | Where It's Defined | Where It's Repeated |
|---------|-----------------|-------------------|-------------------|
| `/get-pattern` before work | Agents consult prior knowledge | CLAUDE.md rule #2 | Every agent definition (Pattern Workflow section), planning-protocol Phase 1, implementation-protocol Phase 1, get-pattern SKILL.md |
| `/reflexion` after work | Agents record feedback | CLAUDE.md rule #3 | Every agent definition (Pattern Workflow section), planning-protocol Phase 4, implementation-protocol Phase 4, scrum-master Learning Gate |
| `/save-pattern` for discoveries | New knowledge gets persisted | CLAUDE.md rule #3 | Every agent definition, both protocols Phase 4, scrum-master Learning Gate |
| Anti-stub check | No TODO/unimplemented left behind | CLAUDE.md rule #4 | ndp-rust-dev Self-Check, ndp-validator Tier 1, implementation-protocol drift check |
| Swarm for features | No solo feature work | CLAUDE.md rule #1 | swarm-protocol complexity detection |
| Scope pre-check | Catch misalignment before wasting a planning cycle | planning-protocol Phase 1 | Only here (good) |

### 2. Context Routing Controls

| Control | Purpose | Where |
|---------|---------|-------|
| Agent ID activates swarm coordination | Agents only write to shared memory when explicitly part of a swarm | swarm-protocol, every agent's Swarm Participation section (identical block in each) |
| Component Map routing | Scrum-master gives each agent only its relevant pseudocode/test-plan files | scrum-master Component Map Routing, implementation-protocol Step 3c |
| Level-1 summary in every prompt | Anti-drift: agents see objective + ADR IDs + constraints + exclusions | swarm-protocol Anti-Drift Config, implementation-protocol Step 3c |
| Agent Context Budget | Don't paste full files into prompts; pass paths | planning-protocol bottom, implementation-protocol bottom |
| Cargo output truncation | Prevent context bloat from build output | rust-workspace rule, ndp-validator, implementation-protocol, ndp-rust-dev |

### 3. Validation Tiers

| Tier | What | Where Defined | Where Invoked |
|------|------|--------------|---------------|
| Tier 1: Compilation | cargo build/test/anti-stub | ndp-validator | implementation-protocol Step 3e |
| Tier 2: Process Adherence | banned deps, stubs, file scope, stale refs | ndp-validator | implementation-protocol Step 3e |
| Tier 3: Spec Compliance | AC coverage, test delta | ndp-validator | implementation-protocol Step 3e |
| Tier 4: Risk Classification | scope/depth/domain risk matrix | ndp-validator | implementation-protocol Step 3e |
| Plan validation (5-check) | artifacts exist, AC coverage, ADR IDs, stale refs, consistency | ndp-validator | planning-protocol Step 3h |

### 4. Behavioral Reminders (repeated per-agent)

These appear in **every** agent definition, virtually identical:

| Block | Content |
|-------|---------|
| Pattern Workflow (Mandatory) | get-pattern before, reflexion after, reward scale, return format |
| Swarm Participation | memory_store on start/progress/complete, memory_retrieve for context |
| Self-Check | Agent-specific checklist run before returning |

The Pattern Workflow and Swarm Participation blocks are **copy-pasted verbatim** across all 17 agent files (with minor variations in the Self-Check).

### 5. Process Flow Controls

| Control | Purpose | Where |
|---------|---------|-------|
| Wave dependencies | Wave 2 can't start until Wave 1 completes | planning-protocol Step 3c, implementation-protocol multi-wave |
| Pre-spawn checklist | Verify registration, tasks, context before spawning | planning-protocol Step 3c, implementation-protocol Step 3c |
| Drift check | Files outside scope, stubs, missed ACs, test count | implementation-protocol Step 3d |
| 2-iteration fix cap | Max corrective passes before escalating | implementation-protocol Step 3d, ndp-validator |
| Exit gate | All tests pass, validation pass, no stubs, GH Issue updated, learning gate | ndp-scrum-master Exit Gate |
| Per-wave AC check | Map completed tasks to acceptance criteria after each wave | implementation-protocol Step 3c.5 |

### 6. Memory System Boundaries

| Rule | What | Where Repeated |
|------|------|---------------|
| AgentDB = permanent, Memory = session | What goes where | swarm-protocol, planning-protocol, implementation-protocol, ndp-scrum-master (4x identical table) |
| `upsert: true` always | Prevent UNIQUE constraint failures | swarm-protocol, implementation-protocol |
| `memory_list` not `memory_search` | Exact-key lookup for JSON payloads | ndp-validator, swarm-protocol |

---

## The Redundancy Pattern

The same information lives in up to **5 places**:

```
CLAUDE.md (non-negotiable rules)
  -> protocols/ (how coordinators execute)
    -> agent definitions (how individual agents behave)
      -> skills/ (how tools are invoked)
        -> rules/ (contextual overrides for specific file types)
```

This layering exists because each file targets a different context window — the primary agent reads CLAUDE.md, the scrum-master reads protocols, workers read their agent definitions. Since agents can't share a single system prompt, the controls must be restated wherever they need to take effect.

**This is the fundamental tension**: agents need the right instructions at the right moment, but the only mechanism for delivering those instructions is static markdown pre-loaded into their context.

---

## Proposed: Agent Orchestration Database (AgentOrchDB)

Replace static markdown orchestration with a database-backed control plane that:

1. **Stores controls as structured data**, not prose
2. **Injects only relevant controls** into each agent's context at spawn time
3. **Learns from failures** and autonomously tightens controls
4. **Provides a UI** for humans to author, visualize, and tune flows

### Data Model

```
┌─────────────────────────────────────────────────────┐
│  CONTROLS                                            │
│  ─────────                                           │
│  id, name, type, severity, description               │
│  trigger_conditions (JSON: agent_type, phase, etc.)  │
│  injection_text (what gets added to agent prompt)    │
│  enabled, version, created_by                        │
│  effectiveness_score (computed from outcomes)         │
└─────────────────────────────────────────────────────┘
         │ 1:N
         ▼
┌─────────────────────────────────────────────────────┐
│  CONTROL_OUTCOMES                                    │
│  ────────────────                                    │
│  id, control_id, session_id, agent_id                │
│  was_followed (bool), violation_type                  │
│  detected_by (validator / human / retrospective)     │
│  outcome_impact (did violation cause rework?)         │
│  timestamp                                           │
└─────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────┐
│  FLOWS                                               │
│  ─────                                               │
│  id, name, trigger_pattern (regex on task desc)      │
│  phases (JSON array of ordered steps)                │
│  active, version                                     │
└─────────────────────────────────────────────────────┘
         │ 1:N
         ▼
┌─────────────────────────────────────────────────────┐
│  FLOW_STEPS                                          │
│  ──────────                                          │
│  id, flow_id, order, step_type                       │
│  agent_type, control_ids[] (which controls apply)    │
│  gate_conditions (JSON: what must pass before next)  │
│  context_paths[] (what files/data to inject)         │
│  timeout, max_retries                                │
└─────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────┐
│  AGENT_PROFILES                                      │
│  ──────────────                                      │
│  id, agent_type, base_prompt                         │
│  always_inject_controls[] (control IDs)              │
│  conditional_controls[] (with trigger_conditions)    │
│  self_check_items[] (checklist before return)        │
└─────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────┐
│  DRIFT_EVENTS                                        │
│  ────────────                                        │
│  id, session_id, agent_id, flow_step_id              │
│  drift_type, description, root_cause                 │
│  resolution (fix applied or escalated)               │
│  suggested_control (proposed new control)            │
│  auto_approved (bool), approved_by                   │
└─────────────────────────────────────────────────────┘
```

### How It Works

#### At Spawn Time (replaces static markdown injection)

```
1. Coordinator determines agent_type + phase + task context
2. Query: SELECT injection_text FROM controls
          WHERE enabled = true
          AND trigger_conditions MATCH (agent_type, phase, context)
          ORDER BY severity DESC
3. Compose agent prompt = base_prompt + relevant controls + task-specific context
4. Spawn agent with dynamically assembled prompt
```

This eliminates the copy-paste problem. The "Pattern Workflow (Mandatory)" block exists once in the database, tagged with `trigger_conditions: {agent_type: "*", phase: "*"}` (applies to all agents, all phases). The cargo truncation rule is tagged `{agent_type: ["ndp-rust-dev", "ndp-validator"], phase: "implementation"}`.

#### At Validation Time (replaces static checklists)

```
1. Validator queries: SELECT * FROM flow_steps
                      WHERE flow_id = current_flow
                      AND step_type = 'gate'
2. For each gate: evaluate gate_conditions against actual state
3. Record CONTROL_OUTCOMES for each control checked
4. Compute pass/fail from gate results
```

#### After Failure (the learning loop)

This is the key differentiator. When drift is detected:

```
1. Validator records DRIFT_EVENT with root_cause analysis
2. System queries: "Has this drift_type occurred before?"
3. If recurring (3+ occurrences):
   a. Generate candidate CONTROL (new injection_text)
   b. Flag for human review OR auto-approve if low severity
   c. Insert into CONTROLS with effectiveness_score = 0.5 (untested)
4. Over time, CONTROL_OUTCOMES data updates effectiveness_score
5. Controls with effectiveness_score < 0.2 after 10+ observations → auto-disable
```

**Example**: Agents keep forgetting to truncate cargo output, blowing context windows. After 3 drift events of type `context_bloat:cargo_output`:

- System proposes a new control: "Pipe all cargo commands through `| tail -30`"
- Tagged to trigger for `agent_type: [ndp-rust-dev, ndp-tester, ndp-validator]`
- Human approves (or auto-approves for WARN-severity)
- Control injected into future agent prompts
- Effectiveness tracked: does context bloat recur?

---

## The UI

A web interface where users can:

### Flow Builder
- Visual DAG editor for flows (drag agents into waves, draw dependency arrows)
- Click a flow step to see/edit which controls apply
- Preview the assembled prompt for any agent at any step
- Clone existing flows as templates (the "Swarm Composition Templates" from agent-routing.md become database entries)

### Control Manager
- CRUD for controls with live preview of injection_text
- Effectiveness dashboard: which controls actually prevent drift?
- "Suggested controls" feed from the learning loop (approve/reject/edit)
- Dependency graph: which controls overlap or conflict?

### Retrospective Dashboard
- Timeline of drift events per session
- Root cause clustering (what types of drift keep happening?)
- Before/after comparison: did adding control X reduce drift type Y?
- Agent performance heatmap: which agent types drift most on which controls?

### Prompt Debugger
- For any past session: show exactly what prompt each agent received
- Highlight which controls were injected and which were missing
- Compare: "this agent drifted — here's what a correctly-controlled prompt would have looked like"

---

## Migration Path

This isn't a rewrite. The existing markdown content IS the seed data.

### Phase 1: Extract (automated)
Parse the existing markdown files into structured records:
- Each "Pattern Workflow (Mandatory)" block → 1 control record
- Each "Swarm Participation" block → 1 control record
- Each self-check item → 1 control record
- Each flow (planning-protocol, implementation-protocol) → 1 flow with N steps
- Each agent definition → 1 agent profile

### Phase 2: Serve (runtime)
Build a thin layer that assembles agent prompts from the database instead of from raw markdown. The scrum-master queries the DB for "what controls apply to ndp-rust-dev in implementation phase?" instead of hoping the agent reads its own definition file.

### Phase 3: Learn (feedback loop)
Connect validator drift detection to write DRIFT_EVENTS and CONTROL_OUTCOMES. The NDP system already generates this kind of data (reflexion entries, glass box reports) — Unimatrix captures it natively instead of through a separate AgentDB MCP layer.

### Phase 4: UI
Build the flow builder, control manager, and retrospective dashboard. This is where the human-in-the-loop moves from "edit markdown files" to "approve suggested controls and tune flows visually."

---

## What This Enables That Markdown Can't

| Capability | NDP Prototype (Markdown + AgentDB) | Unimatrix |
|-----------|--------------------------|-----------|
| Single source of truth per control | No (copied 5x across md files) | Yes (one DB record) |
| Context-aware injection | No (agent reads entire file) | Yes (only relevant controls) |
| Effectiveness measurement | Partial (AgentDB reflexion, but disconnected from controls) | Yes (controls linked to outcomes) |
| Automatic control suggestions | No | Yes (from drift patterns) |
| Visual flow editing | No | Yes |
| Prompt debugging | No (ephemeral) | Yes (full audit trail) |
| A/B testing controls | No | Yes (enable/disable + measure) |
| Cross-project reuse | Copy files between repos | Export/import flows + controls |
| Unified persistence | Split (AgentDB SQLite + markdown + session memory) | Single DB with single API |
| Pattern/reflexion/learning | MCP tool calls to separate AgentDB service | Native — same DB as orchestration |

---

## Key Insight

The framework (as prototyped in NDP) is a prototype of a control plane built from three layers:
- **AgentDB** — permanent knowledge (patterns, reflexions, RL learning)
- **Markdown orchestration** — static protocols, agent definitions, rules, skills
- **Swarm coordination** — session-scoped memory for agent status/progress

Unimatrix replaces all three with a unified system. AgentDB, the markdown files, and the swarm memory layer are all subsumed — they become one database with one set of APIs. The pattern store, reflexion loop, drift detection, control injection, flow orchestration, and session coordination all live in the same place instead of being split across an MCP-backed SQLite DB, dozens of markdown files, and an ephemeral memory layer.

The markdown files are the prototype. AgentDB is the prototype's persistence layer. Unimatrix is the real thing.

### What stays as files (for now)

Claude Code has built-in conventions for `.claude/` directory structure — agent definitions, rules, and skills are loaded based on file paths and frontmatter. Some of these may need to remain as files to stay compatible with Claude Code's runtime. The exact boundary between "what Unimatrix serves dynamically" and "what must stay on disk for Claude Code" is a design question for later. The database should be the source of truth regardless; files can be generated artifacts if needed.

---

## Open Questions

1. **Granularity**: Should controls be atomic (one instruction per record) or composite (a block of related instructions)? Atomic is more flexible but risks prompt fragmentation.

2. **Auto-approve threshold**: Which severity levels can the system auto-approve new controls for? Proposal: WARN-severity auto-approves after human review of the first 3 suggestions; FAIL-severity always requires human approval.

3. **Token budget**: If too many controls match, the prompt gets bloated. Need a priority/budget system — inject highest-severity controls first, stop at token limit.

4. **Cold start**: New projects start with zero drift data. Seed the learning system with the existing drift patterns from NDP as training data? Or let each project learn from scratch?

5. **Multi-model**: Different LLM providers/models may respond differently to the same control injection. Should effectiveness_score be per-model?
