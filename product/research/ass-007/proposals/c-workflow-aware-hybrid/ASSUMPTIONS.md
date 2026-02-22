# Proposal C: Workflow-Aware Hybrid -- Assumptions

## Core Strategic Assumption

Split source of truth. Agent **identity** stays in `.claude/` files (stable, human-owned). Agent **expertise**, **process knowledge**, and **workflow outcomes** live in Unimatrix (dynamic, evolves). Unimatrix never writes `.claude/` files, but the retrospective loop is a first-class feature that tracks outcomes, identifies process gaps, and **proposes** improvements -- humans approve all process changes.

## The Split Defined

### Identity (stays in `.claude/` files)

What makes an agent *that agent*. Rarely changes. Human-authored.

- Role name, type, scope, description (frontmatter)
- Design principles ("How to Think" -- stable philosophy)
- Self-check gates ("Run Before Returning Results")
- Related agents and skills (topology)
- The Unimatrix pull directive: "Before starting work, call `context_briefing`"

**Test**: If you changed this, would the agent *be a different agent*? If yes, it's identity.

### Expertise (goes in Unimatrix, agent-managed)

What the agent *knows about the domain*. Grows constantly. Agents store and retrieve.

- Coding conventions, architectural patterns, ADRs
- Technology-specific knowledge (library APIs, gotchas)
- Project-specific patterns discovered during work
- Error handling strategies, naming conventions

**Test**: Could a different agent in the same role benefit from this knowledge? If yes, it's expertise.

### Process Knowledge (goes in Unimatrix, human-approved changes)

How work *should be done*. Evolves based on outcomes. The system proposes, humans approve.

- Wave structure effectiveness ("wave 2 scope was too large in 4/6 features")
- Gate criteria that catch real issues vs. gate criteria that are noise
- Agent team composition patterns ("features touching storage always need ndp-rust-dev")
- Estimation accuracy patterns, review checklist effectiveness

**Test**: Does changing this affect how agents coordinate, not what they know? If yes, it's process knowledge.

### The Boundary Problem (Honest Assessment)

The identity/expertise boundary is fuzzy. Examples:

| Content | Identity or Expertise? | Resolution |
|---------|----------------------|------------|
| "Use `tracing` macros, not `println!`" | **Expertise** -- project-specific convention | Unimatrix |
| "Always run `cargo clippy` before returning" | **Identity** -- self-check gate | `.claude/` file |
| "Prefer `anyhow` for app errors, `thiserror` for libs" | **Expertise** -- could be learned from corrections | Unimatrix |
| "Check with ndp-architect before changing trait signatures" | **Identity** -- coordination rule | `.claude/` file |
| "Wave 2 should have max 3 parallel agents" | **Process** -- learned from outcome data | Unimatrix (proposed, human-approved) |

**Rule of thumb**: If it's about *what to do in code*, it's expertise (Unimatrix). If it's about *when to stop, who to ask, or how to coordinate*, it's identity (`.claude/`). If it's about *how well the workflow itself performs*, it's process knowledge (Unimatrix, human-gated).

## Human-in-the-Loop for Process Changes

```
Agents work on features
  -> Unimatrix tracks: which entries were used, feature outcome, time taken, issues hit
  -> After N features (or on-demand): system computes process gaps
  -> System generates ProcessProposal entries (category: "process-proposal", status: "pending-review")
  -> Human reviews via context_lookup(category: "process-proposal", status: "pending-review")
  -> Human approves/rejects/modifies via context_correct or context_deprecate
  -> Approved proposals become active process knowledge (category: "process")
  -> Rejected proposals are deprecated with reason (system learns what humans don't want)
```

The human never edits Unimatrix data directly. They approve or reject through MCP tools. If an approved process change requires updating a `.claude/` file (e.g., adding a new self-check gate), the human does that manually -- Unimatrix surfaces the suggestion but never touches the filesystem.

## Workflow Outcome Tracking Model

Every feature lifecycle produces **outcome signals**:

- `outcome:completion` -- feature completed, duration, agent count, wave count
- `outcome:quality` -- bugs found post-merge, rework count, correction count during feature
- `outcome:efficiency` -- entries retrieved vs. entries that were useful (from reflexion data)
- `outcome:process-gap` -- moments where agents were blocked, searched and found nothing, or used wrong knowledge

These are stored as regular Unimatrix entries with `category: "outcome"` and typed tags. The retrospective pipeline aggregates them, compares across features, and generates process proposals.

## Tradeoffs

**Strengths:**
- Learning on both knowledge AND process, but human retains control over process
- Outcome tracking enables data-driven retrospectives (not just "what felt wrong")
- The split keeps `.claude/` files thin and readable (identity only, not 150-line knowledge dumps)
- Rejected proposals teach the system what humans don't want changed

**Weaknesses:**
- The identity/expertise boundary is genuinely ambiguous in ~20% of cases
- Human must actively review process proposals or the queue grows stale
- More complex than Proposal A (knowledge-only) -- more moving parts
- The outcome tracking requires discipline: agents must report outcomes, not just store knowledge
