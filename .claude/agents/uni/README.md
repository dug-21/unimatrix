# Unimatrix Agent Team (uni-)

Agents for Unimatrix product development. These agents implement the Spec-Driven Development with Risk-Based Testing workflow defined in `product/workflow/base-001/001-proposal.md`.

**Creating a new agent?** See [AGENT-CREATION-GUIDE.md](./AGENT-CREATION-GUIDE.md).

## When to Use

**Use `uni-` agents for all Unimatrix product work.** NDP agents in `.claude/agents/ndp/` are retained as reference only.

- Spawn `uni-scrum-master` for feature work. It reads the appropriate protocol and orchestrates the swarm.
- Spawn `uni-bugfix-scrum-master` for bug fixes. It coordinates diagnosis through merge with mandatory human checkpoint.

## Three Session Types

The workflow executes across distinct session types:

| Session | Leader Role | Protocol | What Happens |
|---------|------------|----------|-------------|
| **Session 1 (Design)** | Design Leader | `.claude/protocols/uni/uni-design-protocol.md` | Research → Scope → 3 source docs → Vision check → Brief → Return to human |
| **Session 2 (Delivery)** | Delivery Leader | `.claude/protocols/uni/uni-delivery-protocol.md` | Pseudocode → Gate 3a → Code → Gate 3b → Test → Gate 3c → Deliver |
| **Bug Fix** | Bugfix Scrum Master | `.claude/protocols/uni/uni-bugfix-protocol.md` | Diagnose → Human approve → Fix → Test → Validate → Security review → PR |

Sessions 1 and 2 use `uni-scrum-master` reading different protocols. Bug fixes use `uni-bugfix-scrum-master`.

## Agent Roster

### Coordination (at least one coordinator + validator on every swarm)

| Agent | Role |
|-------|------|
| `uni-scrum-master` | Dual-role coordinator — Design Leader or Delivery Leader |
| `uni-bugfix-scrum-master` | Bug fix coordinator — diagnosis through merge lifecycle |
| `uni-validator` | Validation gate — spawned with different check sets per context |

### Session 1 — Design

| Agent | Phase | What It Produces |
|-------|-------|-----------------|
| `uni-researcher` | 1 | Problem space exploration, writes SCOPE.md with human |
| `uni-architect` | 2a | `architecture/ARCHITECTURE.md` + `ADR-NNN-{name}.md` files |
| `uni-specification` | 2a | `specification/SPECIFICATION.md` |
| `uni-risk-strategist` | 2a | `RISK-TEST-STRATEGY.md` (sacred source doc) |
| `uni-vision-guardian` | 2b | `ALIGNMENT-REPORT.md` |
| `uni-synthesizer` | 2c | `IMPLEMENTATION-BRIEF.md`, `ACCEPTANCE-MAP.md`, GH Issue |

### Session 2 — Delivery

| Agent | Stage | What It Does |
|-------|-------|-------------|
| `uni-pseudocode` | 3a | Per-component pseudocode from source docs |
| `uni-tester` | 3a + 3c | Test plan design (3a) + test execution with RISK-COVERAGE-REPORT.md (3c) |
| `uni-rust-dev` | 3b | Code implementation from validated pseudocode |

### Bug Fix Session

| Agent | Phase | What It Does |
|-------|-------|-------------|
| `uni-bug-investigator` | 1 | Diagnoses root cause, proposes fix, identifies missing tests |
| `uni-security-reviewer` | 4 | Fresh-context security review of PR diff, OWASP assessment |

**Total: 14 agents** (3 coordination + 6 design + 3 delivery + 2 bug fix)

## Swarm Composition Templates

### Design Session

```
Coordinator:  uni-scrum-master (Design Leader)
Phase 1:      uni-researcher
Phase 2a:     uni-architect, uni-specification, uni-risk-strategist  (parallel)
Phase 2b:     uni-vision-guardian                                    (sequential)
Phase 2c:     uni-synthesizer                                        (fresh context)
```

### Delivery Session

```
Coordinator:  uni-scrum-master (Delivery Leader)
Stage 3a:     uni-pseudocode, uni-tester                             (parallel)
Gate 3a:      uni-validator
Stage 3b:     uni-rust-dev                                           (parallel)
Gate 3b:      uni-validator
Stage 3c:     uni-tester                                             (execution)
Gate 3c:      uni-validator
```

### Bug Fix Session

```
Coordinator:  uni-bugfix-scrum-master
Phase 1:      uni-bug-investigator                                   (diagnosis)
              ★ HUMAN CHECKPOINT ★
Phase 2:      uni-rust-dev                                           (fix + tests)
Phase 3:      uni-tester                                             (verification)
Gate 3:       uni-validator                                          (bugfix check set)
Phase 4:      uni-security-reviewer                                  (fresh context)
Phase 5:      Return to human — SESSION ENDS
```

## Agent Coordination Model

No registration, no shared memory, no hooks. Simple spawn → work → return:

1. Coordinator spawns agents via `Task` tool
2. Each agent receives context in the spawn prompt (feature ID, file paths, task)
3. Agent does work, writes artifacts to disk
4. Agent writes report to `product/features/{id}/agents/{agent-id}-report.md`
5. Agent returns summary to coordinator
6. Coordinator reads returns and proceeds

## Three Sacred Source Documents

These are produced in Session 1 and validated against throughout Session 2:

1. **Architecture** — `architecture/ARCHITECTURE.md` + ADR files
2. **Specification** — `specification/SPECIFICATION.md`
3. **Risk-Based Test Strategy** — `RISK-TEST-STRATEGY.md`

## Directory

```
.claude/agents/uni/
├── README.md                  # This file
├── AGENT-CREATION-GUIDE.md    # How to create uni- agents
├── uni-scrum-master.md        # Coordinator (Design + Delivery Leader)
├── uni-bugfix-scrum-master.md  # Coordinator (Bug Fix Leader)
├── uni-validator.md           # Validation gate
├── uni-researcher.md          # Problem space explorer (Phase 1)
├── uni-architect.md           # Architecture + ADRs (Phase 2a)
├── uni-specification.md       # Specification writer (Phase 2a)
├── uni-risk-strategist.md     # Risk strategy (Phase 2a)
├── uni-vision-guardian.md     # Vision alignment (Phase 2b)
├── uni-synthesizer.md         # Brief + maps + GH Issue (Phase 2c)
├── uni-pseudocode.md          # Per-component pseudocode (Stage 3a)
├── uni-tester.md              # Test plans (3a) + execution (3c)
├── uni-rust-dev.md            # Code implementation (Stage 3b)
├── uni-bug-investigator.md    # Bug root cause diagnosis (Bug Fix Phase 1)
└── uni-security-reviewer.md   # Security review of PRs (Bug Fix Phase 4)

.claude/protocols/uni/
├── uni-design-protocol.md     # Session 1 flow
├── uni-delivery-protocol.md   # Session 2 flow
├── uni-bugfix-protocol.md     # Bug fix flow
└── uni-agent-routing.md       # Agent roster, composition templates
```
