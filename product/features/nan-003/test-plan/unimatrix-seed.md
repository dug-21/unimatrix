# Test Plan: `/unimatrix-seed` Skill (Component 2)

## Content Review Checks

### CR-01: YAML Frontmatter
- [ ] File exists at `.claude/skills/unimatrix-seed/SKILL.md`
- [ ] Contains `name: "unimatrix-seed"`
- [ ] Contains `description:` field
- [ ] Frontmatter properly delimited

### CR-02: Prerequisites Section
- [ ] First non-frontmatter section
- [ ] Lists: MCP server running and wired
- [ ] References verification via context_status()
- [ ] Suggests running /unimatrix-init first

### CR-03: Pre-flight Check (R-09, AC-13)
- [ ] context_status() is the VERY FIRST action (before any file reads)
- [ ] Failure path: clear error message about MCP unavailability
- [ ] Instructions to check for error-free status response (not just call completion)

### CR-04: Existing-Check (R-10, AC-13)
- [ ] Calls context_search for convention/pattern/procedure categories
- [ ] Threshold: >= 3 active entries triggers warning
- [ ] Warning appears BEFORE any Level 0 stores
- [ ] Offers supplement vs skip choice
- [ ] STOP gate after warning

### CR-05: Level 0 Flow (AC-06)
- [ ] Reads: README.md, CLAUDE.md, package manifests, .claude/ structure
- [ ] No opt-in required for Level 0 reads
- [ ] Proposes 2-4 entries
- [ ] Quality gate applied to candidates before presentation

### CR-06: Gate 0 (AC-08)
- [ ] Batch approval: entries shown together, single approve/reject
- [ ] STOP gate phrasing present
- [ ] Only approved entries stored
- [ ] Rejected batch -> DONE path
- [ ] Deeper exploration menu presented after approval

### CR-07: Level 1+ Flow (AC-07, AC-08)
- [ ] Requires explicit human opt-in per category
- [ ] Menu of exploration options presented
- [ ] Per-entry individual approval (not batch)
- [ ] STOP gate at each entry and at level transition

### CR-08: Depth Limit (R-07, AC-09)
- [ ] SKILL.md states Level 2 is the final level
- [ ] No Level 3 menu or prompt offered
- [ ] After GATE_2, transitions directly to DONE

### CR-09: Category Restriction (R-03)
- [ ] Only convention/pattern/procedure categories used
- [ ] Excluded categories explicitly listed: decision, outcome, lesson-learned
- [ ] Rationale provided for exclusion

### CR-10: Quality Gate Instructions (R-02)
- [ ] What/Why/Scope gate documented
- [ ] What <= 200 chars
- [ ] Why >= 10 chars
- [ ] Failing entries silently discarded (not shown to human)
- [ ] Tautology guidance included

### CR-11: Error Handling (R-05)
- [ ] context_store failure reported per entry
- [ ] context_search failure at existing-check: warns and proceeds
- [ ] No silent failures

### CR-12: DONE Summary
- [ ] Summary report at end: total entries stored by level
- [ ] Clear session termination

## FR Tracing

| FR | Verified By Check |
|----|------------------|
| FR-13 | CR-03 (pre-flight) |
| FR-14 | CR-04 (existing-check) |
| FR-15 | CR-05 (Level 0 reads) |
| FR-16 | CR-05 (2-4 entries) |
| FR-17 | CR-06 (batch approval) |
| FR-18 | CR-06 (only approved stored) |
| FR-19 | CR-06 (Level 1 menu) |
| FR-20 | CR-07 (per-entry approval) |
| FR-21 | CR-08 (depth limit) |
| FR-22 | CR-10 (quality gate) |
| FR-23 | CR-09 (category restriction) |
| FR-24 | CR-02 (prerequisites) |
| FR-25 | CR-01 (file path) |
| FR-26 | CR-01 (frontmatter) |
| FR-27 | CR-11 (error handling) |

## Risk Coverage

| Risk | Check |
|------|-------|
| R-01 | CR-06, CR-07, CR-08 (STOP gates at every level) |
| R-02 | CR-10 (quality gate) |
| R-03 | CR-09 (category restriction) |
| R-05 | CR-11 (MCP failure handling) |
| R-09 | CR-03 (pre-flight validation) |
| R-10 | CR-04 (existing-check threshold) |
| R-13 | CR-02 (prerequisites) |
