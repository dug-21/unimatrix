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

**Integration test rule**: Stage 3b agents (uni-rust-dev) do NOT run or modify integration tests (`product/test/infra-001/`). Integration testing happens in Stage 3c. If a code change breaks an integration test, the uni-tester in Stage 3c will report it for rework.

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
    - product/test/infra-001/USAGE-PROTOCOL.md

    Execute:
    1. Unit tests: cargo test --workspace 2>&1 | tail -30
    2. Integration smoke tests (MANDATORY GATE):
       cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60
    3. Integration suites relevant to this feature (see suite selection table below)
    4. Feature-level tests mapped to Risk Strategy
    5. Verify every identified risk has test coverage

    INTEGRATION TEST FAILURE TRIAGE (CRITICAL):
    When an integration test fails, determine causation:
    - CAUSED BY THIS FEATURE → Fix the code. Re-run. Document in report.
    - PRE-EXISTING / UNRELATED → Do NOT fix. File a GH Issue and mark the
      test @pytest.mark.xfail(reason='Pre-existing: GH#NNN — description').
      Continue with the feature. See USAGE-PROTOCOL.md for GH Issue template.
    - BAD TEST ASSERTION → Fix the test. Document in report.

    Agents must NEVER fix integration test failures unrelated to the feature
    under development. Unrelated fixes create scope creep, blame diffusion,
    and skip the proper risk assessment lifecycle.

    SUITE SELECTION (run based on what the feature touches):
    | Feature touches...              | Run these suites                        |
    |---------------------------------|-----------------------------------------|
    | Any server tool logic           | tools, protocol                         |
    | Store/retrieval behavior        | tools, lifecycle, edge_cases            |
    | Confidence system               | confidence, lifecycle                   |
    | Contradiction detection         | contradiction                           |
    | Security (scanning, caps)       | security                                |
    | Schema or storage changes       | lifecycle (persistence), volume         |
    | Any change at all               | smoke (minimum gate)                    |

    Output:
    - testing/RISK-COVERAGE-REPORT.md (maps test results to identified risks)
      Must include: unit test counts, integration test counts (per suite),
      any xfail markers added with GH Issue references
    - All unit tests pass
    - Integration smoke tests pass (xfail markers acceptable with GH Issues)

    Return: test results summary, risk coverage gaps (if any), report path,
            any GH Issues filed for pre-existing failures.")

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

    INTEGRATION TEST VALIDATION (MANDATORY):
    - Verify integration smoke tests (pytest -m smoke) passed
    - Verify relevant integration suites were run for this feature
    - Verify any @pytest.mark.xfail markers have corresponding GH Issues
    - Verify no integration tests were deleted or commented out
    - Verify RISK-COVERAGE-REPORT.md includes integration test counts
    - If integration failures were marked xfail, confirm the failures are
      genuinely unrelated to the feature (not masking feature bugs)

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

## Integration Test Harness

The project includes a comprehensive integration test harness at `product/test/infra-001/` that exercises the compiled `unimatrix-server` binary through the MCP JSON-RPC protocol — the exact interface agents use. Full details: `product/test/infra-001/USAGE-PROTOCOL.md`.

**157 tests across 8 suites:** protocol, tools, lifecycle, volume, security, confidence, contradiction, edge_cases.

### Commands

```bash
# From product/test/infra-001/ (binary must be built first)

# Smoke tests — MANDATORY minimum gate for Stage 3c
python -m pytest suites/ -v -m smoke --timeout=60

# Full suite
python -m pytest suites/ -v --timeout=60

# Specific suite
python -m pytest suites/test_security.py -v --timeout=60

# Specific test
python -m pytest suites/test_tools.py::test_store_roundtrip -v
```

### Non-Negotiable Failure Triage Rule

**Agents ONLY fix integration test failures caused by the feature under development.** All other failures follow this protocol:

1. **Caused by this feature** → Fix the code. Re-run. Document fix in gate report.
2. **Pre-existing / unrelated** → Do NOT fix. File a GH Issue:
   ```bash
   gh issue create \
     --title "[infra-001] test_<name>: <brief description>" \
     --label "bug" \
     --body "Discovered by: suites/test_<suite>.py::test_<name>
   Expected: <what test expected>
   Actual: <what happened>
   Not caused by the current feature under development."
   ```
   Then mark the test:
   ```python
   @pytest.mark.xfail(reason="Pre-existing: GH#NNN — description")
   def test_the_failing_test(server):
       ...
   ```
3. **Bad test assertion** → Fix the test. Document in gate report.

**Why**: Fixing unrelated issues in a feature PR creates scope creep, blame diffusion, no audit trail, and skips proper risk assessment. The issue deserves its own lifecycle.

### Agent Rules

| Agent | Integration Test Rule |
|-------|----------------------|
| **uni-rust-dev** (3b) | Do NOT run or modify integration tests. Stage 3c handles this. |
| **uni-tester** (3c) | Run smoke (mandatory) + relevant suites. Triage failures per rule above. Report results + any GH Issues filed in RISK-COVERAGE-REPORT.md. |
| **uni-validator** (Gate 3c) | Verify smoke passed, xfail markers have GH Issues, no tests deleted/commented, RISK-COVERAGE-REPORT includes integration counts. |

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
