# Delivery Session Protocol (Session 2)

Triggers on: implement, build, code, deliver, TDD, refactor, "proceed with implementation".

---

## Execution Model

Session 2 reads the IMPLEMENTATION-BRIEF.md produced in Session 1 and runs three stages autonomously, each with a validation gate. If all gates pass, the feature is delivered. If any gate fails beyond rework, the session stops and returns to the human.

```
SESSION 2 — DELIVERY
════════════════════

Stage 3a: Component Design (pseudocode + test plans)
  ★ Gate 3a: Design Review ★
         ↓
Stage 3b: Code Implementation
  ★ Gate 3b: Code Review ★
         ↓
Stage 3c: Testing & Risk Validation
  ★ Gate 3c: Risk Validation ★
         ↓
Phase 4: Delivery
  ★ RETURN TO HUMAN ★
```

**The Delivery Leader (uni-scrum-master) runs all stages autonomously.** Human re-enters only on scope/feasibility failures or when rework iterations are exhausted.

### Concurrency Rules

- ALWAYS spawn all agents WITHIN each stage in ONE message via Task tool
- ALWAYS batch ALL file reads/writes/edits in ONE message
- ALWAYS batch ALL Bash commands in ONE message

### Delivery Rules

- Agents return: file paths + test pass/fail + issues (NOT file contents)
- Max 2 rework iterations per gate — protects context window
- Cargo output truncated to first error + summary line
- The three source documents (Architecture, Specification, Risk Strategy) are sacred — all work traces back to them

---

## Initialization

The human starts Session 2 by providing the IMPLEMENTATION-BRIEF.md path (or GH Issue number).

The Delivery Leader reads:
1. `product/features/{feature-id}/IMPLEMENTATION-BRIEF.md` — Component Map, ADR references, constraints
2. `product/features/{feature-id}/ACCEPTANCE-MAP.md` — AC verification methods
3. Paths to the three source documents (listed in the brief)

---

## Stage 3a: Component Design & Pseudocode

**Agents**: uni-pseudocode + uni-tester (test plan design)

The Delivery Leader spawns both agents in parallel (ONE message):

```
Task(subagent_type: "uni-pseudocode",
  prompt: "Your agent ID: {feature-id}-agent-1-pseudocode

    Read these files before starting:
    - product/features/{id}/IMPLEMENTATION-BRIEF.md
    - product/features/{id}/architecture/ARCHITECTURE.md
    - product/features/{id}/specification/SPECIFICATION.md
    - product/features/{id}/RISK-TEST-STRATEGY.md

    Decompose the feature into components per the architecture.
    For each component, produce pseudocode files.

    Output:
    - pseudocode/OVERVIEW.md (component interaction, data flow, shared types)
    - pseudocode/{component}.md (per-component pseudocode)

    Return: file paths, component list, open questions.")

Task(subagent_type: "uni-tester",
  prompt: "Your agent ID: {feature-id}-agent-2-testplan

    PHASE: Test Plan Design (Stage 3a)

    Read these files before starting:
    - product/features/{id}/IMPLEMENTATION-BRIEF.md
    - product/features/{id}/architecture/ARCHITECTURE.md
    - product/features/{id}/specification/SPECIFICATION.md
    - product/features/{id}/RISK-TEST-STRATEGY.md

    Produce per-component test plans rooted in the Risk Strategy.

    Output:
    - test-plan/OVERVIEW.md (overall test strategy, risk-to-test mapping)
    - test-plan/{component}.md (per-component test expectations)

    Return: file paths, risk coverage mapping, open questions.")
```

Wait for both agents to complete.

### Gate 3a: Design Review

Spawn `uni-validator` in Gate 3a mode:

```
Task(subagent_type: "uni-validator",
  prompt: "Your agent ID: {feature-id}-gate-3a

    GATE: 3a (Component Design Review)
    Feature: {feature-id}

    Validate:
    - Does each component align with approved Architecture?
    - Does pseudocode implement what Specification requires?
    - Do test plans address risks from Risk-Based Test Strategy?
    - Are component interfaces consistent with architecture contracts?

    Source documents:
    - product/features/{id}/architecture/ARCHITECTURE.md
    - product/features/{id}/specification/SPECIFICATION.md
    - product/features/{id}/RISK-TEST-STRATEGY.md

    Artifacts to validate:
    - product/features/{id}/pseudocode/ (all files)
    - product/features/{id}/test-plan/ (all files)

    Write report to: product/features/{id}/reports/gate-3a-report.md
    Return: PASS / REWORKABLE FAIL / SCOPE FAIL, report path, issues.")
```

**Gate results:**
- **PASS** → Proceed to Stage 3b automatically
- **REWORKABLE FAIL** → Loop back to Stage 3a agents (max 2 iterations). Include failure details in re-spawn prompt.
- **SCOPE FAIL** → Session stops. Return to human with recommendation.

---

## Stage 3b: Code Implementation

**Agents**: uni-rust-dev (+ domain specialists as needed)

The Delivery Leader routes component context from the IMPLEMENTATION-BRIEF Component Map:

```
Task(subagent_type: "uni-rust-dev",
  prompt: "Your agent ID: {feature-id}-agent-3-rustdev

    Read these files before starting:
    - product/features/{id}/IMPLEMENTATION-BRIEF.md
    - product/features/{id}/architecture/ARCHITECTURE.md
    - product/features/{id}/pseudocode/OVERVIEW.md
    - product/features/{id}/pseudocode/{component}.md
    - product/features/{id}/test-plan/OVERVIEW.md
    - product/features/{id}/test-plan/{component}.md

    YOUR TASK: Implement {component} from validated pseudocode.
    Build test cases per the component test plan.
    Execute component-level tests during development.

    Files to create/modify: {paths from brief}

    RETURN FORMAT:
    1. Files modified: [paths]
    2. Tests: pass/fail count
    3. Issues: [blockers]")
```

For multi-component features, spawn one agent per component (or group small components) in ONE message.

### Gate 3b: Code Review

Spawn `uni-validator` in Gate 3b mode:

```
Task(subagent_type: "uni-validator",
  prompt: "Your agent ID: {feature-id}-gate-3b

    GATE: 3b (Code Review)
    Feature: {feature-id}

    Validate:
    - Does code match validated pseudocode from Stage 3a?
    - Does implementation align with approved Architecture?
    - Are component interfaces implemented as specified?
    - Do test cases match component test plans?
    - Does code compile? Are there stubs or placeholders?

    Source documents:
    - product/features/{id}/architecture/ARCHITECTURE.md
    - product/features/{id}/specification/SPECIFICATION.md
    - product/features/{id}/pseudocode/ (all files)
    - product/features/{id}/test-plan/ (all files)

    Write report to: product/features/{id}/reports/gate-3b-report.md
    Return: PASS / REWORKABLE FAIL / SCOPE FAIL, report path, issues.")
```

**Gate results:** Same as Gate 3a.

---

## Stage 3c: Testing & Risk Validation

**Agents**: uni-tester (test execution)

```
Task(subagent_type: "uni-tester",
  prompt: "Your agent ID: {feature-id}-agent-4-tester

    PHASE: Test Execution (Stage 3c)

    Read these files:
    - product/features/{id}/RISK-TEST-STRATEGY.md
    - product/features/{id}/test-plan/ (all files)
    - product/features/{id}/ACCEPTANCE-MAP.md

    Execute:
    1. All component-level tests
    2. Integration tests across components
    3. Feature-level tests mapped to Risk Strategy
    4. Verify every identified risk has test coverage

    Output:
    - testing/RISK-COVERAGE-REPORT.md (maps test results to identified risks)
    - All tests pass

    Return: test results summary, risk coverage gaps (if any), report path.")
```

### Gate 3c: Final Risk-Based Validation

Spawn `uni-validator` in Gate 3c mode:

```
Task(subagent_type: "uni-validator",
  prompt: "Your agent ID: {feature-id}-gate-3c

    GATE: 3c (Final Risk-Based Validation)
    Feature: {feature-id}

    Validate:
    - Do test results prove identified risks are mitigated?
    - Does test coverage match Risk-Based Test Strategy?
    - Are there risks from Phase 2 lacking test coverage?
    - Does delivered code match approved Specification?
    - Does system architecture match approved Architecture?

    Source documents:
    - product/features/{id}/architecture/ARCHITECTURE.md
    - product/features/{id}/specification/SPECIFICATION.md
    - product/features/{id}/RISK-TEST-STRATEGY.md
    - product/features/{id}/ACCEPTANCE-MAP.md

    Artifacts to validate:
    - product/features/{id}/testing/RISK-COVERAGE-REPORT.md
    - All implemented code

    Write report to: product/features/{id}/reports/gate-3c-report.md
    Return: PASS / REWORKABLE FAIL / SCOPE FAIL, report path, issues.")
```

**Gate results:** Same as Gates 3a/3b.

---

## Phase 4: Delivery

**Prerequisite**: All three gates (3a, 3b, 3c) have passed.

The Delivery Leader:
1. Updates the GH Issue with final results
2. Returns to the human with delivery summary

```bash
gh issue comment <N> --body "## Feature Delivered

All three validation gates passed.

### Gate Results
- Gate 3a (Design Review): PASS
- Gate 3b (Code Review): PASS
- Gate 3c (Risk Validation): PASS

### Deliverables
- Code: [file paths]
- Tests: X passed
- Risk Coverage: product/features/{id}/testing/RISK-COVERAGE-REPORT.md
- Gate Reports: product/features/{id}/reports/gate-3{a,b,c}-report.md"
```

**Return format:**
```
SESSION 2 COMPLETE — Feature delivered.

Gates:
- Gate 3a (Design Review): PASS
- Gate 3b (Code Review): PASS
- Gate 3c (Risk Validation): PASS

Files created/modified: [paths]
Tests: X passed, Y new
Risk coverage: [summary]
GH Issue: {URL} (updated)

Reports:
- product/features/{id}/reports/gate-3a-report.md
- product/features/{id}/reports/gate-3b-report.md
- product/features/{id}/reports/gate-3c-report.md
- product/features/{id}/testing/RISK-COVERAGE-REPORT.md
```

---

## Rework Protocol

At every gate, two failure types:

### Reworkable Failures
Component design doesn't match spec, code doesn't match pseudocode, test gaps exist. Loop back to the previous stage's agents with failure details.

**Max 2 rework iterations per gate.** If still failing after 2 iterations, escalate as SCOPE FAIL.

When re-spawning agents for rework:
1. Include the gate report path in the prompt
2. List specific failures to address
3. Instruct agent to read the gate report first

### Scope/Feasibility Failures
Original scope was wrong, technology doesn't work as assumed, architecture can't support a requirement.

**Session stops immediately.** The Delivery Leader returns to the human with:
- Which gate failed and why
- Recommendation: adjust scope (Phase 1), revise design (Phase 2), or approve modified approach
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
DELIVERY LEADER (uni-scrum-master):
  Init:       Read IMPLEMENTATION-BRIEF.md + ACCEPTANCE-MAP.md
  Stage 3a:   Task(uni-pseudocode) + Task(uni-tester) — parallel, ONE message
              ...wait...
              Task(uni-validator, Gate 3a)
              ...PASS → continue / FAIL → rework or stop...
  Stage 3b:   Task(uni-rust-dev) [+ domain specialists] — parallel, ONE message
              ...wait...
              Task(uni-validator, Gate 3b)
              ...PASS → continue / FAIL → rework or stop...
  Stage 3c:   Task(uni-tester, execution mode)
              ...wait...
              Task(uni-validator, Gate 3c)
              ...PASS → continue / FAIL → rework or stop...
  Phase 4:    gh issue comment + return summary — SESSION 2 ENDS
```
