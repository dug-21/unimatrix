# Proposal C: Control Structure -- Split Source of Truth

## What Stays in `.claude/` Files

### Thin-Shell Agent Definition (Proposal C model)

```yaml
---
name: ndp-rust-dev
type: developer
scope: general
description: General Rust developer for Unimatrix
---

# Unimatrix Rust Developer

You are a Rust developer for Unimatrix.

## Orientation (MANDATORY FIRST STEP)

Before starting any work, call:
  context_briefing(role: "ndp-rust-dev", task: "<your assigned task>", phase: "<current phase>")

This returns your conventions, patterns, and relevant process knowledge. Apply it.

## Design Principles

1. Domain Adapter Pattern -- all data sources/stores implement core traits
2. Configuration-Driven -- behavior in YAML, not hardcoded
3. Async-First -- tokio runtime, mpsc channels
4. Graceful Shutdown -- CancellationToken for coordinated cleanup
5. Structured Errors -- CoreError enum with context propagation
6. Tracing Over Logging -- `tracing` macros with structured fields

## Self-Check (Run Before Returning Results)

- [ ] `cargo build --workspace` passes
- [ ] `cargo test --workspace` passes
- [ ] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME` in non-test code
- [ ] All modified files within scope defined in brief
- [ ] `/reflexion` called for each pattern retrieved

## Outcome Reporting (Before Handoff)

Call context_store with category "outcome" to record:
- Entries retrieved and whether they helped (from reflexion data)
- Blockers encountered and how resolved
- Task completion status

## Related Agents

- ndp-architect, ndp-tester, ndp-scrum-master
```

**Compare to current full file**: 162 lines -> ~45 lines. The missing ~117 lines (naming conventions, project structure, implementation approach, code quality checklist) are now **expertise in Unimatrix**, retrieved at runtime via `context_briefing`.

### What else stays in files

| File Type | Content in Proposal C | What moved to Unimatrix |
|-----------|----------------------|------------------------|
| Agent `.md` | Identity + pull directive + self-checks | Conventions, patterns, checklists, project structure |
| `CLAUDE.md` | Constitutional rules (unchanged) | Nothing -- sacred, never generated |
| `protocols/*.md` | Wave structure, gate definitions | Nothing initially; process proposals may suggest changes |
| `rules/*.md` | File-pattern triggers (unchanged) | Nothing -- these are contextual identity |
| `skills/*/SKILL.md` | Stateless procedures (unchanged) | Nothing -- execution scripts don't evolve |

## What Goes in Unimatrix

### Entry Types by Category

**Expertise entries** (agents create freely):
```
{ topic: "ndp-rust-dev", category: "convention",
  content: "Use `anyhow` for app errors, `thiserror` for library errors...",
  tags: ["rust", "error-handling"] }

{ topic: "auth-middleware", category: "decision",
  content: "JWT with RS256, validated at gateway, claims propagated via headers...",
  tags: ["architecture", "auth", "nxs-012"] }
```

**Outcome entries** (agents report at task completion):
```
{ topic: "nxs-012", category: "outcome",
  content: "Feature completed in 3 waves, 4 agents. Wave 2 blocked on missing storage trait pattern...",
  tags: ["outcome:completion", "outcome:blocker", "ndp-rust-dev"] }

{ topic: "nxs-012", category: "outcome",
  content: "Retrieved 8 entries, 5 helpful, 2 irrelevant, 1 outdated (corrected mid-task)...",
  tags: ["outcome:efficiency", "ndp-rust-dev"] }
```

**Process proposal entries** (system-generated from outcome analysis):
```
{ topic: "wave-structure", category: "process-proposal",
  status: "pending-review",
  content: "PROPOSAL: Limit wave 2 to max 3 parallel agents.\n\nEVIDENCE: In 4/6 recent features, wave 2 with 4+ agents had merge conflicts requiring rework. Features with 2-3 agents in wave 2 completed 30% faster.\n\nSUGGESTED ACTION: Update protocols/planning.md wave-2 section.",
  tags: ["process", "wave-sizing", "evidence:4-features"] }
```

**Approved process entries** (human-approved proposals):
```
{ topic: "wave-structure", category: "process",
  content: "Wave 2 should have max 3 parallel agents to avoid merge conflicts...",
  tags: ["process", "wave-sizing", "approved:2026-02-20"],
  supersedes: <proposal_entry_id> }
```

## How Thin Shells Pull from Unimatrix at Runtime

The mechanism is the **`context_briefing` tool call driven by the agent file's orientation section**. The flow:

1. Scrum-master spawns `ndp-rust-dev` with task prompt
2. Agent reads its `.claude/` file -- sees "call `context_briefing` first"
3. Agent calls `context_briefing(role: "ndp-rust-dev", task: "implement storage trait for embeddings", phase: "implementation")`
4. Unimatrix assembles:
   - `lookup(topic: "ndp-rust-dev", category: "convention")` -> coding conventions
   - `lookup(topic: "ndp-rust-dev", category: "duties")` -> role-specific duties
   - `lookup(category: "process", tags: ["phase:implementation"])` -> approved process knowledge
   - `search(query: "implement storage trait for embeddings", k: 3)` -> relevant patterns
5. Returns compiled briefing (<2000 tokens)
6. Agent proceeds with full context

**For orchestrator-passes-context pattern**: The scrum-master calls `context_briefing` on behalf of the subagent and injects the result into the spawn prompt. The subagent never touches Unimatrix directly.

## The Retrospective Loop

```
  Feature lifecycle completes
          |
          v
  Outcome entries accumulated (agents reported throughout)
          |
          v
  Human (or scrum-master) calls:
    context_retrospective(feature: "nxs-012")
          |
          v
  Unimatrix aggregates:
    - All outcome entries tagged with feature
    - Entry usage data (which entries were retrieved, which helped)
    - Compares against previous features (trend detection)
          |
          v
  Returns analysis + generates process-proposal entries:
    - "Wave 2 had 4 agents, 3 merge conflicts" -> propose wave sizing limit
    - "ndp-rust-dev searched 'storage trait' 3x, found nothing" -> propose missing knowledge area
    - "Entry #42 was used 6 times, always helpful" -> no action (working well)
          |
          v
  Human reviews proposals:
    context_lookup(category: "process-proposal", status: "pending-review")
          |
     approve -> context_correct(original_id, updated_content) moves to active process
     reject  -> context_deprecate(id, reason: "not applicable") with reason
     modify  -> context_correct(original_id, modified_content) with human edits
```

## New Project Bootstrap

1. **CLI**: `unimatrix init` -- creates DB, appends 5-line CLAUDE.md section
2. **Starter `.claude/` files**: Human authors thin-shell agent definitions (or copies a template set). Each file is ~40-50 lines, not 150+.
3. **Seed data**: `unimatrix seed --template rust-project` loads baseline expertise entries (common Rust conventions, standard process patterns). These are low-confidence entries that get superseded as the project's actual patterns emerge.
4. **First feature cycle**: Agents report outcomes, knowledge accumulates, no process proposals yet (insufficient data).
5. **After 3-5 features**: Enough outcome data for meaningful retrospective. First process proposals generated.
