# Bug Fix Protocol (Single-Session)

Triggers on: bug, fix, bugfix, defect, regression, broken, failing, error, crash.

---

## Execution Model

A single-session workflow that takes a bug report from diagnosis through merge. The session includes a human checkpoint after diagnosis — the human must agree with the root cause analysis before any code changes are made.

**You become the Bugfix Leader(uni-scrum-master).** Read the SM agent definition (`.claude/agents/uni/uni-scrum-master.md`) for your role boundaries. You orchestrate — you NEVER generate content. Spawn specialist agents for all work.

```
Bugfix Leader (you)                                  Specialist Agents
───────────────────                                  ─────────────────
read protocol + bug report
spawn investigator (Phase 1) ───────────────────────► diagnosis + proposed fix
◄────────────────────────────────────────────────────
spawn architect (Phase 1b) ─────────────────────────► design review of proposed fix
◄────────────────────────────────────────────────────
present diagnosis + design review to human
★ HUMAN CHECKPOINT ★
human approves diagnosis + design
create branch (Phase 2)
spawn rust-dev ─────────────────────────────────────► implement fix + tests
◄────────────────────────────────────────────────────
spawn tester (Phase 3) ─────────────────────────────► full test suite results
◄────────────────────────────────────────────────────
spawn validator (Gate 3) ───────────────────────────► validate fix
◄──────────────────────────────────────────────────── PASS / REWORKABLE FAIL / SCOPE FAIL
on PASS: open PR
spawn security-reviewer ────────────────────────────► review PR diff (fresh context)
◄────────────────────────────────────────────────────
◄──────────────────────────────  present PR + security assessment
human reviews and merges
```

**The Bugfix Manager runs the session autonomously after human approves diagnosis and design.** Human re-enters only on scope/feasibility failures, rework exhaustion, or final PR review.

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
3. **Declares feature cycle** — before any agent spawning:
   ```
   context_cycle(
     type: "start",
     topic: "bugfix-{issue-number}",
     goal: "{1-2 sentence summary of the goal or problem to be fixed}",
     next_phase: "discovery",
     agent_id: "{issue-number}-bugfix-leader"
   )
   ```
5. Passes relevant info & goal to the investigator in Phase 1

Worker agents are spawned with `isolation: "worktree"` for branch isolation (see `/uni-git` Worktree Isolation).

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

---

## Phase 1b: Design Review

**Agent**: uni-architect

After the investigator returns, the Bugfix Manager spawns `uni-architect` to review the proposed fix design **before any human checkpoint or code changes**:

```
Task(subagent_type: "uni-architect",
  prompt: "Your agent ID: {issue-number}-design-reviewer

    DESIGN REVIEW — proposed bug fix approach only. Do NOT implement.

    Bug report: {bug description}
    Investigator's proposed fix: {proposed fix approach from investigator}
    Affected files: {from investigator}

    Review the proposed fix for:
    1. Hot-path risks — does the fix introduce DB reads, locks, or I/O on a
       background tick, request handler, or other hot path? If so, flag and
       propose a safer alternative (cache, pagination, SQL filter pushed down).
    2. Blast radius — what is the worst case if the fix has a subtle bug?
    3. Architectural fit — does the approach follow established patterns for
       this subsystem? Query Unimatrix for relevant ADRs and conventions.
    4. Missing constraints — are there caps, idempotency guards, or error
       recovery steps missing from the proposed approach?
    5. Security surface — any new trust boundaries, input validation gaps, or
       privilege changes introduced by the approach?

    Return:
    - Design assessment: APPROVED / APPROVED WITH NOTES / REWORK NEEDED
    - Findings: list of concerns with severity (blocking / non-blocking)
    - Revised fix approach (if REWORK NEEDED): concrete amendments to the
      investigator's proposal that address the findings
    - ## Knowledge Stewardship block with Queried: and Stored:/Declined: entries")
```

Wait for the architect to complete.

**If REWORK NEEDED**: The Bugfix Manager incorporates the architect's revised approach into the fix plan presented to the human. The investigator is NOT re-spawned — the architect's revised approach supersedes the investigator's proposal for the affected parts.

### Human Checkpoint (MANDATORY — do NOT proceed without human approval)

After both investigator and architect return, the Bugfix Manager presents the combined diagnosis and design review to the human:

```
DIAGNOSIS + DESIGN REVIEW COMPLETE — Awaiting approval.

Root Cause: {summary from investigator}
Affected Files: {list from investigator}
Proposed Fix: {approach — investigator's if APPROVED, architect's revised approach if REWORK NEEDED}
Design Review: {APPROVED | APPROVED WITH NOTES | REWORK NEEDED} — {architect findings summary}
Risk Assessment: {from investigator + architect}
Missing Test: {what test should have caught this}

Human action required: Review diagnosis and design, then approve to proceed with fix.
If either the diagnosis or design is wrong, provide feedback and I will re-investigate.
```

On human approval:

```
context_cycle(type: "phase-end", phase: "discovery", next_phase: "fix", agent_id: "{issue-number}-bugfix-leader")
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

Wait for the rust-dev to complete. Provide updates back to the GH issue periodically.

```
context_cycle(type: "phase-end", phase: "fix", next_phase: "testing", agent_id: "{issue-number}-bugfix-leader")
```

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
    - CAUSED BY THIS BUG FIX → Report back to Bugfix Leader for rework.
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
    - Knowledge stewardship: investigator and rust-dev reports contain ## Knowledge Stewardship block with Queried/Stored/Declined entries

    Bug report: {bug description}
    Root cause diagnosis: {from approved diagnosis}
    Changed files: {from rust-dev return}
    New tests: {from rust-dev return}

    Post report as a comment on GH Issue #{issue-number}.

    Return: PASS / REWORKABLE FAIL / SCOPE FAIL, specific issues.")
```

**Gate results:**
- **PASS** →
  1. `context_cycle(type: "phase-end", phase: "testing", next_phase: "bug-review", agent_id: "{issue-number}-bugfix-leader")`
  2. Commit fix code + tests, open PR, proceed to Phase 4
- **REWORKABLE FAIL** → Loop back to Phase 2 with failure details (max 2 iterations). Include the gate report path in the re-spawn prompt.
- **SCOPE FAIL** → Session stops. Return to human with recommendation.

On PASS, the Bugfix Manager:
1. Stages and commits fix code, tests, and feature artifacts:
   ```bash
   git add product/features/{issue-number}/
   git commit -m "fix: {description} (#{issue-number})"
   ```
2. Pushes branch: `git push -u origin bugfix/{issue-number}-{short-description}`
3. Opens PR: `gh pr create --title "fix: {description} (#{issue-number})" --body "..."`

---

## Phase 4: PR Review

After the PR is opened, invoke `/uni-review-pr` with the PR number, feature/issue ID, and GH Issue number. This spawns a fresh-context security reviewer and assesses merge readiness.

For bugfix PRs, the review verifies the single gate report (not three delivery gates).

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
1. Merges the PR with `gh pr merge --rebase` (if human requests it)
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
BUGFIX LEADER (you):
  Init:       /uni-query-patterns + /uni-knowledge-search — prior knowledge
              context_cycle(type: "start", topic: "bugfix-{issue-number}", next_phase: "discovery", agent_id: "{issue-number}-bugfix-leader")
  Phase 1:    Task(uni-bug-investigator) — diagnose root cause → GH Issue comment
              ...wait...
  Phase 1b:   Task(uni-architect) — design review of proposed fix
              ...present diagnosis + design review to human...
              ★ HUMAN CHECKPOINT — human approves diagnosis + design ★
              context_cycle(type: "phase-end", phase: "discovery", next_phase: "fix", ...)
  Phase 2:    git checkout -b bugfix/{issue}-{desc}
              Task(uni-rust-dev) — implement fix + tests → GH Issue comment
              ...wait...
              context_cycle(type: "phase-end", phase: "fix", next_phase: "testing", ...)
  Phase 3:    Task(uni-tester) — full test suite verification → GH Issue comment
              ...wait...
  Gate 3:     Task(uni-validator, bugfix check set) → GH Issue comment
              ...PASS → context_cycle(phase-end, testing → bug-review) → commit + push + PR
              ...FAIL → rework or stop...
  Phase 4:    /uni-review-pr — security review + merge readiness → GH Issue comment
              ...wait...
  Phase 5:    Present PR + security assessment to human — SESSION ENDS
              context_cycle(type: "phase-end", phase: "bug-review", ...)
              context_cycle(type: "stop", topic: "bugfix-{issue-number}", outcome: "...", agent_id: "{issue-number}-bugfix-leader")
              /uni-store-lesson (if generalizable)
```

---

## Outcome Recording

After presenting the PR to the human, close the bug-review phase and stop the cycle:

```
context_cycle(
  type: "phase-end",
  phase: "bug-review",
  agent_id: "{issue-number}-bugfix-leader"
)

context_cycle(
  type: "stop",
  topic: "bugfix-{issue-number}",
  outcome: "Bugfix complete. Root cause: {summary}. PR: {url}",
  agent_id: "{issue-number}-bugfix-leader"
)
```

Then use Unimatrix skills as applicable:

1. **If root cause is generalizable**: `/uni-store-lesson` — persist the root cause pattern so future investigators find it via `/uni-knowledge-search`. Tag with `caused_by_feature:{feature-id}` when applicable. Include what could have been done during the originating feature's design phase to prevent the bug.
2. **If diagnostic/repair sequence is reproducible**: `/uni-store-procedure` — store the technique so future agents can find it.

### Stewardship Compliance

The bugfix gate validator checks stewardship compliance for investigator and rust-dev agents:
- Investigator report must include `## Knowledge Stewardship` with `Queried:` and `Stored:`/`Declined:` entries
- Rust-dev report must include `## Knowledge Stewardship` with `Queried:` and `Stored:`/`Declined:` entries
- Missing stewardship block = REWORKABLE FAIL

All phase outputs (diagnosis, fix summary, gate results, security review) are posted as **GH Issue comments** — never written to the filesystem.
