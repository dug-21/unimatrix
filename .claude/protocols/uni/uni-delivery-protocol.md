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

**You are the Delivery Leader.** Read the SM agent definition (`.claude/agents/uni/uni-scrum-master.md`) for role boundaries. You orchestrate — you NEVER generate content. Spawn specialist agents for all work. Run all stages autonomously. Human re-enters only on scope/feasibility failures or when rework iterations are exhausted.

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
4. **Creates feature branch**: `git checkout -b feature/{phase}-{NNN}` (see `/uni-git`)
5. **Declares feature cycle** — before any agent spawning:
   ```
   context_cycle(
     type: "start",
     topic: "{feature-id}",
     next_phase: "spec",
     agent_id: "{feature-id}-delivery-leader"
   )
   ```
6. Plans Stage 3b waves from the IMPLEMENTATION-BRIEF before spawning any implementation agents

---

## Stage 3a: Component Design & Pseudocode

**Agents**: uni-pseudocode + uni-tester (test plan design)

The Delivery Leader spawns both agents in parallel (ONE message):

```
Task(subagent_type: "uni-pseudocode",
  prompt: "Your agent ID: {feature-id}-agent-1-pseudocode

    Before starting, search Unimatrix for relevant patterns and this feature's ADRs:
    - context_search(query: '{feature area} patterns conventions', category: 'pattern')
    - context_search(query: '{feature-id} architectural decisions', category: 'decision', topic: '{feature-id}')
    Fall back to reading ADR files in product/features/{id}/architecture/ if results are insufficient.

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

    Before starting, search Unimatrix for this feature's ADRs and relevant test patterns:
    - context_search(query: '{feature-id} architectural decisions', category: 'decision', topic: '{feature-id}')
    - context_search(query: '{feature area} testing patterns edge cases')
    Fall back to reading ADR files in product/features/{id}/architecture/ if results are insufficient.

    Read these files before starting:
    - product/features/{id}/IMPLEMENTATION-BRIEF.md
    - product/features/{id}/architecture/ARCHITECTURE.md
    - product/features/{id}/specification/SPECIFICATION.md
    - product/features/{id}/RISK-TEST-STRATEGY.md

    Produce per-component test plans rooted in the Risk Strategy.
    Integration test planning is a required part of test plans —
    your agent definition has the full suite catalog and planning
    guidance. OVERVIEW.md MUST include an integration harness section.

    Output:
    - test-plan/OVERVIEW.md (test strategy, risk mapping, integration harness plan)
    - test-plan/{component}.md (per-component test expectations)

    Return: file paths, risk coverage mapping, integration suite plan, open questions.")
```

Wait for both agents to complete.

```
context_cycle(type: "phase-end", phase: "spec", next_phase: "spec-review", agent_id: "{feature-id}-delivery-leader")
```

### Component Map Update (MANDATORY — between Stage 3a and Gate 3a)

After Stage 3a agents return, the Delivery Leader MUST update the IMPLEMENTATION-BRIEF.md with actual file paths before proceeding to Gate 3a.

1. Collect component lists and file paths from both agents' returns
2. Update the **Component Map** table in `product/features/{id}/IMPLEMENTATION-BRIEF.md`:
   ```
   | Component | Pseudocode | Test Plan |
   |-----------|-----------|-----------|
   | {component-1} | pseudocode/{component-1}.md | test-plan/{component-1}.md |
   | {component-2} | pseudocode/{component-2}.md | test-plan/{component-2}.md |
   ```
3. Update the **Cross-Cutting Artifacts** section with actual paths:
   ```
   | Artifact | Path | Consumed By |
   |----------|------|-------------|
   | Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
   | Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |
   ```
4. The Component Map drives Stage 3b per-component routing. The Cross-Cutting Artifacts drive Stage 3c routing — the integration harness plan in `test-plan/OVERVIEW.md` tells the Stage 3c tester which suites to run and what new integration tests to write.

**Do NOT skip this step.** The IMPLEMENTATION-BRIEF from Session 1 has placeholder components from the architecture. Stage 3a produces the actual pseudocode/test-plan files. Both tables must reflect reality before validation or implementation begins.

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
- **PASS** →
  1. `context_cycle(type: "phase-end", phase: "spec-review", next_phase: "develop", agent_id: "{feature-id}-delivery-leader")`
  2. Commit pseudocode + test plans + updated brief (`pseudocode: component design + test plans (#{issue})`), then proceed to Stage 3b
- **REWORKABLE FAIL** → Loop back to Stage 3a agents (max 2 iterations). Include failure details in re-spawn prompt.
- **SCOPE FAIL** → Session stops. Return to human with recommendation.

---

## Stage 3b: Code Implementation (Wave-Based)

**Agents**: uni-rust-dev (one per component per wave)

**Prerequisite**: Gate 3a PASSED. Component Map in IMPLEMENTATION-BRIEF.md is updated with actual pseudocode/test-plan file paths.

### Wave Planning (MANDATORY — before spawning any agent)

The Delivery Leader reads the IMPLEMENTATION-BRIEF and groups components into **waves** based on dependency order. Components in the same wave are independent of each other and can be implemented in parallel. Components in later waves depend on earlier waves being committed first.

**How to identify waves:**
1. Read the IMPLEMENTATION-BRIEF for any mandatory ordering: numbered steps, "must be first", "blocking prerequisite", or explicit dependency statements.
2. Group components: Wave 1 = components with no dependencies on other components. Wave 2 = components that require Wave 1 to be complete. And so on.
3. If no ordering constraints exist, all components are Wave 1.

**One wave = current parallel behavior.** Multiple waves = execute sequentially, committing between each.

### Wave Execution

For each wave (in order):

1. **Spawn all agents in the wave in ONE message** — one agent per component, no worktree isolation (agents work directly on the feature branch):

```
Task(subagent_type: "uni-rust-dev",
  prompt: "Your agent ID: {feature-id}-agent-3-{component-1}

    Before implementing, search Unimatrix for relevant patterns and this feature's ADRs:
    - context_search(query: '{component area} implementation patterns', category: 'pattern')
    - context_search(query: '{feature-id} architectural decisions', category: 'decision', topic: '{feature-id}')
    Fall back to reading ADR files in product/features/{id}/architecture/ if results are insufficient.

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

2. **Wait for all agents in the wave to complete.**
3. **Commit the wave**: `git add -p && git commit -m "impl: wave {N} — {component list} (#{issue})"`
4. **Spawn the next wave**, which now builds on the committed state.

Each agent receives ONLY its component's pseudocode and test plan — not every file. Stage 3b agents do NOT run or modify integration tests — that is Stage 3c.

### Gate 3b: Code Review

**Pre-gate (Delivery Leader):** Before spawning the validator, run `git status --short` and commit any modified production files that are not yet committed. Gates check committed HEAD — working tree changes are invisible to them.

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
- **PASS** →
  1. `context_cycle(type: "phase-end", phase: "develop", next_phase: "test", agent_id: "{feature-id}-delivery-leader")`
  2. Commit all implementation code (`impl: Stage 3b complete (#{issue})`), then proceed to Stage 3c
- **REWORKABLE FAIL** / **SCOPE FAIL** → Same as Gate 3a

---

## Stage 3c: Testing & Risk Validation

**Agents**: uni-tester (test execution)

```
Task(subagent_type: "uni-tester",
  prompt: "Your agent ID: {feature-id}-agent-4-tester

    PHASE: Test Execution (Stage 3c)

    Read these files:
    - product/features/{id}/IMPLEMENTATION-BRIEF.md (Cross-Cutting Artifacts section for your inputs)
    - product/features/{id}/RISK-TEST-STRATEGY.md
    - product/features/{id}/test-plan/OVERVIEW.md (contains the integration harness plan from Stage 3a)
    - product/features/{id}/test-plan/{component}.md (per-component test plans)
    - product/features/{id}/ACCEPTANCE-MAP.md
    - product/test/infra-001/USAGE-PROTOCOL.md

    Execute unit tests, integration smoke tests (mandatory gate), and
    relevant integration suites per the integration harness plan in
    test-plan/OVERVIEW.md. Write new integration tests identified in that
    plan. Triage any integration failures per USAGE-PROTOCOL.md rules.

    Output:
    - testing/RISK-COVERAGE-REPORT.md (risk mapping, unit + integration counts, xfail references)
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

**Gate results:**
- **PASS** →
  1. `context_cycle(type: "phase-end", phase: "test", next_phase: "pr-review", agent_id: "{feature-id}-delivery-leader")`
  2. Proceed to Phase 4
- **REWORKABLE FAIL** / **SCOPE FAIL** → Same as Gates 3a/3b.

---

## Phase 4: Delivery

**Prerequisite**: All three gates (3a, 3b, 3c) have passed.

The Delivery Leader:
1. Commits final artifacts (`test: risk coverage + gate reports (#{issue})`)
2. Pushes feature branch and opens PR (see `/uni-git` for PR template)
3. Updates GH Issue with PR link
4. Evaluates documentation trigger criteria (see below) — spawns `uni-docs` if mandatory
5. Invokes `/uni-review-pr` for security review and merge readiness
6. Combines impl + deploy results in the return to human

```bash
# Commit final artifacts
git add product/features/{id}/testing/ product/features/{id}/reports/
git commit -m "test: risk coverage + gate reports (#{issue})"
git push -u origin feature/{phase}-{NNN}

# Open PR (see uni-git skill for full template)
gh pr create --title "[{feature-id}] {title}" --body "..."
```

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

**No gate.** This step is advisory — it does not block delivery. If `uni-docs` fails to produce useful output, proceed to `/uni-review-pr` without documentation updates. Documentation changes are part of the reviewed PR.

---

### PR Review (after PR opens)

Invoke `/uni-review-pr` with the PR number, feature ID, and GH Issue number. This spawns a fresh-context security reviewer and assesses merge readiness.

**Error handling:**
- Review fails → return delivery results only, note "PR review failed"
- Review returns BLOCKED → include blocking items in combined return

**Return format:**
```
SESSION 2 COMPLETE — Feature delivered.

Gates: 3a PASS, 3b PASS, 3c PASS
Security Review: {risk level} — {summary}
Merge readiness: {READY | BLOCKED}

Files created/modified: [paths]
Tests: X passed, Y new
Risk coverage: [summary]
PR: {URL}
GH Issue: {URL} (updated)

Human action required: {Approve and merge | Address blocking items}.
```

After returning to the human, close the pr-review phase and stop the cycle:

```
context_cycle(type: "phase-end", phase: "pr-review", agent_id: "{feature-id}-delivery-leader")
context_cycle(type: "stop", topic: "{feature-id}", outcome: "Session 2 complete. All gates passed. PR: {url}", agent_id: "{feature-id}-delivery-leader")
```

### Post-Delivery Review (Optional)

After Phase 4, the Delivery Leader may optionally review for tech debt or cleanup opportunities discovered during implementation. If found, file GH Issues — do not include in this PR.

If a reusable multi-step technique was used or discovered during this session, store it via `/uni-store-procedure`.

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

# Test: summary only (prefer JSON when available)
cargo test --workspace 2>&1 | tail -30
# Or: cargo test --workspace -- --format json 2>&1 | tail -30

# Clippy: first warnings only
cargo clippy --workspace -- -D warnings 2>&1 | head -30

# Dependency audit (run during Gate 3b)
cargo audit 2>&1 | tail -20
```

NEVER pipe full cargo output into context.

---

## Quick Reference: Message Map

```
DELIVERY LEADER (you):
  Init:       Read IMPLEMENTATION-BRIEF.md + ACCEPTANCE-MAP.md
              context_cycle(type: "start", topic: "{feature-id}", next_phase: "spec", agent_id: "{feature-id}-delivery-leader")
  Stage 3a:   Task(uni-pseudocode) + Task(uni-tester) — parallel, ONE message
              ...wait for both to complete...
              context_cycle(type: "phase-end", phase: "spec", next_phase: "spec-review", ...)
              UPDATE Component Map in IMPLEMENTATION-BRIEF.md with actual file paths
              Task(uni-validator, Gate 3a) — MANDATORY BLOCK
              ...PASS → context_cycle(phase-end, spec-review → develop) → commit → Stage 3b
              ...FAIL → rework or stop...
  Stage 3b:   PLAN waves from IMPLEMENTATION-BRIEF (1 wave = all parallel, N waves = sequential)
              FOR EACH WAVE: Task(uni-rust-dev × components-in-wave) — ONE message, no worktree isolation
              Each agent gets ONLY its component's pseudocode + test plan
              ...wait for wave... commit wave... spawn next wave...
              ...wait...
              Task(uni-validator, Gate 3b)
              ...PASS → context_cycle(phase-end, develop → test) → commit → Stage 3c
              ...FAIL → rework or stop...
  Stage 3c:   Task(uni-tester, execution mode)
              ...wait...
              Task(uni-validator, Gate 3c)
              ...PASS → context_cycle(phase-end, test → pr-review) → Phase 4
              ...FAIL → rework or stop...
  Phase 4:    git commit + push + gh pr create
              [CONDITIONAL] uni-docs — documentation update (if trigger criteria met)
              /uni-review-pr — security review + merge readiness
              Combined return — SESSION 2 ENDS
              context_cycle(type: "phase-end", phase: "pr-review", ...)
              context_cycle(type: "stop", topic: "{feature-id}", outcome: "...", agent_id: "{feature-id}-delivery-leader")
```

---

## Integration Test Harness

**Authoritative reference**: `product/test/infra-001/USAGE-PROTOCOL.md` — contains commands, suite descriptions, failure triage decision tree, GH Issue templates, and xfail workflow.

The uni-tester agent has full integration harness knowledge (suite selection, triage rules, commands). The delivery protocol does not duplicate those details. Key rules for the Design Leader:

- **uni-rust-dev** (Stage 3b): Do NOT run or modify integration tests. Stage 3c handles this.
- **uni-tester** (Stage 3a): Include integration harness plan in test-plan/OVERVIEW.md — which suites apply, new tests needed.
- **uni-tester** (Stage 3c): Run smoke (mandatory gate) + relevant suites. Triage failures per USAGE-PROTOCOL.md. Report results in RISK-COVERAGE-REPORT.md.
- **uni-validator** (Gate 3c): Verify smoke passed, xfail markers have GH Issues, no tests deleted, RISK-COVERAGE-REPORT includes integration counts.

---

## Outcome Recording

After Phase 4, close the feature cycle:

```
context_cycle(
  type: "phase-end",
  phase: "pr-review",
  agent_id: "{feature-id}-delivery-leader"
)

context_cycle(
  type: "stop",
  topic: "{feature-id}",
  outcome: "Session 2 complete. All gates passed. PR: {url}",
  agent_id: "{feature-id}-delivery-leader"
)
```
