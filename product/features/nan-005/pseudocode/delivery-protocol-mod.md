# Pseudocode: delivery-protocol-mod

## Purpose

Add a conditional documentation step to Phase 4 of `uni-delivery-protocol.md`. The step spawns the `uni-docs` agent when trigger criteria are met, ensuring README.md stays current after each user-facing feature delivery.

---

## Current Phase 4 Structure (lines 325-369 of uni-delivery-protocol.md)

```
## Phase 4: Delivery

**Prerequisite**: All three gates (3a, 3b, 3c) have passed.

The Delivery Leader:
1. Commits final artifacts (`test: risk coverage + gate reports (#{issue})`)
2. Pushes feature branch and opens PR (see `/uni-git` for PR template)
3. Updates GH Issue with PR link
4. Invokes `/review-pr` for security review and merge readiness
5. Combines impl + deploy results in the return to human

```bash
# Commit final artifacts
git add product/features/{id}/testing/ product/features/{id}/reports/
git commit -m "test: risk coverage + gate reports (#{issue})"
git push -u origin feature/{phase}-{NNN}

# Open PR (see uni-git skill for full template)
gh pr create --title "[{feature-id}] {title}" --body "..."
```

### PR Review (after PR opens)

Invoke `/review-pr` ...
```

## Modified Phase 4 Structure

The modification inserts a new subsection between the `gh pr create` block and the `### PR Review` subsection.

### Exact Insertion Point

**After**: The `gh pr create` bash block (line ~344 in current file)
**Before**: `### PR Review (after PR opens)` (line ~346 in current file)

### Updated Numbered List

The Delivery Leader numbered list changes from:
```
1. Commits final artifacts
2. Pushes feature branch and opens PR
3. Updates GH Issue with PR link
4. Invokes `/review-pr`
5. Combines impl + deploy results
```

To:
```
1. Commits final artifacts
2. Pushes feature branch and opens PR
3. Updates GH Issue with PR link
4. [CONDITIONAL] Spawns uni-docs agent if trigger criteria met
5. Invokes `/review-pr`
6. Combines impl + deploy results
```

---

## New Content to Insert

The following subsection is inserted after the `gh pr create` bash block and before `### PR Review`:

```markdown
### Documentation Update (conditional — after PR opens)

Evaluate whether the feature requires a README update using the trigger criteria table below.

#### Trigger Criteria

| Feature Change Type | Documentation Step |
|--------------------|--------------------|
| New or modified MCP tool | **MANDATORY** |
| New or modified skill | **MANDATORY** |
| New CLI subcommand or flag | **MANDATORY** |
| New knowledge category | **MANDATORY** |
| New operational constraint for users | **MANDATORY** |
| Schema change with user-visible behavior change | **MANDATORY** |
| Internal refactor (no user-visible change) | SKIP |
| Test-only feature | SKIP |
| Documentation-only feature | SKIP |

**Decision rule**: Read the feature's SCOPE.md Goals section. If any goal matches a MANDATORY trigger, spawn `uni-docs`. If all goals are internal, skip.

#### Spawn Template

```
Task(subagent_type: "uni-docs",
  prompt: "Your agent ID: {feature-id}-docs

    Feature: {feature-id}
    Issue: #{issue}

    Read these files:
    - product/features/{id}/SCOPE.md
    - product/features/{id}/specification/SPECIFICATION.md
    - README.md

    Identify README sections affected by this feature.
    Propose and commit targeted edits to the feature branch.
    Commit message: docs: update README for {feature-id} (#{issue})

    Return: sections modified, commit hash, or 'no changes needed'.")
```

**No gate.** This step is advisory — it does not block delivery. If `uni-docs` fails to produce useful output, proceed to `/review-pr` without documentation updates. Documentation changes are part of the reviewed PR.
```

---

## Edit Specification

### Edit 1: Update the numbered list

**File**: `.claude/protocols/uni/uni-delivery-protocol.md`
**Location**: Phase 4 numbered list (around line 329-334)

**Old text** (exact match required):
```
The Delivery Leader:
1. Commits final artifacts (`test: risk coverage + gate reports (#{issue})`)
2. Pushes feature branch and opens PR (see `/uni-git` for PR template)
3. Updates GH Issue with PR link
4. Invokes `/review-pr` for security review and merge readiness
5. Combines impl + deploy results in the return to human
```

**New text**:
```
The Delivery Leader:
1. Commits final artifacts (`test: risk coverage + gate reports (#{issue})`)
2. Pushes feature branch and opens PR (see `/uni-git` for PR template)
3. Updates GH Issue with PR link
4. Evaluates documentation trigger criteria (see below) — spawns `uni-docs` if mandatory
5. Invokes `/review-pr` for security review and merge readiness
6. Combines impl + deploy results in the return to human
```

### Edit 2: Insert Documentation Update subsection

**File**: `.claude/protocols/uni/uni-delivery-protocol.md`
**Location**: After the `gh pr create` bash block, before `### PR Review (after PR opens)`

Insert the full "Documentation Update (conditional -- after PR opens)" subsection as specified above.

### Edit 3: Update Quick Reference message map

**File**: `.claude/protocols/uni/uni-delivery-protocol.md`
**Location**: Quick Reference section, Phase 4 line

**Old text**:
```
  Phase 4:    git commit + push + gh pr create
              /review-pr — security review + merge readiness
              Combined return — SESSION 2 ENDS
```

**New text**:
```
  Phase 4:    git commit + push + gh pr create
              [CONDITIONAL] uni-docs — documentation update (if trigger criteria met)
              /review-pr — security review + merge readiness
              Combined return — SESSION 2 ENDS
```

---

## Constraints

- **Additive only** (C-03, NFR-06): No existing phases, gates, or steps are removed or reordered.
- **No gate** (C-07, FR-12f): The documentation step does not block delivery.
- **Trigger criteria in protocol** (R-07): The decision table is embedded in the protocol text — the Delivery Leader does not need to consult external documents.
- **Spawn template included** (SR-07): The exact Task prompt is in the protocol so the Delivery Leader has a concrete template.

---

## Error Handling

- `uni-docs` agent fails: Delivery Leader logs "documentation update failed" and proceeds to `/review-pr`.
- `uni-docs` returns "no changes": Normal — feature had no user-visible impact. Proceed.
- SCOPE.md not found by uni-docs: Agent returns immediately. Delivery continues.

---

## Key Test Scenarios

1. Documentation step appears after `gh pr create` and before `/review-pr` in Phase 4 (AC-07, R-06).
2. Trigger criteria table lists all mandatory conditions: MCP tool, skill, CLI subcommand, knowledge category, operational constraint, user-visible schema change (AC-08, R-07).
3. Trigger criteria table lists all skip conditions: internal refactor, test-only, documentation-only (AC-08).
4. Spawn template includes feature ID, issue number, artifact paths, and commit message format.
5. Step explicitly states "no gate" / advisory only (FR-12f).
6. Diff between old and new protocol shows only additions — no deletions or reordering of existing content (NFR-06).
7. Quick Reference message map updated to reflect the new step.
8. Numbered list in Phase 4 intro updated with step 4 (documentation trigger evaluation).
