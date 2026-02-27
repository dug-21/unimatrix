# Bug Fix Protocol (Single-Session)

Triggers on: bug, fix, bugfix, defect, regression, broken, failing, error, crash.

---

## Execution Model

A single-session workflow that takes a bug report from diagnosis through merge. The session includes a human checkpoint after diagnosis — the human must agree with the root cause analysis before any code changes are made.

```
Primary Agent                    uni-bugfix-manager              Specialist Agents
─────────────                    ──────────────────              ─────────────────
read bug report (GH Issue or description)
spawn bugfix-manager ──────────► read protocol + bug report
                                 spawn investigator (Phase 1)
                                 ◄──────────────────────────── diagnosis + proposed fix
                                 present diagnosis to human
                                 ★ HUMAN CHECKPOINT ★
                                 human approves diagnosis
                                 create branch (Phase 2)
                                 spawn rust-dev ─────────────► implement fix + tests
                                 ◄──────────────────────────── changed files + test results
                                 spawn tester (Phase 3)
                                 ◄──────────────────────────── full test suite results
                                 spawn validator (Gate 3) ───► validate fix
                                 ◄──────────────────────────── PASS / REWORKABLE FAIL / SCOPE FAIL
                                 on PASS: open PR
                                 spawn security-reviewer ────► review PR diff (fresh context)
                                 ◄──────────────────────────── security assessment
◄──────────────────────────────  present PR + security assessment
human reviews and merges
```

**The Bugfix Manager runs the session autonomously after human approves diagnosis.** Human re-enters only on scope/feasibility failures, rework exhaustion, or final PR review.

### Concurrency Rules

- ALWAYS batch ALL file reads/writes/edits in ONE message
- ALWAYS batch ALL Bash commands in ONE message
- Bug fix phases are mostly sequential — only one specialist active at a time

### Bugfix Rules

- Agents return: file paths + test pass/fail + issues (NOT file contents)
- Max 2 rework iterations at the validation gate — protects context window
- Cargo output truncated to first error + summary line
- The coordinator never generates fix code or diagnoses bugs — only orchestrates

---

## Initialization

The human starts a bug fix session by providing a bug report. This can be:
- A GH Issue URL
- A description of the bug (symptoms, reproduction steps, affected area)

The Bugfix Manager:
1. Reads the bug report (fetches GH Issue if URL provided)
2. Identifies the feature area and any related feature directories
3. Proceeds to Phase 1

---

## Phase 1: Discovery

**Agent**: uni-bug-investigator

The Bugfix Manager spawns `uni-bug-investigator` to diagnose the root cause:

```
Task(subagent_type: "uni-bug-investigator",
  prompt: "Your agent ID: {issue-number}-investigator

    Bug report:
    {bug description or GH Issue URL}

    Affected area (if known): {area hint}

    Explore the codebase, trace the affected code paths,
    identify the root cause, and propose a targeted fix.

    Return: root cause analysis, affected files, proposed fix approach,
    risk assessment, missing test identification.")
```

Wait for the investigator to complete.

### Human Checkpoint (MANDATORY — do NOT proceed without human approval)

After the investigator returns, the Bugfix Manager presents the diagnosis to the human:

```
DIAGNOSIS COMPLETE — Awaiting approval.

Root Cause: {summary from investigator}
Affected Files: {list from investigator}
Proposed Fix: {approach from investigator}
Risk Assessment: {from investigator}
Missing Test: {what test should have caught this}

Report: write back to the GH issue
(or inline if no feature-id applies)

Human action required: Review diagnosis and approve to proceed with fix.
If the diagnosis is wrong, provide feedback and I will re-investigate.
```

**If the human disagrees**: Re-spawn the investigator with the human's feedback:

```
Task(subagent_type: "uni-bug-investigator",
  prompt: "Your agent ID: {issue-number}-investigator-v2

    REWORK: Human disagrees with initial diagnosis.

    Human feedback: {feedback}
    Previous diagnosis report: {path to investigator report}

    Read your previous report first from issue, then re-investigate
    with the human's feedback in mind.

    Return: revised root cause analysis, affected files,
    revised fix approach, risk assessment.")
```

---

## Phase 2: Fix Execution

**Prerequisite**: Human has approved the diagnosis.

**Agent**: uni-rust-dev

The Bugfix Manager:
1. Creates the bug fix branch: `git checkout -b bugfix/{issue-number}-{short-description}`
2. Spawns `uni-rust-dev` with the agreed fix approach:

```
Task(subagent_type: "uni-rust-dev",
  prompt: "Your agent ID: {issue-number}-agent-1-fix

    BUG FIX — not a feature implementation.

    Bug report: {bug description}
    Root cause: {from approved diagnosis}
    Affected files: {from diagnosis}
    Proposed fix approach: {from approved diagnosis}
    Missing test: {from diagnosis}

    YOUR TASK:
    1. Implement the fix as described in the approved approach
    2. Write targeted test(s) that reproduce the bug and verify the fix
    3. Ensure the fix is minimal — do not include unrelated changes
    4. Run component-level tests during development

    RETURN FORMAT:
    1. Files modified: [paths]
    2. New tests: [test function names]
    3. Tests: pass/fail count
    4. Issues: [blockers]")
```

Wait for the rust-dev to complete.
Provide updates back to the GH issue periodically

---

## Phase 3: Verification

**Agent**: uni-tester (execution mode)

The Bugfix Manager spawns `uni-tester` to run the full test suite:

```
Task(subagent_type: "uni-tester",
  prompt: "Your agent ID: {issue-number}-agent-2-verify

    PHASE: Test Execution (Bug Fix Verification)

    Read: product/test/infra-001/USAGE-PROTOCOL.md

    Execute:
    1. The new bug-specific tests written by the developer
    2. Full workspace test suite (cargo test --workspace)
    3. Clippy check (cargo clippy --workspace -- -D warnings)
    4. Integration smoke tests (MANDATORY):
       cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60
    5. Integration suites relevant to the bug area (see USAGE-PROTOCOL.md suite table)

    INTEGRATION TEST FAILURE TRIAGE (CRITICAL):
    - CAUSED BY THIS BUG FIX → Report back to bugfix-manager for rework.
    - PRE-EXISTING / UNRELATED → Do NOT fix. File a GH Issue, mark the test
      @pytest.mark.xfail(reason='Pre-existing: GH#NNN — description').
    - BAD TEST ASSERTION → Fix the test. Document in results.
    Agents must NEVER fix integration test failures unrelated to the bug fix.

    If the bug was discovered by an integration test (GH Issue references
    a specific test), verify that specific test now passes and remove its
    @pytest.mark.xfail marker.

    Return: test results summary, any failures, clippy warnings,
            integration test counts, any GH Issues filed.")
```

Wait for the tester to complete.
Write updates back to GH issue

---

## Gate 3: Validation

**Agent**: uni-validator

Spawn `uni-validator` with the bugfix check set:

```
Task(subagent_type: "uni-validator",
  prompt: "Your agent ID: {issue-number}-gate-bugfix

    GATE: Bug Fix Validation
    Issue: #{issue-number}

    Validate:
    - Fix addresses the diagnosed root cause (not just symptoms)
    - No todo!(), unimplemented!(), TODO, FIXME, or placeholder functions
    - All tests pass (new bug-specific tests + existing suite)
    - No new clippy warnings
    - No unsafe code introduced
    - Fix is minimal (no unrelated changes included)
    - New test(s) would have caught the original bug
    - Integration smoke tests passed
    - Any xfail markers added have corresponding GH Issues
    - If bug was discovered by integration test, that test's xfail marker was removed

    Bug report: {bug description}
    Root cause diagnosis: {from approved diagnosis}
    Changed files: {from rust-dev return}
    New tests: {from rust-dev return}

    Write report to GH issue

    Return: PASS / REWORKABLE FAIL / SCOPE FAIL, report path, issues.")
```

**Gate results:**
- **PASS** → Commit fix code + tests, open PR, proceed to Phase 4
- **REWORKABLE FAIL** → Loop back to Phase 2 with failure details (max 2 iterations). Include the gate report path in the re-spawn prompt.
- **SCOPE FAIL** → Session stops. Return to human with recommendation.

On PASS, the Bugfix Manager:
1. Commits: `fix: {description} (#{issue-number})`
2. Pushes branch: `git push -u origin bugfix/{issue-number}-{short-description}`
3. Opens PR: `gh pr create --title "fix: {description} (#{issue-number})" --body "..."`

---

## Phase 4: Security Review

**Agent**: uni-security-reviewer (FRESH CONTEXT WINDOW)

After the PR is opened, spawn `uni-security-reviewer` with a fresh context window:

```
Task(subagent_type: "uni-security-reviewer",
  prompt: "You are reviewing a bug fix PR for security risks.
    Your agent ID: {issue-number}-security-reviewer

    PR: #{pr-number} on branch bugfix/{issue-number}-{short-description}
    Bug report: {bug description or GH Issue URL}
    Root cause analysis: {path to investigator report}

    Read these:
    - The git diff: run git diff main...HEAD
    - The investigator's report from the GH issue
    - Any relevant ADRs in the affected crate

    Assess security risks, blast radius, and regression potential.
    Comment findings on the PR via gh CLI.

    Return: risk level (low/medium/high/critical), findings summary,
    whether any findings are blocking.")
```

Write results to GH issue

---

## Phase 5: Human Review & Merge

The Bugfix Manager presents the PR and security assessment to the human:

```
BUG FIX COMPLETE — PR ready for review.

PR: {PR URL}
Branch: bugfix/{issue-number}-{short-description}

Fix Summary:
- Root cause: {summary}
- Files changed: {list}
- New tests: {list}
- All tests passing: yes/no

Gate: Bug Fix Validation — PASS
Security Review: {risk level} — {summary of findings}
Blocking findings: {yes/no + details}

Reports:
- Link to GH issue

Human action required: Review PR and approve merge.
```

On human approval, the Bugfix Manager:
1. Merges the PR (if human requests it)
2. Closes the GH Issue with reference to the PR (if applicable)

---

## GH Issue Lifecycle

If the bug has a GH Issue:

1. **Phase 1 complete**: Comment with diagnosis summary
2. **Phase 2 complete**: Comment with fix summary and changed files
3. **Gate PASS**: Comment with gate result and PR link
4. **Merge**: Close issue with reference to merged PR

**Comment format** (post after each phase):
```
## {Phase Name} — {Status}
- Summary: {brief description}
- Files: [paths]
- Tests: X passed, Y new
- Issues: [if any]
```

---

## Rework Protocol

### Reworkable Failures
Fix doesn't address root cause, test gaps exist, code quality issues. Loop back to Phase 2 agents with failure details.

**Max 2 rework iterations.** If still failing after 2 iterations, escalate as SCOPE FAIL.

When re-spawning agents for rework:
1. Include the gate report path in the prompt
2. List specific failures to address
3. Instruct agent to read the gate report first

### Scope/Feasibility Failures
Root cause is deeper than expected, fix requires architectural changes, bug is actually a design issue.

**Session stops immediately.** The Bugfix Manager returns to the human with:
- What failed and why
- Recommendation: simple fix insufficient, needs feature work or design change
- All artifacts produced so far

---

## Cargo Output Truncation (CRITICAL)

Always truncate cargo output:
```bash
# Build: first error + summary
cargo build --workspace 2>&1 | grep -A5 "^error" | head -20
cargo build --workspace 2>&1 | tail -3

# Test: summary only
cargo test --workspace 2>&1 | tail -30

# Clippy: first warnings only
cargo clippy --workspace -- -D warnings 2>&1 | head -30
```

NEVER pipe full cargo output into context.

---

## Quick Reference: Message Map

```
BUGFIX MANAGER (uni-bugfix-manager):
  Phase 1:    Task(uni-bug-investigator) — diagnose root cause
              ...present diagnosis to human...
              ★ HUMAN CHECKPOINT — human approves diagnosis ★
  Phase 2:    git checkout -b bugfix/{issue}-{desc}
              Task(uni-rust-dev) — implement fix + tests
              ...wait...
  Phase 3:    Task(uni-tester) — full test suite verification
              ...wait...
  Gate 3:     Task(uni-validator, bugfix check set)
              ...PASS → continue / FAIL → rework or stop...
              git commit + push + gh pr create
  Phase 4:    Task(uni-security-reviewer) — PR security review (fresh context)
              ...wait...
  Phase 5:    Present PR + security assessment to human — SESSION ENDS
```

---

## Outcome Recording

After presenting the PR to the human, record the bugfix outcome in Unimatrix:

```
context_store(
  category: "outcome",
  feature_cycle: "{bug-id}",
  tags: ["type:bugfix", "phase:delivery", "result:pass"],
  content: "Bugfix complete. Root cause: {summary}. PR: {url}"
)
```
