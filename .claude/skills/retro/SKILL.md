---
name: "retro"
description: "Post-merge retrospective — extracts patterns, procedures, and lessons from shipped features into Unimatrix. Use after a feature PR is merged."
---

# Retro — Post-Merge Knowledge Extraction

## What This Skill Does

Analyzes a shipped feature and extracts reusable knowledge — patterns, procedures, and lessons — into Unimatrix. This is how the project learns.

---

## Inputs

From the invoker:
- Feature ID (e.g., `col-011`)
- PR number (merged)
- GH Issue number

---

## Phase 1: Data Gathering

Gather all evidence about the shipped feature:

1. **Run retrospective analysis** (if observation data exists):
   ```
   mcp__unimatrix__context_retrospective(feature_cycle: "{feature-id}")
   ```
   This returns session telemetry, hotspots, and detection rule results.

2. **Read feature artifacts**:
   - `product/features/{id}/architecture/ARCHITECTURE.md`
   - `product/features/{id}/pseudocode/OVERVIEW.md`
   - `product/features/{id}/testing/RISK-COVERAGE-REPORT.md`
   - `product/features/{id}/reports/gate-3a-report.md`
   - `product/features/{id}/reports/gate-3b-report.md`
   - `product/features/{id}/reports/gate-3c-report.md`

3. **Check for rework signals**: Did any gate fail before passing? Read the gate report for what went wrong.

4. **Review the git log** for this feature's branch:
   ```bash
   git log main..HEAD --oneline
   ```
   Look for rework commits (`fix(gate):`) — these indicate where the process struggled.

---

## Phase 2: Pattern & Procedure Extraction (MUST be a subagent)

Spawn `uni-architect` to review what was built and extract reusable knowledge:

```
Agent(uni-architect, "
  Your agent ID: {feature-id}-retro-architect
  Your Unimatrix agent_id: uni-architect
  MODE: retrospective (not design)
  Feature: {feature-id}

  You are reviewing a SHIPPED feature to extract reusable knowledge.
  You are NOT designing anything new.

  Read these artifacts:
  - product/features/{id}/architecture/ARCHITECTURE.md
  - product/features/{id}/pseudocode/OVERVIEW.md (component structure)
  - product/features/{id}/reports/gate-3a-report.md (design review)
  - product/features/{id}/reports/gate-3b-report.md (code review)
  - product/features/{id}/reports/gate-3c-report.md (risk validation)
  - product/features/{id}/testing/RISK-COVERAGE-REPORT.md

  Retrospective data (if available): {paste retrospective summary}
  Rework signals: {list any gate failures or rework commits}

  YOUR TASKS:

  1. PATTERN EXTRACTION — For each component implemented:
     a. Use /query-patterns to find existing patterns for the affected crate(s)
     b. If the component followed an existing pattern: verify it's still accurate.
        If the pattern drifted, use /store-procedure or context_correct to update it.
     c. If the component established a NEW reusable structure (used in 2+ features
        or clearly generic): store it via context_store(category: 'pattern').
     d. If the component was one-off: skip — don't store patterns for unique work.

  2. PROCEDURE REVIEW — Check if any HOW-TO changed:
     a. Did the build/test/integration process change?
     b. Did schema migration steps change?
     c. Was there a new technique that future developers need?
     If yes: use /store-procedure (new) or context_correct (update existing).

  3. ADR VALIDATION — For each ADR created during this feature:
     a. Was the decision validated by successful implementation?
     b. Did implementation reveal that an ADR was wrong or incomplete?
        If so: flag for supersession (do NOT supersede without human approval).

  4. LESSON EXTRACTION — From gate failures and rework:
     a. What went wrong? (root cause, not symptoms)
     b. Is the lesson generalizable beyond this feature?
     c. If yes: use /store-lesson.

  Return:
  1. Patterns: [new entries with IDs, updated entries with IDs, skipped with reason]
  2. Procedures: [new/updated with IDs]
  3. ADR status: [validated ADRs, flagged-for-supersession ADRs with reason]
  4. Lessons: [new entries with IDs]
  5. Observations: [anything notable about the feature cycle]")
```

---

## Phase 3: ADR Supersession (if flagged)

If the architect flagged any ADRs for supersession:

1. Present each flagged ADR to the human:
   ```
   ADR #{entry-id}: "{title}"
   Architect's finding: {why it should be superseded}
   Proposed replacement: {what the new decision should be}

   Approve supersession?
   ```

2. If human approves: spawn architect to perform the supersession via `/store-adr`.
3. If human disagrees: note as "ADR validated with caveat".

---

## Phase 4: Worktree Cleanup

Worker agents spawned with `isolation: "worktree"` create directories under `.claude/worktrees/`. Each contains a full `target/` build directory (~1-2GB). Clean up after merge.

```bash
# List worktrees to find stale agent-created ones
git worktree list

# Remove each stale worktree (safe — feature is merged)
git worktree remove .claude/worktrees/{agent-id}/ 2>/dev/null

# Prune stale entries
git worktree prune
```

If a worktree has uncommitted changes, warn the human — do NOT force-remove.

---

## Phase 5: Summary & Outcome

Collect all knowledge base changes from Phases 2-3.

Use `/record-outcome` with:
- Feature: `{feature-id}`
- Type: `retro`
- Phase: `retro`
- Result: `pass`
- Content: `Retrospective complete. {N} patterns, {N} procedures, {N} lessons extracted. {N} ADRs validated.`

**Return format:**
```
RETROSPECTIVE COMPLETE — Knowledge base updated.

Feature: {feature-id}
PR: #{pr-number} (merged)

Knowledge extracted:
- Patterns: {count} new, {count} updated
- Procedures: {count} new, {count} updated
- Lessons learned: {count} new
- ADRs validated: {count}
- ADRs superseded: {count}

Details:
{list each entry with Unimatrix ID, title, and whether new or updated}
```

---

## When to Go Lightweight

Not every feature needs a full retro:

| Situation | Action |
|---|---|
| Zero gate failures, no rework | Skip lesson extraction. Focus on patterns/procedures only. |
| Minor enhancement (1-2 components) | Check for pattern drift only, skip procedure review. |
| New infrastructure introduced | Full retro — high likelihood of new patterns and procedures. |
| Multiple SCOPE FAILs or heavy rework | Full retro — prioritize lesson extraction. |
