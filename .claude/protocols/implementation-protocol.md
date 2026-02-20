# Implementation Swarm Protocol

Triggers on: implement, TDD, build, code, fix, refactor, migrate, SPARC R/C phases.

---

## Execution Model

Implementation swarms use **coordinator delegation**: the primary agent spawns `ndp-scrum-master` as the single swarm coordinator. The scrum-master then spawns implementation agents, monitors results, detects drift, runs validation, and updates the GH Issue.

```
Primary Agent                    ndp-scrum-master                 Implementation Agents
─────────────                    ────────────────                 ─────────────────────
get-pattern
read brief
spawn scrum-master ──────────►   read protocol + brief
                                 swarm init
                                 TaskCreate (all tasks)
                                 seed shared memory
                                 spawn agents (wave) ──────────► execute tasks
                                 ◄──────────────────────────────  return results
                                 drift check
                                 validate
                                 gh issue comment
◄──────────────────────────────  return summary
reflexion
save-pattern
```

Do NOT use TeamCreate — swarms are coordinator-driven via Task tool spawn-and-wait.

### Concurrency Rules

Each message batches ALL related operations of the same type:

- ALWAYS batch ALL TaskCreate calls in ONE message
- ALWAYS spawn ALL agents in ONE message via Task tool
- ALWAYS batch ALL file reads/writes/edits in ONE message
- ALWAYS batch ALL Bash commands in ONE message
- ALWAYS batch ALL coordination operations in ONE message

### Agent Rules

- Agents return: file paths + test pass/fail + issues (NOT file contents)
- Read the IMPLEMENTATION BRIEF from the GH Issue body — not the full spec tree
- GH Issue is the single source of truth — all progress updates go to the issue via `gh issue comment`
- Do NOT write progress to markdown files (STATUS.md, completion reports, etc.)
- Max 2 validation fix iterations to protect context window
- Cargo output truncated to first error + summary line

---

## Flow: 4 Phases

**Read the implementation brief from the GitHub Issue body** (`gh issue view <N> --json body`). The GH Issue is the single source of truth.

If the GH Issue body does not contain a brief, check `product/features/{feature-id}/IMPLEMENTATION-BRIEF.md` as fallback. If neither exists, ask the user: "No implementation brief found. Should I read the full SPARC specs, or generate a brief first?"

Worker agents SHOULD read their component-specific SPARC artifacts. The scrum master's spawn prompt provides the specific file paths. Workers read:
1. IMPLEMENTATION-BRIEF.md — orchestration context, component map, constraints
2. architecture/ARCHITECTURE.md — ADRs, integration surface findings
3. pseudocode/OVERVIEW.md — how their component connects to others
4. pseudocode/{component}.md — implementation detail for their specific component
5. test-plan/OVERVIEW.md — overall test strategy
6. test-plan/{component}.md — test expectations for their component

The scrum master determines which component files each agent needs based on the Component Map in the brief.

### Phase 2: Delegation (primary agent)

Spawn `ndp-scrum-master` with the full context needed to run the swarm. ONE Task call.

```
Task(
  subagent_type: "ndp-scrum-master",
  prompt: "You are coordinating the implementation swarm for {feature-id}.

    Read the implementation protocol: .claude/protocols/implementation-protocol.md
    Read the brief: {GH Issue number or IMPLEMENTATION-BRIEF.md path}

    Pattern IDs from get-pattern: {list IDs}
    Feature namespace: {feature-id}

    Execute the swarm: init → define tasks → spawn agents → drift check →
    validate → update GH Issue.
    Return: files changed, test results, validation result, issues encountered."
)
```

After spawning: tell the user that the scrum-master is coordinating, then STOP. Wait for the scrum-master to return.

### Phase 3: Swarm Execution (ndp-scrum-master)

The scrum-master executes the following steps autonomously. These details are here so the scrum-master can read this file and follow them.

#### Step 3a: Initialize Coordination Layer

Three required operations (batch in ONE message):

1. **Register agents** in the coordination layer — one entry per planned implementation agent.
2. **Seed shared context** with the task description, goals, and constraints so all agents share a common understanding.
3. **Verify readiness** — confirm that agent registration and context seeding succeeded before proceeding.

#### Step 3b: Definition (batch with Step 3a)

Define ALL tasks in the SAME message as Step 3a. Batch aggressively.

Retrieve the Level-1 summary for the feature from the compiled spec artifacts.

Then batch task creation with the coordination calls from Step 3a:
```
TaskCreate("Task 1 subject", "Task 1 description", "Active form 1")
TaskCreate("Task 2 subject", "Task 2 description", "Active form 2")
... (5-10+ tasks, with dependencies set via TaskUpdate)
```

Set task dependencies with TaskUpdate after creation.

#### Step 3c: Agent Spawning

Spawn ALL agents for the current wave in ONE message (parallel).

**Pre-spawn checklist** (verify before ANY Task call):
- [ ] Agents registered in the coordination layer
- [ ] Tasks defined (TaskCreate completed)
- [ ] Shared context seeded
- [ ] Brief read
- [ ] `cargo build --workspace` passes (abort if fails — do not spawn agents on a broken workspace)

If ANY item is unchecked, STOP. Complete the missing step first.

Agent types for implementation: `ndp-rust-dev`, `ndp-tester`, `ndp-timescale-dev`, `ndp-parquet-dev`

Each agent prompt MUST include:
1. `Your agent ID: {feature}-agent-N-{role}` — activates the Swarm Coordination block in agent definitions
2. **Level-1 summary** from the compiled spec
3. Task description (2-3 sentences)
4. Specific file paths from the brief's "Files to Create/Modify" section
5. **Component-specific SPARC artifact paths** (see template below)
6. Instructions to retrieve relevant ADRs before implementing — use `/get-pattern`

The Level-1 summary gives agents the objective (WHY), ADR list with pattern IDs (WHAT CONSTRAINS THEM), constraints, and scope exclusions (WHAT TO AVOID). Without it, agents have tunnel vision on their narrow subtask and drift from architectural decisions.

**Agent spawn prompt template:**
```
Task(
  subagent_type: "{ndp-agent-type}",
  prompt: "You are implementing {subtask} for {feature-id}.
    Your agent ID: {feature-id}-agent-N-{role}

    Read these files before starting:
    - product/features/{feature-id}/IMPLEMENTATION-BRIEF.md
    - product/features/{feature-id}/architecture/ARCHITECTURE.md
    - product/features/{feature-id}/pseudocode/OVERVIEW.md
    - product/features/{feature-id}/pseudocode/{component}.md
    - product/features/{feature-id}/test-plan/OVERVIEW.md
    - product/features/{feature-id}/test-plan/{component}.md

    YOUR TASK: {description}
    Files to create/modify: {paths}

    RETURN FORMAT (required):
    1. Files modified: [paths]
    2. Tests: pass/fail
    3. Issues: [blockers]

)
```

The scrum master populates `{component}` from the Component Map in the IMPLEMENTATION-BRIEF.md. If an agent's work spans multiple components, include ALL relevant component files.

#### Step 3c.5: Per-Wave Acceptance Check

After agents return from each wave, map completed tasks to acceptance criteria:

1. Read the ACCEPTANCE-MAP.md for the feature
2. For each completed task, identify which AC-IDs it covers
3. Run the verification method for each covered AC (file-check, grep, shell, test)
4. Update AC status: PENDING -> IN_PROGRESS or PASS
5. Report: "Wave N: X/Y ACs verified (list AC-IDs)"

If an AC fails verification, either spawn a fix agent or flag it in the wave summary.

#### Step 3d: Drift Check

After agents return, check results against the brief:

| Check | Action |
|-------|--------|
| Files modified outside scope | Flag in summary |
| TODOs, stubs, `unimplemented!()` left | Spawn fix agent |
| Acceptance criteria missed | Spawn gap-fill agent |
| Test count decreased | Investigate before next wave |

Max 2 corrective iterations per wave. If drift persists, return to primary agent.

#### Step 3e: Validation (spawn ndp-validator)

Spawn `ndp-validator` as a dedicated agent. Do NOT run validation inline.

```
Task(
  subagent_type: "ndp-validator",
  prompt: "You are validating the implementation swarm for {feature-id}.

    Swarm type: implementation
    Feature: {feature-id}
    Wave: {N}

    Read your agent definition: .claude/agents/ndp/ndp-validator.md
    Run the full /validate skill (4-tier).
    Write glass box report.
    Record ALL trust entries in the pattern system.
    Return: PASS/WARN/FAIL, report path, confidence score, issues."
)
```

The validator runs Tiers 1-4 (compilation, process adherence, spec compliance, risk), writes the glass box report, and records trust entries in the pattern system. See `.claude/agents/ndp/ndp-validator.md` for the full procedure.

**Do NOT proceed to Step 3f until the validator returns.** If the validator returns FAIL, spawn a fix agent (max 2 iterations) then re-spawn the validator.

#### Step 3f: GH Issue Update

Post results as an issue comment:
```bash
gh issue comment <N> --body "## Wave X Complete
- Files: [list paths]
- Tests: X passed, Y new
- Validation: PASS/WARN/FAIL
- Issues: [if any]"
```


#### Multi-Wave Features

For features with sequential waves:
- Spawn ALL agents within a wave in ONE message (parallel)
- Wait for the wave to complete
- Run drift check (Step 3d)
- Store wave results in shared context
- Spawn the next wave's agents in a NEW message
- Repeat until complete
- Post `gh issue comment` after each wave

Do NOT spawn agents from different waves in the same message if Wave N+1 depends on Wave N.

### Phase 4: Completion (primary agent)

After ndp-scrum-master returns:

1. Review the summary (files changed, tests, validation result)

---


---

## Agent Context Budget

Each spawned implementation agent should receive:
- Task description (2-3 sentences)
- Coordination context for the swarm
- Specific file paths to read and modify
- Component-specific SPARC artifact paths (brief, architecture, pseudocode/{component}, test-plan/{component})
- Relevant pattern IDs from the pattern store (not full pattern text)

Do NOT paste: full spec documents, full source files, full cargo output, or implementation brief contents into agent prompts. Agents read files themselves. The scrum master routes ONLY the component-specific paths each agent needs — not every pseudocode or test-plan file in the feature.

---

## Cargo Output Truncation

Always truncate cargo output to prevent context bloat:
```bash
# Build: first error + summary
cargo build --workspace 2>&1 | grep -A5 "^error" | head -20
cargo build --workspace 2>&1 | tail -3

# Test: summary only
cargo test --workspace 2>&1 | tail -30

# Clippy: first warnings only
cargo clippy --workspace -- -D warnings 2>&1 | head -30
```

---
