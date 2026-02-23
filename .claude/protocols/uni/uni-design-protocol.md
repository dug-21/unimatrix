# Design Session Protocol (Session 1)

Triggers on: specification, architecture, design, research, scope, risk strategy, SCOPE.md creation.

---

## Execution Model

Session 1 produces three sacred source-of-truth documents, a vision alignment report, an implementation brief, and an acceptance map. The session ends by returning everything to the human for review and approval.

```
Primary Agent                    uni-scrum-master (Design Leader)    Design Agents
─────────────                    ────────────────────────────────    ─────────────
read SCOPE.md (or initiate)
spawn scrum-master ──────────►   read protocol + SCOPE.md
                                 spawn researcher (Phase 1)
                                 ◄──────────────────────────────── SCOPE.md written
                                 human approves SCOPE.md
                                 spawn 2 specialists (Phase 2a) ──► produce arch + spec
                                 ◄──────────────────────────────── return artifact paths
                                 spawn risk strategist (Phase 2a+)► produce risk strategy
                                 ◄──────────────────────────────── return artifact path
                                 spawn vision guardian (Phase 2b)
                                 spawn synthesizer (Phase 2c)
◄──────────────────────────────  return all artifacts to human
human reviews and approves
```

**Session 1 ends when artifacts are returned to the human.** The human decides whether to proceed to Session 2 (Delivery).

### Concurrency Rules

Each message batches ALL related operations of the same type:

- ALWAYS spawn all agents WITHIN each phase step in ONE message via Task tool
- ALWAYS batch ALL file reads/writes/edits in ONE message

### Design Rules

- Output goes to `product/features/{feature-id}/` ONLY
- NO code changes. NO file edits outside `product/features/`
- NO launching delivery agents (uni-rust-dev, uni-pseudocode, uni-tester)
- Agents return: artifact paths + key decisions + open questions (NOT full file contents)

---

## Flow: Phase 1 + Phase 2

### Phase 1: Research & Scope Definition

**Participants**: Human + uni-researcher

The Design Leader spawns `uni-researcher` to collaborate with the human on scope definition.

```
Task(
  subagent_type: "uni-researcher",
  prompt: "You are researching the problem space for {feature-id}.
    Your agent ID: {feature-id}-researcher

    High-level intent: {human's description}

    Explore the problem space — existing codebase patterns, technical landscape,
    constraints, and relevant project knowledge.

    Synthesize findings and propose scope boundaries with rationale.
    Write SCOPE.md to product/features/{feature-id}/SCOPE.md.

    Return: SCOPE.md path, key findings, open questions for human."
)
```

After the researcher returns, the Design Leader presents SCOPE.md to the human for review and approval. **Do not proceed to Phase 2 until the human approves SCOPE.md.**

### Phase 2: Design (Three Source Documents + Vision + Synthesis)

Phase 2 has five sequential steps: 2a (architect + spec parallel) → 2a+ (risk strategist) → 2b (vision check) → 2c (synthesis) → 2d (return to human).

#### Phase 2a: Architect + Specification (Parallel, ONE message)

The Design Leader spawns two specialists in parallel:

**uni-architect → Architecture** (`architecture/ARCHITECTURE.md` + `ADR-NNN-{name}.md`)

- High-level system design, component breakdown and boundaries
- How components interact (interfaces, contracts, data flow)
- Technology decisions with rationale
- Integration points and dependencies
- ADRs as individual files in `architecture/`

**uni-specification → Specification** (`specification/SPECIFICATION.md`)

- Functional and non-functional requirements
- User workflows and use cases
- Acceptance criteria with verification methods
- Domain models and ubiquitous language
- Constraints and dependencies

Each specialist receives:
1. `Your agent ID: {feature-id}-agent-N-{role}`
2. Path to approved SCOPE.md
3. Task description

```
# Spawn both in ONE message:
Task(subagent_type: "uni-architect", prompt: "Your agent ID: {id}-agent-1-architect ...")
Task(subagent_type: "uni-specification", prompt: "Your agent ID: {id}-agent-2-spec ...")
```

Wait for BOTH to complete before proceeding to Phase 2a+.

#### Phase 2a+: Risk Strategist (After Architect + Specification)

The Design Leader spawns the risk strategist with the architecture and specification as additional inputs. This allows risk identification against concrete component boundaries, ADRs, acceptance criteria, and domain models — not just the scope.

**uni-risk-strategist → Risk-Based Test Strategy** (`RISK-TEST-STRATEGY.md`)

- Feature-level risk identification — what could fail and impact users
- Risk-to-testing-scenario mapping
- Coverage requirements per risk
- Prioritization by severity and likelihood
- Integration risks, edge cases, failure modes

The risk strategist receives:
1. `Your agent ID: {feature-id}-agent-3-risk`
2. Path to approved SCOPE.md
3. Paths to architecture and specification artifacts (from Phase 2a)
4. Task description

```
Task(
  subagent_type: "uni-risk-strategist",
  prompt: "Your agent ID: {id}-agent-3-risk
    ...
    Read these artifacts for context:
    - SCOPE.md: product/features/{id}/SCOPE.md
    - Architecture: product/features/{id}/architecture/ARCHITECTURE.md
    - ADRs: {list ADR file paths from architect's return}
    - Specification: product/features/{id}/specification/SPECIFICATION.md

    Use the architecture (component boundaries, integration points, ADRs)
    and specification (acceptance criteria, domain models, constraints)
    to inform your risk analysis. Identify risks that are specific to
    the designed architecture — not generic risks."
)
```

Wait for the risk strategist to complete before proceeding to Phase 2b.

#### Phase 2b: Vision Alignment Check

Spawn `uni-vision-guardian`:

```
Task(
  subagent_type: "uni-vision-guardian",
  prompt: "Your agent ID: {feature-id}-vision-guardian

    Read the product vision: product/PRODUCT-VISION.md
    Read the three source documents:
    - product/features/{id}/architecture/ARCHITECTURE.md
    - product/features/{id}/specification/SPECIFICATION.md
    - product/features/{id}/RISK-TEST-STRATEGY.md
    Read the scope: product/features/{id}/SCOPE.md

    Produce ALIGNMENT-REPORT.md at product/features/{id}/ALIGNMENT-REPORT.md.
    Flag any variances requiring human attention.
    Return: report path, variance summary."
)
```

#### Phase 2c: Synthesizer (Fresh Context Window)

After vision alignment, spawn `uni-synthesizer` with a fresh context window:

```
Task(
  subagent_type: "uni-synthesizer",
  prompt: "You are compiling the implementation brief for {feature-id}.
    Your agent ID: {feature-id}-synthesizer

    Read these artifacts:
    - product/features/{id}/SCOPE.md
    - product/features/{id}/specification/SPECIFICATION.md
    - product/features/{id}/architecture/ARCHITECTURE.md
    - product/features/{id}/architecture/ADR-*.md (all ADR files)
    - product/features/{id}/RISK-TEST-STRATEGY.md
    - product/features/{id}/ALIGNMENT-REPORT.md

    ADR file paths from architect: {list from architect's return}
    Vision variances: {from vision guardian's return}

    Produce: IMPLEMENTATION-BRIEF.md, ACCEPTANCE-MAP.md, GH Issue.
    Return: file paths + GH Issue URL."
)
```

The synthesizer gets a fresh context window — it reads artifacts directly for higher quality synthesis.

#### Phase 2d: Return to Human

The Design Leader collects all artifacts and returns to the human:

**Artifacts to present:**
- `SCOPE.md` — approved in Phase 1
- `architecture/ARCHITECTURE.md` + `ADR-NNN-{name}.md` files
- `specification/SPECIFICATION.md`
- `RISK-TEST-STRATEGY.md`
- `ALIGNMENT-REPORT.md` — highlight any VARIANCE or FAIL items
- `IMPLEMENTATION-BRIEF.md` — the handoff document for Session 2
- `ACCEPTANCE-MAP.md`
- GH Issue URL

**Return format:**
```
SESSION 1 COMPLETE — Design artifacts ready for review.

Artifacts:
- SCOPE.md: product/features/{id}/SCOPE.md
- Architecture: product/features/{id}/architecture/ARCHITECTURE.md
- ADRs: {list ADR file paths}
- Specification: product/features/{id}/specification/SPECIFICATION.md
- Risk Strategy: product/features/{id}/RISK-TEST-STRATEGY.md
- Alignment Report: product/features/{id}/ALIGNMENT-REPORT.md
- Implementation Brief: product/features/{id}/IMPLEMENTATION-BRIEF.md
- Acceptance Map: product/features/{id}/ACCEPTANCE-MAP.md
- GH Issue: {URL}

Vision Alignment: {summary — PASS/WARN/VARIANCE/FAIL counts}
Variances requiring approval: {list or "none"}
Open questions: {list or "none"}

Human action required: Review artifacts and approve to proceed to Session 2 (Delivery).
```

**Session 1 ends here.** The human reviews everything. Session 2 is a separate invocation.

---

## Agent Context Budget

Each spawned agent receives:
- Agent ID
- Task description (2-3 sentences)
- SCOPE.md path (agents read it themselves)
- Specific file paths to read (not file contents)

Do NOT paste full documents into agent prompts. Agents read files themselves.

---

## Quick Reference: Message Map

```
DESIGN LEADER (uni-scrum-master):
  Phase 1:    Task(uni-researcher) — scope exploration with human
              ...human approves SCOPE.md...
  Phase 2a:   Task(uni-architect) + Task(uni-specification) — parallel, ONE message
              ...wait for both...
  Phase 2a+:  Task(uni-risk-strategist) — receives arch + spec artifact paths
              ...wait...
  Phase 2b:   Task(uni-vision-guardian) — alignment check
  Phase 2c:   Task(uni-synthesizer) — brief + maps + GH Issue (fresh context)
  Phase 2d:   Return all artifacts to human — SESSION 1 ENDS
```
