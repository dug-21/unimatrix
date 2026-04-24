# Delivery Session (Session 2) — Workflow Guide

A Delivery Session takes the design artifacts from Session 1 and implements, tests, and ships the feature. It runs autonomously through three stages with mandatory validation gates between each. The human re-enters only for scope failures, rework exhaustion, or final PR approval.

**Prerequisite**: `product/features/{feature-id}/IMPLEMENTATION-BRIEF.md` must exist. If it does not, run a Design Session first.

**What it produces**: Implemented code, tests, gate reports, PR, and security review.

---

## Stage Flow (Overall)

```mermaid
flowchart TD
    H([Human starts: provides IMPLEMENTATION-BRIEF path]) --> INIT

    subgraph INIT [Initialization]
        I1[Delivery Leader reads IMPLEMENTATION-BRIEF + ACCEPTANCE-MAP]
        I2[Creates feature branch]
        I3[Commits design artifacts from Session 1]
        I4[Calls context_cycle start\nUnimatrix: delivery session opens]
        I5[Plans Stage 3b waves from IMPLEMENTATION-BRIEF]
        I1 --> I2 --> I3 --> I4 --> I5
    end

    INIT --> S3A

    subgraph S3A [Stage 3a — Component Design]
        PA[Pseudocode Agent\nDecomposes feature into components\nWrites pseudocode/ files]
        TA[Test Plan Agent\nWrites test-plan/ files\nIncludes integration harness plan]
    end

    S3A --> MAPUP[Delivery Leader updates Component Map\nin IMPLEMENTATION-BRIEF.md]
    MAPUP --> CYCLE3A[Unimatrix: phase-end spec → spec-review]
    CYCLE3A --> G3A

    G3A{Gate 3a: Design Review\nValidator checks pseudocode + test plans\nagainst Architecture + Specification}
    G3A -->|PASS| COMMIT3A[Commit pseudocode + test plans\nUnimatrix: phase-end spec-review → develop]
    G3A -->|REWORKABLE FAIL\nup to 2 retries| S3A
    G3A -->|SCOPE FAIL| STOP1([Stop — return to human\nwith recommendation])

    COMMIT3A --> S3B

    subgraph S3B [Stage 3b — Code Implementation Wave-Based]
        W1[Wave 1: rust-dev agents in parallel\none per independent component]
        W2[Commit Wave 1\nWave 2: agents for dependent components]
        WN[... continue until all waves complete]
        W1 --> W2 --> WN
    end

    S3B --> G3B

    G3B{Gate 3b: Code Review\nValidator checks code against pseudocode\nand Architecture}
    G3B -->|PASS| COMMIT3B[Commit all implementation\nUnimatrix: phase-end develop → test]
    G3B -->|REWORKABLE FAIL\nup to 2 retries| S3B
    G3B -->|SCOPE FAIL| STOP2([Stop — return to human])

    COMMIT3B --> S3C[Stage 3c: Tester\nRuns unit tests + integration smoke suite\nWrites RISK-COVERAGE-REPORT.md]
    S3C --> G3C

    G3C{Gate 3c: Risk Validation\nValidator checks risk coverage\nand integration test results}
    G3C -->|PASS| COMMIT3C[Commit test artifacts\nUnimatrix: phase-end test → pr-review]
    G3C -->|REWORKABLE FAIL\nup to 2 retries| S3C
    G3C -->|SCOPE FAIL| STOP3([Stop — return to human])

    COMMIT3C --> P4[Phase 4: Delivery]

    style INIT fill:#e8f4f8
    style G3A fill:#f8d7da
    style G3B fill:#f8d7da
    style G3C fill:#f8d7da
    style CYCLE3A fill:#fff3cd
    style COMMIT3A fill:#fff3cd
    style COMMIT3B fill:#fff3cd
    style COMMIT3C fill:#fff3cd
```

---

## Phase 4 — Delivery Detail

```mermaid
flowchart TD
    START([All three gates passed]) --> PUSH[Commit final artifacts\nPush feature branch\nOpen PR via gh CLI]
    PUSH --> DOCCHECK{Does feature trigger\ndocumentation update?\nNew MCP tool / skill / CLI command\nNew knowledge category / schema change}
    DOCCHECK -->|Yes| DOCS[Spawn uni-docs\nUpdates README on feature branch]
    DOCCHECK -->|No — internal change| REVIEW
    DOCS --> REVIEW[Invoke uni-review-pr\nSpawns security reviewer with fresh context\nReviews full diff cold]
    REVIEW --> SECRESULT{Security review result}
    SECRESULT -->|No blocking findings| CLOSE[Unimatrix: phase-end pr-review\nUnimatrix: context_cycle stop — session closes]
    SECRESULT -->|Blocking findings| FIX[Address blocking items\nthen re-review]
    FIX --> REVIEW
    CLOSE --> RETURN([Return to human:\nGates 3a/3b/3c PASS\nSecurity risk level\nMerge readiness: READY or BLOCKED\nPR URL + GH Issue URL])

    style DOCCHECK fill:#f8d7da
    style SECRESULT fill:#f8d7da
    style CLOSE fill:#fff3cd
```

---

## Rework Protocol

Every gate can produce three outcomes:

| Result | What happens |
|--------|-------------|
| **PASS** | Proceed to next stage. Phase-end recorded in Unimatrix. |
| **REWORKABLE FAIL** | Re-spawn the previous stage's agents with the gate report. Max 2 retries per gate. On third failure, escalate to SCOPE FAIL. |
| **SCOPE FAIL** | Session stops immediately. Return to human: which gate failed, why, and recommendation (adjust scope / revise design / approve modified approach). |

---

## Unimatrix Integration Points

| Moment | Unimatrix Call | Purpose |
|--------|---------------|---------|
| Session start | `context_cycle(type: "start", next_phase: "spec")` | Opens delivery session attribution |
| Stage 3a complete | `context_cycle(type: "phase-end", phase: "spec", next_phase: "spec-review")` | Records pseudocode/test-plan phase |
| Gate 3a PASS | `context_cycle(type: "phase-end", phase: "spec-review", next_phase: "develop")` | Records design gate pass |
| Gate 3b PASS | `context_cycle(type: "phase-end", phase: "develop", next_phase: "test")` | Records code gate pass |
| Gate 3c PASS | `context_cycle(type: "phase-end", phase: "test", next_phase: "pr-review")` | Records test gate pass |
| Phase 4 complete | `context_cycle(type: "phase-end", phase: "pr-review")` then `context_cycle(type: "stop")` | Closes the feature cycle opened in Session 1 |
| All agents — before starting work | `context_briefing(...)` + `context_search(...)` | Agents retrieve relevant ADRs and patterns before implementing |

**Key**: The `context_cycle(type: "stop")` at the end of Phase 4 closes the cycle that was opened in Session 1. The full feature lifecycle — from scope through delivery — is recorded as a single cycle.

---

## Stage 3b Wave Planning

Before spawning any implementation agents, the Delivery Leader reads the IMPLEMENTATION-BRIEF and groups components into dependency waves:

- **Wave 1**: Components with no dependencies on other components in this feature — all run in parallel.
- **Wave 2+**: Components that depend on Wave 1 outputs — run after Wave 1 is committed.

> [!NOTE]
> **Context window management**: Each rust-dev agent receives the full Architecture and Specification (so it understands the whole system) plus only *its own component's* pseudocode and test plan. Agents are not given every component's pseudocode — this is intentional. It prevents context overflow and keeps each agent focused on its specific implementation contract. The component breakdown produced by the Architect in Session 1 directly determines how implementation work is partitioned here.

> [!NOTE]
> **Artifact strategy in Delivery**: Pseudocode, test plans, gate reports, and the risk coverage report are written as Markdown files in `product/features/{id}/`. Reusable patterns, updated procedures, and lessons discovered during implementation are stored in Unimatrix by the retro session after merge — not during delivery itself.

Agents do not run integration tests — that is Stage 3c.

---

## What the Human Receives

At the end of Phase 4:

- Gate results: 3a PASS, 3b PASS, 3c PASS
- Security review: risk level and summary
- Merge readiness: READY or BLOCKED (with blocking items listed)
- PR URL and GitHub Issue URL (updated with gate comments throughout)

**Human action required**: Approve and merge, or address any blocking security findings.
