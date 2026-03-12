# nan-003: Unimatrix Onboarding Skills (`/unimatrix-init` + `/unimatrix-seed`)

## Problem Statement

When a new repo adopts Unimatrix, there is no guided path to (1) wire the three-layer chain that makes Claude aware of available skills, and (2) populate a baseline knowledge store that makes `context_briefing`, `context_search`, and `context_lookup` useful from day one.

Without these skills:
- Developers must manually author CLAUDE.md Unimatrix blocks and agent orientation instructions, re-inventing conventions on each new repo.
- The knowledge store starts empty — early `context_briefing` calls return nothing, undermining agent confidence in the tool.
- The `uni-init` brownfield bootstrap (a batch agent for Unimatrix's own agent/protocol files) does not address general-purpose repo seeding: it is scoped to extracting duties/conventions from `.claude/agents/uni/` and protocols, not to exploring arbitrary codebases.

The alc-001 research identified this gap: the three-layer chain (CLAUDE.md awareness → skill invocation → agent behavior) must be explicitly established in each target repo, and the uni-init prototype produced 67 low-quality entries because it was fully automated with no human direction.

## Goals

1. Deliver `/unimatrix-init` — a deterministic skill that appends a self-contained Unimatrix block to `CLAUDE.md` and produces read-only agent orientation recommendations.
2. Deliver `/unimatrix-seed` — a conversational, human-directed skill that explores repo structure, stores foundational knowledge entries, and invites deeper exploration on explicit human opt-in.
3. Ensure `/unimatrix-init` is idempotent — running it twice on a repo does not duplicate the CLAUDE.md block.
4. Establish the `unimatrix-` prefix naming convention for all production skills created going forward (init and seed are the first two under this convention).
5. Prevent the quality failure of the uni-init prototype: human controls seed depth, agent does not over-generate entries automatically.

## Non-Goals

- Installing the Unimatrix binary, ONNX model download, or wiring `settings.json` — those are **nan-004**.
- Modifying existing `.claude/agents/` files in the target repo — recommendations only; no auto-editing.
- Renaming or migrating existing skills (store-adr, retro, etc.) to the `unimatrix-` prefix — those stay as-is.
- Creating or modifying agent definitions for the target repo — the skill produces a printed recommendation report, not file writes.
- Seeding Unimatrix with the target repo's agent/protocol files (that is the `uni-init` agent's job, invoked separately).
- Supporting non-Claude-Code environments or non-MCP transports.
- Deep code analysis (function signatures, type hierarchies, dependency graphs) — high-level structural understanding only.

## Background Research

### Skill File Format
Each skill is a directory under `.claude/skills/{name}/` containing a `SKILL.md` file. YAML frontmatter (`name`, `description`) is followed by markdown. The **directory name** becomes the slash command (`/name`). Therefore `/unimatrix-init` requires a directory `.claude/skills/unimatrix-init/` with `SKILL.md`.

### Existing Skills (11 total, no `unimatrix-` prefix)
`store-adr`, `review-pr`, `retro`, `query-patterns`, `store-pattern`, `store-procedure`, `store-lesson`, `record-outcome`, `knowledge-lookup`, `knowledge-search`, `uni-git`. The `unimatrix-` prefix is a NEW convention for nan-003 skills only — existing skills are unchanged.

### CLAUDE.md Unimatrix Section Pattern
The Unimatrix repo's own `CLAUDE.md` contains a short block:
```
## Unimatrix
Knowledge engine (MCP server). Use it.
- /query-patterns — before designing or implementing, check what exists
- /store-adr — after each architectural decision
- /record-outcome — at the end of every session
...
```
The `/unimatrix-init` CLAUDE.md block follows this pattern but must be self-contained for a repo that has no existing Unimatrix familiarity. It should include: available `unimatrix-*` skill names + one-line descriptions (NOT all skills — only `unimatrix-*` prefixed ones), category conventions (decision/pattern/procedure/convention/lesson-learned), and trigger guidance (when to invoke each skill).

### Idempotency Marker
To prevent double-appending, the appended block should begin with a unique versioned sentinel comment:
`<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->`. Init checks for this marker before appending. The version number enables a future `/unimatrix-init --update` to detect and replace stale blocks.

### uni-init Agent vs `/unimatrix-init` Skill — Naming Collision Risk
`uni-init` (`.claude/agents/uni/uni-init.md`) is a batch agent that bootstraps Unimatrix from `.claude/` agent/protocol files. `/unimatrix-init` (this feature) is a skill that sets up the CLAUDE.md block and produces agent orientation recommendations. These are complementary, not competing — but the names are close enough to cause confusion. The SCOPE.md and skill documentation must clarify the distinction.

### Three-Layer Chain (ASS-011 Confirmed)
From ASS-011 conclusion: Skills stay as `.md` files (platform-native), protocols stay as files, knowledge goes in Unimatrix. This is the correct and settled architecture. `/unimatrix-init` establishes the chain by making Claude aware of skills via CLAUDE.md.

### Agent Orientation Problem
Current Unimatrix agents (`uni-researcher`, `uni-rust-dev`, etc.) reference skills like `/query-patterns`, `/store-adr`, and include `context_briefing` calls. Agents in a target repo (if it has any) likely do NOT do this. The recommendation output of `/unimatrix-init` should give concrete, per-agent suggestions with skill-level examples (e.g., "add `/unimatrix-query-patterns` before implementation"), NOT raw MCP tool calls. Output is terminal-only — not written to a file (files become stale immediately; the scan can be re-run).

### Seed Quality Problem (uni-init Prototype)
The prototype (67 entries, all later deprecated) failed because: (1) automated extraction without human validation produced low-signal entries, (2) entries were created from structured files (agent defs) not from actual codebase understanding. `/unimatrix-seed` addresses this by: requiring explicit human approval for each exploration level, and letting the human decide what categories of knowledge are worth seeding.

### CLAUDE.md May Not Exist
Target repos may have no CLAUDE.md at all. `/unimatrix-init` must handle both: append to existing file, or create a minimal CLAUDE.md with just the Unimatrix block.

### Seed Entry Categories
Foundational knowledge entries from `/unimatrix-seed` are likely `convention` (project-level standards), `pattern` (architectural patterns found in the repo), and `procedure` (how-to for repo-specific workflows). ADRs and outcomes are generated during real feature work, not seeding.

## Proposed Approach

### `/unimatrix-init` Skill Design

**Input**: No required arguments. Optional: `--dry-run` (print what would be written, no changes).

**Phase 1: Pre-flight check**
- Check if CLAUDE.md exists; read it if so.
- Search for the sentinel `<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->`.
- If found: report "already initialized" and stop (idempotency guard).

**Phase 2: Agent scan (read-only)**
- Glob `.claude/agents/**/*.md` in the current directory.
- For each agent file found: read it, check for `context_briefing` usage, outcome reporting, and `unimatrix-*` skill references.
- Produce a printed recommendation table (terminal-only, no file output): agent name | missing patterns | concrete suggested additions with skill-level examples.

**Phase 3: CLAUDE.md append**
- Compose the Unimatrix block (skills listing + category guide + basic usage).
- Append it to CLAUDE.md (or create CLAUDE.md if absent).
- Confirm the write.

**Output**: Confirmation of what was written + agent recommendation report (printed, not saved to file).

### `/unimatrix-seed` Skill Design

**Input**: No required arguments. Conversational — the skill guides the human through levels.

**Level 0 (automatic, no opt-in required)**:
- Read: `README.md`, top-level `CLAUDE.md`, package manifests (`package.json`, `Cargo.toml`, `pyproject.toml`, `go.mod`), `.claude/` structure if present.
- Produce: 2-4 high-level entries: repo purpose, tech stack, project structure.
- Present entries for batch approval (Level 0 is high-level, low risk — batch is the default).
- Store approved entries immediately.
- Present summary + ask: "Want to go deeper? I can explore: [A] module structure, [B] key conventions, [C] build/test workflow."

**Level 1 (per-category opt-in)**:
- Human selects which exploration paths to pursue.
- Agent performs per-category exploration (module dirs, test dirs, config files) and proposes entries before storing.
- Human approves/rejects each proposed entry individually (Level 1+ is deeper, human-directed — individual approval is the default).
- Store approved entries.

**Depth limit**: Maximum 2 opt-in levels beyond Level 0. No unbounded exploration.

**Entry quality rule**: Every entry must pass the What/Why/Scope test before being stored (same quality gate as `/store-pattern`).

## Acceptance Criteria

- AC-01: `/unimatrix-init` appends a Unimatrix block to `CLAUDE.md` containing: (a) a skills table listing only `unimatrix-*` prefixed skills with one-line descriptions, (b) category convention guide (what goes in each category), and (c) usage trigger instructions (when to invoke each skill).
- AC-02: `/unimatrix-init` is idempotent — running it a second time on a repo that already has the versioned sentinel marker produces no changes to `CLAUDE.md` and prints "already initialized."
- AC-03: `/unimatrix-init` handles the case where `CLAUDE.md` does not exist — creates the file with the Unimatrix block.
- AC-04: `/unimatrix-init` scans `.claude/agents/**/*.md` and produces a terminal-only recommendation report (no file written) with concrete, skill-level examples identifying agents missing: `context_briefing` orientation, outcome reporting references, and `unimatrix-*` skill references. No agent files are modified.
- AC-05: `/unimatrix-init` supports `--dry-run` mode — prints what would be written to `CLAUDE.md` and the agent recommendation report without modifying any files.
- AC-06: `/unimatrix-seed` Level 0 automatically reads README, package manifests, and top-level structure without requiring human opt-in, proposes 2-4 high-level foundational entries for batch approval, and stores approved entries.
- AC-07: `/unimatrix-seed` Level 1+ requires explicit human opt-in for each exploration category. The skill presents a menu of exploration options and waits for human selection before proceeding.
- AC-08: `/unimatrix-seed` Level 0 uses batch approval by default (low risk); Level 1+ uses individual entry approval by default (human-directed, higher stakes). Only approved entries are stored.
- AC-09: `/unimatrix-seed` depth is bounded — no more than 2 opt-in levels beyond Level 0 in a single invocation.
- AC-10: Both skills are delivered as `.claude/skills/unimatrix-init/SKILL.md` and `.claude/skills/unimatrix-seed/SKILL.md` following the existing skill file format (YAML frontmatter + markdown).
- AC-11: The CLAUDE.md block appended by `/unimatrix-init` is self-contained — a developer with no prior Unimatrix knowledge can read it and understand what skills are available and when to use them.
- AC-12: `/unimatrix-init` skill documentation clarifies the distinction between `/unimatrix-init` (CLAUDE.md setup + agent recommendations) and the `uni-init` agent (brownfield bootstrap of `.claude/` knowledge extraction).
- AC-13: `/unimatrix-seed` warns if seed entries already exist (via `context_search` check) and offers to supplement rather than re-seed, saving tokens and avoiding near-duplicates.
- AC-14: The sentinel marker includes a version number (`<!-- unimatrix-init v1: ... -->`) to enable future `/unimatrix-init --update` detection of stale blocks.

## Constraints

- Skills are markdown files — they must be physically present in `.claude/skills/` of the target repo. There is no auto-install mechanism (nan-004 scope). This is a hard dependency.
- The MCP server (Unimatrix) must already be wired in the target repo's Claude settings. `/unimatrix-seed` calls `context_store` — if MCP is not available, it fails. Skills should fail gracefully with a clear error message, not silently.
- Skill SKILL.md files cannot execute code — they are instructions for Claude to follow, not scripts. All file reads, writes, and MCP calls are performed by Claude following skill instructions.
- The sentinel marker approach (`<!-- unimatrix-init: ... -->`) only works if the CLAUDE.md file is a markdown file that Claude reads. If the file is non-markdown or excluded, idempotency cannot be guaranteed.
- Server-side dedup (0.92 cosine similarity) in `context_store` prevents exact duplicate seed entries but does not prevent near-duplicate entries if seed is run twice. The skill should warn if `/unimatrix-seed` appears to have been run before (check for existing entries in relevant categories).
- Skills are instructions for the Claude model — their quality depends on the model's ability to follow them. Complex multi-step skills (like `unimatrix-seed`) rely on the model maintaining state across a conversation. This is an inherent platform constraint.

## Resolved Design Decisions

1. **Skills listing**: Only `unimatrix-*` prefixed skills in the CLAUDE.md block. Not a general skill inventory.
2. **Recommendation format**: Terminal output only. No file — files become stale; the scan can be re-run.
3. **Seed re-run behavior**: WARN if entries already exist. Offer to supplement, not re-seed.
4. **Sentinel versioning**: Yes — `<!-- unimatrix-init v1: ... -->`. Cheap to add now, painful to retrofit.
5. **Agent recommendation specificity**: Concrete with skill-level examples (e.g., "add `/unimatrix-query-patterns` before implementation"), NOT raw MCP tool calls.
6. **Seed entry approval**: Batch by default for Level 0 (high-level, low risk). Individual for Level 1+ (deeper, human-directed).

## Open Questions

1. **Skills installation in target repos**: Must the human manually copy skill files to the target repo? (Currently assumed: manual copy — nan-004 handles automation.)

## Tracking

https://github.com/dug-21/unimatrix/issues/211
