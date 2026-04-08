# Design Session Protocol (Session 1)

Triggers on: specification, architecture, design, research, scope, risk strategy, SCOPE.md creation.

---

## Execution Model

Session 1 produces three sacred source-of-truth documents, a scope risk assessment, a vision alignment report, an implementation brief, and an acceptance map. All artifacts land in `product/features/{feature-id}/` as untracked files — no git operations occur during design. Session 2 (Delivery) creates the feature branch, commits the design artifacts, and continues from there.

**You are the Design Leader.** Read the SM agent definition (`.claude/agents/uni/uni-scrum-master.md`) for role boundaries. You orchestrate — you NEVER generate content. Spawn specialist agents for all work.

```
Design Leader (you)                                  Design Agents
───────────────────                                  ─────────────
read protocol + SCOPE.md (or initiate)
spawn researcher (Phase 1) ─────────────────────────► SCOPE.md written
◄────────────────────────────────────────────────────
human approves SCOPE.md
spawn risk strategist (Phase 1b) ───────────────────► scope risk assessment
◄────────────────────────────────────────────────────
spawn 2 specialists (Phase 2a) ─────────────────────► produce arch + spec
◄────────────────────────────────────────────────────
spawn risk strategist (Phase 2a+) ──────────────────► produce risk strategy
◄────────────────────────────────────────────────────
spawn vision guardian (Phase 2b)
spawn synthesizer (Phase 2c)
return all artifacts to human
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

### No Git Operations in Design

Design produces artifacts in `product/features/{feature-id}/` only. No branch is created, no commits are made, no PR is opened. The working tree stays on whatever branch is currently checked out. Git operations begin in Session 2 (Delivery), which creates the feature branch and commits the design artifacts as its first action.

### Feature Cycle Attribution

Before spawning any agents, call `context_cycle` to declare the feature cycle:

```
context_cycle(
  type: "start",
  topic: "{feature-id}",
  next_phase: "scope",
  agent_id: "{feature-id}-design-leader"
)
```

This sets session-level feature attribution so all subsequent tool calls are tracked against this feature.

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

After the researcher returns, the Design Leader presents SCOPE.md to the human for review and approval. **Do not proceed to Phase 1b until the human approves SCOPE.md.**

### Phase 1b: Scope Risk Assessment

**Participants**: uni-risk-strategist (scope-risk mode)

After SCOPE.md approval, the Design Leader spawns the risk strategist in scope-risk mode. This surfaces product-level risks (technology bets, dependency risks, scope boundary risks) BEFORE the architect and spec writer begin — so they can design with risk awareness.

```
Task(
  subagent_type: "uni-risk-strategist",
  prompt: "Your agent ID: {feature-id}-agent-0-scope-risk
    MODE: scope-risk

    Assess scope-level risks for {feature-id}.

    Read these artifacts:
    - SCOPE.md: product/features/{id}/SCOPE.md
    - Product vision: product/PRODUCT-VISION.md

    Produce SCOPE-RISK-ASSESSMENT.md at product/features/{id}/SCOPE-RISK-ASSESSMENT.md.
    Return: file path, risk summary, top 3 risks for architect attention."
)
```

Wait for the scope risk assessment to complete before proceeding to Phase 2.

```
context_cycle(
  type: "phase-end",
  topic: "{feature-id}",
  phase: "scope",
  outcome: "SCOPE.md approved. Scope risk assessment complete.",
  next_phase: "design",
  agent_id: "{feature-id}-design-leader"
)
```

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
- **Store each ADR in Unimatrix** immediately after writing the file — `context_store(category: "decision", topic: "{feature-id}", feature_cycle: "{feature-id}", title: "{ADR title}", tags: ["adr", "{feature-id}"])` — so delivery agents can retrieve decisions via search without reading every ADR file

**uni-specification → Specification** (`specification/SPECIFICATION.md`)

- Functional and non-functional requirements
- User workflows and use cases
- Acceptance criteria with verification methods
- Domain models and ubiquitous language
- Constraints and dependencies

Each specialist receives:
1. `Your agent ID: {feature-id}-agent-N-{role}`
2. Path to approved SCOPE.md
3. Path to SCOPE-RISK-ASSESSMENT.md (from Phase 1b)
4. Task description

```
# Spawn both in ONE message:
Task(subagent_type: "uni-architect", prompt: "Your agent ID: {id}-agent-1-architect
    ...
    Read scope risk assessment: product/features/{id}/SCOPE-RISK-ASSESSMENT.md
    Address SR-XX risks in your architecture decisions where applicable.

    After writing each ADR file, store it in Unimatrix:
    context_store(category: 'decision', topic: '{feature-id}', feature_cycle: '{feature-id}',
      title: '{ADR title}', tags: ['adr', '{feature-id}'], content: '{full ADR content}')
    ...")
Task(subagent_type: "uni-specification", prompt: "Your agent ID: {id}-agent-2-spec
    ...
    Read scope risk assessment: product/features/{id}/SCOPE-RISK-ASSESSMENT.md
    Consider SR-XX risks when defining constraints and acceptance criteria. ...")
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
    MODE: architecture-risk
    ...
    Read these artifacts for context:
    - SCOPE.md: product/features/{id}/SCOPE.md
    - Architecture: product/features/{id}/architecture/ARCHITECTURE.md
    - ADRs: {list ADR file paths from architect's return}
    - Specification: product/features/{id}/specification/SPECIFICATION.md
    - Scope Risk Assessment: product/features/{id}/SCOPE-RISK-ASSESSMENT.md

    Use the architecture (component boundaries, integration points, ADRs)
    and specification (acceptance criteria, domain models, constraints)
    to inform your risk analysis. Identify risks that are specific to
    the designed architecture — not generic risks.

    Trace each SR-XX scope risk in the Scope Risk Traceability table."
)
```

Wait for the risk strategist to complete before proceeding to Phase 2b.

```
context_cycle(
  type: "phase-end",
  topic: "{feature-id}",
  phase: "design",
  outcome: "Architecture, specification, and risk strategy complete.",
  next_phase: "design-review",
  agent_id: "{feature-id}-design-leader"
)
```

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
    Read the scope risk assessment: product/features/{id}/SCOPE-RISK-ASSESSMENT.md

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
    - product/features/{id}/SCOPE-RISK-ASSESSMENT.md
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

```
context_cycle(
  type: "phase-end",
  topic: "{feature-id}",
  phase: "design-review",
  outcome: "Vision aligned. Synthesis complete.",
  next_phase: "spec",
  agent_id: "{feature-id}-design-leader"
)
```

Then returns to the human:

```
SESSION 1 COMPLETE — Design artifacts ready for review.

GH Issue: {URL}

Artifacts (untracked — git begins in Session 2):
- product/features/{feature-id}/SCOPE.md
- product/features/{feature-id}/SCOPE-RISK-ASSESSMENT.md
- product/features/{feature-id}/architecture/ARCHITECTURE.md
- product/features/{feature-id}/specification/SPECIFICATION.md
- product/features/{feature-id}/RISK-TEST-STRATEGY.md
- product/features/{feature-id}/ALIGNMENT-REPORT.md
- product/features/{feature-id}/IMPLEMENTATION-BRIEF.md
- product/features/{feature-id}/ACCEPTANCE-MAP.md

Vision Alignment: {summary}
Variances requiring approval: {list or "none"}
Open questions: {list or "none"}

Human action required: Review design artifacts. Then start Session 2 to deliver.
```

**Session 1 ends here.** No branch, no commit, no PR — artifacts sit untracked in `product/features/{feature-id}/` until Session 2 picks them up.

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
DESIGN LEADER (you):
  Init:       context_cycle(type: "start", topic: "{feature-id}", next_phase: "scope", agent_id: "{feature-id}-design-leader")
  Phase 1:    Task(uni-researcher) — scope exploration with human
              ...human approves SCOPE.md...
  Phase 1b:   Task(uni-risk-strategist, MODE: scope-risk) — scope risk assessment
              ...wait...
              context_cycle(type: "phase-end", phase: "scope", outcome: "...", next_phase: "design", ...)
  Phase 2a:   Task(uni-architect) + Task(uni-specification) — parallel, ONE message
              ...wait for both...
  Phase 2a+:  Task(uni-risk-strategist, MODE: architecture-risk) — receives arch + spec + scope risks
              ...wait...
              context_cycle(type: "phase-end", phase: "design", outcome: "...", next_phase: "design-review", ...)
  Phase 2b:   Task(uni-vision-guardian) — alignment check
  Phase 2c:   Task(uni-synthesizer) — brief + maps + GH Issue (fresh context)
  Phase 2d:   context_cycle(type: "phase-end", phase: "design-review", outcome: "...", next_phase: "spec", ...) — SESSION 1 ENDS
              return artifacts to human (no git ops — artifacts are untracked)
```

---

## Outcome Recording

After Phase 2d, record the phase transition (do NOT call `stop` — the cycle remains open for the delivery session):

```
context_cycle(
  type: "phase-end",
  topic: "{feature-id}",
  phase: "design-review",
  outcome: "Vision aligned. Synthesis complete.",
  next_phase: "spec",
  agent_id: "{feature-id}-design-leader"
)
```

The cycle is closed by `context_cycle(type: "stop")` at the end of Session 2 (delivery). If delivery never occurs, the cycle will be drained by GC or `context_status(maintain: true)`.
