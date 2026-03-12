# Test Plan: Agent Scan Algorithm (Component 4)

## Content Review Checks

### CR-01: Glob Pattern
- [ ] SKILL.md instructs glob `.claude/agents/**/*.md`
- [ ] Handles case where `.claude/agents/` does not exist
- [ ] Handles case where glob returns 0 results

### CR-02: Three Check Patterns (AC-04)
- [ ] Check 1: `context_briefing` reference/invocation
- [ ] Check 2: outcome reporting (`/record-outcome` or `context_store` with `outcome`)
- [ ] Check 3: `unimatrix-*` skill reference

### CR-03: Output Format
- [ ] Terminal-only output (no file writes)
- [ ] Table or structured format with: agent name, missing patterns, suggestions
- [ ] Suggestions use skill-level examples (e.g., `/record-outcome`), not raw MCP tool calls

### CR-04: No File Modification (C-07, NFR-07)
- [ ] SKILL.md does NOT instruct writing to any agent file
- [ ] Recommendations are explicitly labeled as terminal output only

### CR-05: Edge Cases
- [ ] "No agents found" message when directory is empty or absent
- [ ] "All agents fully wired" message when all checks pass
- [ ] Handles subdirectories in .claude/agents/ (e.g., .claude/agents/uni/*.md)

## Risk Coverage

| Risk | Check |
|------|-------|
| R-11 | CR-02 (check patterns defined), CR-05 (edge cases) |
