# SPECIFICATION — nan-003: Unimatrix Onboarding Skills

## Objective

Deliver two Claude Code skills — `/unimatrix-init` and `/unimatrix-seed` — that establish the three-layer Unimatrix chain (CLAUDE.md awareness → skill invocation → agent behavior) in a new repository and populate a quality-controlled baseline knowledge store. These skills are the first to use the `unimatrix-` naming prefix convention, which becomes the standard for all future production skills.

---

## Functional Requirements

### `/unimatrix-init` — Repository Initialization Skill

**FR-01** The skill shall perform a pre-flight idempotency check by reading `CLAUDE.md` (if it exists) and scanning for the sentinel string `<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->` before making any changes.

**FR-02** If the sentinel is found, the skill shall print "already initialized" and halt without modifying any files. No further phases shall execute.

**FR-03** If `CLAUDE.md` does not exist, the skill shall create it containing only the Unimatrix block (the sentinel + block content). No other content shall be added.

**FR-04** If `CLAUDE.md` exists and the sentinel is absent, the skill shall append the Unimatrix block to the existing file, preserving all existing content.

**FR-05** The appended Unimatrix block shall contain:
  - (a) The versioned sentinel comment on its first line: `<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->`
  - (b) A skills table listing only `unimatrix-*` prefixed skills with one-line descriptions
  - (c) A category convention guide describing what knowledge belongs in each Unimatrix category (decision, pattern, procedure, convention, lesson-learned, outcome)
  - (d) Usage trigger instructions: when to invoke each `unimatrix-*` skill

**FR-06** The Unimatrix block shall be self-contained — a developer with no prior Unimatrix knowledge shall be able to read it and understand what skills are available and when to use them.

**FR-07** The skill shall include a disambiguation notice clarifying the difference between `/unimatrix-init` (CLAUDE.md setup + agent recommendations) and the `uni-init` agent (brownfield bootstrap of `.claude/` knowledge extraction from agent/protocol files).

**FR-08** After the CLAUDE.md write, the skill shall perform a read-only agent scan: glob `.claude/agents/**/*.md` in the current working directory and read each matched file.

**FR-09** For each agent file found, the skill shall check for the presence of: `context_briefing` invocation, outcome reporting references (e.g., `/record-outcome`), and `unimatrix-*` skill references.

**FR-10** The skill shall produce a terminal-only agent recommendation report: a table or list showing agent name | missing patterns | concrete suggested additions with `unimatrix-*` skill-level examples. No file shall be written for this report.

**FR-11** The skill shall support `--dry-run` mode: when invoked as `/unimatrix-init --dry-run`, it shall print what would be written to `CLAUDE.md` and the agent recommendation report, without modifying any files.

**FR-12** The skill shall include a prerequisites section at the top of its `SKILL.md` documenting what must be in place before running: skills files present in `.claude/skills/`, MCP server wired in Claude settings.

### `/unimatrix-seed` — Knowledge Base Seeding Skill

**FR-13** At skill entry (before any file reads or stores), the skill shall call `context_status` to verify MCP server availability. If the call fails, the skill shall halt with a clear error message stating that Unimatrix MCP is not available.

**FR-14** Before any Level 0 stores, the skill shall call `context_search` to check whether seed entries already exist (checking for entries with categories `convention`, `pattern`, or `procedure`). If matching entries are found, the skill shall present a warning and offer to supplement rather than re-seed, then await human confirmation before proceeding.

**FR-15** Level 0 (automatic, no opt-in required) shall read the following files without requiring human confirmation: `README.md`, top-level `CLAUDE.md`, package manifests (`package.json`, `Cargo.toml`, `pyproject.toml`, `go.mod`), and top-level `.claude/` directory structure (if present).

**FR-16** From Level 0 reads, the skill shall propose 2–4 high-level foundational entries covering: repository purpose, technology stack, and project structure.

**FR-17** Level 0 proposed entries shall be presented to the human for batch approval (shown together as a group). The skill shall await a single approve/reject response before storing any entries.

**FR-18** Only approved entries shall be stored via `context_store`. Rejected entries shall be discarded without storage.

**FR-19** After Level 0, the skill shall print a summary of stored entries and present a menu of exploration options for Level 1: at minimum, choices covering module structure, key conventions, and build/test workflow. The skill shall STOP and await explicit human selection before proceeding.

**FR-20** Level 1+ exploration shall be gated: for each category selected by the human, the skill shall explore relevant files (module directories, test directories, config files) and propose entries. Each proposed entry at Level 1+ shall require individual human approval before storage.

**FR-21** The skill shall enforce a depth limit: no more than 2 opt-in levels beyond Level 0 (i.e., Level 0 + Level 1 + Level 2 maximum) within a single invocation.

**FR-22** Every proposed seed entry (at any level) shall pass the What/Why/Scope quality gate before being presented to the human for approval:
  - **What**: The entry content in one statement (max 200 chars)
  - **Why**: What goes wrong or is unclear without this knowledge (min 10 chars)
  - **Scope**: The component, module, or context where this applies

**FR-23** Seed entries shall use categories `convention`, `pattern`, or `procedure` only. Categories `decision`, `outcome`, and `lesson-learned` shall not be used by seed — these emerge from real feature work.

**FR-24** The skill shall include a prerequisites section at the top of its `SKILL.md` documenting what must be in place: MCP server wired and operational.

### Shared Requirements

**FR-25** Both skills shall be delivered as `SKILL.md` files in the existing skill directory format:
  - `/unimatrix-init` → `.claude/skills/unimatrix-init/SKILL.md`
  - `/unimatrix-seed` → `.claude/skills/unimatrix-seed/SKILL.md`

**FR-26** Both `SKILL.md` files shall include YAML frontmatter with `name` and `description` fields, followed by markdown content, matching the format of existing skills.

**FR-27** Both skills shall fail gracefully with a clear, actionable error message when required prerequisites are absent, rather than silently failing or executing partially.

---

## Non-Functional Requirements

**NFR-01** (Idempotency) `/unimatrix-init` shall produce identical outcomes on repeated invocations when the sentinel is already present. Running it N times shall produce the same CLAUDE.md state as running it once.

**NFR-02** (Depth Control) `/unimatrix-seed` shall not explore beyond 2 opt-in levels. The depth limit is a hard boundary, not a soft suggestion. Skill instructions shall use explicit STOP gates at each level transition.

**NFR-03** (Approval Gating) No entry shall be stored by `/unimatrix-seed` without explicit human acknowledgement (either batch approval at Level 0, or individual approval at Level 1+). Automated storage without human confirmation is prohibited.

**NFR-04** (Token Efficiency) `/unimatrix-seed` Level 0 reads shall cover only high-signal, small-footprint files: README, package manifests, top-level CLAUDE.md, `.claude/` directory listing. Deep source code traversal is out of scope.

**NFR-05** (Entry Quality) Proposed seed entries that fail the What/Why/Scope gate shall be silently discarded — not presented to the human for approval of low-quality entries.

**NFR-06** (Fail-Fast) Both skills shall check prerequisites at the earliest possible point. `/unimatrix-init` checks CLAUDE.md for sentinel before performing agent scan or write. `/unimatrix-seed` calls `context_status` before any file reads.

**NFR-07** (No Auto-Modification) `/unimatrix-init` shall not modify any `.claude/agents/` files, `.claude/protocols/` files, or `settings.json`. Its only write target is `CLAUDE.md`. Agent recommendations are printed only.

**NFR-08** (Platform Compatibility) Both skills shall work in any repository that has Claude Code with MCP configured. No assumption about the host language, framework, or directory structure beyond what is explicitly read in Level 0.

**NFR-09** (Self-Contained Block) The CLAUDE.md block produced by `/unimatrix-init` shall stand alone — it shall not require the reader to consult external files to understand the skills available and when to use them.

---

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|---------------------|
| AC-01 | `/unimatrix-init` appends a block to `CLAUDE.md` containing: (a) `unimatrix-*` skills table with one-line descriptions, (b) category convention guide, (c) usage trigger instructions | Manual: run skill on repo without existing CLAUDE.md block; inspect output file for all three elements |
| AC-02 | Running `/unimatrix-init` a second time on a repo with the sentinel present produces no changes to `CLAUDE.md` and prints "already initialized" | Manual: run twice; diff CLAUDE.md before and after second run; verify "already initialized" in output |
| AC-03 | Running `/unimatrix-init` when `CLAUDE.md` does not exist creates the file with the Unimatrix block | Manual: delete CLAUDE.md; run skill; verify file created with block |
| AC-04 | `/unimatrix-init` scans `.claude/agents/**/*.md` and produces a terminal-only recommendation report with concrete `unimatrix-*` skill examples; no agent files are modified | Manual: run on repo with agent files; verify terminal output; verify no agent file timestamps changed |
| AC-05 | `/unimatrix-init --dry-run` prints what would be written and the agent recommendation without modifying any files | Manual: run `--dry-run`; diff all files before and after; verify no changes; verify output includes block content and recommendations |
| AC-06 | `/unimatrix-seed` Level 0 reads README, package manifests, top-level structure without opt-in; proposes 2–4 high-level entries for batch approval; stores approved entries | Manual: run skill; observe Level 0 auto-reads; verify proposal count 2–4; approve batch; verify stored via `context_status` |
| AC-07 | `/unimatrix-seed` Level 1+ requires explicit human opt-in per exploration category; presents menu; waits for human selection before proceeding | Manual: run skill to Level 0 completion; verify skill stops and presents menu; verify no Level 1 reads occur until selection is made |
| AC-08 | Level 0 uses batch approval by default; Level 1+ uses individual entry approval by default; only approved entries are stored | Manual: run full seed; reject one Level 0 batch and one Level 1 entry; verify rejected entries absent from `context_search` results |
| AC-09 | `/unimatrix-seed` terminates after 2 opt-in levels; does not offer a Level 3 menu | Manual: run through Level 0 → Level 1 → Level 2; verify no further opt-in is offered after Level 2 |
| AC-10 | Both skills are delivered as `.claude/skills/unimatrix-init/SKILL.md` and `.claude/skills/unimatrix-seed/SKILL.md` with YAML frontmatter (`name`, `description`) | Code review: verify file paths; verify YAML frontmatter present and valid |
| AC-11 | The CLAUDE.md block is self-contained — a developer with no Unimatrix knowledge can read it and understand available skills and when to use them | Peer review: have a developer unfamiliar with Unimatrix read the block and answer: "what skills exist and when do I use each?" |
| AC-12 | `/unimatrix-init` SKILL.md includes a disambiguation notice distinguishing it from the `uni-init` agent | Code review: verify disambiguation notice present in SKILL.md content |
| AC-13 | `/unimatrix-seed` warns if seed entries already exist (via `context_search` pre-check) and offers to supplement rather than re-seed, before any Level 0 stores | Manual: run seed on repo with existing entries; verify warning appears before any new entries are stored |
| AC-14 | The sentinel marker includes a version number: `<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->` | Code review: verify sentinel string in generated CLAUDE.md and in SKILL.md idempotency-check logic |

---

## Domain Models

### Entities

**Skill** (`/unimatrix-init`, `/unimatrix-seed`)
A markdown instruction file at `.claude/skills/{name}/SKILL.md` with YAML frontmatter. Not executable code — instructions for the Claude model. The directory name becomes the slash command. Skills are stateless across invocations; multi-turn state is maintained by Claude's conversation context only.

**CLAUDE.md**
A markdown file read by Claude Code at session start to establish project context. May or may not exist in a target repo. The append target for `/unimatrix-init`. Contains free-form sections; the Unimatrix block is one section.

**Sentinel Marker**
The string `<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->` embedded as the first line of the Unimatrix block. Its presence signals that `/unimatrix-init` has already run. The version number (`v1`) enables future `--update` detection of stale blocks.

**Unimatrix Block**
The self-contained section appended by `/unimatrix-init` to CLAUDE.md. Contains: sentinel, skills table (unimatrix-* only), category guide, usage triggers. Designed to be independently readable.

**Seed Entry**
A Unimatrix knowledge entry created during `/unimatrix-seed`. Categories limited to `convention`, `pattern`, or `procedure`. Must pass What/Why/Scope quality gate. Stored only after human approval. Represents foundational repo knowledge visible from day one.

**Exploration Level**
A bounded depth tier in `/unimatrix-seed`:
- Level 0: Automatic. Reads README, manifests, top-level structure. Batch approval.
- Level 1: Opt-in per category. Reads module/test/config structure. Individual approval.
- Level 2: Opt-in per category. Deeper per-category reads. Individual approval. Terminal level.

**Agent Recommendation Report**
Terminal-only output from `/unimatrix-init`. A table of discovered agent files cross-referenced with missing Unimatrix patterns (context_briefing, outcome reporting, unimatrix-* skill references). Contains concrete skill-level examples, not raw MCP tool calls. Not persisted to disk.

**Quality Gate (What/Why/Scope)**
The three-field content standard for seed entries. Same gate used by `/store-pattern`. Entries failing the gate are discarded before human review.

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| **three-layer chain** | The architecture of Claude Code integration: CLAUDE.md (awareness) → skill files (invocation) → knowledge in Unimatrix (behavior). Established by `/unimatrix-init`. |
| **unimatrix-* prefix** | The naming convention for production skills introduced in nan-003. Existing skills (store-adr, retro, etc.) retain their names; only new production skills use this prefix. |
| **pre-flight check** | A prerequisite verification performed at skill entry before any state-changing operation. Fail-fast pattern. |
| **idempotency guard** | The sentinel-based mechanism ensuring `/unimatrix-init` can be safely re-run. |
| **dry-run** | A mode where the skill prints intended actions without executing them. |
| **STOP gate** | An explicit pause in `/unimatrix-seed` where skill execution halts and awaits a human response before proceeding. Each level transition is a STOP gate. |
| **batch approval** | Human approves or rejects a set of proposed entries as a group (Level 0 default). |
| **individual approval** | Human approves or rejects each proposed entry separately (Level 1+ default). |
| **seed re-run** | Running `/unimatrix-seed` on a repo where seed entries already exist. Triggers the supplementation warning flow. |

---

## User Workflows

### Workflow 1: New Repository Onboarding (Full Flow)

**Actor**: Developer setting up Unimatrix in a new repo.

**Prerequisites**: Unimatrix binary installed, MCP server wired in Claude `settings.json`, skill files copied to `.claude/skills/`.

1. Developer opens Claude Code in the new repository.
2. Developer invokes `/unimatrix-init`.
3. Skill checks for sentinel in CLAUDE.md → not found.
4. Skill scans `.claude/agents/**/*.md` → produces recommendation report in terminal.
5. Skill appends Unimatrix block to CLAUDE.md (or creates CLAUDE.md if absent).
6. Developer reviews recommendation report and manually updates agent files per suggestions.
7. Developer invokes `/unimatrix-seed`.
8. Skill calls `context_status` → MCP available.
9. Skill calls `context_search` → no existing seed entries found.
10. Skill reads Level 0 files (README, manifests, top-level structure).
11. Skill proposes 2–4 high-level entries → developer approves batch.
12. Entries stored. Skill presents Level 1 exploration menu.
13. Developer selects one or more exploration categories.
14. Skill proposes Level 1 entries individually → developer approves/rejects each.
15. Approved entries stored. Skill presents Level 2 menu (if depth not exhausted).
16. Developer opts out or selects Level 2 categories.
17. Seed session ends. Knowledge base now has foundational entries.

**Outcome**: CLAUDE.md has Unimatrix block; knowledge base has 4–15 foundational entries; future `context_briefing` calls return meaningful results.

### Workflow 2: Already-Initialized Repository

**Actor**: Developer re-running `/unimatrix-init` on a repo that already has the block.

1. Developer invokes `/unimatrix-init`.
2. Skill reads CLAUDE.md → finds sentinel.
3. Skill prints "already initialized" and exits.
4. CLAUDE.md unchanged.

**Outcome**: No changes. Safe to re-run at any time.

### Workflow 3: Dry-Run Inspection

**Actor**: Developer wanting to preview changes before committing.

1. Developer invokes `/unimatrix-init --dry-run`.
2. Skill prints what would be written to CLAUDE.md.
3. Skill prints agent recommendation report.
4. No files written.

**Outcome**: Developer can review and decide whether to proceed with the actual run.

### Workflow 4: Supplement an Existing Seed

**Actor**: Developer adding knowledge after initial seeding.

1. Developer invokes `/unimatrix-seed`.
2. Skill calls `context_status` → MCP available.
3. Skill calls `context_search` → existing seed entries found.
4. Skill warns: "Seed entries already exist. Supplement rather than re-seed?"
5. Developer confirms supplement.
6. Level 0 runs with awareness of existing entries (avoids near-duplicates).
7. Skill continues normally through Level 1/2 as directed.

**Outcome**: New entries added without duplicating existing baseline knowledge.

---

## Constraints

**C-01** (File-Only Delivery) Both skills are markdown files. They cannot execute code, use environment variables, or access the filesystem except through Claude's built-in tools (Read, Write, Glob, MCP calls). All file operations are performed by Claude following skill instructions.

**C-02** (Manual Installation) Skill files must be manually copied to `.claude/skills/` in the target repo. There is no auto-install mechanism — this is nan-004 scope. The skill documentation must make this prerequisite explicit.

**C-03** (MCP Dependency) `/unimatrix-seed` requires an operational MCP server (Unimatrix). If MCP is unavailable, the skill shall fail at pre-flight, not mid-exploration after state has been partially modified.

**C-04** (Sentinel Assumption) The idempotency sentinel works only when CLAUDE.md is a readable markdown file of manageable size. Machine-generated, encrypted, or excessively large CLAUDE.md files may defeat sentinel detection. This is accepted as a known limitation.

**C-05** (Near-Duplicate Gap) Server-side dedup (0.92 cosine threshold) blocks exact duplicates but not near-duplicates. The `context_search` pre-check (FR-14) is the primary mitigation — it must occur before any Level 0 stores, not only as a warning. (SR-07)

**C-06** (Conversational State) `/unimatrix-seed` relies on Claude's conversation context to maintain approval state and depth tracking across turns. No persistent state mechanism is available within a skill. Skill instructions must be written with explicit STOP gates to compensate. (SR-01)

**C-07** (No Agent File Modification) `/unimatrix-init` must not modify any `.claude/agents/` files. Recommendations are terminal-only output.

**C-08** (Skill Naming Convention) Both skills use the `unimatrix-` prefix. Existing skills (`store-adr`, `retro`, `record-outcome`, etc.) retain their names unchanged. Only nan-003 and future production skills use this convention.

**C-09** (Seed Category Restriction) `/unimatrix-seed` shall not store entries in `decision`, `outcome`, or `lesson-learned` categories. These emerge from real feature work, not seeding.

**C-10** (Non-Goals Boundary) Neither skill handles: binary installation, ONNX model download, `settings.json` wiring, creation or modification of agent definitions, seeding from `.claude/agents/` files (that is `uni-init` agent scope).

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| MCP server (`unimatrix-server`) | Runtime | Required for `/unimatrix-seed` (context_store, context_status, context_search). Must be running and wired. |
| `context_status` tool | MCP call | Pre-flight check in `/unimatrix-seed` (SR-06) |
| `context_search` tool | MCP call | Pre-seed duplicate check in `/unimatrix-seed` (AC-13, SR-07) |
| `context_store` tool | MCP call | Storing approved seed entries |
| Claude Code skill loader | Platform | Directory name → slash command routing. Existing mechanism, no changes needed. |
| Existing skills (store-pattern, record-outcome, etc.) | Reference | `/unimatrix-init` CLAUDE.md block documents `unimatrix-*` prefixed skills only; existing skills unchanged. |
| `.claude/skills/{name}/SKILL.md` format | Convention | Both deliverables follow this format (YAML frontmatter + markdown). |
| `uni-init` agent (`.claude/agents/uni/uni-init.md`) | Adjacent | Distinct from `/unimatrix-init`; handles brownfield bootstrap from `.claude/` files. Disambiguation required. |

---

## NOT in Scope

- Installing the Unimatrix binary, ONNX model download, or wiring `settings.json` → **nan-004**
- Modifying existing agent definitions in `.claude/agents/` (recommendation only)
- Renaming or migrating existing skills to the `unimatrix-` prefix (store-adr, retro, etc. stay as-is)
- Creating or modifying agent definitions in the target repo
- Seeding Unimatrix from `.claude/agents/` or `.claude/protocols/` files → **uni-init agent**
- Supporting non-Claude-Code environments or non-MCP transports
- Deep code analysis (function signatures, type hierarchies, dependency graphs)
- Automated test harness for skill behavior (skills are model instructions; verification is manual per SR-03)
- `/unimatrix-init --update` for replacing stale blocks (future, enabled by AC-14 versioned sentinel)
- Automated skill installation across repos (nan-004 scope)
- Documentation of existing skills in the CLAUDE.md block (only `unimatrix-*` prefixed skills listed)

---

## Open Questions

1. **Skills installation path**: Must the human manually copy `unimatrix-init/SKILL.md` and `unimatrix-seed/SKILL.md` to each target repo? (Currently assumed: manual copy — nan-004 handles automation.) The spec assumes manual copy; nan-004 must provide a path from init completion to skill availability in target repos.

2. **Sentinel partial-read fallback** (SR-02): Should `/unimatrix-init` add a secondary idempotency check (e.g., `context_search` for sentinel text, or reading only the last N lines of CLAUDE.md) as a fallback for large files? Architect should decide whether the single-pass full-read is sufficient or whether a tail-read fallback is warranted.

3. **Level 0 batch rejection**: If the human rejects the entire Level 0 batch, should the skill halt entirely, or offer to re-propose individual entries? SCOPE.md is silent on this case. Recommended: halt and report 0 entries stored, letting the human re-invoke with more specific guidance.

---

## Knowledge Stewardship

- Queried: `/query-patterns` for onboarding, skill format, initialization, seed — no results directly applicable to `/unimatrix-init` or `/unimatrix-seed` patterns. Closest match (#552 Skill File as Single Source of Truth) confirms skills are the correct vehicle but provides no implementation patterns for this feature's specific concerns.
