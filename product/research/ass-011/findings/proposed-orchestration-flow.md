# Proposed Flow: Passive Observation + Context Engine

**Date:** 2026-02-27
**Spike:** ASS-011
**Status:** Revised after viability review

> **Note (2026-02-27):** For this version we determined adding the hook metadata did not provide enough value to implement at this time. The risks of adding silent interface keys vs the incremental value did not weigh out as something that needed to be done at this time. Retained for historical purposes.

---

## Core Principle

Unimatrix does not orchestrate. `.claude/` files orchestrate. Hooks passively observe. Unimatrix accumulates what it observes and serves increasingly relevant project context through `context_briefing`.

Over time, Unimatrix gets smarter — not because it controls more, but because it sees more and connects more dots.

---

## Architecture: Three Layers

| Layer | Responsibility | Does NOT do |
|-------|---------------|-------------|
| **`.claude/` files** | Orchestration, routing, agent behavior, project conventions | Change dynamically; depend on Unimatrix availability |
| **Hooks** | Passive metadata collection, session_id tagging | Inject context, enforce gates, block actions, contain workflow logic |
| **Unimatrix** | Knowledge bank + project context engine; connects the dots across sessions via `session_id` + `feature_cycle` | Drive orchestration, manage workflow state, control agent lifecycle |

### What lives where

| Content | Home | Why |
|---------|------|-----|
| "Feature work → spawn uni-scrum-master" | CLAUDE.md | Project routing — always loaded, zero latency |
| Project structure, directory conventions | CLAUDE.md | Stable, every agent needs it, rarely changes |
| Feature naming conventions (`{phase}-{NNN}`) | CLAUDE.md | Project-level, not role-dependent |
| Non-negotiable rules (anti-stub, no root files) | CLAUDE.md | Universal enforcement |
| Agent role, duties, workflow procedures | `.claude/agents/` | Full agent definitions — current size, current behavior |
| Protocol logic (design session, delivery, gates) | `.claude/protocols/` | Orchestration procedures — agents follow these |
| Accumulated conventions, patterns, decisions | Unimatrix knowledge base | Searchable, correctable, evolves across features |
| Role-specific relevant knowledge | Unimatrix via `context_briefing` | Served dynamically — right knowledge to right role |
| Session timeline, agent tracking | Hooks (passive observation) | No agent cooperation needed |
| Cross-session feature linkage | Unimatrix via `feature_cycle` | Ties design → implementation across sessions |

---

## The Flow: Beginning to End

### Scenario: User says "Let's begin feature col-002"

---

#### Act 1: Session Bootstrap

**User types:** "Let's begin feature col-002"

**UserPromptSubmit hook fires.**
```
Receives: { session_id: "4c0ee78c", prompt: "Let's begin feature col-002",
            cwd: "/workspaces/unimatrix", permission_mode: "bypassPermissions" }
Records:  { session: "4c0ee78c", event: "session_start", prompt_hash, timestamp }
```

> *LLM reads CLAUDE.md. Sees: "Feature work → spawn uni-scrum-master." Decides to spawn.*

---

#### Act 2: Scrum-Master Activation

**LLM spawns `uni-scrum-master`.**

**SubagentStart hook fires.**
```
Receives: { session_id: "4c0ee78c", agent_type: "uni-scrum-master", agent_id: "a7f3b2c1" }
Records:  { session: "4c0ee78c", event: "agent_start", agent_type: "uni-scrum-master",
            agent_id: "a7f3b2c1", timestamp }
```

> *Scrum-master reads its full agent definition file (~150-200 lines): role boundaries, wave management, gate procedures, GitHub lifecycle. Also reads the design protocol. Has complete orchestration instructions without any Unimatrix call.*

**Scrum-master calls `context_briefing(role="scrum-master", task="design col-002: retrospective pipeline", feature="col-002")`.**

**PreToolUse hook fires.**
```
Receives: { session_id: "4c0ee78c", tool_name: "mcp__unimatrix__context_briefing",
            tool_input: { role: "scrum-master", task: "design col-002...", feature: "col-002" },
            tool_use_id: "toolu_01..." }
Injects:  agent_id: "4c0ee78c" into tool_input (merge, not replace)
Records:  { session, tool: "context_briefing", role_claimed: "scrum-master",
            feature: "col-002", timestamp }
```

**Unimatrix returns project context** (not orchestration — the agent file handles that):

```
Briefing for scrum-master: design col-002

Conventions: 4
  - Feature directories follow product/features/{phase}-{NNN}/ structure
  - ADRs stored in architecture/ subdirectory with ADR-NNN-{name}.md naming
  - Test infrastructure is cumulative across features
  - Agent reports go in agents/ subdirectory

Relevant Knowledge: 3
  #5:  Roadmap milestones 5-6 — orchestration and real-time interface goals
  #42: Prior col-001 outcome tracking patterns (related feature)
  #38: Observation pipeline findings from ass-010 (feeds into col-002 design)

Prior Decisions (feature-boosted):
  ADR-001: Confidence base scores (affects retrospective scoring)
  ADR-003: Conflict heuristic design (relevant to contradiction analysis in retrospectives)
```

**PostToolUse hook fires.**
```
Receives: { session_id: "4c0ee78c", tool_name: "context_briefing",
            tool_input: { role: "scrum-master", task: "...", feature: "col-002", agent_id: "4c0ee78c" },
            tool_result: "<briefing content above>" }
Records:  { session, event: "briefing_served", role: "scrum-master", feature: "col-002",
            entries_served: [5, 42, 38], timestamp }
```

> *Scrum-master now has orchestration instructions (from agent file/protocol) AND project context (from Unimatrix). Knows which agents to spawn from the protocol. Knows relevant project history from the briefing. Begins Wave 1.*

---

#### Act 3: Research Phase

> *Scrum-master spawns `uni-researcher` with task: "Produce SCOPE.md for col-002: retrospective pipeline."*

**SubagentStart hook fires.**
```
Receives: { session_id: "4c0ee78c", agent_type: "uni-researcher", agent_id: "b9e4d3f2" }
Records:  { session, event: "agent_start", agent_type: "uni-researcher",
            agent_id: "b9e4d3f2", timestamp }
```

> *Researcher reads its agent definition file (~80-120 lines): how to explore problem spaces, SCOPE.md structure and required sections, research methodology, output expectations.*

**Researcher calls `context_briefing(role="researcher", task="col-002 retrospective pipeline scope", feature="col-002")`.**

**PreToolUse hook fires.**
```
Receives: { session_id: "4c0ee78c", tool_name: "context_briefing",
            tool_input: { role: "researcher", task: "col-002...", feature: "col-002" } }
Injects:  agent_id: "4c0ee78c"
Records:  { session, tool: "context_briefing", role_claimed: "researcher",
            feature: "col-002", timestamp }
```

**Unimatrix returns researcher-relevant context:**

```
Briefing for researcher: col-002 retrospective pipeline scope

Conventions: 2
  - Research questions use RQ-N numbering
  - Constraints section lists what NOT to change in existing crates

Relevant Knowledge: 3
  #5:  Roadmap — M5 goals for orchestration engine
  #23: Retrospective patterns observed in prior features
  #38: Observation pipeline findings (ass-010 — data collection approach)
```

**PostToolUse hook fires.**
```
Receives: { session_id, tool_result: "<briefing>" }
Records:  { session, event: "briefing_served", role: "researcher", feature: "col-002",
            entries_served: [5, 23, 38], timestamp }
```

> *Researcher explores codebase, reads prior research docs, reads PRODUCT-VISION.md, drafts SCOPE.md.*

**Researcher writes SCOPE.md.**

**PostToolUse hook fires (on Write).**
```
Receives: { session_id: "4c0ee78c", tool_name: "Write",
            tool_input: { file_path: "/workspaces/unimatrix/product/features/col-002/SCOPE.md" },
            tool_result: "File created successfully..." }
Records:  { session, event: "file_written",
            path: "product/features/col-002/SCOPE.md", timestamp }
```

> *Researcher may call `context_search` or `context_store` during work — each call gets session_id tagged via PreToolUse. Unimatrix accumulates what was accessed and what was stored.*

> *Researcher finishes and returns results to scrum-master.*

**SubagentStop hook fires.**
```
Receives: { session_id: "4c0ee78c", agent_type: "uni-researcher", agent_id: "b9e4d3f2" }
Records:  { session, event: "agent_stop", agent_type: "uni-researcher",
            agent_id: "b9e4d3f2", timestamp }
```

---

#### Act 4: Architecture & Specification

> *Scrum-master reviews SCOPE.md. Decides to spawn architect and spec writer (per protocol).*

**For each agent, the same pattern repeats:**

1. **SubagentStart** → hook records `{ session, agent_type, agent_id }`

2. > *Agent reads its full agent definition file. Has complete role instructions.*

3. **Agent calls `context_briefing(role=X, task=Y, feature="col-002")`**
   - **PreToolUse** → tags with session_id, records role + feature
   - **Unimatrix** → returns role-relevant conventions + feature-relevant knowledge
   - **PostToolUse** → records entries served

4. > *Agent does work — reads files, writes deliverables, may call context_search/store.*

5. **PostToolUse on Write/Edit** → hook records each file path written

6. > *Agent returns to scrum-master.*

7. **SubagentStop** → hook records `{ session, agent_type, agent_id }`

**Unimatrix sees all of this via session_id.** It knows:
- Scrum-master, researcher, architect, spec writer all briefed for col-002 in the same session
- Each was served different knowledge entries based on their role
- Specific files were written to `product/features/col-002/`
- Specific knowledge entries were accessed (search) and created (store)

---

#### Act 5: Validation Gate

> *Scrum-master follows protocol — spawns `uni-validator` with gate criteria from the protocol file.*

**SubagentStart hook fires.**
```
Receives: { session_id: "4c0ee78c", agent_type: "uni-validator", agent_id: "e5f6a7b8" }
Records:  { session, event: "agent_start", agent_type: "uni-validator", timestamp }
```

> *Validator reads its agent definition. Knows gate checking procedures.*

**Validator calls `context_briefing(role="validator", task="gate 3a for col-002", feature="col-002")`.**

**Unimatrix returns validator-relevant context:**
```
Briefing for validator: gate 3a for col-002

Conventions: 2
  - Gate reports use PASS/REWORKABLE FAIL/SCOPE FAIL classification
  - Gate reports go in reports/ subdirectory

Relevant Knowledge: 1
  #31: Prior gate report format and checklist patterns
```

> *Validator checks deliverables against gate criteria (from its agent file), produces gate report, returns.*

**PostToolUse hook fires (on Write — gate report).**
```
Records: { session, event: "file_written",
           path: "product/features/col-002/reports/gate-3a-report.md", timestamp }
```

**SubagentStop hook fires.**
```
Records: { session, event: "agent_stop", agent_type: "uni-validator", timestamp }
```

---

#### Act 6: Session Wrap-Up

> *Scrum-master reads gate results. Summarizes session to user. Creates GitHub issue if needed.*

**Stop hook fires.**
```
Receives: { session_id: "4c0ee78c" }
Records:  { session, event: "session_end", timestamp }
```

---

## What Unimatrix Knows After This Session

Assembled entirely from passive hook observation + `session_id` correlation + `feature_cycle` tagging:

```
Session: 4c0ee78c
Feature: col-002 (from feature param on briefing calls)

Agent Timeline:
  00:01 uni-scrum-master started (a7f3b2c1)
  00:02   briefing served: role=scrum-master, feature=col-002, entries=[5,42,38]
  00:03 uni-researcher started (b9e4d3f2)
  00:04   briefing served: role=researcher, feature=col-002, entries=[5,23,38]
  00:12   file written: product/features/col-002/SCOPE.md
  00:13   context_store called (new knowledge entry created)
  00:14 uni-researcher stopped
  00:15 uni-architect started (c3d4e5f6)
  00:16   briefing served: role=architect, feature=col-002, entries=[5,42]
  00:25   file written: product/features/col-002/architecture/ARCHITECTURE.md
  00:26   file written: product/features/col-002/architecture/ADR-001-foo.md
  00:27 uni-architect stopped
  00:28 uni-specification started (d4e5f6a7)
        ...
  00:45 uni-validator started (e5f6a7b8)
  00:46   briefing served: role=validator, feature=col-002, entries=[31]
  00:50   file written: product/features/col-002/reports/gate-3a-report.md
  00:51 uni-validator stopped
  00:52 session ended

Knowledge Served:    entries [5, 23, 31, 38, 42] across 5 briefings
Knowledge Created:   1 new entry (by researcher)
Files Written:       7 files in product/features/col-002/
```

### Cross-Session Continuity

When Session 2 (implementation) begins days later:
- Different `session_id` (new session)
- Same `feature_cycle: "col-002"` on briefing calls
- Unimatrix knows col-002 had a design session: which knowledge was served, what was produced
- Implementation briefing can reference: "Design session produced SCOPE.md, ARCHITECTURE.md, 1 ADR. Validator passed gate 3a."
- Knowledge entries created during design are now available and boosted by co-access patterns

---

## Layer Responsibilities (Final)

### `.claude/` Files — Orchestration

**CLAUDE.md** (current size, project conventions):
- Project identity, vision, structure
- Feature naming, directory conventions
- Non-negotiable rules
- Routing: "Feature work → spawn uni-scrum-master"

**Agent files** (current size, full role definitions):
- Role identity and boundaries
- Duties and procedures per phase
- Gate management logic
- Output expectations

**Protocols** (current size, workflow procedures):
- Design session steps
- Delivery session steps
- Gate criteria and flow

These files are version-controlled, reviewed, and stable. They are the orchestration layer. Unimatrix does not replace them.

### Hooks — Passive Metadata

| Hook | Fires on | Records |
|------|----------|---------|
| **UserPromptSubmit** | Every user prompt | `{ session_id, timestamp }` |
| **SubagentStart** | Agent spawn | `{ session_id, agent_type, agent_id, timestamp }` |
| **SubagentStop** | Agent completion | `{ session_id, agent_type, agent_id, timestamp }` |
| **PreToolUse** | `mcp__unimatrix__context_.*` | Merges `agent_id` into tool_input |
| **PostToolUse** | `mcp__unimatrix__context_.*` | `{ session_id, tool, role, feature, entries_served, timestamp }` |
| **PostToolUse** | `Write\|Edit` | `{ session_id, file_path, timestamp }` |
| **Stop** | Session end | `{ session_id, timestamp }` |

No workflow logic. No context injection. No gate enforcement. No blocking. Pure observation.

### Unimatrix — Context Engine

What Unimatrix does today (unchanged):
- Stores and retrieves knowledge entries (conventions, decisions, patterns)
- Serves role-relevant context via `context_briefing`
- Tracks usage, confidence, co-access, contradictions
- Provides correction chains, deprecation, quarantine

What hooks ADD (new capability):
- `session_id` on every MCP call → Unimatrix can correlate all activity in a session
- `feature` param on briefing calls → Unimatrix can link sessions to features
- Agent spawn/stop observations → Unimatrix knows which roles participated
- File write observations → Unimatrix knows what deliverables were produced
- Entry access patterns per session → Unimatrix knows what knowledge was useful

What Unimatrix does with this (future, incremental):
- Session timeline reconstruction (who did what, when)
- Cross-session feature tracking (design → implementation continuity)
- Deviation detection (expected patterns vs observed patterns)
- Knowledge relevance refinement (entries served but never used → lower confidence)
- Process intelligence (col-002 retrospective pipeline, fed by observation data)

---

## What This Changes vs. Previous Proposal

| Previous Proposal | This Revision |
|-------------------|---------------|
| Agent files shrink to ~15-20 lines | **Agent files stay current size** — they own orchestration |
| `context_briefing` returns workflow instructions | **`context_briefing` returns knowledge only** — conventions, patterns, relevant entries |
| Hooks inject workflow context via SubagentStart | **Hooks only observe** — no context injection except session_id tagging |
| Workflow state machine in Unimatrix | **No state machine** — session correlation via passive observation |
| `workflow_start/advance/report` MCP tools | **No new tools** — existing 9 tools are sufficient |
| Unimatrix drives orchestration | **Unimatrix serves context, `.claude/` drives orchestration** |

## What This Preserves

- All existing `.claude/` files, agents, protocols — no migration needed
- All 9 existing MCP tools — no new tools required
- The product vision: "delivering the right context to the right agent at the right workflow moment"
- The SDK hybrid path for future autonomous workflows
- The unimatrix-client path for future multi-repo + transport security

## What's New

- Hooks give Unimatrix eyes and ears it didn't have before
- Session_id + feature_cycle let Unimatrix connect the dots across agents and sessions
- Observation data feeds the retrospective pipeline (col-002) with real session evidence
- Knowledge relevance improves over time based on what agents actually access
- The foundation for process intelligence: patterns emerge from observation, not opinion
