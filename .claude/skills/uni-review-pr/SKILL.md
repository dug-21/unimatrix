---
name: "uni-review-pr"
description: "PR security review and merge readiness check. Use after delivery or bugfix opens a PR, or standalone when human wants to review a PR."
---

# Review PR — Security Review + Merge Readiness

## What This Skill Does

Verifies gate results, spawns a fresh-context security reviewer, and assesses merge readiness. Works as:
- **Auto-invoked** by `uni-scrum-master` at the end of delivery/bugfix protocols
- **Standalone** when human invokes `/uni-review-pr {pr-number}` directly

---

## Inputs

From the invoker (SM or human):
- PR number or URL
- Feature ID (if available)
- GH Issue number (if available)

If invoked standalone with just a PR number, extract feature ID and GH Issue from the PR body.

---

## Step 1: Verify Gate Results

Read the feature directory for gate reports:

- `product/features/{id}/reports/gate-3a-report.md` — exists and shows PASS
- `product/features/{id}/reports/gate-3b-report.md` — exists and shows PASS
- `product/features/{id}/reports/gate-3c-report.md` — exists and shows PASS
- `product/features/{id}/testing/RISK-COVERAGE-REPORT.md` — exists

For bugfix PRs, check for the single gate report instead.

If any gate report is missing or shows FAIL, **stop and report** — delivery is incomplete.

---

## Step 2: Security Review (Fresh Context — MUST be a subagent)

Spawn `uni-security-reviewer` as a subagent for fresh-context review:

```
Agent(uni-security-reviewer, "
  Your agent ID: {feature-id}-security-reviewer
  Your Unimatrix agent_id: uni-security-reviewer
  SECURITY REVIEW — Fresh context. Read the PR diff cold.

  PR: #{pr-number} on branch {branch-name}
  Feature: {feature-id}
  GH Issue: #{issue-number}

  Read these with fresh eyes:
  - git diff main...HEAD (the full change set)
  - product/features/{id}/architecture/ARCHITECTURE.md (if exists)
  - product/features/{id}/RISK-TEST-STRATEGY.md (if exists)

  Assess:
  - OWASP concerns: injection, access control, deserialization, input validation
  - Blast radius: worst case if code has a subtle bug
  - Regression risk: could this break existing functionality
  - Dependency safety: new dependencies, known vulnerabilities
  - Secrets: no hardcoded credentials, tokens, or keys

  Comment findings on the PR via gh CLI.
  If any finding is blocking: use gh pr review --request-changes.

  Return: risk level (low/medium/high/critical), findings summary,
  blocking findings (yes/no + details).")
```

---

## Step 3: Merge Readiness Assessment

After security review returns, check all criteria:

- [ ] All delivery gates passed (from gate reports)
- [ ] Security review completed — no blocking findings
- [ ] PR description references GH Issue
- [ ] No merge conflicts with main

If all pass → `Merge readiness: READY`
If any blocking item → `Merge readiness: BLOCKED` with specific items listed

---

## Step 4: Report

Post security review result to GH Issue:
```
## Security Review — {PASS|BLOCKED}
- Summary: {risk level} — {findings summary}
- Merge readiness: {READY|BLOCKED}
- Issues: [blocking items if any]
```

**Return format** (to SM or human):
```
PR REVIEW COMPLETE

PR: {URL}
Feature: {feature-id}

Security Review: {risk level} — {summary}
Blocking findings: {yes/no + details}
Merge readiness: {READY | BLOCKED}

If BLOCKED:
- Blocking items: [list]
- Recommended action: [fix items or discuss]

Human action required: Approve and merge, or address blocking items.
```

---

## Step 5: Record Outcome

Use `/uni-record-outcome` with:
- Feature: `{feature-id}`
- Type: `feature` (or `bugfix`)
- Phase: `review`
- Result: `pass` (or `blocked`)
- Content: `PR review complete. Security: {risk level}. Merge readiness: {READY|BLOCKED}.`
