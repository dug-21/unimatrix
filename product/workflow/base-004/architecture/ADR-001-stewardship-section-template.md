## ADR-001: Three-Tier Stewardship Section Template

### Context

12 agent definitions need Knowledge Stewardship sections. The uni-architect agent already has a gold-standard section (~40 lines), but copying that verbatim to every agent would bloat context windows by 400+ tokens per agent (SR-01). Meanwhile, agents that produce no generalizable knowledge (synthesizer) still need explicit exemption to avoid validator false positives.

The design priority says "err on better descriptions for this iteration" but also demands "context window discipline -- push structure enforcement into skills, not agent definitions."

### Decision

Define three tiers of stewardship with different section sizes:

**Active storage tier** (rust-dev, tester, validator, risk-strategist, researcher, bug-investigator, vision-guardian, security-reviewer): 10-15 lines. Three subsections: Before Starting (query guidance), After Completing (store guidance with skill and category), Report (reference to stewardship block format). The skill enforces content structure -- the agent definition just says which skill and which category.

**Read-only tier** (pseudocode, specification): 6-8 lines. Two subsections: Before Starting (query guidance), Report (stewardship block with "no storage expected" rationale). These agents query Unimatrix to inform their work but do not generate knowledge entries.

**Exempt tier** (synthesizer): 2 lines. Single statement: "No storage or query expected. This agent compiles existing artifacts without generating new knowledge." Prevents validator from flagging legitimate non-storage.

The architect agent keeps its existing extended stewardship section unchanged -- it is the ADR authority with unique lifecycle requirements that justify the larger section.

### Consequences

- Context window cost is bounded: ~150 tokens for active agents, ~80 for read-only, ~20 for exempt. Total across all agents: ~1,200 tokens (vs. ~4,800 if all got the architect-sized section).
- Quality enforcement lives in skills (C4), not agent definitions. Agent defs say "use /store-pattern" -- the skill says "content must have what/why/scope."
- The three-tier model creates a clear validator expectation: the validator knows which agents should have `Stored:` entries vs. `Queried:` entries vs. nothing.
- Adding a new agent requires classifying it into a tier. The classification criteria: "Does this agent discover knowledge invisible in source artifacts?" (active) vs. "Does this agent consume knowledge to inform its work?" (read-only) vs. "Does this agent only compile existing artifacts?" (exempt).
