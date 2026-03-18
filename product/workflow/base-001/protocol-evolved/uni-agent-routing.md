# Agent Routing and Swarm Composition

## Agent Preference

Always use `uni-` agents for Unimatrix product work:

| Instead of | Use | Why |
|------------|-----|-----|
| generic coder | `uni-rust-dev` | Knows Unimatrix Rust patterns, queries `/query-patterns` before implementing |
| generic architect | `uni-architect` | ADR authority, stores decisions in Unimatrix |
| generic tester | `uni-tester` | Risk-based testing, dual-phase role |
| generic planner | Specialized scrum master (see routing table) | Workflow baked in, no protocol file reads |
| generic reviewer | `uni-validator` | Three-gate validation model |
| generic debugger | `uni-bugfix-scrum-master` | Coordinates diagnosis → fix → review |
| generic security auditor | `uni-security-reviewer` | Fresh-context security review of diffs |

---

## Coordinator Routing

Choose the coordinator based on intent:

| User intent | Coordinator | Triggers |
|-------------|-------------|----------|
| Design, scope, spec, architecture | `uni-design-scrum-master` | specification, architecture, design, research, scope, risk strategy |
| Implement, build, code, deliver | `uni-implementation-scrum-master` | implement, build, code, deliver, TDD, refactor, "proceed with implementation" |
| Bug fix | `uni-bugfix-scrum-master` | bug, fix, bugfix, defect, regression, broken, failing, error, crash |
| PR review, merge, release | `uni-deploy-scrum-master` | review PR, merge, release, deploy, ship |
| Retrospective | `uni-retro-scrum-master` | retrospective, retro, extract patterns, knowledge review |

Every swarm also includes `uni-validator` at gates. Non-negotiable.

---

## Complete Agent Roster

### Coordinators (5 agents — exactly one coordinator per session)

| Agent | What It Does |
|-------|-------------|
| `uni-design-scrum-master` | Session 1 design. Spawns design agents in phase order, manages human checkpoints |
| `uni-implementation-scrum-master` | Session 2 delivery. Runs 3 stages with 3 validation gates, manages component routing |
| `uni-bugfix-scrum-master` | Bug fix. Diagnosis → human checkpoint → fix → test → validate → security review |
| `uni-deploy-scrum-master` | PR review/release. Verifies gate results, runs security review, assesses merge readiness |
| `uni-retro-scrum-master` | Retrospective. Extracts patterns, procedures, lessons from shipped features |

### Validation (1 agent — spawned at every gate)

| Agent | What It Does |
|-------|-------------|
| `uni-validator` | Validation gate. Spawned with different check sets per context. Reports PASS / REWORKABLE FAIL / SCOPE FAIL |

### Design Session Specialists (6 agents)

| Agent | Type | Phase | What It Produces |
|-------|------|-------|-----------------|
| `uni-researcher` | specialist | 1 | Problem space exploration, writes SCOPE.md with human |
| `uni-architect` | specialist | 2a | `architecture/ARCHITECTURE.md` + ADRs in Unimatrix. ADR authority |
| `uni-specification` | specialist | 2a | `specification/SPECIFICATION.md` — requirements, ACs, domain models |
| `uni-risk-strategist` | specialist | 1b + 2a+ | `SCOPE-RISK-ASSESSMENT.md` (1b) + `RISK-TEST-STRATEGY.md` (2a+) |
| `uni-vision-guardian` | specialist | 2b | `ALIGNMENT-REPORT.md` — checks source docs against product vision |
| `uni-synthesizer` | synthesizer | 2c | `IMPLEMENTATION-BRIEF.md`, `ACCEPTANCE-MAP.md`, GH Issue (fresh context) |

### Delivery Session Specialists (3 agents)

| Agent | Type | Stage | What It Does |
|-------|------|-------|-------------|
| `uni-pseudocode` | specialist | 3a | Per-component pseudocode. Queries `/query-patterns` before designing |
| `uni-tester` | specialist | 3a + 3c | Test plan design (3a) + test execution with RISK-COVERAGE-REPORT.md (3c) |
| `uni-rust-dev` | developer | 3b | Implements code from validated pseudocode. Queries `/query-patterns` before implementing |

### Bug Fix Specialists (2 agents)

| Agent | Type | Phase | What It Does |
|-------|------|-------|-------------|
| `uni-bug-investigator` | specialist | 1 | Diagnoses root cause, proposes fix approach, identifies missing tests |
| `uni-security-reviewer` | specialist | 4 | Fresh-context security review of PR diff, blast radius, OWASP assessment |

**Total: 17 agents** (5 coordinators + 1 validator + 6 design + 3 delivery + 2 bug fix)

---

## Swarm Composition Templates

### Design Session

```
Coordinator:  uni-design-scrum-master
Phase 1:      uni-researcher (scope exploration with human)
              ★ HUMAN CHECKPOINT — approve SCOPE.md ★
Phase 1b:     uni-risk-strategist (scope-risk mode)
Phase 2a:     uni-architect + uni-specification                    (parallel)
Phase 2a+:    uni-risk-strategist (architecture-risk mode)
Phase 2b:     uni-vision-guardian (alignment check)
Phase 2c:     uni-synthesizer (brief + maps + GH Issue)            (fresh context)
Phase 2d:     Return to human — SESSION 1 ENDS
```

### Delivery Session (includes auto-chain deploy)

```
Coordinator:  uni-implementation-scrum-master
Init:         Read IMPLEMENTATION-BRIEF.md, create feature branch
Stage 3a:     uni-pseudocode + uni-tester (test plans)             (parallel)
              UPDATE Component Map in IMPLEMENTATION-BRIEF.md
Gate 3a:      uni-validator (design review) — MANDATORY BLOCK
Stage 3b:     uni-rust-dev × N (one per component, MANDATORY)      (parallel)
Gate 3b:      uni-validator (code review)
Stage 3c:     uni-tester (test execution)
Gate 3c:      uni-validator (risk validation)
Phase 4:      Commit, push, open PR
              → uni-deploy-scrum-master (auto-chain: security review + merge readiness)
              Combined return (impl + deploy) — SESSION 2 ENDS
```

### Bug Fix Session

```
Coordinator:  uni-bugfix-scrum-master
Phase 1:      uni-bug-investigator (diagnosis)
              ★ HUMAN CHECKPOINT — approve diagnosis ★
Phase 2:      uni-rust-dev (fix + targeted tests)
Phase 3:      uni-tester (full test suite verification)
Gate 3:       uni-validator (bugfix check set)
              git commit + push + PR
Phase 4:      uni-security-reviewer (PR security review)           (fresh context)
Phase 5:      Return PR + security assessment — SESSION ENDS
```

### PR Review / Release

```
Coordinator:  uni-deploy-scrum-master
Step 1:       Verify gate reports (3a, 3b, 3c all PASS)
Step 2:       uni-security-reviewer (fresh-context PR review)
Step 3:       Merge readiness assessment
Step 4:       Return to human — REVIEW ENDS
```

### Retrospective

```
Coordinator:  uni-retro-scrum-master
Phase 1:      Data gathering (context_cycle_review + artifact review)
Phase 2:      uni-architect (pattern/procedure extraction + ADR validation)
Phase 3:      ADR supersession (if flagged, requires human approval)
Phase 4:      Summary + outcome recording — RETRO ENDS
```

---

## Composition Rules

1. **Every session**: exactly one coordinator. No exceptions.
2. **Validation gates**: `uni-validator` spawned at each gate by the coordinator.
3. **Design session**: All six design agents in defined phase order.
4. **Delivery session**: pseudocode + tester + rust-dev + validator at three gates.
5. **Bug fix**: bugfix-scrum-master + bug-investigator + rust-dev + tester + validator + security-reviewer.
6. **PR review**: deploy-scrum-master + security-reviewer.
7. **Retrospective**: retro-scrum-master + architect (+ tester if testing lessons needed).
8. **Skip swarm for**: typos, single-line obvious fixes, config-only changes, docs, exploration.
9. **Max workers per stage**: 5. Split into waves if more needed.

---

## Agent Coordination Model

The coordination model is simple — no registration, no shared memory, no hooks:

1. Coordinator spawns agents via `Agent` tool
2. Each agent receives context in the spawn prompt (feature ID, file paths to read, task description)
3. Agent does work, writes artifacts to disk
4. Agent writes report to `product/features/{feature-id}/agents/{agent-id}-report.md`
5. Agent returns summary to coordinator (file paths, results, issues)
6. Coordinator reads returns and proceeds

---

## Skills Available to Agents

| Skill | When | Who |
|-------|------|-----|
| `/query-patterns` | BEFORE designing or implementing | uni-architect, uni-pseudocode, uni-rust-dev |
| `/store-adr` | AFTER each design decision | uni-architect |
| `/record-outcome` | END of every session | All coordinators |
| `/store-procedure` | After successful sessions (reusable techniques) | All coordinators, uni-bug-investigator |
| `/store-lesson` | After failures | uni-bug-investigator, uni-validator, coordinators |
| `/knowledge-search` | Exploring what's known | Any agent |
| `/knowledge-lookup` | Exact-match retrieval | Any agent |
| `/uni-git` | Git conventions | Coordinators |
