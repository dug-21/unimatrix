# nan-003: Unimatrix Onboarding Skills — Architecture

## System Overview

nan-003 delivers two skill files that bootstrap Unimatrix adoption in new repos. Both skills are markdown instruction documents read and executed by Claude; they are not compiled code. Their quality depends on model instruction-following fidelity, which is the primary platform constraint.

**Position in the system**: nan-003 sits at the entry point of the three-layer chain established by alc-001 research:

```
CLAUDE.md awareness → skill invocation → agent behavior
```

`/unimatrix-init` establishes the first link (CLAUDE.md awareness). `/unimatrix-seed` populates Unimatrix with enough knowledge to make the second and third links meaningful from day one.

**Relationship to existing components**:
- Skills produced here live in `.claude/skills/` alongside the 11 existing skills
- Seed entries are stored via the MCP server's `context_store` tool (vnc-002/vnc-003)
- Pre-flight check uses `context_status` (vnc-003)
- Server-side dedup (0.92 cosine, vnc-009) operates on seed entries automatically
- `uni-init` agent (alc-001) is complementary, not competing — it reads `.claude/agents/` and protocol files; `/unimatrix-seed` reads arbitrary repo structure

---

## Component Breakdown

### Component 1: `/unimatrix-init` Skill

**File**: `.claude/skills/unimatrix-init/SKILL.md`

**Responsibility**: Deterministic, one-time setup of CLAUDE.md and agent orientation for a target repo.

**Phases** (executed in strict order):

```
Phase 1: Pre-flight check
  → Read CLAUDE.md (if exists)
  → Search for sentinel marker
  → If found: print "already initialized", STOP

Phase 2: Agent scan (read-only)
  → Glob .claude/agents/**/*.md
  → Per agent: check for context_briefing, outcome reporting, unimatrix-* skill refs
  → Produce terminal recommendation table (no file write)

Phase 3: CLAUDE.md append
  → Compose Unimatrix block from CLAUDE.md block template
  → Append to existing CLAUDE.md OR create new file
  → Confirm write

[--dry-run mode: Phase 2 + print Phase 3 output, no writes]
```

**Inputs**: optional `--dry-run` flag
**Outputs**: mutated CLAUDE.md + terminal recommendation report

---

### Component 2: `/unimatrix-seed` Skill

**File**: `.claude/skills/unimatrix-seed/SKILL.md`

**Responsibility**: Human-directed, conversational knowledge seeding. Populates Unimatrix with foundational entries for the target repo. Gated by human approval at each level to prevent quality failures.

**State machine** (see detail in Component 5 below):

```
Entry → Pre-flight → Existing-check → Level 0 → Gate-0 → [Level 1] → Gate-1 → [Level 2] → Done
```

**Inputs**: none (conversational)
**Outputs**: Unimatrix entries stored via `context_store`

---

### Component 3: CLAUDE.md Block Template

The block appended by `/unimatrix-init`. Self-contained for a developer with no prior Unimatrix knowledge.

**Structure**:
```markdown
<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->
## Unimatrix

Knowledge engine (MCP server). Makes agent expertise searchable, trustworthy, and self-improving.

### Available Skills

| Skill | When to Use |
|-------|-------------|
| `/unimatrix-init` | First-time setup: wire CLAUDE.md and get agent recommendations |
| `/unimatrix-seed` | Populate Unimatrix with foundational repo knowledge |

### Knowledge Categories

| Category | What Goes Here |
|----------|---------------|
| `decision` | Architectural decisions (ADRs) — use `/store-adr` |
| `pattern` | Reusable implementation patterns — use `/store-pattern` |
| `procedure` | Step-by-step workflows — use `/store-procedure` |
| `convention` | Project-wide coding/process standards |
| `lesson-learned` | Post-failure takeaways — use `/store-lesson` |

### When to Invoke

- Before implementing anything new → search knowledge base
- After each architectural decision → store ADR
- After each shipped feature → run retrospective
- When a technique evolves → update procedure
<!-- end unimatrix-init v1 -->
```

**Sentinel design** (ADR-002): versioned open/close comments enable future `--update` to locate and replace the entire block.

---

### Component 4: Agent Scan Algorithm

Used in `/unimatrix-init` Phase 2. Read-only; no files modified.

**Input**: path to `.claude/agents/` directory (defaults to CWD)

**Algorithm**:
```
1. Glob: .claude/agents/**/*.md
2. If no agents found: skip scan, note "no agents found"
3. For each agent file:
   a. Read file content
   b. Check presence of:
      - context_briefing call (or context_briefing reference)
      - outcome reporting (context_store with category: "outcome", or /record-outcome ref)
      - any unimatrix-* skill reference (/unimatrix-init, /unimatrix-seed, /unimatrix-*)
   c. Emit row: [agent name | missing patterns | concrete suggested additions]
4. Print recommendation table to terminal
5. If all agents have all three patterns: print "All agents fully wired ✓"
```

**Output format** (terminal only):
```
Agent Orientation Report
========================
Agent                    | Missing                   | Suggested Addition
-------------------------|---------------------------|------------------------------------------
uni-rust-dev             | context_briefing          | Add: call context_briefing before coding
uni-tester               | outcome reporting         | Add: /record-outcome at session end
custom-dev               | all three                 | See: /unimatrix-init quickstart
```

**Design rationale** (ADR-004): terminal-only output — recommendation files become stale immediately as agents evolve; re-running the scan is free.

---

### Component 5: Seed State Machine

The core behavioral model for `/unimatrix-seed`. Models `/unimatrix-seed` as a discrete state machine with hard STOP gates between each level. Each gate is a yes/no human decision; no default progression.

```
States:
  PREFLIGHT      → calls context_status; fails hard if MCP unavailable (ADR-003)
  EXISTING_CHECK → calls context_search for existing seed entries; warns if found
  LEVEL_0        → auto-runs: README + manifests + top-level structure
  GATE_0         → HARD STOP: present proposed entries for batch approval; ask "go deeper?"
  LEVEL_1        → per-category exploration (human selects A/B/C/...)
  GATE_1         → HARD STOP: present per-entry approval; ask "continue to Level 2?"
  LEVEL_2        → deeper exploration within selected Level 1 categories
  GATE_2         → HARD STOP: per-entry approval; done
  DONE           → summary report

Transitions:
  PREFLIGHT      → (pass) → EXISTING_CHECK
  PREFLIGHT      → (fail) → terminate with error
  EXISTING_CHECK → (none found) → LEVEL_0
  EXISTING_CHECK → (found) → warn + ask human: supplement or skip?
  LEVEL_0        → GATE_0
  GATE_0         → (approved + yes deeper) → LEVEL_1
  GATE_0         → (approved + no deeper) → DONE
  GATE_0         → (rejected entries) → revise + re-present OR DONE
  LEVEL_1        → GATE_1
  GATE_1         → (approved + yes deeper) → LEVEL_2
  GATE_1         → (approved + no deeper) → DONE
  GATE_1         → (rejected) → DONE
  LEVEL_2        → GATE_2
  GATE_2         → DONE

Depth limit: Maximum 2 opt-in levels beyond Level 0. No Level 3.
```

**Gate phrasing** (enforces SR-01 mitigation):
Every gate in SKILL.md must use explicit "STOP. Wait for human response before proceeding." language. The model must not advance to the next level until the human has responded to the gate question.

**Level exploration scope**:

| Level | Explores | Approval Mode |
|-------|----------|---------------|
| 0 | README, package manifests, top-level CLAUDE.md, .claude/ structure | Batch (low risk, 2-4 entries) |
| 1 | Module dirs, test dirs, config files, build tooling | Per-entry (higher stakes) |
| 2 | Selected Level 1 categories in greater depth | Per-entry (highest stakes) |

---

### Component 6: Entry Quality Gate

Applied to **all** proposed entries before presenting to human for approval. Mirrors the `/store-pattern` quality gate.

**Required fields** (What/Why/Scope):

| Field | Requirement | Reject if |
|-------|-------------|-----------|
| `what` | One sentence, ≤ 200 chars | Exceeds 200 chars; missing |
| `why` | ≥ 10 chars explaining consequence or motivation | Under 10 chars; missing; tautological |
| `scope` | Where it applies (repo section, module, workflow) | Missing |

**Category assignment rules**:

| Category | Use when |
|----------|----------|
| `convention` | Project-level standards (naming, file layout, process) |
| `pattern` | Reusable architectural/implementation approach |
| `procedure` | Step-by-step workflow specific to this repo |

ADR (`decision`) and `outcome` entries are **excluded from seeding** — they emerge from real feature work, not initial exploration.

**Dedup gate**: Before storing any approved entry, the skill checks if a semantically similar entry already exists (server-side 0.92 cosine dedup catches exact matches; the pre-flight EXISTING_CHECK warns for broader re-run risk).

---

## Component Interactions

```
                    ┌─────────────────────────────────────────┐
                    │           Target Repo                    │
                    │  .claude/agents/**/*.md  ──────────────► │
                    │  CLAUDE.md               ──read─────────►│
                    └─────────────────────────────────────────┘
                              │                    ▲
                         [Phase 2]            [Phase 3]
                         Agent Scan          Append Block
                              │                    │
                    ┌─────────▼────────────────────┴──────────┐
                    │         /unimatrix-init Skill            │
                    │  Pre-flight → Agent Scan → CLAUDE Append │
                    └──────────────────────────────────────────┘
                              │
                        [terminal output]
                        recommendation report


                    ┌──────────────────────────────────────────┐
                    │         /unimatrix-seed Skill             │
                    │  PREFLIGHT → EXISTING_CHECK → L0 →       │
                    │  GATE_0 → [L1] → GATE_1 → [L2] → DONE   │
                    └──────────────────────────────────────────┘
                              │                    │
                         [reads]              [stores via]
                              │                    │
                    ┌─────────▼──────┐   ┌─────────▼──────────┐
                    │  Target Repo   │   │  Unimatrix MCP      │
                    │  Files         │   │  context_status     │
                    │  (README, etc) │   │  context_search     │
                    └────────────────┘   │  context_store      │
                                         └────────────────────┘
```

---

## Technology Decisions

All technology choices are driven by the platform constraint: skills are markdown files, not executable code. Claude reads and follows them.

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Skill format | YAML frontmatter + markdown | Platform-native; matches all existing skills |
| State management | Explicit STOP gate phrasing in SKILL.md | Only reliable enforcement mechanism for multi-turn state (ADR-001) |
| Idempotency | Versioned sentinel comment in CLAUDE.md | Cheap to check; version enables future `--update` (ADR-002) |
| MCP availability | context_status pre-flight | Fail fast before exploration wastes context (ADR-003) |
| Recommendation output | Terminal-only (no file write) | Files become stale; re-run is free (ADR-004) |
| CLAUDE.md block scope | unimatrix-* skills only | Entry-point API; existing skills target experienced users (ADR-005) |
| Entry categories | convention/pattern/procedure only | ADR+outcome emerge from feature work, not seeding (ADR-006) |

---

## Integration Points

### MCP Server (Unimatrix)

`/unimatrix-seed` requires the Unimatrix MCP server to be operational. Pre-flight validates this.

### Target Repo File System

Both skills read from the target repo's file system via Claude's Read/Glob tools. `/unimatrix-init` writes to CLAUDE.md.

### Existing Skills

The CLAUDE.md block produced by `/unimatrix-init` does NOT duplicate the existing skill catalog. It lists only `unimatrix-*` skills (the two from this feature). Existing skills (`store-adr`, `retro`, etc.) are for agents already inside the Unimatrix workflow — they appear in agent definition files, not in the onboarding block.

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `context_status()` | MCP tool → system health object | vnc-003 |
| `context_search(query, category?, k?)` | MCP tool → ranked entries | vnc-002 |
| `context_store(title, content, topic, category, tags, agent_id)` | MCP tool → entry ID | vnc-002 |
| Glob `.claude/agents/**/*.md` | File glob → list of paths | Claude Read/Glob |
| Read `CLAUDE.md` | File read → string content | Claude Read |
| Write/Append `CLAUDE.md` | File write → confirmation | Claude Write/Edit |
| Sentinel: `<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->` | String marker in CLAUDE.md | Defined here |
| Sentinel close: `<!-- end unimatrix-init v1 -->` | String marker in CLAUDE.md | Defined here |

---

## Decisions Table

| ADR | Title | Unimatrix ID | Notes |
|-----|-------|-------------|-------|
| ADR-001 | Hard STOP Gates for Seed State Machine | #1090 | SR-01 mitigation |
| ADR-002 | Versioned Sentinel + Head-Check Fallback | #1091 | SR-02 mitigation |
| ADR-003 | context_status Pre-flight for Seed | #1092 | SR-06 mitigation |
| ADR-004 | Terminal-Only Agent Recommendation Output | #1093 | Resolved design decision |
| ADR-005 | CLAUDE.md Block Lists unimatrix-* Skills Only | #1094 | Scope boundary |
| ADR-006 | Seed Entry Categories and Quality Gate | #1095 | Quality failure prevention |

---

## Open Questions

1. **Skills installation mechanism** (SCOPE.md open question): Must the human manually copy skill files to the target repo? Architecture assumes manual copy is the method — nan-004 may automate this. Both skills should include a "Prerequisites" section documenting this requirement explicitly (SR-04 mitigation).

2. **Agent scan check coverage**: The three checks (context_briefing, outcome reporting, unimatrix-* refs) are the minimum viable orientation signals. Should the scan also check for `context_search` usage (knowledge querying before implementation)? Defer to spec writer to confirm check scope.

3. **Seed existing entries threshold**: How many existing entries trigger the "already seeded" warning? One? Five? The pre-flight check needs a concrete threshold. Proposed: warn if ≥ 3 active entries exist in categories convention/pattern/procedure for this topic (or any topic matching the repo name).

4. **CLAUDE.md block update path**: The sentinel version (`v1`) exists for future `/unimatrix-init --update`. What is the update behavior? Replace the block between sentinels? Out of scope for nan-003 — but the sentinel open/close markers are the infrastructure.
