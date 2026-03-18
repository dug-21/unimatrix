---
name: "uni-retro"
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

## Phase 1: Data Gathering & Retrospective Analysis

Gather all evidence about the shipped feature:

1. **Run retrospective analysis** (if observation data exists):
   ```
   mcp__unimatrix__context_cycle_review(feature_cycle: "{feature-id}")
   ```
   This returns structured data: metrics, hotspots, baseline comparisons, narratives, and recommendations.

2. **Analyze the retrospective data** — extract actionable findings:

   a. **Hotspots by severity** — Classify each hotspot:
      - `Warning` hotspots → potential lessons or procedure gaps
      - `Info` hotspots → note trends, may not need action
      - Key hotspot types to watch:
        - `permission_retries` → settings.json allowlist may need updating
        - `sleep_workarounds` → agents using sleep instead of run_in_background
        - `cold_restart` → context loss after gaps, agents re-reading files
        - `coordinator_respawns` → SM lifetime/handoff issues
        - `post_completion_work` → significant work after task marked done (scope issue?)
        - `lifespan` → agent running too long (context overflow risk)
        - `mutation_spread` → touching too many files (coupling/scope creep?)
        - `file_breadth` / `reread_rate` → agents inefficiently navigating codebase

   b. **Baseline outliers** — Any metric with `status: "Outlier"` deserves attention:
      - Is it a positive shift (e.g., higher `parallel_call_rate`)? Note as trend.
      - Is it a problem (e.g., high `post_completion_work`)? Extract lesson.
      - Is it a `NewSignal`? First time this metric has a non-zero value — note for future tracking.

   c. **Recommendations** — The retrospective returns specific actionable recommendations.
      Each one maps to either a procedure update or a lesson learned.

   d. **Narratives** — Temporal clustering of events. Look for:
      - Burst patterns (many events in short window → agent struggling)
      - Sequence patterns (repeated cycles → inefficient workflow)
      - Top files (which files caused the most friction)

3. **Read feature artifacts**:
   - `product/features/{id}/architecture/ARCHITECTURE.md`
   - `product/features/{id}/pseudocode/OVERVIEW.md`
   - `product/features/{id}/testing/RISK-COVERAGE-REPORT.md`
   - `product/features/{id}/reports/gate-3a-report.md`
   - `product/features/{id}/reports/gate-3b-report.md`
   - `product/features/{id}/reports/gate-3c-report.md`

4. **Check for rework signals**: Did any gate fail before passing? Read the gate report for what went wrong.

5. **Review the git log** for this feature's branch:
   ```bash
   git log main..HEAD --oneline
   ```
   Look for rework commits (`fix(gate):`) — these indicate where the process struggled.

---

## Phase 1b: Stewardship Quality Review

Before extracting new patterns, review the quality of entries agents stored during this feature cycle.

1. **Query entries stored during the feature**:
   ```
   mcp__unimatrix__context_search(
     query: "{feature-id}",
     k: 20
   )
   ```
   Also search by feature_cycle tag if available. Use content/title matching as fallback — not all agents tag consistently.

2. **Assess each entry against its category template**:
   - **Patterns**: Has what/why/scope structure? Is "why" substantive (not "it works")?
   - **Lessons**: Has what-happened/root-cause/takeaway? Is takeaway actionable?
   - **Procedures**: Has numbered steps? Are steps specific (not generic)?

3. **Curate**:
   - **Low-quality entries** (missing structure, no substantive "why", API docs disguised as patterns): deprecate via `context_deprecate` with reason.
   - **High-quality entries** confirmed by successful delivery: note for the architect to validate during pattern extraction.
   - **Miscategorized entries** (lesson stored as pattern, or vice versa): note for correction.

4. **Report** the stewardship review results before proceeding to Phase 2:
   ```
   Stewardship Quality Review:
   - Entries found: {N}
   - Quality: {N} good, {N} deprecated (low quality), {N} flagged for recategorization
   - Details: {list each entry with assessment}
   ```

---

## Phase 2: Pattern & Procedure Extraction (MUST be a subagent)

**Before spawning the architect**, prepare a structured retrospective briefing from Phase 1. This replaces the vague "paste summary" — give the architect concrete data to work with.

Build the briefing:

```
RETROSPECTIVE BRIEFING for {feature-id}
========================================

Session stats: {session_count} sessions, {total_records} records, {total_tool_calls} tool calls, {total_duration_secs}s

HOTSPOTS ({count} detected):
{For each hotspot: "- [{severity}] {rule_name}: {claim} (measured: {measured}, threshold: {threshold})"}

BASELINE OUTLIERS:
{For each baseline entry with status "Outlier" or "NewSignal":
  "- {metric_name}: {current_value} vs mean {mean} (stddev {stddev}) — {status}"}

RECOMMENDATIONS FROM RETROSPECTIVE:
{For each recommendation: "- [{hotspot_type}] {action} — {rationale}"}

REWORK SIGNALS:
{gate failures, rework commits from Phase 1 step 4-5}
```

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

  {paste the RETROSPECTIVE BRIEFING from above}

  YOUR TASKS:

  1. PATTERN EXTRACTION — For each component implemented:
     a. Use /uni-query-patterns to find existing patterns for the affected crate(s)
     b. If the component followed an existing pattern: verify it's still accurate.
        If the pattern drifted, use /uni-store-procedure or context_correct to update it.
     c. If the component established a NEW reusable structure (used in 2+ features
        or clearly generic): store it via context_store(category: 'pattern').
     d. If the component was one-off: skip — don't store patterns for unique work.

  2. PROCEDURE REVIEW — Check if any HOW-TO changed:
     a. Did the build/test/integration process change?
     b. Did schema migration steps change?
     c. Was there a new technique that future developers need?
     If yes: use /uni-store-procedure (new) or context_correct (update existing).

  3. ADR VALIDATION — For each ADR created during this feature:
     a. Was the decision validated by successful implementation?
     b. Did implementation reveal that an ADR was wrong or incomplete?
        If so: flag for supersession (do NOT supersede without human approval).

  4. LESSON EXTRACTION — Two sources:

     A. From gate failures and rework:
        a. What went wrong? (root cause, not symptoms)
        b. Is the lesson generalizable beyond this feature?
        c. If yes: use /uni-store-lesson.

     B. From retrospective hotspots and recommendations:
        For each Warning-severity hotspot, ask:
        - Is this a recurring problem (check baseline — is it consistently above threshold)?
        - Can it be prevented by a procedure change or config update?
        - If yes: store as lesson (/uni-store-lesson) or procedure (/uni-store-procedure).

        For each recommendation from the retrospective:
        - Check if a matching procedure already exists (/uni-query-patterns).
        - If not, and the recommendation is actionable: store as procedure.
        - If it updates existing guidance: use context_correct.

     C. From baseline outliers:
        - Positive outliers (improvements): note what changed and why — may be a new pattern.
        - Negative outliers (regressions): root-cause and store as lesson if generalizable.

  Return:
  1. Patterns: [new entries with IDs, updated entries with IDs, skipped with reason]
  2. Procedures: [new/updated with IDs]
  3. ADR status: [validated ADRs, flagged-for-supersession ADRs with reason]
  4. Lessons: [new entries with IDs]
  5. Retrospective findings: [hotspot-derived lessons, recommendation actions taken, outlier notes]")
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

2. If human approves: spawn architect to perform the supersession via `/uni-store-adr`.
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

**Commit retro artifacts** before recording outcome:
```bash
git add product/features/{id}/agents/
git commit -m "chore: add retro artifacts ({feature-id})"
git push origin main
```

Use `/uni-record-outcome` with:
- Feature: `{feature-id}`
- Type: `retro`
- Phase: `retro`
- Result: `pass`
- Content: `Retrospective complete. {N} patterns, {N} procedures, {N} lessons extracted. {N} ADRs validated. Hotspots: {count} ({warning_count} warnings). Outliers: {list outlier metric names}.`

**Return format:**
```
RETROSPECTIVE COMPLETE — Knowledge base updated.

Feature: {feature-id}
PR: #{pr-number} (merged)

Retrospective summary:
- Sessions: {session_count}, Tool calls: {total_tool_calls}, Duration: {duration}
- Hotspots: {count} ({warning_count} warnings, {info_count} info)
- Baseline outliers: {list metric names and status}

Knowledge extracted:
- Patterns: {count} new, {count} updated
- Procedures: {count} new, {count} updated
- Lessons learned: {count} new ({count} from hotspots, {count} from gate failures)
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
| Zero gate failures, no rework, zero hotspots | Skip lesson extraction. Focus on patterns/procedures only. |
| Minor enhancement (1-2 components) | Check for pattern drift only, skip procedure review. |
| New infrastructure introduced | Full retro — high likelihood of new patterns and procedures. |
| Multiple SCOPE FAILs or heavy rework | Full retro — prioritize lesson extraction. |
| Many Warning hotspots or baseline outliers | Full retro — prioritize hotspot-driven lessons and procedure updates. |
