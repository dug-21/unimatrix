# Architecture: base-004 Mandatory Knowledge Stewardship

## System Overview

base-004 closes the knowledge feedback loop in Unimatrix by making knowledge storage an enforced obligation across all swarm agents, not an optional voluntary action. The feature modifies four artifact types: agent definitions, validator check sets, skills, and the retro skill. No Rust code changes.

The system works in three layers:
1. **Agent definitions** provide concise stewardship guidance (what to store, which skill, which category)
2. **Skills** enforce structure and quality (content templates, required fields, validation)
3. **Validator gate checks** verify compliance from agent reports (structured stewardship block)

The retro skill adds a fourth layer: post-merge curation of entries stored during the feature cycle.

## Component Breakdown

### C1: Agent Definition Stewardship Sections

**Responsibility**: Add or strengthen Knowledge Stewardship sections across 12 agent definitions.

Three tiers of agents based on stewardship expectations:

| Tier | Agents | Stewardship Model |
|------|--------|-------------------|
| **Active storage** | rust-dev, tester, validator, risk-strategist, researcher, bug-investigator, vision-guardian, security-reviewer | Store findings via skills; report what was stored |
| **Read-only** | pseudocode, specification | Query before working; no storage expected; report what was queried |
| **Exempt** | synthesizer | No query, no storage; compiles existing artifacts only |

Each stewardship section follows a standard template (see ADR-001).

### C2: Agent Report Stewardship Block

**Responsibility**: Define a structured, machine-parseable block that agents include in their reports to declare stewardship compliance.

The block uses a fixed format that the validator can reliably parse without fragile heuristics (see ADR-002).

### C3: Validator Gate Check Integration

**Responsibility**: Add one stewardship check per gate (3a, 3b, 3c) to the validator's existing check sets.

The checks verify that agent reports contain the stewardship block and that its content is consistent with the agent's tier (see ADR-003).

### C4: /store-pattern Skill

**Responsibility**: New skill for implementation-level patterns with enforced what/why/scope template.

Distinct from /store-procedure (ordered steps) and /store-lesson (failure-driven). Patterns capture reusable solutions to recurring problems -- the gotchas invisible in source code (see ADR-004).

### C5: Retro Stewardship Quality Pass

**Responsibility**: Add a stewardship review phase to the /retro skill that queries, assesses, and curates entries stored during the feature cycle.

### C6: Bugfix Protocol Linkage

**Responsibility**: Add causal feature linkage to the bugfix protocol so outcomes and lessons reference the feature that caused the bug, not just the bugfix feature cycle (see ADR-005).

## Component Interactions

```
Agent Definition (C1)          Skill (C4)              Agent Report (C2)
  |                              |                        |
  | "store patterns via          | enforces               | "## Knowledge Stewardship"
  |  /store-pattern"             | what/why/scope          | block with entry IDs
  v                              v                        v
Agent executes ──────────────► Skill validates ──────► Agent writes report
                                content                   |
                                                          v
                                                     Validator (C3)
                                                       parses stewardship block
                                                       checks compliance per tier
                                                          |
                                                          v
                                                     Gate result (PASS/FAIL)
                                                          |
                                                          v (post-merge)
                                                     Retro (C5)
                                                       queries entries by feature_cycle
                                                       assesses quality
                                                       curates (deprecate/promote)
```

## Technology Decisions

All changes are markdown file modifications. No build tools, no dependencies, no schema changes.

| Decision | Resolution | ADR |
|----------|-----------|-----|
| Stewardship section template | Three-block structure: Before/After/Report | ADR-001 |
| Agent report stewardship block | Structured `## Knowledge Stewardship` section with fixed fields | ADR-002 |
| Validator check integration | One composite check per gate, not per-agent | ADR-003 |
| Store-pattern skill design | Separate skill with what/why/scope required fields | ADR-004 |
| Bugfix causal linkage | `caused_by_feature` field in outcome recording | ADR-005 |

## Integration Surface

Since this is a workflow feature (no code), the integration surface is file-format contracts between components.

| Integration Point | Format | Source | Consumer |
|-------------------|--------|--------|----------|
| Stewardship section in agent def | Markdown: `## Knowledge Stewardship` with Before/After/Report subsections | C1 (agent defs) | Agents at runtime |
| Stewardship block in agent report | Markdown: `## Knowledge Stewardship` with `- Stored:` / `- Queried:` / `- Declined:` items | C2 (report format) | C3 (validator) |
| Validator stewardship check | One numbered check per gate checking stewardship block presence and content | C3 (validator def) | Validator at gate time |
| /store-pattern SKILL.md | Skill file at `.claude/skills/store-pattern/SKILL.md` | C4 (new skill) | Agents via `/store-pattern` |
| Retro quality pass | New phase between existing Phase 2 and Phase 3 in retro SKILL.md | C5 (retro skill) | Retro skill runner |
| Bugfix outcome `caused_by_feature` | Tag field in `/record-outcome` call within bugfix protocol | C6 (bugfix protocol) | Unimatrix knowledge base |

## Stewardship Section Template (C1 Detail)

The standard stewardship section for agent definitions follows three blocks. The exact content varies per agent tier, but the structure is fixed.

### Active Storage Agents

```markdown
## Knowledge Stewardship

### Before Starting (MANDATORY)
- Use `/query-patterns` with {relevant crate/area} to find existing patterns
- Use `/knowledge-search` with {relevant category} to find prior findings

### After Completing (MANDATORY)
- Store {what} via {which skill} with topic: "{topic convention}", category: "{category}"
- Content must follow {template reference} -- the skill enforces this
- If nothing novel was discovered, note "nothing novel to store" in your report, but do not add this note to knowledge base

### Report
Include a `## Knowledge Stewardship` section in your agent report (see format below).
```

### Read-Only Agents

```markdown
## Knowledge Stewardship

### Before Starting (MANDATORY)
- Use `/query-patterns` with {relevant crate/area} to find existing patterns
- Note deviations from established patterns in your report

### Report
Include a `## Knowledge Stewardship` section in your agent report:
- Queried: {what was searched and key findings}
- No storage expected: {rationale}
```

### Exempt Agents

```markdown
## Knowledge Stewardship

No storage or query expected. This agent compiles existing artifacts without generating new knowledge.
```

**Budget**: Active agents get 10-15 lines. Read-only agents get 6-8 lines. Exempt agents get 2 lines. This addresses SR-01 (context bloat risk).

## Agent Report Stewardship Block (C2 Detail)

All agents (except exempt tier) include this in their agent report:

```markdown
## Knowledge Stewardship

- Queried: `/query-patterns` for {area} -- {summary of findings or "no results"}
- Stored: entry #{id} "{title}" via /store-pattern (or "nothing novel to store -- {reason}")
- Declined: {category} -- {reason for not storing, if applicable}
```

Rules:
- `Queried:` line is required for all non-exempt agents
- `Stored:` line is required for active-storage agents (either entry ID or "nothing novel" with reason)
- `Declined:` line is optional -- used when agent considered storing but decided against it
- The validator checks for presence of the `## Knowledge Stewardship` heading and at least one `Queried:` or `Stored:` line

This structured format addresses SR-02 (brittle parsing risk). The validator parses heading + bullet prefixes, not free-form prose.

## Validator Gate Checks (C3 Detail)

One stewardship check added to each gate. The check is composite (covers multiple agents) but counts as a single check item to avoid doubling the check count.

**Gate 3a -- Stewardship compliance (design agents)**:
- Architect agent report contains `## Knowledge Stewardship` with `Stored:` entries (ADRs)
- Risk strategist agent report contains `Queried:` and `Stored:` or "nothing novel"
- Pseudocode agent report contains `Queried:` line

**Gate 3b -- Stewardship compliance (implementation agents)**:
- Each rust-dev agent report contains `## Knowledge Stewardship` with `Stored:` or "nothing novel"
- Vision guardian report (if spawned) contains stewardship block

**Gate 3c -- Stewardship compliance (test agents)**:
- Tester agent report contains `Queried:` line (procedures before test plan)
- Tester agent report contains `Stored:` or "nothing novel" for test patterns

**Enforcement level**: REWORKABLE FAIL if stewardship block is missing entirely. WARN if block is present but thin (e.g., "nothing novel" without a reason). This addresses SR-07 (adoption friction) -- the block must exist, but content judgment is a WARN not a FAIL.

## /store-pattern Skill (C4 Detail)

File: `.claude/skills/store-pattern/SKILL.md`

### Required Fields

| Field | Required | Description |
|-------|----------|-------------|
| topic | Yes | Crate name or module area (e.g., `unimatrix-store`) |
| what | Yes | The pattern in one sentence |
| why | Yes | What goes wrong without it -- the quality floor |
| scope | Yes | Where it applies (crate, module, context) |

### Content Template

The skill constructs the `content` parameter from the three fields:

```
What: {what}
Why: {why}
Scope: {scope}
```

### Validation Rules

1. Reject if `why` is missing or fewer than 10 characters (quality floor)
2. Reject if `what` exceeds 200 characters (conciseness enforcement)
3. Auto-inject `feature_cycle` from the current feature context (addresses SR-06)
4. Check for existing patterns via `context_search` before storing (dedup)

### Distinction from Other Skills

| Skill | Category | Content Shape | Use When |
|-------|----------|--------------|----------|
| `/store-pattern` | `pattern` | What/Why/Scope | Reusable gotcha or solution |
| `/store-lesson` | `lesson-learned` | What happened/Root cause/Takeaway | After a failure |
| `/store-procedure` | `procedure` | Numbered steps | How-to sequence |

Decision rule for bug-investigator ambiguity (addresses SR-04): If the finding is triggered by a specific failure and the takeaway is "don't do X again," use `/store-lesson`. If the finding is a generalizable pattern applicable regardless of whether a failure occurred, use `/store-pattern`.

## Retro Stewardship Quality Pass (C5 Detail)

New phase inserted between existing Phase 2 (Pattern & Procedure Extraction) and Phase 3 (ADR Supersession) in the retro SKILL.md.

### Phase 2b: Stewardship Quality Review

1. **Query entries stored during the feature cycle**:
   ```
   mcp__unimatrix__context_search(query: "{feature-id}", tags: ["{feature-id}"], k: 20)
   ```

2. **Assess each entry against its category's quality template**:
   - Patterns: Does it have what/why/scope? Is the "why" actionable?
   - Lessons: Does it have what happened/root cause/takeaway?
   - Procedures: Does it have numbered steps?
   - Conventions: Is it a clear rule statement?

3. **Curate**:
   - **Low-value entries** (missing "why", trivially obvious, feature-specific not generalizable): deprecate with reason
   - **High-value entries** (well-structured, generalizable, confirmed by successful delivery): note as validated
   - **Duplicate entries** (same pattern stored by different agents): merge via context_correct, deprecate the weaker version

4. **Report stewardship quality metrics** in the retro summary:
   ```
   Stewardship quality:
   - Entries stored during cycle: {N}
   - Validated (high quality): {N}
   - Deprecated (low quality): {N}
   - Merged (deduped): {N}
   ```

## Bugfix Protocol Linkage (C6 Detail)

When a bugfix session completes, the `/record-outcome` call in Phase 5 of the bugfix protocol adds a `caused_by_feature` tag linking to the originating feature:

```
/record-outcome
  type: bugfix
  result: pass
  tags: ["caused_by_feature:{originating-feature-id}"]
  content: "... Root cause originated in {feature-id}: {brief description of original design gap} ..."
```

The bug-investigator's stewardship section is updated to include guidance: "If the root cause traces to a design decision from a prior feature, note the originating feature ID in your diagnosis. The bugfix protocol uses this for causal linkage."

Similarly, `/store-lesson` calls from bugfix sessions include the `caused_by_feature` tag so the lesson is discoverable when reviewing the originating feature's knowledge trail.

## Open Questions

1. **Feature_cycle injection mechanism**: The /store-pattern skill needs to auto-inject `feature_cycle` (SR-06). Since skills are markdown instructions (not executable code), the skill must instruct the agent to include `feature_cycle` as a parameter. There is no automatic injection -- the agent must pass it. The skill template should make this explicit.

2. **Retro quality pass scope**: Should the retro quality pass also review entries stored by agents in other feature cycles that touch the same crates? This could catch stale patterns. For this iteration, scoping to the current feature cycle only is simpler and sufficient.

3. **CLAUDE.md /store-pattern mention (SR-03)**: CLAUDE.md already lists `/store-pattern` is not mentioned. The skill list in CLAUDE.md appears manually curated. Adding `/store-pattern` to CLAUDE.md's skill list is a one-line change but is outside the stated scope ("No CLAUDE.md changes"). Recommend relaxing this constraint for skill discoverability.
