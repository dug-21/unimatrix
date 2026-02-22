# Spec-Driven Development with Risk-Based Testing
## Complete End-to-End Workflow

---

## Overview

This workflow establishes a human-directed, agent-executed development process. Humans set direction and validate critical decisions. Agents handle decomposition, implementation, and validation. The workflow centers on three source-of-truth documents created in Phase 2 and validated against throughout: Architecture, Specification, and Risk-Based Test Strategy.

The key innovation: specifications are not monolithic. A global feature-level spec drives component-level pseudocode and test plans. Agents work on components, not entire features. Every stage validates back to the three approved source documents.

### Execution Model: Two Sessions, Two Leaders

The workflow executes across **two distinct sessions** with a human approval gate between them.

- **Session 1 (Design)**: A **Design Leader** orchestrates Phase 1 (research/scope) and Phase 2 (design). The session ends by returning three source documents, a vision alignment report, an implementation brief, and an acceptance map to the human for review and approval.

- **Session 2 (Delivery)**: A **Delivery Leader** orchestrates Phase 3 (stages 3a → 3b → 3c) and Phase 4 (delivery). The Delivery Leader runs autonomously through all three stages and their validation gates. If all gates pass, the feature is delivered. If any gate fails beyond rework, the session stops and returns to the human.

Both leaders are the same coordinator agent (ndp-scrum-master) reading different protocols. The **Implementation Brief** is the handoff document between sessions — it contains the component map, acceptance criteria map, GH Issue number, and paths to all source documents.

```
SESSION 1 — DESIGN                          SESSION 2 — DELIVERY
═══════════════════                          ════════════════════

Phase 1: Research & Scope                   Phase 3a: Component Design
        ↓                                     ★ Gate 3a ★
Phase 2: Design                                     ↓
  3 source docs + vision check              Phase 3b: Implementation
  + implementation brief                      ★ Gate 3b ★
  + acceptance map + GH Issue                       ↓
        ↓                                   Phase 3c: Testing & Risk Validation
  ★ RETURN TO HUMAN ★                         ★ Gate 3c ★
  Human reviews and approves                        ↓
                                            Phase 4: Delivery
                                              ★ RETURN TO HUMAN ★
```

---

## Phase 1: Research & Scope Definition

**Participants**: Human + Research Agent (ndp-researcher)

**Process**:

1. Human initiates the feature with high-level intent.
2. Research agent explores the problem space — existing codebase patterns, technical landscape, competitive approaches, constraints, and relevant project knowledge (AgentDB patterns).
3. Agent synthesizes findings and proposes scope boundaries with rationale.
4. Human and agent iterate — refine assumptions, challenge scope, converge on shared understanding.
5. Research agent writes SCOPE.md capturing the agreed scope.
6. Human reviews and approves SCOPE.md.

**Output**: SCOPE.md (agent-authored, human-approved).

**Outcome**: Clear, agreed-upon definition of what we're building. Ready for detailed design.

---

## Phase 2: Design (Architecture, Specification, Risk Strategy)

**Participants**: Three specialist agents (create) + Vision Guardian (check) + Synthesizer (compile) + Human (reviews and approves)

**Objective**: Create the three foundational source-of-truth documents, verify vision alignment, compile the coordinator's operating brief, and present everything to the human for approval.

### 2a: Three Specialist Agents Create Three Documents (Parallel)

The Design Leader receives the approved SCOPE.md and spawns three specialist agents in parallel:

**ndp-architect → Architecture** (`architecture/ARCHITECTURE.md`)

- High-level system design
- Component breakdown and boundaries
- How components interact (interfaces, contracts, data flow)
- Technology stack with specific versions
- Integration points and dependencies
- Integration Surface analysis (exact view names, column types, schemas)
- ADRs stored in AgentDB via `/save-pattern`

**ndp-specification → Specification** (`specification/SPECIFICATION.md`)

- Detailed feature requirements
- User workflows and use cases
- Functional and non-functional requirements
- Success criteria and acceptance conditions
- Domain models and ubiquitous language

**ndp-risk-strategist → Risk-Based Test Strategy** (`RISK-TEST-STRATEGY.md`)

- Identifies critical risks at the feature level — what could fail and impact users or the business
- Maps each risk to specific testing scenarios
- Defines test coverage needed to validate each risk is mitigated
- Prioritizes tests by risk severity and likelihood
- Addresses integration risks, edge cases, and failure modes
- This is NOT unit test strategy — it is feature-level risk mitigation validation

The risk strategist thinks "what could go wrong?" — distinct from the tester who thinks "how do I verify it works?"

### 2b: Vision Alignment Check

After the three specialist agents complete, the Design Leader spawns the **ndp-vision-guardian**. The vision guardian reads all three source documents and the product vision criteria, producing an **ALIGNMENT-REPORT.md** that flags any variances requiring human attention.

### 2c: Synthesizer Compiles Coordinator Brief

After vision alignment, the Design Leader spawns the **ndp-synthesizer** with a fresh context window. The synthesizer reads all three source documents, the alignment report, and the architect's ADR pattern IDs, and produces:

**IMPLEMENTATION-BRIEF.md** — The coordinator's operating document for Session 2. Contains:
- Component Map (which components the feature touches, mapped to pseudocode and test-plan files)
- Resolved Decisions table (ADR references with AgentDB pattern IDs)
- Files to create/modify (paths with summaries)
- Data structures and function signatures
- Constraints, dependencies, and scope exclusions
- Wave structure for delivery

**ACCEPTANCE-MAP.md** — Maps every acceptance criterion from SCOPE.md to a verification method (test, manual, file-check, grep, shell) with specific verification detail. Used by the validator at every gate.

**GH Issue** — Created from the brief, becomes the tracking artifact across both sessions.

### 2d: Human Review & Approval

The Design Leader returns all artifacts to the human. **Session 1 ends here.**

Human reviews:

- **Three source documents**: Do the architecture, specification, and risk strategy make sense? Are risks complete and realistic?
- **Alignment report**: Any vision variances that need resolution?
- **Acceptance map**: Are the ACs complete and verifiable?
- **Implementation brief**: Does the component map and wave structure look right?

**Gate Decision**: Approve or request revision. Agents do not proceed until the human approves. This is the boundary between Session 1 and Session 2.

**Outcome**: Three approved source-of-truth documents, a coordinator brief, and a tracking issue. Everything downstream must trace back to these.

---

## Phase 3: Agent Delivery (Multi-Stage with Validation Gates)

**Session 2 begins.** The human initiates Session 2 by approving the design. The **Delivery Leader** reads the IMPLEMENTATION-BRIEF.md (which contains the component map, acceptance criteria, GH Issue number, and paths to all source documents) and runs Stages 3a → 3b → 3c autonomously.

**Key Principle**: Every stage has a validation gate. Gates that pass proceed automatically. Gates that fail stop the session and return to the human. The validator is the human's eyes — it enforces the standards the human approved in the three source documents.

---

### Stage 3a: Component Design & Pseudocode Generation

**Participants**: ndp-pseudocode + ndp-tester (component test plans)

**Process**:

1. Agents receive the three approved Phase 2 documents (via paths in the implementation brief).
2. Decompose feature into logical components based on the architecture.
3. For each component, generate:
   - Component-level architecture (how it fits into the larger system, interfaces with other components)
   - Detailed pseudocode (algorithm and logic before implementation)
   - Component-level test plan (what tests validate this component, rooted in the feature-level risks from the Risk-Based Test Strategy)

**Output**: Per-component design documents:
```
pseudocode/
  OVERVIEW.md              — component interaction, data flow, shared types
  {component-1}.md         — per-component pseudocode
  {component-2}.md
test-plan/
  OVERVIEW.md              — overall test strategy, integration surface
  {component-1}.md         — per-component test expectations
  {component-2}.md
```

---

### Gate 3a: Validation Agent (Component Design Review)

**Participants**: ndp-validator (Gate 3a mode)

**Process**:

The validation agent maps every component design document back to the three source documents:

- Does each component align with the approved Architecture?
- Does the pseudocode implement what the Specification requires?
- Does the component test plan address the relevant risks from the Risk-Based Test Strategy?
- Are component interfaces consistent with the architecture's defined contracts?

**Gate Result**:

- ✓ Pass → Delivery Leader proceeds to Stage 3b automatically
- ✗ Reworkable Fail → Loop back to component design agents (max 2 iterations)
- ✗ Scope/Feasibility Fail → Session stops, returns to human with recommendation

---

### Stage 3b: Code Implementation

**Participants**: ndp-rust-dev + domain specialists as needed

**Process**:

1. Coding agents receive the validated component design documents (pseudocode + test plans) via paths routed by the Delivery Leader from the component map.
2. Implement code for each component based on the pseudocode.
3. Build test cases per the component test plans.
4. Execute component-level tests during development.

**Output**: Implemented code + test cases + test results per component.

---

### Gate 3b: Validation Agent (Code Review)

**Participants**: ndp-validator (Gate 3b mode)

**Process**:

The validation agent maps implemented code back to the source documents:

- Does the code match the validated pseudocode from Stage 3a?
- Does the implementation align with the approved Architecture?
- Are component interfaces implemented as specified?
- Do the test cases match the component test plans?
- Does the code compile? Are there stubs or placeholders?

**Gate Result**:

- ✓ Pass → Delivery Leader proceeds to Stage 3c automatically
- ✗ Reworkable Fail → Loop back to coding agents (max 2 iterations)
- ✗ Scope/Feasibility Fail → Session stops, returns to human with recommendation

---

### Stage 3c: Testing & Risk Validation

**Participants**: ndp-tester (test execution)

**Process**:

1. Execute all component-level tests.
2. Execute integration tests across components.
3. Execute feature-level tests mapped to the Risk-Based Test Strategy.
4. Verify that every risk identified in Phase 2 has corresponding test coverage.
5. Verify that all tests pass.

**Output**: Complete test results + RISK-COVERAGE-REPORT.md (maps test results to identified risks, proving coverage).

---

### Gate 3c: Validation Agent (Final Risk-Based Validation)

**Participants**: ndp-validator (Gate 3c mode)

**Process**:

The validation agent performs final validation against all three source documents:

- Do test results prove the identified risks are mitigated?
- Does test coverage match the Risk-Based Test Strategy?
- Are there any risks from Phase 2 that lack test coverage?
- Does the delivered code match the approved Specification?
- Does the system architecture match the approved Architecture?

**Gate Result**:

- ✓ Pass → Proceed to Phase 4 (Delivery)
- ✗ Reworkable Fail → Loop back to testing or coding agents (max 2 iterations)
- ✗ Scope/Feasibility Fail → Session stops, returns to human with recommendation

---

## Phase 4: Delivery

**Prerequisite**: All three validation gates (3a, 3b, 3c) have passed.

The Delivery Leader updates the GH Issue with final results and returns to the human.

**Outcome**: Code ships. All deliverables provably trace back to the three human-approved source documents (Architecture, Specification, Risk-Based Test Strategy).

---

## Summary: Validation Gates

| Gate | What It Validates | Validates Against | On Pass | On Fail |
|------|-------------------|-------------------|---------|---------|
| Gate 3a | Component designs, pseudocode, test plans | Architecture, Specification, Risk Strategy | Auto-proceed to 3b | Rework (2x) or stop |
| Gate 3b | Implemented code, test cases | Pseudocode, Architecture, Specification | Auto-proceed to 3c | Rework (2x) or stop |
| Gate 3c | Test results, risk coverage | Risk Strategy, Specification, Architecture | Deliver | Rework (2x) or stop |

All three gates use the same validation agent (ndp-validator) with different focused check sets per gate.

---

## Feedback Loops & Escalation

At every gate, two types of failures can occur:

**Reworkable failures**: Component design doesn't match spec, code doesn't match pseudocode, test gaps exist. These loop back to the previous stage's agents for correction. Maximum 2 rework iterations per gate — this protects the context window.

**Scope/feasibility failures**: Original scope was wrong, technology doesn't work as assumed, architecture can't support a requirement. These stop the session entirely. The Delivery Leader returns to the human with a recommendation — human decides whether to adjust scope (return to Phase 1), revise design (return to Phase 2), or approve a modified approach.

The human only re-enters the process when scope or feasibility is in question, or when rework iterations are exhausted. All other validation is automated through the validator agent — which enforces the standards the human approved in the three source documents.

---

## Artifacts & Directory Structure

```
product/features/{phase}-{NNN}/
├── SCOPE.md                    # Phase 1: agent-authored, human-approved
├── specification/              # Phase 2: source document
│   └── SPECIFICATION.md
├── architecture/               # Phase 2: source document
│   └── ARCHITECTURE.md
├── RISK-TEST-STRATEGY.md       # Phase 2: source document
├── ALIGNMENT-REPORT.md         # Phase 2: vision check
├── IMPLEMENTATION-BRIEF.md     # Phase 2: coordinator's operating document
├── ACCEPTANCE-MAP.md           # Phase 2: AC verification map
├── pseudocode/                 # Phase 3a: per-component pseudocode
│   ├── OVERVIEW.md
│   └── {component}.md
├── test-plan/                  # Phase 3a: per-component test plans
│   ├── OVERVIEW.md
│   └── {component}.md
├── testing/                    # Phase 3c: test execution output
│   └── RISK-COVERAGE-REPORT.md
└── reports/                    # Validation gate reports
    ├── gate-3a-report.md
    ├── gate-3b-report.md
    └── gate-3c-report.md
```

---

## Agent Roster

### Session 1 — Design

| Agent | Phase | Role |
|-------|-------|------|
| ndp-scrum-master | 1-2 | **Design Leader** — orchestrates research, design, vision check, synthesis |
| ndp-researcher | 1 | Problem space exploration, collaborative scope definition, writes SCOPE.md |
| ndp-architect | 2 | Architecture document + ADRs stored in AgentDB |
| ndp-specification | 2 | Specification document |
| ndp-risk-strategist | 2 | Risk-Based Test Strategy document |
| ndp-vision-guardian | 2 | Alignment report (checks source docs against product vision) |
| ndp-synthesizer | 2 | Implementation Brief + Acceptance Map + GH Issue |

### Session 2 — Delivery

| Agent | Stage | Role |
|-------|-------|------|
| ndp-scrum-master | 3a-4 | **Delivery Leader** — runs three stages with gates, auto-proceeds or stops |
| ndp-pseudocode | 3a | Per-component pseudocode from source docs |
| ndp-tester | 3a | Per-component test plans derived from Risk-Based Test Strategy |
| ndp-rust-dev | 3b | Code implementation from pseudocode |
| (domain specialists) | 3b | As needed: ndp-parquet-dev, ndp-timescale-dev, etc. |
| ndp-tester | 3c | Test execution + RISK-COVERAGE-REPORT.md |
| ndp-validator | 3a,3b,3c | One agent, three focused spawns — the human's eyes at each gate |

---

## Key Principles

1. **Three source documents are sacred.** Architecture, Specification, and Risk-Based Test Strategy — approved by human in Phase 2 — are the source of truth for everything downstream.

2. **Two sessions, two leaders.** Session 1 (Design) ends for human review. Session 2 (Delivery) runs autonomously through three validated stages. The Implementation Brief is the handoff.

3. **Specialist agents, not generalists.** Each document is produced by a specialist — architect, specification writer, risk strategist. The coordinator ensures consistency across them.

4. **Component-level specs, not monolithic docs.** Agents work on components, not features. Each component gets its own pseudocode and test plan derived from the global spec.

5. **Risk drives testing.** The test strategy is not about unit test coverage. It's about proving that identified feature-level risks are mitigated.

6. **Every stage validates backward.** Nothing moves forward unless it traces to the approved source documents. The validator enforces the human's approved standards.

7. **Agents own delivery, humans own direction.** After Phase 2 approval, agents handle decomposition, implementation, testing, and validation autonomously. Humans re-enter only when scope or feasibility breaks.

8. **Fail fast on scope issues.** If agents discover the original scope was wrong during any stage, that surfaces immediately — not after code is written. Rework is capped at 2 iterations per gate.

9. **The validator is the human's proxy.** The ndp-validator agent — spawned three times with focused checks — represents the human's approved quality bar. It validates against the documents the human reviewed, so the human doesn't need to re-enter for quality control.
