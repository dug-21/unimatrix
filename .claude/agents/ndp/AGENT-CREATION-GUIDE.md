# Unimatrix Agent Creation Guide

This guide defines how to create new agents for the Unimatrix that align with established patterns, maintain consistency across the team, and leverage the pattern memory system effectively.

---

## Core Principles

### 1. Agents Know WHEN, Skills Know HOW

**Agents contain:**
- Domain expertise and design principles
- Decision-making guidance ("how to think")
- References to skills BY NAME ONLY
- Stable technology fundamentals

**Skills contain:**
- CLI commands and syntax
- Step-by-step procedures
- Tool-specific details

**Never put CLI commands in agent definitions.** If an agent needs to execute something, reference the skill that contains those commands.

### 2. The Stability Boundary

Content in agent definitions should be **stable over time**. Implementation specifics that may change belong in **patterns** (retrieved via `get-pattern`).

| In Agent (Stable) | In Patterns (May Change) |
|-------------------|--------------------------|
| Design principles | Trait signatures |
| Technology fundamentals | Struct definitions |
| Architectural concepts | Current field names |
| Decision frameworks | Implementation examples |
| Skill references | Configuration formats |

**Rule of thumb:** If it might change when the codebase evolves, put it in a pattern. If it's a fundamental concept that guides thinking, put it in the agent.

### 3. Teach How to Think, Not What to Implement

Agents should provide **design principles** that guide decision-making, not specific code to copy. Example:

**Wrong (implementation-specific):**
```markdown
## Implementation
Use this struct:
struct TimeSeriesPoint {
    timestamp: DateTime<Utc>,
    fields: HashMap<String, Value>,
}
```

**Right (principle-based):**
```markdown
## Design Principles
- **Timestamp-First**: All time-series data uses DateTime<Utc> as primary ordering
- **Flexible Schema**: Fields stored as key-value maps to support varying data shapes

For CURRENT struct definitions and implementations:
→ Use `get-pattern` skill with domain "development"
```

---

## Agent File Structure

### Required Frontmatter

```yaml
---
name: ndp-{role}
type: developer | engineer | specialist | coordinator
scope: narrow | specialized | broad
description: One-line description of the agent's focus
capabilities:
  - capability_1
  - capability_2
  - capability_3
---
```

**Scope definitions:**
- `narrow`: Single technology or layer (e.g., Grafana only, Parquet only)
- `specialized`: Domain expertise across multiple technologies (e.g., data quality, analytics)
- `broad`: Cross-cutting concerns, coordination (e.g., architect, scrum-master)

### Required Sections

Every Unimatrix agent MUST have these sections:

```markdown
# Unimatrix {Role Name}

You are the {role description} for the Unimatrix. {1-2 sentences on primary responsibility}.

## Your Scope

- **{Scope level}**: {What this agent focuses on}
- {Bullet list of specific responsibilities}

## MANDATORY: Before Any Implementation

### 1. Get Relevant Patterns

Use the `get-pattern` skill to retrieve {domain} patterns for Unimatrix.

### 2. Read Architecture Documents

- {List relevant architecture docs}

## Design Principles (How to Think)

{Numbered list of stable design principles that guide this agent's work}

For CURRENT {specifics}, use `get-pattern` skill with domain "{domain}".

## {Domain-Specific Sections}

{Agent-specific content - architecture diagrams, decision frameworks, etc.}

## Swarm Coordination

**This section activates ONLY when your spawn prompt includes `Your agent ID: <id>`.**
If no agent ID was provided, skip this section entirely.

{Copy the standard Swarm Coordination block from any existing Unimatrix agent.
This enables agents to report status/progress/completion to shared memory
when participating in a swarm. The coordinator passes agent IDs; agents
handle the rest automatically. Do NOT include this section in ndp-scrum-master.}

## Related Agents

- `ndp-{agent}` - {When to consult}

## Related Skills

- `ndp-github-workflow` - Branch, commit, PR conventions (REQUIRED)
- `get-pattern` - Retrieve {domain} patterns (REQUIRED)
- `save-pattern` - Store new {domain} patterns (REQUIRED)
- `reflexion` - Record whether retrieved patterns helped (REQUIRED)

---

## Pattern Integration (REQUIRED)

### BEFORE {Work Type}

Use `get-pattern` skill with domain "{domain}" to retrieve:
- {Pattern type 1}
- {Pattern type 2}
- {Pattern type 3}

### DURING {Work Type}

Track what you learn:
- {Learning type 1}
- {Learning type 2}
- {Learning type 3}

### AFTER {Work Type}

1. Use `reflexion` skill to record whether retrieved patterns helped
2. Use `save-pattern` skill with domain "{domain}" to store new approaches
```

---

## Pattern Domains

Each agent has a primary pattern domain for storing and retrieving patterns:

| Agent | Domain | Pattern Types |
|-------|--------|---------------|
| `ndp-rust-dev` | `development` | Trait implementations, error handling, async patterns |
| `ndp-tester` | `testing` | Test strategies, mocking approaches, fixtures |
| `ndp-architect` | `architecture` | ADRs, system design, cross-cutting concerns |
| `ndp-scrum-master` | `procedures` | Workflows, checklists, coordination |
| `ndp-meteorologist` | `weather` | NWS interpretation, forecast evaluation |
| `ndp-air-quality-specialist` | `air-quality` | AQI calculations, sensor calibration, EPA standards |
| `ndp-parquet-dev` | `storage` | Bronze layer, WAL, Parquet optimization |
| `ndp-timescale-dev` | `silver` | Hypertables, continuous aggregates, ETL |
| `ndp-dq-engineer` | `data-quality` | Validation rules, transparency tables |
| `ndp-analytics-engineer` | `analytics` | Silver→Gold transforms, metric definitions |
| `ndp-feature-engineer` | `features` | Windowing, aggregations, ML features |
| `ndp-ml-engineer` | `ml` | ruv-FANN, training, inference |
| `ndp-grafana-dev` | `dashboards` | Panel configuration, queries, alerting |
| `ndp-alert-engineer` | `alerting` | Thresholds, notifications, rule engine |

When creating a new agent, assign a domain that doesn't overlap with existing agents or clearly extends an existing domain.

---

## Required Skills Reference

Every Unimatrix agent MUST reference these four skills:

### 1. `ndp-github-workflow`
- **When:** ALL git operations (branch, commit, PR)
- **Why:** Ensures consistent branch naming (`feature/{phase}-{NNN}`), commit format, PR templates

### 2. `get-pattern`
- **When:** BEFORE implementing anything
- **Why:** Retrieves established project patterns, prevents reinventing solutions

### 3. `save-pattern`
- **When:** AFTER discovering new reusable approaches
- **Why:** Captures knowledge for future agents/sessions

### 4. `reflexion`
- **When:** AFTER using patterns from `get-pattern`
- **Why:** Provides feedback that improves pattern recommendations over time

**Format in agent file:**
```markdown
## Related Skills

- `ndp-github-workflow` - Branch, commit, PR conventions (REQUIRED)
- `get-pattern` - Retrieve {domain} patterns (REQUIRED)
- `save-pattern` - Store new {domain} patterns (REQUIRED)
- `reflexion` - Record whether retrieved patterns helped (REQUIRED)
```

---

## Pattern Integration Section

The Pattern Integration section is REQUIRED and follows a standard format:

```markdown
## Pattern Integration (REQUIRED)

### BEFORE {Work Type}

Use `get-pattern` skill with domain "{domain}" to retrieve:
- {Specific pattern type relevant to this agent}
- {Another pattern type}
- {Third pattern type}

### DURING {Work Type}

Track what you learn:
- {Type of discovery this agent might make}
- {Another discovery type}
- {Third discovery type}

### AFTER {Work Type}

1. Use `reflexion` skill to record whether retrieved patterns helped
2. Use `save-pattern` skill with domain "{domain}" to store new approaches
```

**Customize the {Work Type} and bullet points for each agent's domain.**

---

## Creating Feature-Aware Patterns

When agents save patterns, include feature identifiers for cross-agent discovery:

**Architect saves:**
```
domain: "architecture"
tags: ["dp-001", "timescaledb-schema", "silver-layer"]
```

**Other agents query:**
```
get-pattern with "dp-001 schema"
get-pattern with "dp-001 silver layer"
```

This enables agents working on the same feature to find each other's patterns.

---

## Agent Collaboration Model

### Team Construction by Work Type

| Work Type | Core Team | Domain Specialists |
|-----------|-----------|-------------------|
| Schema/ETL | architect, timescale-dev, dq-engineer | meteorologist OR air-quality-specialist |
| Analytics | analytics-engineer, grafana-dev | domain specialist for metrics |
| New Data Source | architect, rust-dev, parquet-dev | domain specialist for validation |
| ML/Predictions | feature-engineer, ml-engineer | domain specialist for features |
| Alerts | alert-engineer, rust-dev | air-quality-specialist for thresholds |

### Collaboration Rules

1. **Always include domain specialist** when working with weather or air quality data
2. **Always include dq-engineer** when schema or ETL changes affect data quality
3. **Always include architect** for cross-cutting or schema changes
4. **Consult domain specialists first** before implementing domain logic in code

---

## Example: Well-Structured Agent

Reference `ndp-rust-dev.md` as the template for the "teach how to think" approach:

```markdown
## Design Principles (How to Think)

These principles guide ALL Rust development in Unimatrix:

1. **Domain Adapter Pattern** - All data sources/stores implement core traits
2. **Configuration-Driven** - Behavior defined in YAML configs, not hardcoded
3. **Async-First** - tokio runtime, mpsc channels for data flow
4. **Graceful Shutdown** - CancellationToken for coordinated cleanup
5. **Structured Errors** - CoreError enum with context propagation
6. **Tracing Over Logging** - Use `tracing` macros with structured fields

For CURRENT trait signatures, struct definitions, and implementation patterns:
→ Use `get-pattern` skill with domain "development" before implementing
```

Notice:
- Principles are stable concepts that won't change
- Specific implementations deferred to `get-pattern`
- No code examples that might become outdated

---

## Checklist: New Agent Creation

Before finalizing a new agent, verify:

- [ ] Frontmatter includes name, type, scope, description, capabilities
- [ ] Scope section clearly defines boundaries
- [ ] "MANDATORY: Before Any Implementation" section present
- [ ] Design Principles section focuses on "how to think"
- [ ] No CLI commands anywhere in the document
- [ ] All four required skills listed (ndp-github-workflow, get-pattern, save-pattern, reflexion)
- [ ] Pattern Integration section with BEFORE/DURING/AFTER
- [ ] Domain assigned and documented
- [ ] Related Agents section lists collaboration points
- [ ] No implementation-specific code that may become outdated

---

## Updating Existing Agents

When modifying an existing agent:

1. **Check stability boundary** - Is this content stable or might it change?
2. **Remove CLI commands** - Move them to skills if needed
3. **Add missing sections** - Especially Pattern Integration and reflexion skill
4. **Update domain references** - Ensure domain is consistent throughout
5. **Test with get-pattern** - Verify patterns exist for referenced domains

---

## The Pattern Memory System

Understanding how patterns flow through the system:

```
Feature Work (Parallel Agents)       After Feature (User-Invoked)
──────────────────────────────       ────────────────────────────
Architect   ─→ reflexion  ─┐
Rust-dev    ─→ reflexion  ─┼──→  User: /learner  ─→  Auto-discovered
Tester      ─→ reflexion  ─┤                          patterns
Specialist  ─→ reflexion  ─┘

Each agent:                          Learner skill:
1. get-pattern (BEFORE)              1. Analyzes reflexion history
2. Does work                         2. Finds successful approaches
3. reflexion (AFTER)                 3. Creates new patterns
4. save-pattern (if new discovery)   4. Consolidates into skills
```

**Key insight:** The `learner` skill is USER-INVOKED after feature completion, not run by agents during parallel work. This solves the timing problem where scrum-master would finish before other agents complete their reflexions.

---

## Summary

Creating effective Unimatrix agents requires:

1. **Clear boundaries** - Know what goes in agent vs patterns
2. **Principle-based guidance** - Teach how to think, not what to implement
3. **Skill references only** - No CLI commands in agents
4. **Required skills** - Always include the four core skills
5. **Pattern integration** - Standard BEFORE/DURING/AFTER format
6. **Domain assignment** - Clear, non-overlapping pattern domains
7. **Feature awareness** - Include feature identifiers in patterns

Follow this guide to create agents that maintain consistency, leverage collective knowledge, and improve over time through the pattern memory system.
