# Planning Swarm Protocol

Triggers on: specification, pseudocode, architecture, design, research, scope, roadmap, SPARC S/P/A phases.

---

## Execution Model

Planning swarms use **coordinator delegation**: the primary agent spawns `ndp-scrum-master` as the single coordinator. The scrum-master spawns planning agents, runs vision alignment, generates the implementation brief, and creates the GH Issue.

```
Primary Agent                    ndp-scrum-master                 Planning Agents
─────────────                    ────────────────                 ───────────────
get-pattern
read SCOPE.md
spawn scrum-master ──────────►   read protocol + SCOPE.md
                                 swarm init
                                 TaskCreate (all tasks)
                                 seed shared memory
                                 spawn agents ────────────────►  produce SPARC artifacts
                                 ◄────────────────────────────── return artifact paths
                                 spawn vision guardian
                                 generate IMPLEMENTATION-BRIEF.md
                                 gh issue create
◄──────────────────────────────  return summary
present variances to user
reflexion
save-pattern
```


### Concurrency Rules

Each message batches ALL related operations of the same type:

- ALWAYS batch ALL TaskCreate calls in ONE message
- ALWAYS spawn all agents WITHIN each wave in ONE message via Task tool
- ALWAYS batch ALL file reads/writes/edits in ONE message
- ALWAYS batch ALL Bash commands in ONE message

### Planning Rules

- Output goes to `product/features/{feature-id}/{phase}/` ONLY
- NO code changes. NO file edits outside `product/features/`
- NO launching implementation agents (ndp-rust-dev, sparc-coder)
- Each planning agent gets: SCOPE.md + relevant existing SPARC artifacts + relevant stored patterns
- Agents return: artifact paths + key decisions + open questions (NOT full file contents)

---

## Flow: 4 Phases

Note which pattern IDs were returned for reflexion later.

Read `product/features/{feature-id}/SCOPE.md` — this defines what the planning swarm must produce.

#### Scope Pre-Check (REQUIRED)

Before spawning the coordinator, perform a quick alignment scan of SCOPE.md against `docs/product-vision/feature-roadmap.md`. Check the alignment principles at a surface level:

If any red flags are found, present them to the user BEFORE spawning the planning swarm. This prevents wasting a full planning cycle on a misaligned scope.

### Phase 2: Delegation (primary agent)

Spawn `ndp-scrum-master` with the full context needed to run the planning swarm. ONE Task call.

```
Task(
  subagent_type: "ndp-scrum-master",
  prompt: "You are coordinating the planning swarm for {feature-id}.

    Read the planning protocol: .claude/protocols/planning-protocol.md
    Read the scope: product/features/{feature-id}/SCOPE.md

    Pattern IDs from get-pattern: {list IDs}
    Feature namespace: {feature-id}

    Execute the planning swarm: init → define tasks → spawn planning agents →
    vision alignment → generate brief → create GH Issue.
    Return: artifacts produced, key decisions, open questions, GH Issue URL,
    and any vision alignment variances requiring user approval."
)
```

After spawning: tell the user that the scrum-master is coordinating, then STOP.

### Phase 3: Swarm Execution (ndp-scrum-master)

The scrum-master executes the following steps autonomously.



#### Step 3b: Definition (batch with Step 3a)

Define ALL tasks in the SAME message as Step 3a. Batch all TaskCreate calls together:

```
# Wave 1 — parallel (no dependencies)
TaskCreate("Specification artifact", "Produce SPECIFICATION.md for {feature}", "Writing specification")
TaskCreate("Task decomposition", "Produce TASK-DECOMPOSITION.md for {feature}", "Decomposing tasks")
TaskCreate("Architecture ADRs", "Produce ARCHITECTURE.md with codebase-consulted ADRs for {feature}", "Designing architecture")

# Wave 2 — parallel, BLOCKED BY Wave 1 (spec + arch must complete first)
TaskCreate("Pseudocode (per-component)", "Produce pseudocode/OVERVIEW.md + per-component pseudocode files for {feature}", "Writing component pseudocode")
TaskCreate("Test plan (per-component)", "Produce test-plan/OVERVIEW.md + per-component test plan files for {feature}", "Writing test plans")

# Wave 3 — sequential, BLOCKED BY Wave 2
TaskCreate("Vision alignment", "Produce ALIGNMENT-REPORT.md for {feature}", "Checking alignment")
TaskCreate("Implementation brief", "Produce IMPLEMENTATION-BRIEF.md for {feature}", "Generating brief")
TaskCreate("GH Issue creation", "Create GH Issue from brief", "Creating GH Issue")
```

Set task dependencies with TaskUpdate after creation:
- Wave 2 tasks (pseudocode, test-plan) are `addBlockedBy` Wave 1 tasks (spec, arch)
- Wave 3 tasks (alignment, brief, issue) are `addBlockedBy` Wave 2 tasks (pseudocode, test-plan)

#### Step 3c: Agent Spawning (3 Waves)

Agents are spawned in three waves. Each wave must complete before the next begins. This is required because pseudocode and test plans depend on architecture output (exact view names, column types, integration surfaces).

**Pre-spawn checklist** (verify before Wave 1):
- [ ] Coordination layer initialized
- [ ] Agents registered
- [ ] Tasks defined with wave dependencies set
- [ ] Shared context seeded with feature info
- [ ] SCOPE.md read
- [ ] All agents visible in coordination layer

Agent types for planning: `ndp-architect`, `ndp-specification`, `ndp-pseudocode`, `ndp-tester`, `ndp-synthesizer`

Do NOT spawn: `ndp-rust-dev`, `coder`, `sparc-coder`.

Each agent prompt MUST include:
1. `Your agent ID: {feature}-agent-N-{role}` — activates the Swarm Coordination block in agent definitions
2. Task description (2-3 sentences)
3. Specific SPARC phase to produce
4. The SCOPE.md path

##### Wave 1: Specification + Architecture (parallel, ONE message)

Spawn in ONE message:

- **Specification agent** (`ndp-specification`): produces `specification/SPECIFICATION.md` and `specification/TASK-DECOMPOSITION.md`
- **Architecture agent** (`ndp-architect`): performs codebase consultation, produces `architecture/ARCHITECTURE.md` with ADRs, integration surface details. **The architect stores each ADR in the pattern store via `/save-pattern` before returning** (architect is the ADR authority — see ndp-architect agent definition). The architect returns ADR pattern IDs in its completion message.

**Architecture agent (ndp-architect) MUST produce individual ADRs** in `product/features/{feature-id}/architecture/ARCHITECTURE.md` using this format:

```markdown
## ADR-NNN: {Title}

### Context
{Why this decision is needed — the forces at play}

### Decision
{What was decided — concrete implementation approach with code examples}

### Consequences
{Tradeoffs — what this enables, what it costs, what it rules out}
```

Each ADR must cover a distinct architectural choice (not a grab-bag). Good ADR scoping: one decision per ADR, with cross-references between related ADRs.

Wait for BOTH Wave 1 agents to complete before proceeding to Wave 2.

##### Wave 2: Pseudocode + Test Plan (parallel, ONE message, AFTER Wave 1)

Spawn in ONE message:

- **Pseudocode agent** (`ndp-pseudocode`): reads spec + architecture output, produces per-component pseudocode files:
  ```
  pseudocode/
    OVERVIEW.md           -- how components interact, data flow between them
    {component-1}.md      -- e.g., ndp-intelligence.md, ndp-lib.md, deploy-sh.md
    {component-2}.md
  ```
  Components map to cargo workspace members and deployment artifacts (e.g., ndp-intelligence, ndp-lib, air-quality-app, deploy-sh, ndp-cli). The pseudocode agent determines which components the feature touches from the specification.

- **Test plan agent** (`ndp-tester`): reads spec + architecture output, produces per-component test plan files:
  ```
  test-plan/
    OVERVIEW.md           -- overall test strategy, integration surface, testbed design
    {component-1}.md      -- component-specific test expectations, assertions
    {component-2}.md
  ```
  The test plan agent identifies integration surfaces from the architecture and writes per-component test expectations including unit tests, integration tests, and validation commands.

Each Wave 2 agent prompt MUST additionally include:
5. Paths to Wave 1 outputs: `specification/SPECIFICATION.md` and `architecture/ARCHITECTURE.md`
6. Instruction to read those artifacts before producing output

Wait for BOTH Wave 2 agents to complete before proceeding to Wave 3.

##### Wave 3: Vision Alignment, Brief, GH Issue, Validation (sequential, AFTER Wave 2)

Wave 3 steps run sequentially (each depends on the previous). See Steps 3d through 3h below.

#### Step 3d: Vision Alignment

After planning agents complete, spawn `ndp-vision-guardian`:

```
"Read product/vision/ALIGNMENT-CRITERIA.md and the SPARC artifacts at
 product/features/{feature-id}/. Produce ALIGNMENT-REPORT.md.
 Flag any variances requiring user approval."
```

Save to `product/features/{feature-id}/ALIGNMENT-REPORT.md`.

Include variances in the return summary. The primary agent will present them to the user.

#### Step 3e: Spawn Synthesizer (brief compilation + GH Issue)

ADR storage is handled by the architect in Wave 1 (see architect agent definition). The architect returns ADR pattern IDs in its completion message. Pass these to the synthesizer.

Spawn `ndp-synthesizer` with all SPARC artifact paths and the architect's ADR pattern IDs:

```
Task(
  subagent_type: "ndp-synthesizer",
  prompt: "You are compiling the implementation brief for {feature-id}.
    Your agent ID: {feature-id}-synthesizer

    Read these SPARC artifacts:
    - product/features/{id}/SCOPE.md
    - product/features/{id}/specification/SPECIFICATION.md
    - product/features/{id}/specification/TASK-DECOMPOSITION.md
    - product/features/{id}/architecture/ARCHITECTURE.md
    - product/features/{id}/pseudocode/OVERVIEW.md
    - product/features/{id}/pseudocode/{component-1}.md
    - product/features/{id}/pseudocode/{component-2}.md
    - product/features/{id}/test-plan/OVERVIEW.md
    - product/features/{id}/test-plan/{component-1}.md
    - product/features/{id}/test-plan/{component-2}.md
    - product/features/{id}/ALIGNMENT-REPORT.md

    ADR pattern IDs from architect: {list from architect's return}
    Vision variances: {from vision guardian's return}

    Produce: IMPLEMENTATION-BRIEF.md, ACCEPTANCE-MAP.md, LAUNCH-PROMPT.md, GH Issue.
    Return: file paths + GH Issue URL."
)
```

The synthesizer gets a fresh context window — it reads artifacts directly for higher quality synthesis. See `ndp-synthesizer` agent definition for deliverable specifications.

#### Step 3h: Validate Planning Artifacts (spawn ndp-validator)

Spawn `ndp-validator` as a dedicated agent. Do NOT run validation inline.

```
Task(
  subagent_type: "ndp-validator",
  prompt: "You are validating the planning swarm for {feature-id}.

    Swarm type: planning
    Feature: {feature-id}

    Read your agent definition: .claude/agents/ndp/ndp-validator.md
    Run the full /validate-plan skill (5 checks).
    Write glass box report.
    Record ALL trust entries in the pattern system.
    Return: PASS/WARN/FAIL, report path, confidence score, issues."
)
```

The validator checks artifact existence, AC coverage, ADR pattern IDs, stale references, and internal consistency. See `.claude/agents/ndp/ndp-validator.md` for the full procedure.

**Expected artifacts for existence check:**

| Artifact | Path |
|----------|------|
| Specification | product/features/{feature-id}/specification/SPECIFICATION.md |
| Architecture | product/features/{feature-id}/architecture/ARCHITECTURE.md |
| Pseudocode Overview | product/features/{feature-id}/pseudocode/OVERVIEW.md |
| Pseudocode Components | product/features/{feature-id}/pseudocode/{component}.md (1+ files) |
| Test Plan Overview | product/features/{feature-id}/test-plan/OVERVIEW.md |
| Test Plan Components | product/features/{feature-id}/test-plan/{component}.md (1+ files) |
| Testbed | product/features/{feature-id}/testbed/ (if qualifying feature) |

**Do NOT proceed to Phase 4 until the validator returns.** If the validator returns FAIL, fix issues before returning to the primary agent.

### Phase 4: Completion (primary agent)

After ndp-scrum-master returns:

1. Review: artifacts produced, key decisions, open questions, GH Issue URL
2. Present vision alignment variances to user (if any require approval)
3. Record learning:
   ```
   /reflexion — record pattern effectiveness (per pattern used, referencing IDs)
   /save-pattern — store new discoveries (if any)
   ```

---

## Quick Reference: Message Map

```
PRIMARY AGENT:
  Message 1:  /get-pattern + Read SCOPE.md
  Message 2:  Task(ndp-scrum-master) — delegate planning swarm
  ...wait...
  Message 3:  Review results + present variances + /reflexion + /save-pattern

NDP-SCRUM-MASTER (internal):
  Step 3a:  Initialize coordination: register agents + seed shared context
  Step 3b:  TaskCreate (batch ALL with wave deps) — in SAME message as 3a
  Step 3c:  Wave 1: Task(ndp-specification) + Task(ndp-architect) — parallel, ONE message
            (architect stores ADRs via /save-pattern, returns pattern IDs)
            ...wait for Wave 1...
            Wave 2: Task(ndp-pseudocode) + Task(ndp-tester) — parallel, ONE message
            ...wait for Wave 2...
            Wave 3 (Steps 3d-3f, sequential):
  Step 3d:  Task(ndp-vision-guardian) — alignment check
  Step 3e:  Task(ndp-synthesizer) — brief + maps + GH Issue (fresh context, reads all artifacts)
  Step 3f:  Task(ndp-validator) — 5-check planning validation + trust recording
```

---

## Agent Context Budget

Each spawned planning agent should receive:
- Task description (2-3 sentences)
- SCOPE.md path (agents read it themselves)
- Specific file paths to read
- Relevant pattern IDs from the pattern store

Do NOT paste full spec documents, source files, or cargo output into planning agent prompts.

---

## Persistence and Coordination

Unimatrix provides a unified persistence and coordination layer. Patterns, conventions, and architectural decisions are stored via `/get-pattern`, `/save-pattern`, and `/reflexion`. Agent coordination, status tracking, and shared context are managed through the coordination layer initialized in Step 3a. There is no need to manage separate memory systems — the platform handles persistence and session state as a single integrated system.
