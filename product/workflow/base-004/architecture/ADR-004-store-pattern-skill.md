## ADR-004: Separate /store-pattern Skill with What/Why/Scope Template

### Context

Implementation-level patterns ("don't hold lock_conn() across await points -- deadlocks under concurrent requests") have no dedicated storage skill. /store-procedure covers ordered steps. /store-lesson covers failure analysis. Neither fits a reusable gotcha or solution pattern.

Two options considered:
1. **Extend /store-procedure** with a `mode: pattern` flag. Fewer skills to maintain. But overloads the skill's purpose (procedures are ordered steps; patterns are not) and makes the skill file larger.
2. **New /store-pattern skill**. Each category gets its own skill -- clearer purpose, self-evident from the skill name, smaller skill files (better for context windows).

SR-04 flags the ambiguity between /store-pattern and /store-lesson for bug-investigator findings. A clear decision rule is needed.

### Decision

Create `/store-pattern` as a separate skill at `.claude/skills/store-pattern/SKILL.md`.

Required fields: topic, what, why, scope. The skill rejects entries where `why` is missing or under 10 characters. The skill also rejects `what` over 200 characters (conciseness).

The skill constructs content as:
```
What: {what}
Why: {why}
Scope: {scope}
```

Category is always `pattern`. The skill instructs the agent to pass `feature_cycle` as a tag for retro traceability (addresses SR-06).

**Decision rule for pattern vs. lesson** (for bug-investigator and others): If the knowledge was triggered by a specific failure and the takeaway is preventive ("don't do X"), use `/store-lesson`. If the knowledge is a reusable solution applicable regardless of failure context ("when doing X, use approach Y because Z"), use `/store-pattern`. Include this decision rule in the skill's documentation.

### Consequences

- Clear category-to-skill mapping: pattern -> /store-pattern, lesson -> /store-lesson, procedure -> /store-procedure, decision -> /store-adr. Each skill is self-documenting.
- The what/why/scope template sets a quality floor. "I used Arc::clone" fails (no why). "Don't hold lock_conn() across await -- deadlocks under concurrent MCP requests" passes naturally.
- One more skill file to maintain. Mitigated by keeping it small (~80 lines, matching /store-lesson size).
- The `feature_cycle` tag instruction is in the skill, not the agent definition. This keeps agent defs concise (ADR-001) while ensuring traceability (SR-06).
- The pattern-vs-lesson decision rule prevents inconsistent categorization. It goes in both skills' documentation so agents see it regardless of which skill they open.
