# Test Plan: `/unimatrix-init` Skill (Component 1)

## Content Review Checks

### CR-01: YAML Frontmatter
- [ ] File exists at `.claude/skills/unimatrix-init/SKILL.md`
- [ ] First line is `---`
- [ ] Contains `name: "unimatrix-init"`
- [ ] Contains `description:` field
- [ ] Frontmatter closed with `---`

### CR-02: Prerequisites Section
- [ ] Prerequisites section is the first non-frontmatter section
- [ ] Lists: skill files in `.claude/skills/`
- [ ] Lists: MCP server wired (for seed)
- [ ] References installation documentation

### CR-03: Disambiguation Notice (AC-12)
- [ ] SKILL.md contains reference to "uni-init"
- [ ] Clearly distinguishes `/unimatrix-init` (CLAUDE.md setup) from `uni-init` agent (brownfield bootstrap)
- [ ] grep: `grep -i "uni-init" .claude/skills/unimatrix-init/SKILL.md` returns matches

### CR-04: Sentinel Check Logic (R-04, AC-14)
- [ ] SKILL.md instructs to read CLAUDE.md first
- [ ] Contains exact sentinel string: `<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->`
- [ ] Instructs: if sentinel found, print "already initialized" and halt
- [ ] Contains head-check fallback: check last 30 lines for files > 200 lines (ADR-002)

### CR-05: Agent Scan Instructions
- [ ] Instructs glob `.claude/agents/**/*.md`
- [ ] Three check patterns listed: context_briefing, outcome reporting, unimatrix-* skills
- [ ] Output is terminal-only — no file write instruction for recommendations
- [ ] Handles "no agents found" case

### CR-06: CLAUDE.md Block
- [ ] Block content matches template from architecture (5 categories, 2 skills)
- [ ] Sentinel open: `<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->`
- [ ] Sentinel close: `<!-- end unimatrix-init v1 -->`
- [ ] Uses append/Edit semantics, NOT Write/overwrite
- [ ] Handles CLAUDE.md creation when file absent (FR-03)

### CR-07: Dry-Run Mode (R-06, AC-05)
- [ ] SKILL.md checks for `--dry-run` argument
- [ ] In dry-run: prints block content + agent recommendations
- [ ] In dry-run: explicitly states no file writes
- [ ] In dry-run: does NOT create or modify CLAUDE.md

### CR-08: Phase Ordering
- [ ] Phase 1 (sentinel check) before Phase 2 (agent scan) before Phase 3 (write)
- [ ] Sentinel check is fail-fast — halts before scan if found

## FR Tracing

| FR | Verified By Check |
|----|------------------|
| FR-01 | CR-04 (sentinel scan) |
| FR-02 | CR-04 (halt if found) |
| FR-03 | CR-06 (create CLAUDE.md) |
| FR-04 | CR-06 (append semantics) |
| FR-05 | CR-06 (block content) |
| FR-06 | CR-06 (self-contained) |
| FR-07 | CR-03 (disambiguation) |
| FR-08 | CR-05 (agent glob) |
| FR-09 | CR-05 (three checks) |
| FR-10 | CR-05 (terminal-only) |
| FR-11 | CR-07 (dry-run) |
| FR-12 | CR-02 (prerequisites) |
| FR-25 | CR-01 (file path) |
| FR-26 | CR-01 (frontmatter) |
| FR-27 | CR-08 (fail-fast) |

## Risk Coverage

| Risk | Check |
|------|-------|
| R-04 | CR-04 (sentinel + fallback) |
| R-06 | CR-07 (dry-run guard) |
| R-11 | CR-05 (scan checks) |
| R-12 | CR-06 (append not overwrite) |
| R-13 | CR-02 (prerequisites) |
