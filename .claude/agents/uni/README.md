# Unimatrix Agent Team (uni-)

Agents for Unimatrix product development. These agents implement the Spec-Driven Development with Risk-Based Testing workflow defined in `product/workflow/base-001/001-proposal.md`.

**Creating a new agent?** See [AGENT-CREATION-GUIDE.md](./AGENT-CREATION-GUIDE.md).

## When to Use

**Use `uni-` agents for all Unimatrix product work.** NDP agents in `.claude/agents/ndp/` are retained as reference only.

- For feature work (design, delivery, or bugfix): read the protocol and `uni-scrum-master.md`, then act as coordinator — spawn specialist agents, never generate content.
- Use `/review-pr` for PR security review and merge readiness.
- Use `/retro` for post-merge knowledge extraction.

## Three Session Types

The workflow executes across distinct session types. One coordinator reads the protocol for the session:

| Session | Protocol | What Happens |
|---------|----------|-------------|
| **Design** | `.claude/protocols/uni/uni-design-protocol.md` | Research → Scope → 3 source docs → Vision check → Brief → Return to human |
| **Delivery** | `.claude/protocols/uni/uni-delivery-protocol.md` | Pseudocode → Gate 3a → Code → Gate 3b → Test → Gate 3c → PR → Review |
| **Bug Fix** | `.claude/protocols/uni/uni-bugfix-protocol.md` | Diagnose → Human approve → Fix → Test → Validate → PR → Review |

## Agent Roster

### Coordination (1 coordinator + 1 validator on every swarm)

| Agent | Role |
|-------|------|
| `uni-scrum-master` | Protocol-driven coordinator — reads the right protocol for the session type |
| `uni-validator` | Validation gate — spawned with different check sets per context |

### Design Session Specialists

| Agent | Phase | What It Produces |
|-------|-------|-----------------|
| `uni-researcher` | 1 | Problem space exploration, writes SCOPE.md with human |
| `uni-architect` | 2a | `architecture/ARCHITECTURE.md` + ADRs in Unimatrix |
| `uni-specification` | 2a | `specification/SPECIFICATION.md` |
| `uni-risk-strategist` | 1b + 2a+ | `SCOPE-RISK-ASSESSMENT.md` (1b) + `RISK-TEST-STRATEGY.md` (2a+) |
| `uni-vision-guardian` | 2b | `ALIGNMENT-REPORT.md` |
| `uni-synthesizer` | 2c | `IMPLEMENTATION-BRIEF.md`, `ACCEPTANCE-MAP.md`, GH Issue |

### Delivery Session Specialists

| Agent | Stage | What It Does |
|-------|-------|-------------|
| `uni-pseudocode` | 3a | Per-component pseudocode from source docs |
| `uni-tester` | 3a + 3c | Test plan design (3a) + test execution with RISK-COVERAGE-REPORT.md (3c) |
| `uni-rust-dev` | 3b | Code implementation from validated pseudocode |

### Shared Specialists

| Agent | Used By | What It Does |
|-------|---------|-------------|
| `uni-bug-investigator` | Bugfix Phase 1 | Diagnoses root cause, proposes fix, identifies missing tests |
| `uni-security-reviewer` | `/review-pr` skill | Fresh-context security review of PR diff, OWASP assessment |

**Total: 14 agents** (1 coordinator + 1 validator + 6 design + 3 delivery + 2 shared specialists) + 1 architect in retro mode

## Swarm Composition Templates

### Design Session

```
Coordinator:  uni-scrum-master (reads uni-design-protocol.md)
Phase 1:      uni-researcher
              ★ HUMAN CHECKPOINT — approve SCOPE.md ★
Phase 1b:     uni-risk-strategist (scope-risk mode)
Phase 2a:     uni-architect + uni-specification                    (parallel)
Phase 2a+:    uni-risk-strategist (architecture-risk mode)
Phase 2b:     uni-vision-guardian                                  (sequential)
Phase 2c:     uni-synthesizer                                      (fresh context)
Phase 2d:     git commit + push + draft PR — SESSION 1 ENDS
```

### Delivery Session

```
Coordinator:  uni-scrum-master (reads uni-delivery-protocol.md)
Stage 3a:     uni-pseudocode + uni-tester                          (parallel)
              UPDATE Component Map
Gate 3a:      uni-validator                                        (MANDATORY BLOCK)
Stage 3b:     uni-rust-dev × N (one per component)                 (parallel)
Gate 3b:      uni-validator
Stage 3c:     uni-tester                                           (execution)
Gate 3c:      uni-validator
Phase 4:      git commit + push + PR + /review-pr — SESSION 2 ENDS
```

### Bug Fix Session

```
Coordinator:  uni-scrum-master (reads uni-bugfix-protocol.md)
Phase 1:      uni-bug-investigator                                 (diagnosis)
              ★ HUMAN CHECKPOINT ★
Phase 2:      uni-rust-dev                                         (fix + tests)
Phase 3:      uni-tester                                           (verification)
Gate 3:       uni-validator                                        (bugfix check set)
Phase 4:      git commit + push + PR + /review-pr
Phase 5:      Return to human — SESSION ENDS
```

## Agent Coordination Model

No registration, no shared memory, no hooks. Simple spawn → work → return:

1. Coordinator spawns agents via `Agent` tool
2. Each agent receives context in the spawn prompt (feature ID, file paths, task)
3. Agent does work, writes artifacts to disk
4. Agent writes report to `product/features/{id}/agents/{agent-id}-report.md`
5. Agent returns summary to coordinator
6. Coordinator reads returns and proceeds

## Three Sacred Source Documents

These are produced in Session 1 and validated against throughout Session 2:

1. **Architecture** — `architecture/ARCHITECTURE.md` + ADRs in Unimatrix
2. **Specification** — `specification/SPECIFICATION.md`
3. **Risk-Based Test Strategy** — `RISK-TEST-STRATEGY.md`

## Directory

```
.claude/agents/uni/
├── README.md                  # This file
├── AGENT-CREATION-GUIDE.md    # How to create uni- agents
├── uni-scrum-master.md        # Coordinator (reads protocol per session)
├── uni-validator.md           # Validation gate
├── uni-researcher.md          # Problem space explorer (Phase 1)
├── uni-architect.md           # Architecture + ADRs (Phase 2a, retro mode)
├── uni-specification.md       # Specification writer (Phase 2a)
├── uni-risk-strategist.md     # Risk strategy (Phase 1b + 2a+)
├── uni-vision-guardian.md     # Vision alignment (Phase 2b)
├── uni-synthesizer.md         # Brief + maps + GH Issue (Phase 2c)
├── uni-pseudocode.md          # Per-component pseudocode (Stage 3a)
├── uni-tester.md              # Test plans (3a) + execution (3c)
├── uni-rust-dev.md            # Code implementation (Stage 3b)
├── uni-bug-investigator.md    # Bug root cause diagnosis
└── uni-security-reviewer.md   # Security review of PRs

.claude/protocols/uni/
├── uni-design-protocol.md     # Session 1 flow
├── uni-delivery-protocol.md   # Session 2 flow
├── uni-bugfix-protocol.md     # Bug fix flow
└── uni-agent-routing.md       # Agent roster, composition templates, skills

.claude/skills/
├── review-pr/SKILL.md         # PR security review + merge readiness
├── retro/SKILL.md             # Post-merge knowledge extraction
├── uni-git/SKILL.md           # Git conventions
├── query-patterns/SKILL.md    # Check existing patterns before work
├── store-adr/SKILL.md         # Store architectural decisions
├── record-outcome/SKILL.md    # Record session outcomes
├── store-procedure/SKILL.md   # Store reusable techniques
├── store-lesson/SKILL.md      # Store failure lessons
├── knowledge-search/SKILL.md  # Semantic search
└── knowledge-lookup/SKILL.md  # Exact-match retrieval
```
