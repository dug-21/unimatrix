# Agent Report: base-004-agent-2-spec

## Task
Write SPECIFICATION.md for base-004 (Mandatory Knowledge Stewardship).

## Status: COMPLETE

## Artifacts Produced
- `/workspaces/unimatrix/product/workflow/base-004/specification/SPECIFICATION.md`

## Key Decisions

1. **Structured stewardship report format over free-form prose** -- Specified a markdown table with fixed Action keywords (`Stored`, `Queried`, `Updated`, `Declined`) under a `## Stewardship` heading. This directly addresses SR-02 (validator parsing reliability). The validator matches on heading + keyword, not grep heuristics.

2. **15-line budget per agent stewardship section** -- Set a hard ceiling on stewardship guidance text to address SR-01 (context window bloat). The skill enforces quality, not the agent definition.

3. **No advisory rollout period** -- Stewardship checks are REWORKABLE FAIL immediately. The bar is low (store or explicitly decline); a warn-only phase would delay the feedback loop closure that is the core objective. SR-07 noted this risk but the design priority of "quality of knowledge entries" outweighs adoption friction.

4. **Feature_cycle tagging is recommended, not enforced** -- SR-06 flagged risk of inconsistent tagging. Rather than requiring the skill to auto-inject (which would need MCP tool changes, out of scope), the skill recommends the tag and the retro quality pass queries by multiple strategies (feature_cycle tag + agent_id matching).

5. **Causal feature linkage for bugfixes** -- Per human direction, bugfix agents must identify and tag the originating feature. This enables retroactive gap analysis in retro quality passes.

6. **Uni-pseudocode is read-only, uni-synthesizer has no obligation** -- Pseudocode agent queries patterns but stores nothing (per resolved question in SCOPE.md). Synthesizer compiles existing artifacts with no generalizable findings.

7. **Decision rule for /store-pattern vs /store-lesson** -- Addresses SR-04 (ambiguous boundary for bug-investigator): failure-triggered takeaways go to /store-lesson; reusable solutions generalizable beyond the failure context go to /store-pattern.

## Open Questions

1. **Feature_cycle reliability** -- The retro quality pass depends on finding entries stored during the feature cycle. If agents inconsistently tag entries, the quality pass may miss entries. The spec mitigates by using multiple query strategies, but this should be monitored after the first delivery.

2. **Validator self-stewardship** -- The validator stores lesson-learned entries about recurring gate failures. Should the validator check its own stewardship compliance? Currently specified as having a stewardship section with self-check, but no gate checks itself (it is the gate). The architect may want to address this.

3. **Vision guardian in design session** -- The vision guardian is spawned during Session 1 (design). It is not currently covered by any gate's stewardship check (gates are Session 2). Should a design-phase gate check be added for vision guardian stewardship, or is the self-check sufficient?

## Stewardship

| Action | Detail |
|--------|--------|
| Declined | Nothing novel to store -- specification work; no generalizable patterns discovered |
