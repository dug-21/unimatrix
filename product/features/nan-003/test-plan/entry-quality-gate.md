# Test Plan: Entry Quality Gate (Component 6)

## Content Review Checks

### CR-01: Quality Gate Rules (R-02)
- [ ] SKILL.md documents What/Why/Scope gate
- [ ] What: max 200 chars rule stated
- [ ] Why: min 10 chars rule stated
- [ ] Scope: must be present rule stated
- [ ] Failing entries are discarded (not shown to human)

### CR-02: Category Restriction (R-03, ADR-006)
- [ ] Allowed categories listed: convention, pattern, procedure
- [ ] Excluded categories listed: decision, outcome, lesson-learned
- [ ] Rationale for exclusion provided
- [ ] No instruction to use excluded categories

### CR-03: Tautology Guidance
- [ ] SKILL.md warns against tautological "why" fields
- [ ] Example or definition of tautological content provided

### CR-04: Discard Behavior (NFR-05)
- [ ] Explicit instruction: entries failing gate are silently discarded
- [ ] Not "ask human to fix" — discard and generate fewer entries
- [ ] Quality gate runs BEFORE presentation to human

### CR-05: Integration with context_store
- [ ] Entry format matches context_store parameters
- [ ] Title maps to "what"
- [ ] Content includes What/Why/Scope fields
- [ ] Category field uses allowed categories only
- [ ] Tags include "seed" and level indicator

## Risk Coverage

| Risk | Check |
|------|-------|
| R-02 | CR-01 (quality gate enforcement), CR-04 (discard behavior) |
| R-03 | CR-02 (category restriction) |
