# Agent Routing and Swarm Composition

## Agent Preference

Always use `uni-` agents for Unimatrix product work:

| Instead of | Use | Why |
|------------|-----|-----|
| generic coder (Rust) | `uni-rust-dev` | Knows Unimatrix Rust patterns, queries `/uni-query-patterns` before implementing |
| generic coder (JS/TS) | `uni-js-dev` | Knows the edge-client fail-open + zero-dep + size-budget + parity contracts |
| generic architect | `uni-architect` | ADR authority, stores decisions in Unimatrix |
| generic tester | `uni-tester` | Risk-based testing, dual-phase role |
| generic planner | `uni-scrum-master` (Design Leader) | Protocol-driven, reads the right protocol for the session |
| generic reviewer | `uni-validator` | Three-gate validation model |
| generic debugger | `uni-scrum-master` (Bugfix Leader) | Reads bugfix protocol, coordinates diagnosis → fix → review |
| generic security auditor | `uni-security-reviewer` | Fresh-context security review of diffs |

---

## Coordinator Routing

The primary agent spawns one `uni-scrum-master` coordinator, which reads the protocol for the session type and runs it end to end:

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

### Coordinator (`uni-scrum-master`, spawned by the primary agent)

The primary agent spawns `uni-scrum-master` as the coordinator and acts only as the human proxy (kickoff, relay escalations, resume via `SendMessage`). The coordinator reads the protocol for the session type, spawns specialist agents, manages gates, updates GH Issues, and escalates at each human gate. It reads `.claude/agents/uni/uni-scrum-master.md` for role boundaries, the escalation handshake, and behavioral rules.

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

### Delivery Session Specialists (4 agents)

| Agent | Type | Stage | What It Does |
|-------|------|-------|-------------|
| `uni-pseudocode` | specialist | 3a | Per-component pseudocode. Queries `/uni-query-patterns` before designing |
| `uni-tester` | specialist | 3a + 3c | Test plan design (3a) + test execution with RISK-COVERAGE-REPORT.md (3c) |
| `uni-rust-dev` | developer | 3b | Implements Rust components (`crates/**/*.rs`) from validated pseudocode |
| `uni-js-dev` | developer | 3b | Implements JS/TS edge-client + Node tooling components (`packages/unimatrix/**`) from validated pseudocode |

Stage 3b routes one dev agent per component by target language (see uni-delivery-protocol.md → Dev-Agent Selection). A mixed wave spawns both types in one message.

### Bug Fix Specialists (1 agent)

| Agent | Type | Phase | What It Does |
|-------|------|-------|-------------|
| `uni-bug-investigator` | specialist | 1 | Diagnoses root cause, proposes fix approach, identifies missing tests |

### Shared Specialist (1 agent — used by `/uni-review-pr` skill)

| Agent | Type | Phase | What It Does |
|-------|------|-------|-------------|
| `uni-security-reviewer` | specialist | review | Fresh-context security review of PR diff, blast radius, OWASP assessment |

**Total: 14 specialist agents** (1 validator + 6 design + 4 delivery + 1 bug fix + 1 security + 1 retro-mode architect). The spawned `uni-scrum-master` coordinates them; the primary spawns the SM.

---

## Swarm Composition Templates

### Design Session

```
Primary:      spawn uni-scrum-master, then relay escalations + resume via SendMessage
Coordinator:  uni-scrum-master (reads uni-design-protocol.md + uni-scrum-master.md)
Phase 1:      uni-researcher (scope exploration; proposes SCOPE.md)
              ★ HUMAN CHECKPOINT — escalate SCOPE.md to primary; resume on approval ★
Phase 1b:     uni-risk-strategist (scope-risk mode)
Phase 2a:     uni-architect + uni-specification                    (parallel)
Phase 2a+:    uni-risk-strategist (architecture-risk mode)
Phase 2b:     uni-vision-guardian (alignment check)
Phase 2c:     uni-synthesizer (brief + maps + GH Issue)            (fresh context)
Phase 2d:     git commit + push + gh pr create --draft
              Escalate artifacts to primary → human review — SESSION 1 ENDS
```

### Delivery Session

```
Primary:      spawn uni-scrum-master, then relay escalations + resume via SendMessage
Coordinator:  uni-scrum-master (reads uni-delivery-protocol.md + uni-scrum-master.md)
Init:         Read IMPLEMENTATION-BRIEF.md, create feature branch
Stage 3a:     uni-pseudocode + uni-tester (test plans)             (parallel)
              UPDATE Component Map in IMPLEMENTATION-BRIEF.md
Gate 3a:      uni-validator (design review) — MANDATORY BLOCK
Stage 3b:     uni-rust-dev / uni-js-dev × N (one per component,    (parallel)
              by target language, MANDATORY)
Gate 3b:      uni-validator (code review)
Stage 3c:     uni-tester (test execution)
Gate 3c:      uni-validator (risk validation)
Phase 4:      Commit, push, open PR
              /uni-review-pr — security review + merge readiness
              Escalate to primary → human merge gate — SESSION 2 ENDS
```

### Bug Fix Session

```
Primary:      spawn uni-scrum-master, then relay escalations + resume via SendMessage
Coordinator:  uni-scrum-master (reads uni-bugfix-protocol.md + uni-scrum-master.md)
Init:         /uni-query-patterns + /uni-knowledge-search — prior knowledge
Phase 1:      uni-bug-investigator (diagnose root cause)
              ★ HUMAN CHECKPOINT — escalate diagnosis to primary; resume on approval ★
Phase 2:      git checkout -b bugfix/{issue}-{desc}
              uni-rust-dev / uni-js-dev (implement fix + tests, by language of the fix)
Phase 3:      uni-tester (full test suite verification)
Gate 3:       uni-validator (bugfix check set)
              git commit + push + gh pr create
Phase 4:      /uni-review-pr — security review + merge readiness
Phase 5:      Escalate PR + review assessment to primary → human merge — SESSION ENDS
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
              — if the response carries a transcript_candidates section (crt-052), consume it
                per uni-retro "Consuming transcript_candidates": advisory family_hints, Q8 folds,
                provenance/loss weighting, call-time-vs-cached, feature-attributed context_store
Phase 2:      uni-architect (pattern/procedure extraction + ADR validation)
Phase 3:      ADR supersession (if flagged, requires human approval)
Phase 4:      Worktree cleanup
Phase 5:      Summary + outcome recording — RETRO ENDS
```

---

## Composition Rules

1. **Every swarm session**: the primary spawns `uni-scrum-master` as the coordinator; the SM reads the protocol and its definition and runs the session. The primary is the human proxy only (kickoff, relay, resume). No exceptions.
2. **Validation gates**: `uni-validator` spawned at each gate by you.
3. **Design session**: All six design agents in defined phase order per protocol.
4. **Delivery session**: pseudocode + tester + rust-dev/js-dev (per component language) + validator at three gates per protocol.
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
| `/uni-record-outcome` | END of every session | uni-scrum-master (coordinator), `/uni-review-pr`, `/uni-retro` |
| `/uni-store-procedure` | After successful sessions (reusable techniques) | uni-scrum-master (coordinator), uni-bug-investigator |
| `/uni-store-lesson` | After failures | uni-bug-investigator, uni-validator, uni-scrum-master (coordinator) |
| `/uni-knowledge-search` | Exploring what's known | Any agent |
| `/uni-knowledge-lookup` | Exact-match retrieval | Any agent |
| `/uni-git` | Git conventions | uni-scrum-master (coordinator) |
| `/uni-review-pr` | After PR creation or standalone | uni-scrum-master (coordinator), human |
| `/uni-retro` | After merge | human |
