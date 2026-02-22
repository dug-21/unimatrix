# Spec-Driven Development with Risk-Based Testing
## Complete End-to-End Workflow

---

## Overview

This workflow establishes a human-directed, agent-executed development process. Humans set direction and validate critical decisions. Agents handle decomposition, implementation, and validation. The workflow centers on three source-of-truth documents created in Phase 2 and validated against throughout: Architecture, Specification, and Risk-Based Test Strategy.

The key innovation: specifications are not monolithic. A global feature-level spec drives component-level pseudocode and test plans. Agents work on components, not entire features. Every stage validates back to the three approved source documents.

---

## Phase 1: Research & Scope Definition

**Participants**: Human + Research Agent

**Process**:

1. Human initiates the feature with high-level intent.
2. Research agent explores the problem space — existing systems, technical landscape, competitive approaches, constraints.
3. Agent synthesizes findings and proposes scope.
4. Human and agent iterate — refine assumptions, challenge scope, converge on shared understanding.

**Output**: Feature scope document (approved by human).

**Outcome**: Clear, agreed-upon definition of what we're building. Ready for detailed design.

---

## Phase 2: Design (Architecture, Specification, Risk Strategy)

**Participants**: Design Agent (creates) + Human (reviews and approves)

**Objective**: Create the three foundational documents that guide all downstream agent work.

### 2a: Agent Creates Three Documents

The design agent receives the approved scope and generates:

**Document 1: Architecture**

- High-level system design
- Component breakdown and boundaries
- How components interact (interfaces, contracts, data flow)
- Technology stack with specific versions
- Integration points and dependencies

**Document 2: Specification**

- Detailed feature requirements
- User workflows and use cases
- Functional and non-functional requirements
- Success criteria and acceptance conditions
- Domain models and ubiquitous language

**Document 3: Risk-Based Test Strategy**

- Identifies critical risks at the feature level — what could fail and impact users or the business
- Maps each risk to specific testing scenarios
- Defines test coverage needed to validate each risk is mitigated
- Prioritizes tests by risk severity and likelihood
- Addresses integration risks, edge cases, and failure modes
- This is NOT unit test strategy — it is feature-level risk mitigation validation

### 2b: Human Review & Approval

Human reviews all three documents:

- Does the architecture make sense for the problem?
- Does the specification match the original intent from Phase 1?
- Are the identified risks complete and realistic?
- Are the test strategies sufficient to prove the risks are addressed?

**Gate Decision**: Approve or request revision. Agents do not proceed until all three documents are approved.

**Outcome**: Three approved source-of-truth documents. Everything downstream must trace back to these.

---

## Phase 3: Agent Delivery (Multi-Stage with Validation Gates)

**Participants**: Multiple agent teams + Human (escalation only)

**Objective**: Decompose the approved design into components, implement, test, and validate that everything traces back to the three source documents.

**Key Principle**: Every stage has a validation gate. Nothing moves forward unless it maps back to the approved Architecture, Specification, and Risk-Based Test Strategy.

---

### Stage 3a: Component Design & Pseudocode Generation

**Participants**: Pseudocode & Component Design Agents

**Process**:

1. Agents receive the three approved Phase 2 documents.
2. Decompose feature into logical components based on the architecture.
3. For each component, generate:
   - Component-level architecture (how it fits into the larger system, interfaces with other components)
   - Detailed pseudocode (algorithm and logic before implementation)
   - Component-level test plan (what tests validate this component, rooted in the feature-level risks from the Risk-Based Test Strategy)

**Output**: Component design documents — one per component — each containing pseudocode and test plan.

---

### Gate 3a: Validation Agent (Component Design Review)

**Participants**: Validation Agent

**Process**:

The validation agent maps every component design document back to the three source documents:

- Does each component align with the approved Architecture?
- Does the pseudocode implement what the Specification requires?
- Does the component test plan address the relevant risks from the Risk-Based Test Strategy?
- Are component interfaces consistent with the architecture's defined contracts?

**Decision Point — Scope & Feasibility Check**:

During component design, agents may discover that:

- The original scope was wrong or incomplete
- A technology doesn't work as assumed
- The architecture can't support a requirement
- A risk was missed or underestimated

**If issues are found**:

- Minor issues: Validation agent sends back to component design agents for rework within the existing scope.
- Scope or feasibility issues: Flag for human review. Human decides whether to adjust scope (return to Phase 1), revise design documents (return to Phase 2), or approve a modified approach.

**Gate Result**:

- ✓ Pass → Proceed to Stage 3b (Implementation)
- ✗ Minor Fail → Rework component design
- ✗ Scope/Feasibility Fail → Escalate to human review

---

### Stage 3b: Code Implementation

**Participants**: Coding Agents

**Process**:

1. Coding agents receive the validated component design documents (pseudocode + test plans).
2. Implement code for each component based on the pseudocode.
3. Build test cases per the component test plans.
4. Execute component-level tests during development.

**Output**: Implemented code + test cases + test results per component.

---

### Gate 3b: Validation Agent (Code Review)

**Participants**: Validation Agent

**Process**:

The validation agent maps implemented code back to the source documents:

- Does the code match the validated pseudocode from Stage 3a?
- Does the implementation align with the approved Architecture?
- Are component interfaces implemented as specified?
- Do the test cases match the component test plans?

**If issues are found**:

- Code doesn't match pseudocode: Send back to coding agents for rework.
- Architectural deviation discovered: Flag — rework or escalate to human.
- New technical constraint uncovered: Escalate to human for scope decision.

**Gate Result**:

- ✓ Pass → Proceed to Stage 3c (Testing & Risk Validation)
- ✗ Fail → Rework or escalate

---

### Stage 3c: Testing & Risk Validation

**Participants**: QA / Testing Agents

**Process**:

1. Execute all component-level tests.
2. Execute integration tests across components.
3. Execute feature-level tests mapped to the Risk-Based Test Strategy.
4. Verify that every risk identified in Phase 2 has corresponding test coverage.
5. Verify that all tests pass.

**Output**: Complete test results + risk coverage report.

---

### Gate 3c: Validation Agent (Final Risk-Based Validation)

**Participants**: Validation Agent

**Process**:

The validation agent performs final validation against all three source documents:

- Do test results prove the identified risks are mitigated?
- Does test coverage match the Risk-Based Test Strategy?
- Are there any risks from Phase 2 that lack test coverage?
- Does the delivered code match the approved Specification?
- Does the system architecture match the approved Architecture?

**If issues are found**:

- Missing risk coverage: Send back to testing or coding agents to fill gaps.
- Test failures: Send back to coding agents to debug.
- Unresolvable issues: Escalate to human review.

**Gate Result**:

- ✓ Pass → Proceed to Phase 4 (Delivery)
- ✗ Fail → Rework, fill gaps, or escalate

---

## Phase 4: Delivery

**Prerequisite**: All three validation gates (3a, 3b, 3c) have passed.

**Outcome**: Code ships. All deliverables provably trace back to the three human-approved source documents (Architecture, Specification, Risk-Based Test Strategy).

---

## Summary: Validation Gates

| Gate | What It Validates | Validates Against |
|------|-------------------|-------------------|
| Gate 3a | Component designs, pseudocode, test plans | Architecture, Specification, Risk Strategy |
| Gate 3b | Implemented code, test cases | Pseudocode, Architecture, Specification |
| Gate 3c | Test results, risk coverage | Risk Strategy, Specification, Architecture |

---

## Feedback Loops & Escalation

At every gate, two types of failures can occur:

**Reworkable failures**: Component design doesn't match spec, code doesn't match pseudocode, test gaps exist. These loop back to the previous agent team for correction.

**Scope/feasibility failures**: Original scope was wrong, technology doesn't work as assumed, architecture can't support a requirement. These escalate to human review with a recommendation — human decides whether to adjust scope (Phase 1), revise design (Phase 2), or approve a modified approach.

The human only re-enters the process when scope or feasibility is in question. All other validation is automated through the agent team.

---

## Key Principles

1. **Three source documents are sacred.** Architecture, Specification, and Risk-Based Test Strategy — approved by human in Phase 2 — are the source of truth for everything downstream.

2. **Component-level specs, not monolithic docs.** Agents work on components, not features. Each component gets its own pseudocode and test plan derived from the global spec.

3. **Risk drives testing.** The test strategy is not about unit test coverage. It's about proving that identified feature-level risks are mitigated.

4. **Every stage validates backward.** Nothing moves forward unless it traces to the approved source documents.

5. **Agents own delivery, humans own direction.** After Phase 2 approval, agents handle decomposition, implementation, testing, and validation. Humans re-enter only when scope or feasibility breaks.

6. **Fail fast on scope issues.** If agents discover the original scope was wrong during component design, that surfaces immediately — not after code is written.