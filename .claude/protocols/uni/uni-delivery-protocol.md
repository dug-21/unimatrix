# Delivery Session Protocol (Session 2)

Triggers on: implement, build, code, deliver, TDD, refactor, "proceed with implementation".

---

## Execution Model

Session 2 reads the IMPLEMENTATION-BRIEF.md produced in Session 1 and runs three stages autonomously, each with a validation gate. If all gates pass, the feature is delivered. If any gate fails beyond rework, the session stops and returns to the human.

```
SESSION 2 — DELIVERY
════════════════════

Stage 3a: Component Design (pseudocode + test plans)
  ↓ Update Component Map in IMPLEMENTATION-BRIEF.md
  ★ Gate 3a: Design Review (MANDATORY BLOCK) ★
         ↓
Stage 3b: Code Implementation (parallelized by component)
  ★ Gate 3b: Code Review ★
         ↓
Stage 3c: Testing & Risk Validation
  ★ Gate 3c: Risk Validation ★
         ↓
Phase 4: Delivery
  ★ RETURN TO HUMAN ★
```

**Critical sequence**: Stage 3a produces pseudocode + test plans → Delivery Leader updates the Component Map → Gate 3a validates designs → ONLY THEN does Stage 3b begin. Stage 3b agents each receive their specific component's validated pseudocode and test plan.

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

The Delivery Leader:
1. Reads `product/features/{feature-id}/IMPLEMENTATION-BRIEF.md` — Component Map, ADR references, constraints
2. Reads `product/features/{feature-id}/ACCEPTANCE-MAP.md` — AC verification methods
3. Reads paths to the three source documents (listed in the brief)
4. **Creates feature branch**: `git checkout -b feature/{phase}-{NNN}` (see `.claude/skills/uni-git/SKILL.md`)

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

### Component Map Update (MANDATORY — between Stage 3a and Gate 3a)

After Stage 3a agents return, the Delivery Leader MUST update the IMPLEMENTATION-BRIEF.md Component Map with actual file paths before proceeding to Gate 3a.

1. Collect component lists and file paths from both agents' returns
2. Update the Component Map table in `product/features/{id}/IMPLEMENTATION-BRIEF.md`:
   ```
   | Component | Pseudocode | Test Plan |
   |-----------|-----------|-----------|
   | {component-1} | pseudocode/{component-1}.md | test-plan/{component-1}.md |
   | {component-2} | pseudocode/{component-2}.md | test-plan/{component-2}.md |
   ```
3. This updated Component Map is what Gate 3a validates and what Stage 3b uses for per-component routing

**Do NOT skip this step.** The IMPLEMENTATION-BRIEF from Session 1 has placeholder components from the architecture. Stage 3a produces the actual pseudocode/test-plan files. The Component Map must reflect reality before validation or implementation begins.

### Gate 3a: Design Review (MANDATORY BLOCK — do NOT proceed to Stage 3b without PASS)

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
- **PASS** → Commit pseudocode + test plans + updated brief (`pseudocode: component design + test plans (#{issue})`), then proceed to Stage 3b
- **REWORKABLE FAIL** → Loop back to Stage 3a agents (max 2 iterations). Include failure details in re-spawn prompt.
- **SCOPE FAIL** → Session stops. Return to human with recommendation.

---

## Stage 3b: Code Implementation (Parallelized by Component)

**Agents**: uni-rust-dev (one per component, + domain specialists as needed)

**Prerequisite**: Gate 3a PASSED. Component Map in IMPLEMENTATION-BRIEF.md is updated with actual pseudocode/test-plan file paths.

The Delivery Leader reads the updated Component Map and spawns **one implementation agent per component** (or groups small components). Each agent receives ONLY its component's pseudocode and test plan — not every file.

```
# For each component in the Component Map, spawn in ONE message:

Task(subagent_type: "uni-rust-dev",
  prompt: "Your agent ID: {feature-id}-agent-3-{component-1}

    Read these files before starting:
    - product/features/{id}/IMPLEMENTATION-BRIEF.md
    - product/features/{id}/architecture/ARCHITECTURE.md
    - product/features/{id}/pseudocode/OVERVIEW.md
    - product/features/{id}/pseudocode/{component-1}.md    ← YOUR component
    - product/features/{id}/test-plan/{component-1}.md     ← YOUR component's test plan

    YOUR TASK: Implement {component-1} from validated pseudocode.
    Build test cases per the component test plan.
    Execute component-level tests during development.
    Keep files modular — no file should exceed 500 lines.

    Files to create/modify: {paths from brief for this component}

    RETURN FORMAT:
    1. Files modified: [paths]
    2. Tests: pass/fail count
    3. Issues: [blockers]")

Task(subagent_type: "uni-rust-dev",
  prompt: "Your agent ID: {feature-id}-agent-4-{component-2}
    ...same structure, with {component-2}'s pseudocode and test plan...")
```

**Key**: Each agent gets its OWN component's `pseudocode/{component}.md` and `test-plan/{component}.md`. Do NOT dump all pseudocode files into every agent.

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

**Gate results:**
- **PASS** → Commit all implementation code (`impl: Stage 3b complete (#{issue})`), then proceed to Stage 3c
- **REWORKABLE FAIL** / **SCOPE FAIL** → Same as Gate 3a

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
1. Commits final artifacts (`test: risk coverage + gate reports (#{issue})`)
2. Pushes feature branch and opens PR (see `.claude/skills/uni-git/SKILL.md` for PR template)
3. Updates GH Issue with PR link
4. Returns to the human — **human reviews PR and merges**

```bash
# Commit final artifacts
git add product/features/{id}/testing/ product/features/{id}/reports/
git commit -m "test: risk coverage + gate reports (#{issue})"
git push -u origin feature/{phase}-{NNN}

# Open PR (see uni-git skill for full template)
gh pr create --title "[{feature-id}] {title}" --body "..."
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

# Dependency audit (run during Gate 3b)
cargo audit 2>&1 | tail -20
```

NEVER pipe full cargo output into context.

---

## Quick Reference: Message Map

```
DELIVERY LEADER (uni-scrum-master):
  Init:       Read IMPLEMENTATION-BRIEF.md + ACCEPTANCE-MAP.md
  Stage 3a:   Task(uni-pseudocode) + Task(uni-tester) — parallel, ONE message
              ...wait for both to complete...
              UPDATE Component Map in IMPLEMENTATION-BRIEF.md with actual file paths
              Task(uni-validator, Gate 3a) — MANDATORY BLOCK
              ...PASS → continue / FAIL → rework or stop...
  Stage 3b:   Task(uni-rust-dev per component) — parallel by component, ONE message
              Each agent gets ONLY its component's pseudocode + test plan
              ...wait...
              Task(uni-validator, Gate 3b)
              ...PASS → continue / FAIL → rework or stop...
  Stage 3c:   Task(uni-tester, execution mode)
              ...wait...
              Task(uni-validator, Gate 3c)
              ...PASS → continue / FAIL → rework or stop...
  Phase 4:    gh issue comment + return summary — SESSION 2 ENDS
```

---

## Outcome Recording

After Phase 4, record the session outcome in Unimatrix:

```
context_store(
  category: "outcome",
  feature_cycle: "{feature-id}",
  tags: ["type:feature", "phase:delivery", "result:pass", "gate:3c"],
  content: "Session 2 complete. All gates passed. PR: {url}"
)
```
