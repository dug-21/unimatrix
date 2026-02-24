# Agent Routing and Swarm Composition

## Agent Preference

Always use `uni-` agents for Unimatrix product work:

| Instead of | Use | Why |
|------------|-----|-----|
| generic coder | `uni-rust-dev` | Knows Unimatrix Rust patterns |
| generic architect | `uni-architect` | File-based ADRs, Unimatrix architecture |
| generic tester | `uni-tester` | Risk-based testing, dual-phase role |
| generic planner | `uni-scrum-master` | Reads design/delivery protocols |
| generic reviewer | `uni-validator` | Three-gate validation model |
| generic debugger | `uni-bugfix-manager` | Reads bugfix protocol, coordinates diagnosis → fix → review |
| generic security auditor | `uni-security-reviewer` | Fresh-context security review of diffs |

---

## Every Swarm Has These Two Agents

| Agent | Role | Spawned By |
|-------|------|------------|
| `uni-scrum-master` or `uni-bugfix-manager` | **Coordinator** — reads protocol, spawns workers, manages gates | Primary agent |
| `uni-validator` | **Validation gate** — spawned at each gate with focused checks | Coordinator |

Non-negotiable. No swarm runs without a coordinator and no swarm completes without validation.

---

## Complete Agent Roster

### Coordination (3 agents — at least one coordinator + validator on every swarm)

| Agent | Type | What It Does |
|-------|------|-------------|
| `uni-scrum-master` | coordinator | Design Leader (Session 1) or Delivery Leader (Session 2). Reads protocol, spawns workers, manages gates, updates GH Issues |
| `uni-bugfix-manager` | coordinator | Bug fix coordinator. Reads bugfix protocol, manages diagnosis → fix → review lifecycle |
| `uni-validator` | gate | Validation gate. Spawned with different check sets per context. Reports PASS / REWORKABLE FAIL / SCOPE FAIL |

### Session 1 — Design (6 agents)

| Agent | Type | Phase | What It Produces |
|-------|------|-------|-----------------|
| `uni-researcher` | specialist | 1 | Problem space exploration, writes SCOPE.md collaboratively with human |
| `uni-architect` | specialist | 2a | `architecture/ARCHITECTURE.md` + `ADR-NNN-{name}.md` files. ADR authority |
| `uni-specification` | specialist | 2a | `specification/SPECIFICATION.md` — requirements, ACs, domain models |
| `uni-risk-strategist` | specialist | 2a | `RISK-TEST-STRATEGY.md` — risk identification, scenario mapping, coverage requirements |
| `uni-vision-guardian` | specialist | 2b | `ALIGNMENT-REPORT.md` — checks source docs against product vision |
| `uni-synthesizer` | synthesizer | 2c | `IMPLEMENTATION-BRIEF.md`, `ACCEPTANCE-MAP.md`, GH Issue (fresh context) |

Phase 2a agents run in parallel → Phase 2b (vision) → Phase 2c (synthesis) — sequential.

### Session 2 — Delivery (3 agents)

| Agent | Type | Stage | What It Does |
|-------|------|-------|-------------|
| `uni-pseudocode` | specialist | 3a | Per-component pseudocode from the three source docs |
| `uni-tester` | specialist | 3a + 3c | Dual-phase: test plan design (3a) + test execution with RISK-COVERAGE-REPORT.md (3c) |
| `uni-rust-dev` | developer | 3b | Implements code from validated pseudocode |

### Bug Fix Session (2 agents)

| Agent | Type | Phase | What It Does |
|-------|------|-------|-------------|
| `uni-bug-investigator` | specialist | 1 | Diagnoses root cause, proposes fix approach, identifies missing tests |
| `uni-security-reviewer` | specialist | 4 | Fresh-context security review of PR diff, blast radius, OWASP assessment |

**Total: 14 agents** (3 coordination + 6 design + 3 delivery + 2 bug fix)

---

## Swarm Composition Templates

### Design Session (Session 1)

```
Coordinator:  uni-scrum-master (Design Leader)
Phase 1:      uni-researcher (scope exploration with human)
Phase 2a:     uni-architect, uni-specification, uni-risk-strategist  (parallel)
Phase 2b:     uni-vision-guardian (alignment check)                  (sequential)
Phase 2c:     uni-synthesizer (brief + maps + GH Issue)              (fresh context)
Phase 2d:     Return to human — SESSION 1 ENDS
```

Produces: SCOPE.md, ARCHITECTURE.md + ADRs, SPECIFICATION.md, RISK-TEST-STRATEGY.md, ALIGNMENT-REPORT.md, IMPLEMENTATION-BRIEF.md, ACCEPTANCE-MAP.md, GH Issue.

### Delivery Session (Session 2)

```
Coordinator:  uni-scrum-master (Delivery Leader)
Stage 3a:     uni-pseudocode, uni-tester (test plans)                (parallel)
Gate 3a:      uni-validator (design review)
Stage 3b:     uni-rust-dev [+ domain specialists as needed]          (parallel)
Gate 3b:      uni-validator (code review)
Stage 3c:     uni-tester (test execution)
Gate 3c:      uni-validator (risk validation)
Phase 4:      Delivery — SESSION 2 ENDS
```

Produces: pseudocode/, test-plan/, implemented code + tests, RISK-COVERAGE-REPORT.md, gate reports.

### Bug Fix Session (Single Session)

```
Coordinator:  uni-bugfix-manager
Phase 1:      uni-bug-investigator (diagnosis)
              ★ HUMAN CHECKPOINT — approve diagnosis ★
Phase 2:      uni-rust-dev (fix implementation + tests)              (sequential)
Phase 3:      uni-tester (full test suite verification)              (sequential)
Gate 3:       uni-validator (bugfix check set)
              git commit + push + PR
Phase 4:      uni-security-reviewer (PR security review)             (fresh context)
Phase 5:      Return PR + security assessment to human — SESSION ENDS
```

Produces: bug fix code, targeted tests, gate report, security assessment, PR.

---

## Session Mapping

| Session | Protocol | Leader Role | Agents |
|---------|----------|-------------|--------|
| Session 1 (Design) | `.claude/protocols/uni/uni-design-protocol.md` | Design Leader | researcher, architect, specification, risk-strategist, vision-guardian, synthesizer |
| Session 2 (Delivery) | `.claude/protocols/uni/uni-delivery-protocol.md` | Delivery Leader | pseudocode, tester, rust-dev, validator (×3) |
| Bug Fix | `.claude/protocols/uni/uni-bugfix-protocol.md` | Bugfix Manager | bug-investigator, rust-dev, tester, validator, security-reviewer |

---

## Composition Rules

1. **Every swarm**: coordinator (uni-scrum-master or uni-bugfix-manager) + uni-validator (gate). No exceptions.
2. **Session 1**: Always includes all six design agents in the defined phase order.
3. **Session 2**: Always includes pseudocode + tester + rust-dev + validator at three gates.
4. **Bug fix**: uni-bugfix-manager + bug-investigator + rust-dev + tester + validator + security-reviewer.
5. **Domain specialists**: As Unimatrix domain agents are created, they can be added to Stage 3b alongside uni-rust-dev.
6. **Skip swarm for**: typos, single-line obvious fixes, config-only changes, docs, exploration.
7. **Max wave size**: 5 workers. Split into waves if more agents needed.
8. **Bug fix triggers**: bug, fix, bugfix, defect, regression, broken, failing, error, crash.

---

## Agent Coordination Model

The coordination model is simple — no registration, no shared memory, no hooks:

1. Coordinator (uni-scrum-master or uni-bugfix-manager) spawns agents via `Task` tool
2. Each agent receives its context in the spawn prompt (feature ID, file paths to read, task description)
3. Agent does work, writes artifacts to disk
4. Agent writes report to `product/features/{feature-id}/agents/{agent-id}-report.md`
5. Agent returns summary to coordinator (file paths, results, issues)
6. Coordinator reads returns and proceeds
