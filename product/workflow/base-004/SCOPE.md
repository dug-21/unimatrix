# Mandatory Knowledge Stewardship

## Problem Statement

Unimatrix is a self-learning knowledge engine, but the feedback loop is broken. Agents query Unimatrix at task start (read side) but rarely store findings back. Knowledge flows back only when an agent voluntarily calls a store skill (optional, unenforced) or someone manually runs `/retro` after merge (manual, infrequent). The result: implementation gotchas, gate failure patterns, security findings, and spec interpretation decisions are lost between features. The next agent on the next feature starts from zero on problems already solved.

**Who is affected**: All swarm agents, and transitively every future feature that would benefit from accumulated knowledge.

**Why now**: The knowledge base has 53 active entries (all ADRs) and empty active categories for duties, patterns, procedures, and lessons. After 20+ shipped features, the expected knowledge density is much higher. The feedback loop must close before the project scales further.

## Goals

1. Every swarm agent has explicit guidance on what knowledge to store, in what category, and with what topic convention.
2. Validator gate checks enforce that agents either stored findings or explicitly declined (no silent omission).
3. The `/retro` skill includes a stewardship quality pass that curates entries stored during the feature cycle.
4. A `/store-pattern` skill provides a focused interface for implementation-level patterns with quality validation (what/why/scope template).

## Non-Goals

1. **No Rust code changes** -- this feature modifies agent definitions, protocols, skills, and validator check sets only.
2. **No deliberate retrieval confidence boost** -- tracked separately in #199.
3. **No changes to the Unimatrix data model or MCP tool signatures** -- the existing `context_store`, `context_search`, `context_correct`, and `context_deprecate` tools are sufficient.
4. **No automated storage** -- agents decide what to store; the system enforces that they make a decision, not what the decision is.
5. **No CLAUDE.md changes** -- CLAUDE.md already references the stewardship skills; no new behavioral rules are needed there.
6. **No uni-init changes** -- the bootstrap agent is a one-time extraction tool, not a per-feature participant.

## Background Research

### Current Stewardship State Across Agents

Agents fall into three categories based on current stewardship behavior:

**Already has mandatory stewardship (model for others)**:
- `uni-architect` -- Knowledge Stewardship section with mandatory before/after steps, `/store-adr` integration, self-check items for Unimatrix compliance. This is the gold standard.

**Has stewardship section but no enforcement**:
- `uni-validator` -- has section (lesson-learned, pattern), not in self-check, no validator checks (is the validator)
- `uni-researcher` -- has section (pattern, convention), not in self-check
- `uni-risk-strategist` -- has section (pattern), not in self-check
- `uni-bug-investigator` -- has section (lesson-learned), not in self-check

**Has query-only guidance but no store guidance**:
- `uni-rust-dev` -- queries `/query-patterns` and `/knowledge-search` before implementing, no store guidance
- `uni-pseudocode` -- queries `/query-patterns` and `/knowledge-search` before designing, no store guidance
- `uni-tester` -- queries `/knowledge-search` for procedures before starting, no store guidance

**No stewardship at all**:
- `uni-vision-guardian` -- no Knowledge Stewardship section, no query, no store
- `uni-specification` -- no Knowledge Stewardship section, no query, no store
- `uni-synthesizer` -- no Knowledge Stewardship section, no query, no store
- `uni-security-reviewer` -- no Knowledge Stewardship section, no query, no store

### Validator Gate Structure

The validator has three gates (3a, 3b, 3c) with defined check sets. Currently no check set includes stewardship compliance. The validator's own agent definition has a Knowledge Stewardship section but does not include stewardship in its self-check.

### Existing Skills

Ten skills exist. The relevant storage skills are:
- `/store-adr` -- ADRs (architect only)
- `/store-lesson` -- lessons learned (after failures)
- `/store-procedure` -- technical procedures (retrospective)
- `/query-patterns` -- query before designing/implementing
- `/record-outcome` -- session outcomes

No skill exists for implementation-level patterns (the "what/why/scope" gotchas that rust-dev, tester, and bug-investigator discover). `/store-procedure` covers step-by-step how-tos. `/store-lesson` covers failure analysis. Neither fits "don't hold lock_conn() across await points -- deadlocks under concurrent requests."

### Retro Skill

The retro skill (Phase 2) already spawns `uni-architect` to review shipped features and extract patterns, procedures, and lessons. It does not currently assess the quality of entries stored during the feature cycle or curate (deprecate/promote) them.

### Protocol-Level Stewardship

The delivery protocol records an outcome at the end (`context_store` with category "outcome"). The design protocol does the same. The bugfix protocol records outcomes and optionally stores lessons/procedures. None of these protocols check whether individual agents stored knowledge during their work.

## Proposed Approach

### Layer 1: Agent Definition Changes

Add a Knowledge Stewardship section to every agent that lacks one, and strengthen existing sections to include self-check items. Follow the `uni-architect` pattern: specify what to store, which category and topic convention, and which skill to use.

For agents that produce no generalizable knowledge (e.g., `uni-synthesizer`, which compiles existing artifacts), the stewardship section explicitly states "no storage expected" with rationale. This prevents the validator from flagging legitimate non-storage as a failure.

For `uni-rust-dev`, add stewardship with the crate-as-topic convention and what/why/scope content template. This is the highest-value stewardship addition because implementation gotchas are the most frequently lost knowledge.

### Layer 2: Validator Gate Checks

Add one stewardship compliance check to each gate:

- **Gate 3a**: Did the pseudocode agent query patterns before designing? Did the risk strategist query and store risk patterns?
- **Gate 3b**: Did each rust-dev agent store or explicitly decline to store implementation patterns? Did agents query patterns before implementing?
- **Gate 3c**: Did the tester query procedures before designing test plans? Did the tester store any new test infrastructure patterns?

Enforcement: the validator checks agent reports for evidence of stewardship (stored entry reference or explicit "nothing novel" statement). Absence of either is REWORKABLE FAIL.

### Layer 3: Retro Quality Pass

Add a stewardship review step to the `/retro` skill:
- Query all entries stored during the feature cycle (via `feature_cycle` tag)
- Assess quality against the what/why/scope template
- Deprecate low-value entries
- Promote high-value entries (boost confidence)

### /store-pattern Skill

Create a new skill focused on implementation-level patterns. Required fields: topic, what, why, scope. The skill rejects entries missing the "why" field (quality floor). This is distinct from `/store-procedure` (ordered steps) and `/store-lesson` (failure-driven).

## Acceptance Criteria

- AC-01: Every agent definition in `.claude/agents/uni/` has a Knowledge Stewardship section that specifies: what to store (or "no storage expected" with rationale), which category and topic convention to use, and which skill to invoke.
- AC-02: Every agent definition with a Knowledge Stewardship section includes at least one stewardship-related item in its Self-Check list.
- AC-03: The `uni-rust-dev` agent definition includes stewardship guidance with crate-as-topic convention and what/why/scope content template for implementation patterns.
- AC-04: The `uni-validator` agent definition includes stewardship compliance checks in Gate 3a, Gate 3b, and Gate 3c check sets.
- AC-05: The `/retro` skill includes a stewardship quality pass step that queries entries stored during the feature cycle, assesses quality, and deprecates or promotes entries.
- AC-06: A `/store-pattern` skill exists at `.claude/skills/store-pattern/SKILL.md` with required fields (topic, what, why, scope) and validation that rejects entries missing the "why" field.
- AC-07: Agent definitions that legitimately produce no generalizable knowledge (e.g., uni-synthesizer) have a stewardship section that explicitly states "no storage expected" with rationale.
- AC-08: The uni-pseudocode agent definition includes stewardship guidance for querying patterns before designing and noting deviations from established patterns.
- AC-09: The uni-tester agent definition includes stewardship guidance for storing new test infrastructure patterns and querying procedures before test plan design.

## Constraints

1. **File-only changes**: All changes are to `.claude/agents/uni/*.md`, `.claude/skills/*/SKILL.md`, and `.claude/protocols/uni/*.md`. No Rust code, no Cargo.toml, no schema changes.
2. **Backward compatibility**: Agent definitions must remain valid for agents that are mid-session when changes deploy. No structural changes to agent definition format -- only section additions.
3. **Self-check item format**: New self-check items must follow the existing `- [ ] {statement}` format used across all agent definitions.
4. **Skill structure**: The `/store-pattern` skill must follow the same directory and SKILL.md conventions as existing skills (`/store-procedure`, `/store-lesson`).
5. **Validator check format**: New gate checks must follow the existing numbered check format in the validator agent definition.
6. **Agent report evidence**: Stewardship compliance must be verifiable from agent reports without requiring the validator to call Unimatrix APIs. Agents include stored entry IDs or "nothing novel to store" in their reports.

## Resolved Questions

1. **Pseudocode agent**: Does NOT store patterns. May query/lookup patterns by component to inform decomposition. Read-side only.

2. **Vision guardian**: YES, stores recurring misalignment patterns. The whole point is catching patterns that recur across features.

3. **Bugfix stewardship**: YES, gate checks include stewardship compliance for investigator and rust-dev. Additional direction: bugfix agents should identify what could have been done during design to prevent the bug, and link the outcome/rework to the feature that caused the issue (not just the bugfix feature cycle).

4. **New `/store-pattern` skill**: YES, create a new skill. Each category has its own skill — clearer purpose, self-evident from the skill name that the agent is supposed to use it. Smaller skill files are also better for agent context windows.

## Design Priorities

1. **Quality of knowledge entries matters most.** Getting agents to store high-quality, well-structured entries is the primary goal. Better descriptions, clear what/why/scope, proper categorization.
2. **Context window discipline.** Agent definitions must not bloat with stewardship instructions. Concise guidance — the skill itself enforces structure, not the agent definition. Judgment call per agent on how much instruction to add.
3. **For this iteration**: err on the side of better descriptions where agents are storing/updating entries. Quality over brevity in stewardship sections.

## Tracking

{Will be updated with GH Issue link after Session 1}
