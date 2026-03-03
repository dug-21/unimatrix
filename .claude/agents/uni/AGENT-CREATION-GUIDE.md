# Unimatrix Agent Creation Guide (uni-)

How to create new `uni-` agents for Unimatrix product development.

---

## Core Principles

### 1. Agents Know WHEN, Not HOW to Coordinate

Agents receive context in their spawn prompt and return results directly. There is no registration, shared memory, hooks, or coordination layer. The coordinator (uni-scrum-master) manages all orchestration.

### 2. Teach How to Think, Not What to Implement

Agent definitions contain **design principles** that guide decision-making, not specific code to copy.

**Wrong (implementation-specific):**
```markdown
## Implementation
Use this struct:
struct Entry { id: String, content: String }
```

**Right (principle-based):**
```markdown
## Design Principles
- **Type Safety**: Use distinct types for IDs rather than raw strings
- **Serde-First**: All data structures derive Serialize/Deserialize for storage
```

### 3. The Stability Boundary

Content in agent definitions should be **stable over time**.

| In Agent (Stable) | Not In Agent (May Change) |
|-------------------|--------------------------|
| Design principles | Specific struct definitions |
| Architectural concepts | Current field names |
| Decision frameworks | Implementation examples |
| Role boundaries | Configuration formats |

---

## Agent File Structure

### Required Frontmatter

```yaml
---
name: uni-{role}
type: specialist | developer | coordinator | gate | synthesizer
scope: narrow | specialized | broad
description: One-line description of the agent's focus
capabilities:
  - capability_1
  - capability_2
---
```

**Scope definitions:**
- `narrow`: Single technology or component
- `specialized`: Domain expertise across multiple components
- `broad`: Cross-cutting concerns, coordination

### Required Sections

Every `uni-` agent MUST have these sections:

```markdown
# Unimatrix {Role Name}

{1-2 sentences on primary responsibility.}

## Your Scope

- **{Scope level}**: {What this agent focuses on}
- {Bullet list of specific responsibilities}

## What You Receive

{What the coordinator provides in the spawn prompt}

## What You Produce

{Artifacts and their file paths}

## Design Principles (How to Think)

{Numbered list of stable design principles}

## What You Return

{Summary format returned to the coordinator}

---

## Swarm Participation

**Activates ONLY when your spawn prompt includes `Your agent ID: <id>`.**

When part of a swarm, write your agent report to
`product/features/{feature-id}/agents/{agent-id}-report.md` on completion.

## Self-Check (Run Before Returning Results)

{Checklist of verifications before returning}
```

---

## What NOT to Include

These items from the NDP agent system do NOT apply to `uni-` agents:

| Item | Why Not |
|------|---------|
| `get-pattern` / `save-pattern` / `reflexion` skills | NDP pattern store — uni- agents use `/query-patterns` and Unimatrix `context_store` |
| Pattern Integration (REQUIRED) section | NDP-specific — uni- agents use `/query-patterns` in MANDATORY sections |
| Pattern Workflow (Mandatory) section | NDP-specific |
| `ndp-github-workflow` skill reference | Not created for uni- yet |
| AgentDB references | Doesn't exist yet |
| NDP domain specifics (Bronze/Silver/Gold, Parquet, TimescaleDB, ARM64, DuckDB, Polars) | NDP-specific |
| NDP crate paths (`crates/ndp-lib`, `apps/air-quality-app`, etc.) | NDP-specific |
| `CoreError` references | Use generic "project error type" |
| Coordination layer / shared memory / agent registration | Not used — spawn via Task tool |
| Hooks (pre-task, post-task, etc.) | Not used |
| Model routing / Agent Booster | Not used |

---

## Agent Coordination Model

The `uni-` agent coordination model is simple:

1. **Coordinator spawns agent** via `Task` tool with context in the prompt
2. **Agent reads files** specified in the prompt (not pasted into the prompt)
3. **Agent does work** — writes artifacts to disk
4. **Agent writes report** to `product/features/{id}/agents/{agent-id}-report.md`
5. **Agent returns summary** — file paths, results, issues

No registration. No shared memory. No hooks. No status writes to coordination paths.

---

## Checklist: New Agent Creation

Before finalizing a new `uni-` agent, verify:

- [ ] Frontmatter includes name (`uni-{role}`), type, scope, description, capabilities
- [ ] "Your Scope" section clearly defines boundaries
- [ ] "What You Receive" describes spawn prompt contents
- [ ] "What You Produce" lists artifacts with file paths
- [ ] "Design Principles" section focuses on "how to think"
- [ ] "What You Return" describes return format to coordinator
- [ ] Swarm Participation section with agent report path
- [ ] Self-Check section with verifiable checklist
- [ ] No NDP-specific content (see "What NOT to Include" table)
- [ ] No NDP pattern store references (get-pattern, save-pattern, reflexion) — use `/query-patterns` instead
- [ ] No coordination layer references (registration, shared memory, hooks)
- [ ] No implementation-specific code that may become outdated
- [ ] Agent listed in `uni-agent-routing.md` roster

---

## Session and Phase Mapping

When creating a new agent, determine where it fits:

| Session | Phase/Stage | Current Agents |
|---------|------------|----------------|
| Session 1 | Phase 1 (Research) | uni-researcher |
| Session 1 | Phase 2a (Parallel Design) | uni-architect, uni-specification, uni-risk-strategist |
| Session 1 | Phase 2b (Vision Check) | uni-vision-guardian |
| Session 1 | Phase 2c (Synthesis) | uni-synthesizer |
| Session 2 | Stage 3a (Component Design) | uni-pseudocode, uni-tester |
| Session 2 | Stage 3b (Implementation) | uni-rust-dev |
| Session 2 | Stage 3c (Testing) | uni-tester |
| Session 2 | Gates 3a/3b/3c | uni-validator |
| Both | Coordination | uni-scrum-master |

New agents should fit into an existing phase/stage. If a new agent requires a new phase, update the protocols first.
