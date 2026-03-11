# Agent Report: base-004-researcher

## Task
Research problem space for mandatory knowledge stewardship across all swarm agents.

## Key Findings

### Current State Audit
- **1 agent** has mandatory stewardship with enforcement (uni-architect -- the gold standard)
- **4 agents** have stewardship sections but no self-check or validator enforcement (validator, researcher, risk-strategist, bug-investigator)
- **3 agents** have query-only guidance with no store guidance (rust-dev, pseudocode, tester)
- **4 agents** have no stewardship at all (vision-guardian, specification, synthesizer, security-reviewer)

### Knowledge Base Gap
- 53 active entries, all ADRs (category: decision)
- 0 active entries in: duties, convention, pattern, procedure, lesson-learned
- After 20+ features, expected density is much higher -- the write side is broken

### Validator Gap
- Three gates (3a, 3b, 3c) have well-defined check sets
- Zero stewardship checks in any gate
- The validator's own stewardship section exists but is not in its self-check

### Retro Gap
- Retro skill Phase 2 extracts knowledge via uni-architect
- No quality pass on entries stored during the feature cycle
- No curation step (deprecate low-value, promote high-value)

### Skill Gap
- No skill for implementation-level patterns (what/why/scope gotchas)
- `/store-procedure` covers step-by-step how-tos
- `/store-lesson` covers failure analysis
- Implementation gotchas fit neither category cleanly

## Scope Boundaries

### In Scope
- Agent definition changes (add/strengthen Knowledge Stewardship sections)
- Validator gate check additions (stewardship compliance per gate)
- Retro skill enhancement (quality pass step)
- New `/store-pattern` skill

### Out of Scope
- Rust code changes
- Deliberate retrieval confidence boost (#199)
- CLAUDE.md changes
- uni-init changes
- Data model or MCP tool signature changes

### Rationale
This is a workflow feature. The enforcement mechanisms are all at the agent definition and protocol level. The underlying Unimatrix tools (`context_store`, `context_search`, etc.) already support everything needed. No new capabilities are required in the Rust codebase.

## Files Produced
- `/workspaces/unimatrix/product/workflow/base-004/SCOPE.md`

## Open Questions (for human)
1. Should the pseudocode agent store component decomposition patterns, or leave that to the retro architect?
2. Should the vision guardian store recurring misalignment patterns, or are these too feature-specific?
3. Should bugfix protocol gate checks also include stewardship compliance?
4. New `/store-pattern` skill vs extending `/store-procedure` with `mode: pattern`?
