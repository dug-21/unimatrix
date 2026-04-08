# Agent Routing and Swarm Composition

## Agent Preference

Always use `uni-` agents for Unimatrix product work:

| Instead of | Use | Why |
|------------|-----|-----|
| generic coder | `uni-rust-dev` | Knows Unimatrix Rust patterns, queries `/uni-query-patterns` before implementing |
| generic architect | `uni-architect` | ADR authority, stores decisions in Unimatrix |
| generic tester | `uni-tester` | Risk-based testing, dual-phase role |
| generic planner | Design Leader (you) | Protocol-driven, reads the right protocol for the session |
| generic reviewer | `uni-validator` | Three-gate validation model |
| generic debugger | Bugfix Leader (you) | Reads bugfix protocol, coordinates diagnosis → fix → review |
| generic security auditor | `uni-security-reviewer` | Fresh-context security review of diffs |

---

## Coordinator Routing

One coordinator reads the protocol for the session type:

| User intent | Session type | Protocol |
|-------------|-------------|----------|
| Design, scope, spec, architecture | `design` | `.claude/protocols/uni/uni-design-protocol.md` |
| Implement, build, code, deliver | `delivery` | `.claude/protocols/uni/uni-delivery-protocol.md` |
| Bug fix | `bugfix` | `.claude/protocols/uni/uni-bugfix-protocol.md` |

For PR review and retrospective, use skills directly (no coordinator needed):

| User intent | Skill |
|-------------|-------|
| PR review, merge readiness | `/uni-review-pr` |
| Retrospective, knowledge extraction | `/uni-retro` |

Every swarm also includes `uni-validator` at gates. Non-negotiable.

---

## Complete Agent Roster

### Coordinator (you — the primary agent)

You are the coordinator. Read the protocol for the session type, spawn specialist agents, manage gates, update GH Issues. Read `.claude/agents/uni/coordinator (you).md` for role boundaries and behavioral rules.

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
| `uni-pseudocode` | specialist | 3a | Per-component pseudocode. Queries `/uni-query-patterns` before designing |
| `uni-tester` | specialist | 3a + 3c | Test plan design (3a) + test execution with RISK-COVERAGE-REPORT.md (3c) |
| `uni-rust-dev` | developer | 3b | Implements code from validated pseudocode. Queries `/uni-query-patterns` before implementing |

### Bug Fix Specialists (1 agent)

| Agent | Type | Phase | What It Does |
|-------|------|-------|-------------|
| `uni-bug-investigator` | specialist | 1 | Diagnoses root cause, proposes fix approach, identifies missing tests |

### Shared Specialist (1 agent — used by `/uni-review-pr` skill)

| Agent | Type | Phase | What It Does |
|-------|------|-------|-------------|
| `uni-security-reviewer` | specialist | review | Fresh-context security review of PR diff, blast radius, OWASP assessment |

**Total: 13 specialist agents** (1 validator + 6 design + 3 delivery + 1 bug fix + 1 security + 1 retro-mode architect). You coordinate.

---

## Swarm Composition Templates

### Design Session

```
Coordinator:  you (read uni-design-protocol.md + coordinator (you).md)
Phase 1:      uni-researcher (scope exploration with human)
              ★ HUMAN CHECKPOINT — approve SCOPE.md ★
Phase 1b:     uni-risk-strategist (scope-risk mode)
Phase 2a:     uni-architect + uni-specification                    (parallel)
Phase 2a+:    uni-risk-strategist (architecture-risk mode)
Phase 2b:     uni-vision-guardian (alignment check)
Phase 2c:     uni-synthesizer (brief + maps + GH Issue)            (fresh context)
Phase 2d:     git commit + push + gh pr create --draft
              Return to human — SESSION 1 ENDS
```

### Delivery Session

```
Coordinator:  you (read uni-delivery-protocol.md + coordinator (you).md)
Init:         Read IMPLEMENTATION-BRIEF.md, create feature branch
Stage 3a:     uni-pseudocode + uni-tester (test plans)             (parallel)
              UPDATE Component Map in IMPLEMENTATION-BRIEF.md
Gate 3a:      uni-validator (design review) — MANDATORY BLOCK
Stage 3b:     uni-rust-dev × N (one per component, MANDATORY)      (parallel)
Gate 3b:      uni-validator (code review)
Stage 3c:     uni-tester (test execution)
Gate 3c:      uni-validator (risk validation)
Phase 4:      Commit, push, open PR
              /uni-review-pr — security review + merge readiness
              Return to human — SESSION 2 ENDS
```

### Bug Fix Session

```
Coordinator:  you (read uni-bugfix-protocol.md + coordinator (you).md)
Init:         /uni-query-patterns + /uni-knowledge-search — prior knowledge
Phase 1:      uni-bug-investigator (diagnose root cause)
              ★ HUMAN CHECKPOINT — approve diagnosis ★
Phase 2:      git checkout -b bugfix/{issue}-{desc}
              uni-rust-dev (implement fix + tests)
Phase 3:      uni-tester (full test suite verification)
Gate 3:       uni-validator (bugfix check set)
              git commit + push + gh pr create
Phase 4:      /uni-review-pr — security review + merge readiness
Phase 5:      Return PR + review assessment — SESSION ENDS
```

### PR Review (standalone)

```
Human invokes: /uni-review-pr {pr-number}
Step 1:       Verify gate reports
Step 2:       uni-security-reviewer (fresh-context PR review)
Step 3:       Merge readiness assessment
Step 4:       Return to human — REVIEW ENDS
```

### Retrospective (standalone)

```
Human invokes: /uni-retro {feature-id} {pr-number}
Phase 1:      Data gathering (context_cycle_review + artifact review)
Phase 2:      uni-architect (pattern/procedure extraction + ADR validation)
Phase 3:      ADR supersession (if flagged, requires human approval)
Phase 4:      Worktree cleanup
Phase 5:      Summary + outcome recording — RETRO ENDS
```

---

## Composition Rules

1. **Every swarm session**: you are the coordinator. Read the protocol and SM definition. No exceptions.
2. **Validation gates**: `uni-validator` spawned at each gate by you.
3. **Design session**: All six design agents in defined phase order per protocol.
4. **Delivery session**: pseudocode + tester + rust-dev + validator at three gates per protocol.
5. **Bug fix**: bug-investigator + rust-dev + tester + validator per protocol.
6. **PR review**: `/uni-review-pr` skill + security-reviewer.
7. **Retrospective**: `/uni-retro` skill + architect (+ tester if testing lessons needed).
8. **Skip swarm for**: typos, single-line obvious fixes, config-only changes, docs, exploration.
9. **Max workers per stage**: 5. Split into waves if more needed.

---

## Skills Available to Agents

| Skill | When | Who |
|-------|------|-----|
| `/uni-query-patterns` | BEFORE designing or implementing | uni-architect, uni-pseudocode, uni-rust-dev |
| `/uni-store-adr` | AFTER each design decision | uni-architect |
| `/uni-record-outcome` | END of every session | coordinator (you), `/uni-review-pr`, `/uni-retro` |
| `/uni-store-procedure` | After successful sessions (reusable techniques) | coordinator (you), uni-bug-investigator |
| `/uni-store-lesson` | After failures | uni-bug-investigator, uni-validator, coordinator (you) |
| `/uni-knowledge-search` | Exploring what's known | Any agent |
| `/uni-knowledge-lookup` | Exact-match retrieval | Any agent |
| `/uni-git` | Git conventions | coordinator (you) |
| `/uni-review-pr` | After PR creation or standalone | coordinator (you), human |
| `/uni-retro` | After merge | human |
