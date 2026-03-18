# nan-005: Documentation & Onboarding — Specification

## Objective

nan-005 rewrites the Unimatrix README as a comprehensive external-facing document covering all 12 MCP tools, 14 skills, 8 knowledge categories, and every operational constraint users hit on first adoption. It also adds a `uni-docs` agent definition and a documentation update step to the delivery protocol, creating a self-maintaining documentation system that prevents the drift already present in the current README (stale redb references, wrong file extensions, incorrect test counts).

---

## Functional Requirements

### FR-01: README Rewrite

**FR-01a**: README.md MUST contain all of the following sections, in an order that serves an external evaluator before a new user:
1. Hero section — what Unimatrix is (2-3 sentences, capability-first, no crate names)
2. Why Unimatrix — problem solved, key differentiators (auditable lifecycle, invisible delivery, self-learning)
3. Core Capabilities — all major user-facing capabilities grouped by user experience, not by crate
4. Getting Started — npm install path (primary) and build-from-source path (secondary), each with prerequisites and configuration steps
5. Tips for Maximum Value — operational guidance for new users
6. MCP Tool Reference — all 12 tools with name, purpose, key parameters, and when-to-use guidance
7. Skills Reference — all 14 skills with name, purpose, and trigger condition
8. Knowledge Categories — all 8 categories with description and example
9. CLI Reference — all subcommands and flags with descriptions
10. Architecture — minimal high-level section: storage backend (SQLite), hook integration, MCP transport
11. Security Model — user-facing summary of trust hierarchy, content scanning, and audit trail

**FR-01b**: The README MUST NOT contain any of the following known factual errors from the current version:
- References to "redb" in any section (storage backend, crate descriptions, data layout, project structure)
- File extension `.redb` (correct is `.db`)
- "17-table" claim (correct count is 19 SQLite tables per `db.rs` `CREATE TABLE IF NOT EXISTS` calls)
- "1,500+ tests" claim (current count is higher; must verify before authoring or state "1,700+" per PRODUCT-VISION.md)
- "8 crates" claim if the workspace has 9 crates (current workspace: `unimatrix-{store,vector,embed,core,engine,adapt,observe,server,learn}`)
- Incorrect hook event names (correct events used: `UserPromptSubmit`, `PreCompact`, `PreToolUse`, `PostToolUse`, `Stop`)
- "Rust 1.89+" (verified correct in `Cargo.toml` `rust-version = "1.89"` — retain unless changed)

**FR-01c**: Every numeric claim in the README MUST be sourced from live codebase verification at authoring time, not from SCOPE.md estimates. Claims that cannot be verified MUST be omitted or stated as approximate with explicit qualification.

**FR-01d**: The README MUST be a single file. No split into docs/ subdirectory, no separate files per section.

**FR-01e**: The README acknowledgments section MUST be preserved. It credits claude-flow and ruvnet.

---

### FR-02: Core Capabilities Section

**FR-02a**: The Core Capabilities section MUST cover each of the following capabilities as a named subsection or bullet group, with enough detail for an adopter to understand the value without reading source code:

1. **Self-Learning Knowledge Engine** — captures decisions, patterns, conventions, lessons from real feature work; confidence scoring evolves from usage signals
2. **Adaptive Embeddings (MicroLoRA)** — MicroLoRA layer adapts frozen all-MiniLM-L6-v2 ONNX embeddings to project-specific usage via contrastive learning (InfoNCE loss) with EWC++ regularization
3. **Semantic Search with Confidence Ranking** — six-factor additive weighted composite (base, usage, freshness, helpfulness, correction quality, creator trust) plus co-access affinity; search re-ranking formula `0.85 * similarity + 0.15 * confidence + co-access boost (max 0.03) + provenance boost (0.02 for lesson-learned)`
4. **Hook-Driven Invisible Delivery (Cortical Implant)** — automatic context injection on every prompt (UserPromptSubmit), compaction resilience (PreCompact), closed-loop confidence feedback (Stop/SubagentStop), session lifecycle persistence; sub-50ms round-trip budget; disk-backed event queue for graceful degradation
5. **Retrospective Analysis** — 21 detection rules across 4 categories (agent behavior, friction points, session health, scope indicators); historical baselines with 1.5-sigma outlier detection; evidence synthesis; lesson-learned auto-persistence with de-duplication via correction chains
6. **Contradiction Detection** — pairwise heuristic detection across the knowledge base
7. **Correction Chains with Audit Trails** — SHA-256 hash-chained correction histories with `previous_hash` links; append-only audit log with agent identity, session context, and operation
8. **Coherence Gate** — lambda health metric (0.0-1.0) across four dimensions (confidence freshness 0.35, graph quality 0.30, contradiction density 0.20, embedding consistency 0.15); maintenance recommendations when lambda drops below 0.8
9. **Knowledge Effectiveness Analysis** — per-entry utility scoring from injection_log and session outcomes; confidence calibration validation; dead knowledge detection

**FR-02b**: The Core Capabilities section MUST frame capabilities from the user's perspective ("what you can do"), not from the implementation perspective ("what was built").

---

### FR-03: Getting Started Section

**FR-03a**: The Getting Started section MUST document the npm install path as the primary path, with prerequisite (Node.js >=18) and the install command `npm install @dug-21/unimatrix` (package name per `packages/unimatrix/package.json`).

**FR-03b**: The Getting Started section MUST document the build-from-source path as the secondary path, with prerequisites:
- Rust 1.89+ (edition 2024)
- ONNX Runtime 1.20.x shared library
- Platform-specific ONNX Runtime installation instructions for macOS (Homebrew) and Linux (manual download)
- Devcontainer note (pre-installed)

**FR-03c**: The Getting Started section MUST document both configuration steps that are required after installing the binary:
1. MCP server configuration in `.claude/settings.json` (with the exact JSON structure)
2. Hooks configuration in `.claude/settings.json` (with all hook events: `UserPromptSubmit`, `PreCompact`, `PreToolUse`, `PostToolUse`, `Stop`)

**FR-03d**: The Getting Started section MUST include at least 3 illustrative MCP tool call examples showing common first-use patterns (search, store, briefing, correct, retrospective).

---

### FR-04: MCP Tool Reference Section

**FR-04a**: The MCP Tool Reference MUST document all 12 tools. The 12 tools are (verified from `tools.rs`):
`context_search`, `context_lookup`, `context_get`, `context_store`, `context_correct`, `context_deprecate`, `context_quarantine`, `context_status`, `context_briefing`, `context_enroll`, `context_cycle_review`

Wait — that is 11. The README currently lists 11 tools and the SCOPE.md says 12. Verification required: the PRODUCT-VISION.md lists 12 in its MCP Server section but names only 11 in the vinculum bullet. The `tools.rs` file contains exactly 11 `#[tool(...)]` annotated handlers. **The README must state the correct count (11 tools) or the 12th tool must be confirmed to exist.** This is a verifiable fact the implementation agent must confirm before authoring. See Open Question OQ-01.

**FR-04b**: For each tool, the reference MUST include:
- Tool name (exact string as registered)
- One-line purpose statement (accurate, not aspirational)
- Key parameters (not exhaustive — the parameters users actually need to know):
  - `context_search`: `query` (required), `category`, `topic`, `tags`, `k`, `format`, `helpful`
  - `context_lookup`: `topic`, `category`, `tags`, `id`, `status`, `limit`, `format`
  - `context_get`: `id` (required), `format`, `helpful`
  - `context_store`: `content` (required), `topic` (required), `category` (required), `tags`, `title`, `feature_cycle`, `format`
  - `context_correct`: `original_id` (required), `content` (required), `reason`, `format`
  - `context_deprecate`: `id` (required), `reason`, `format`
  - `context_quarantine`: `id` (required), `action` ("quarantine" or "restore"), `reason`, `format`
  - `context_status`: `topic`, `category`, `check_embeddings`, `maintain`, `format`
  - `context_briefing`: `role` (required), `task` (required), `feature`, `max_tokens` (default 3000, range 500-10000), `format`
  - `context_enroll`: `target_agent_id` (required), `trust_level` (required), `capabilities` (required), `agent_id`, `format`
  - `context_cycle_review`: `feature_cycle` (required), `evidence_limit`, `format` ("markdown" default, "json")
- When-to-use note that distinguishes it from similar tools (specifically: search vs. lookup, correct vs. deprecate vs. quarantine)
- Capability requirement where non-obvious (Admin-only: `context_status`, `context_quarantine`, `context_enroll`)

**FR-04c**: The tool reference MUST note that all tools support `format: "summary" | "markdown" | "json"` as a common parameter.

**FR-04d**: The tool reference MUST note that `context_briefing` is gated on the `mcp-briefing` feature flag and returns an error message if not compiled in.

**FR-04e**: The tool reference MUST document `context_status` `maintain` parameter behavior accurately: as of col-013, `maintain` is silently ignored (background tick handles maintenance). The status report reflects the last background maintenance run. Do not document `maintain=true` as triggering inline maintenance.

---

### FR-05: Skills Reference Section

**FR-05a**: The Skills Reference MUST document all 14 skills. The 14 skills are (verified from `.claude/skills/`):
`/query-patterns`, `/store-adr`, `/store-pattern`, `/store-procedure`, `/store-lesson`, `/record-outcome`, `/knowledge-search`, `/knowledge-lookup`, `/review-pr`, `/retro`, `/uni-git`, `/release`, `/unimatrix-init`, `/unimatrix-seed`

**FR-05b**: For each skill, the reference MUST include:
- Skill name (invocation form, e.g. `/query-patterns`)
- One-line purpose statement
- Trigger condition: when exactly to invoke it (e.g., "before designing or implementing any component", "at the end of every session")

**FR-05c**: The skills reference MUST note that skills are Claude Code platform-native files and must be installed via the npm package or by copying `.claude/skills/` directories to the target repository.

**FR-05d**: Skills that interact with the MCP server MUST note the MCP server dependency (e.g., `/knowledge-search`, `/knowledge-lookup`, `/store-adr`, `/unimatrix-seed`).

---

### FR-06: Knowledge Categories Section

**FR-06a**: The Knowledge Categories section MUST document all 8 categories verified from `categories.rs`:
`outcome`, `lesson-learned`, `decision`, `convention`, `pattern`, `procedure`, `duties`, `reference`

**FR-06b**: For each category, the section MUST provide:
- Category name (exact string as used in `context_store` calls)
- One-line description of what belongs in this category
- One concrete example use case

**FR-06c**: The section MUST include a guidance note that category discipline matters for retrieval quality: miscategorized entries surface in wrong contexts during semantic search.

**FR-06d**: The section MUST note that the category allowlist is runtime-extensible via `add_category()` but that the 8 built-in categories cover the primary use cases.

---

### FR-07: CLI Reference Section

**FR-07a**: The CLI Reference MUST document all subcommands and flags verified from `main.rs`:
- Default mode (no subcommand): starts MCP server over stdio
- `hook <EVENT>`: handles a Claude Code lifecycle hook event
- `export [--output <PATH>]`: exports knowledge base to JSONL (no running server required)
- `import --input <PATH> [--skip-hash-validation] [--force]`: imports from JSONL export
- `version [--project-dir <PATH>]`: prints version; with `--project-dir` also initializes the database
- `model-download`: downloads ONNX embedding model to cache

**FR-07b**: The CLI Reference MUST document the global flags:
- `--project-dir <PATH>`: overrides automatic project root detection
- `--verbose` / `-v`: enables debug-level logging to stderr

**FR-07c**: The CLI Reference MUST note that the `hook` subcommand is designed for use in `.claude/settings.json` hook configuration, not direct user invocation.

---

### FR-08: Architecture Section

**FR-08a**: The Architecture section MUST describe the current storage backend as SQLite (not redb). Storage is local, file-based, zero cloud dependency.

**FR-08b**: The Architecture section MUST describe the correct data layout with accurate file names and paths:
```
~/.unimatrix/{project-hash}/
  unimatrix.db               # SQLite knowledge database (schema v11)
  unimatrix.pid              # PID file with flock advisory lock
  unimatrix.sock             # Unix domain socket for hook IPC
  vector/
    unimatrix-vector.hnsw2   # HNSW graph
    unimatrix-vector.meta    # index metadata
~/.cache/unimatrix/models/   # ONNX model files (downloaded once)
```

**FR-08c**: The Architecture section MUST state the correct crate count. Current workspace (`Cargo.toml` `members = ["crates/*"]`) has 9 crates: `unimatrix-{store, vector, embed, core, engine, adapt, observe, server, learn}`. The section must list them with accurate descriptions (SQLite not redb for `unimatrix-store`).

**FR-08d**: The Architecture section MUST describe at a high level how hook-driven delivery works: single binary, Unix domain socket IPC between the `hook` subcommand and the running MCP server.

**FR-08e**: The Architecture section MUST NOT contain crate-level implementation details (scoring formula weights, table schemas, HNSW construction parameters). Those are internal details. The section covers what users need to understand the system.

---

### FR-09: Security Model Section

**FR-09a**: The Security Model section MUST document the 4-tier trust hierarchy: System > Privileged > Internal > Restricted. Unknown agents auto-enroll as Restricted (read + search only).

**FR-09b**: The Security Model section MUST document the four capabilities: `read`, `write`, `search`, `admin`.

**FR-09c**: The Security Model section MUST document content scanning: injection patterns (~25+) and PII patterns (6+) on every write.

**FR-09d**: The Security Model section MUST document the append-only audit log (every operation with agent identity, session context, outcome).

**FR-09e**: The Security Model section MUST document hash-chained corrections (SHA-256 content hashes with `previous_hash` links).

**FR-09f**: The Security Model section MUST document protected agents: `system` and `human` cannot be modified; self-lockout prevention prevents an admin from removing their own Admin capability.

**FR-09g**: The Security Model section MUST NOT describe future security capabilities not yet implemented (OAuth, HTTPS transport, `_meta` agent identity) as current features.

---

### FR-10: Operational Guidance (Tips for Maximum Value) Section

**FR-10a**: The operational guidance section MUST cover all of the following constraints:

1. **Session boundaries**: Each feature cycle should use a new Claude Code session. Context window pollution across features reduces knowledge quality.
2. **Feature cycle naming**: Phase prefix + number (e.g., `col-015`). Used in commits, branches, issue tracking, and as the `feature_cycle` parameter in MCP tool calls.
3. **Commit message format**: `{prefix}: {description} (#{issue})` — see `/uni-git` for the prefix table.
4. **Category discipline**: The right category matters for retrieval. Decisions (`decision`) are not conventions (`convention`); procedures (`procedure`) are not patterns (`pattern`). Miscategorized entries surface in wrong contexts.
5. **Hook latency budget**: Hooks have a sub-50ms round-trip budget. Heavy blocking operations in hooks degrade the user experience.
6. **Knowledge base cold start**: A fresh knowledge base returns empty search results. Use `/unimatrix-seed` to populate foundational entries before relying on search.
7. **Near-duplicate detection threshold**: Entries with cosine similarity ≥ 0.92 to existing entries are rejected as duplicates. Rephrase if a legitimate distinct entry is rejected.

**FR-10b**: The operational guidance section MUST reference `/unimatrix-init` and `/unimatrix-seed` for per-repo setup rather than restating their content. This preserves the boundary established by nan-003 (SR-04 mitigation).

---

### FR-11: Documentation Agent (uni-docs)

**FR-11a**: A `uni-docs` agent definition MUST exist at `.claude/agents/uni/uni-docs.md`.

**FR-11b**: The `uni-docs` agent definition MUST specify these behavioral requirements:
1. Reads the feature's `SCOPE.md` and `SPECIFICATION.md` to understand what was delivered
2. Reads the current `README.md` to identify what sections are affected
3. Identifies specific sections requiring updates (new/changed tools, skills, constraints, capabilities, architecture, security model)
4. Proposes targeted edits — not full rewrites of unaffected sections
5. Commits documentation updates to the feature branch with commit prefix `docs:`
6. Falls back to reading git diff or `CHANGELOG.md` when feature artifacts are incomplete or missing (SR-02 mitigation)

**FR-11c**: The `uni-docs` agent definition MUST explicitly state the scope boundary: it updates README.md only. It does not update `.claude/` files, protocol files, or per-feature documentation.

**FR-11d**: The `uni-docs` agent definition MUST specify that it reads artifacts, not source code. It does not grep through Rust files. Its understanding of what changed comes from feature artifacts.

---

### FR-12: Delivery Protocol Modification

**FR-12a**: The delivery protocol (`uni-delivery-protocol.md`) MUST include a documentation update step in Phase 4 after PR creation and before `/review-pr` invocation.

**FR-12b**: The insertion point in Phase 4 MUST be precisely specified as: after `gh pr create` and before the `/review-pr` invocation. The exact sequence is:
1. Commit final artifacts
2. Push feature branch
3. Open PR (`gh pr create`)
4. **[NEW] Documentation update step (conditional — see FR-12d)**
5. Invoke `/review-pr`
6. Return to human

**FR-12c**: The documentation update step MUST specify that the Delivery Leader spawns `uni-docs` with the feature ID, SCOPE.md path, SPECIFICATION.md path, and README.md path.

**FR-12d**: The documentation update step MUST specify when it is mandatory vs. optional:
- **Mandatory**: feature adds or changes an MCP tool, skill, CLI subcommand, knowledge category, or schema version
- **Optional (skip allowed)**: pure internal refactors, test-only features, infrastructure debt with no user-facing change
- This addresses SR-05: pure optionality risks silent decay

**FR-12e**: The documentation update step MUST specify that documentation updates are committed to the feature branch (same PR) before `/review-pr`. This makes doc updates part of the reviewed PR, maintaining traceability.

**FR-12f**: The documentation update step MUST explicitly state it has no gate — it is advisory and does not block delivery. A failure in `uni-docs` does not fail the delivery.

---

## Non-Functional Requirements

**NFR-01 — Accuracy**: Every factual claim in the README must be verifiable against the codebase at the time of authoring. No aspirational features, no stale references.

**NFR-02 — Completeness**: The README must cover all 11 (or 12 — see OQ-01) MCP tools, all 14 skills, all 8 categories, all CLI subcommands, and all operational constraints listed in FR-10a. Missing entries in any reference table is a defect.

**NFR-03 — Navigability**: A single README containing 11 sections covering 11+ tools and 14 skills will be long. Section headers must use consistent markdown heading levels (H2 for sections, H3 for subsections) to enable browser/editor TOC navigation. Tables MUST be used for reference data (tools, skills, categories, CLI flags) rather than prose lists.

**NFR-04 — No Placeholder Content**: No section may contain "TODO", "TBD", placeholder text, or aspirational ("will be added") language. If a capability cannot be accurately described, it must be omitted. Unknown facts are escalated as open questions, not placeholders.

**NFR-05 — Minimal Architecture Depth**: The architecture section target is 20-40 lines. Users need to know: SQLite local storage, 9-crate Rust workspace, hook integration via UDS, MCP transport via stdio. Not: HNSW construction parameters, SQL table definitions, scoring formula weights.

**NFR-06 — Additive Protocol Change**: The delivery protocol modification adds one step. It does not restructure existing phases, change gate criteria, or modify the rework protocol.

**NFR-07 — Consistency**: Terminology must be consistent throughout the README. The product is "Unimatrix" (not "the Unimatrix" or "UniMatrix"). Tools use their exact registered names (`context_search` not `contextSearch`). Skills use their invocation form (`/query-patterns` not `query-patterns`).

---

## Acceptance Criteria

**AC-01**: README.md is rewritten with all 11 sections from FR-01a. Each section exists, is non-empty, contains no placeholder text, and addresses the stated content requirement.
- Verification: Read README.md; check each section header is present; check each section has substantive content against FR-01a through FR-10.

**AC-02**: The MCP tool reference documents all MCP tools with name, one-line purpose, key parameters, and when-to-use note. The tool count matches the verified count from `tools.rs`.
- Verification: Count `#[tool(name = ...)` annotations in `crates/unimatrix-server/src/mcp/tools.rs`; verify count matches README table; verify each tool has when-to-use guidance distinguishing it from similar tools.

**AC-03**: The skills reference documents all 14 skills with name, one-line purpose, and trigger condition.
- Verification: Count SKILL.md files in `.claude/skills/`; verify count is 14; verify each skill appears in README with a trigger condition.

**AC-04**: All factual errors from FR-01b are corrected: no redb references, no `.redb` extension, correct table count or no explicit count, correct test count or qualified approximation, correct crate count matching workspace `members`.
- Verification: `grep -r "redb\|\.redb\|17-table\|1,500" README.md` returns no matches; crate count matches `ls crates/ | wc -l`.

**AC-05**: The operational guidance section documents all 7 constraints from FR-10a.
- Verification: Each of the 7 constraints is findable by keyword in the README (session boundaries, feature cycle naming, commit format, category discipline, hook latency, cold start, near-duplicate threshold).

**AC-06**: `uni-docs` agent definition exists at `.claude/agents/uni/uni-docs.md` and satisfies FR-11b through FR-11d.
- Verification: File exists; contains artifact-reading instructions (SCOPE.md, SPECIFICATION.md); contains fallback behavior (git diff or CHANGELOG); states scope boundary (README.md only); states no source code reading.

**AC-07**: The delivery protocol (`uni-delivery-protocol.md`) contains a documentation update step in Phase 4 at the exact position specified in FR-12b.
- Verification: Read Phase 4 section; confirm documentation step appears after `gh pr create` and before `/review-pr` invocation.

**AC-08**: The documentation update step in the delivery protocol specifies mandatory vs. optional criteria per FR-12d.
- Verification: The step text explicitly lists conditions that make it mandatory (new tool, skill, CLI subcommand, category, schema version) vs. skippable (pure refactor, test-only).

**AC-09**: The getting started section includes both the npm install path (FR-03a) and the build-from-source path (FR-03b) with prerequisites for each.
- Verification: `grep -n "npm install" README.md` finds the install command; `grep -n "cargo build" README.md` finds the build command; both have prerequisite sections.

**AC-10**: The architecture section correctly describes SQLite storage backend, 9 crates with accurate descriptions, and the correct data layout file names (`.db`, `.pid`, `.sock`) per FR-08a through FR-08d.
- Verification: `grep "redb" README.md` returns no matches; data layout shows `unimatrix.db` not `unimatrix.redb`; crate list has 9 entries.

**AC-11**: The knowledge categories section explains all 8 categories with one-line description and example per FR-06a through FR-06d.
- Verification: All 8 category names from `categories.rs` appear in the README with descriptions and examples; count matches.

**AC-12**: README content is factually accurate against the current codebase — no aspirational features, no stale references, no placeholder content.
- Verification: Manual review of each README section against verified facts from SCOPE.md fact-verification checklist (see Fact Verification Checklist below); check for any "will", "planned", "future" language about current capabilities.

---

## Fact Verification Checklist

The implementation agent MUST verify each of these facts from the live codebase before authoring the README. Each cell must be filled in from actual source, not from SCOPE.md estimates:

| Claim | Verification Source | Expected Value | Notes |
|-------|---------------------|----------------|-------|
| MCP tool count | `grep -c '#\[tool(' crates/unimatrix-server/src/mcp/tools.rs` | Verify | SR-01 |
| Skill count | `ls .claude/skills/ \| wc -l` | 14 | SR-01 |
| Crate count | `ls crates/ \| wc -l` | 9 | SR-01 |
| Schema version | `grep CURRENT_SCHEMA_VERSION crates/unimatrix-store/src/migration.rs` | 11 | SR-01 |
| SQLite table count | `grep -c 'CREATE TABLE IF NOT EXISTS' crates/unimatrix-store/src/db.rs` | 19 | SR-01 |
| Rust version | `grep rust-version Cargo.toml` | 1.89 | SR-01 |
| npm package name | `cat packages/unimatrix/package.json \| jq .name` | @dug-21/unimatrix | SR-01 |
| Test count | `cargo test -- --list 2>/dev/null \| wc -l` | Verify | SR-01 |
| Storage backend | `grep 'redb\|sqlite\|rusqlite' crates/unimatrix-store/Cargo.toml` | SQLite | SR-01 |
| Data dir path | `grep 'unimatrix\.db\|data_dir' crates/unimatrix-engine/src/project.rs` | `~/.unimatrix/{hash}/unimatrix.db` | SR-01 |
| confidence score formula weights (sum) | `grep 'weight\|0\.' crates/unimatrix-engine/src/confidence.rs` | Verify | SR-01 |
| Hook event names | `grep -h 'UserPromptSubmit\|PreCompact\|Stop\|PostToolUse\|PreToolUse' crates/unimatrix-server/src/uds/hook.rs` | Verify all 5 | SR-01 |
| `maintain` param behavior | `grep -A5 'maintain' crates/unimatrix-server/src/mcp/tools.rs` | silently ignored (col-013) | FR-04e |

---

## Domain Models

### Entry

An entry is the fundamental knowledge unit in Unimatrix. Each entry has:
- **id** (u64): immutable primary key
- **title**: short descriptive label
- **content**: the knowledge body (text)
- **topic**: grouping key, typically a feature ID (e.g., `col-015`)
- **category**: one of 8 allowlisted values
- **tags**: optional string list for faceted search
- **status**: `active`, `deprecated`, `proposed`, `quarantined`
- **confidence**: f64 composite score [0.0, 1.0]
- **feature_cycle**: the feature cycle that produced this entry
- **created_by**: agent identity
- **trust_source**: `"agent"`, `"auto"`, `"human"`, `"system"`
- **content_hash** / **previous_hash**: SHA-256 chain for tamper evidence

### Feature Cycle

The identifier for a unit of work: `{phase-prefix}-{NNN}` (e.g., `col-015`, `nan-005`). Used as:
- `topic` in `context_store` calls
- `feature_cycle` parameter for tracking
- Branch name suffix (`feature/col-015`)
- GH Issue reference

### Agent

An enrolled identity in the agent registry with a trust level (System, Privileged, Internal, Restricted) and a capability set (`read`, `write`, `search`, `admin`). Unknown agents auto-enroll as Restricted on first contact.

### Skill

A Claude Code platform-native `/command` defined as a SKILL.md file in `.claude/skills/{skill-name}/SKILL.md`. Skills cannot be called programmatically — they are invoked by humans or agents through the Claude Code skill invocation mechanism.

### Hook Event

A Claude Code lifecycle event routed to the `unimatrix-server hook <EVENT>` subcommand. Events: `UserPromptSubmit`, `PreCompact`, `PreToolUse`, `PostToolUse`, `Stop`. The hook subcommand connects to the running MCP server via Unix domain socket and delivers the event payload.

---

## User Workflows

### Workflow 1: New User Onboarding (External Evaluator)

1. User reads README hero + Why Unimatrix to evaluate fit
2. User reads Getting Started — npm install path
3. User installs binary, configures MCP server in `.claude/settings.json`
4. User configures hooks in `.claude/settings.json`
5. User invokes `/unimatrix-init` to set up CLAUDE.md awareness block
6. User invokes `/unimatrix-seed` to populate foundational knowledge
7. User begins feature work with context injection active

### Workflow 2: Knowledge Storage During Feature Work

1. Agent searches knowledge base before implementing: `context_search(query: "...", category: "pattern")`
2. Agent retrieves full content of a matching entry: `context_get(id: N, format: "markdown")`
3. Agent stores a new architectural decision: via `/store-adr` skill or direct `context_store(category: "decision", ...)`
4. Agent corrects outdated knowledge: `context_correct(original_id: N, content: "...", reason: "...")`

### Workflow 3: Post-Delivery Documentation Update

1. Delivery Leader opens PR (`gh pr create`)
2. Delivery Leader evaluates whether the feature changes user-facing behavior (FR-12d criteria)
3. If mandatory: Delivery Leader spawns `uni-docs` agent with feature artifacts
4. `uni-docs` reads SCOPE.md, SPECIFICATION.md, current README.md
5. `uni-docs` proposes and commits targeted README edits to the feature branch
6. Delivery Leader proceeds to `/review-pr` (doc updates included in reviewed PR)

### Workflow 4: Retrospective Analysis

1. Human invokes `/retro {feature-id}` after PR merge
2. `/retro` calls `context_cycle_review(feature_cycle: "{feature-id}")` to retrieve observation-based findings
3. `/retro` spawns `uni-architect` to extract patterns, procedures, and lessons
4. Extracted knowledge stored in Unimatrix under appropriate categories
5. `/record-outcome` records the retrospective completion

---

## Constraints

**C-01**: README.md is the sole documentation artifact. No docs/ subdirectory, no static site, no generated API docs. (From SCOPE.md)

**C-02**: The documentation agent reads feature artifacts (SCOPE.md, SPECIFICATION.md), not source code. This keeps the agent's task bounded and deterministic. (From SCOPE.md)

**C-03**: Protocol changes are additive. The delivery protocol modification adds one step; it does not restructure existing phases, gates, or the rework protocol. (From SCOPE.md)

**C-04**: No runtime changes. nan-005 produces markdown files and protocol edits only. No Rust code changes, no new MCP tools, no schema changes. (From SCOPE.md)

**C-05**: Skills are Claude Code platform-native. The README describes skills but cannot replace the SKILL.md files. Installation requires the npm package or manual file copying.

**C-06**: The operational guidance section references `/unimatrix-init` and `/unimatrix-seed` rather than restating their content, to avoid divergence from the nan-003 skill definitions. (SR-04 mitigation)

**C-07**: The documentation agent step has no gate. It does not block delivery. If `uni-docs` fails to produce a useful output, the Delivery Leader proceeds to `/review-pr` without documentation updates.

---

## Dependencies

### Existing Files Modified

| File | Change |
|------|--------|
| `README.md` | Complete rewrite per FR-01 through FR-10 |
| `.claude/protocols/uni/uni-delivery-protocol.md` | Add documentation step to Phase 4 per FR-12 |

### New Files Created

| File | Purpose |
|------|---------|
| `.claude/agents/uni/uni-docs.md` | Documentation agent definition per FR-11 |

### External Dependencies for Verification

| Dependency | Purpose |
|------------|---------|
| `crates/unimatrix-server/src/mcp/tools.rs` | Verify tool count, tool names, parameter names |
| `crates/unimatrix-server/src/infra/categories.rs` | Verify category names and count |
| `crates/unimatrix-server/src/main.rs` | Verify CLI subcommands and flags |
| `crates/unimatrix-store/src/migration.rs` | Verify schema version |
| `crates/unimatrix-store/src/db.rs` | Verify table count |
| `crates/unimatrix-engine/src/project.rs` | Verify data layout paths |
| `Cargo.toml` | Verify workspace crate count and rust-version |
| `packages/unimatrix/package.json` | Verify npm package name and version |
| `.claude/skills/*/SKILL.md` | Verify skill names and trigger conditions |
| `.claude/protocols/uni/uni-delivery-protocol.md` | Understand insertion point for Phase 4 step |

### No New Runtime Dependencies

nan-005 introduces no new Rust crates, npm packages, or external services.

---

## NOT in Scope

- **API documentation / rustdoc**: Internal code documentation. Out of scope per SCOPE.md.
- **Tutorial or walkthrough content**: Step-by-step guided learning. The README provides reference-grade documentation only.
- **Documentation website or static site generator**: All documentation remains in-repo markdown.
- **Changelog automation**: nan-004 already handles CHANGELOG.md.
- **Duplicating nan-003 onboarding content**: `/unimatrix-init` and `/unimatrix-seed` content is referenced, not restated.
- **Documenting internal development workflow**: Protocols, agent definitions, swarm orchestration are internal development tools; the README documents the product for external users.
- **Architecture deep-dives**: Scoring formula internals, HNSW construction parameters, SQL table definitions, detection rule internals.
- **Future capabilities**: OAuth, HTTPS transport, `_meta` agent identity, Graph Enablement features, Activity Intelligence features not yet shipped.
- **Crate-level internal documentation**: No additions to module-level doc comments in Rust source.
- **ADR creation**: nan-005 produces no architectural decisions that warrant ADR entries (no architectural choices; the structure is prescribed in SCOPE.md).

---

## Open Questions

**OQ-01 — Tool Count**: SCOPE.md states 12 MCP tools. PRODUCT-VISION.md's vinculum description lists 12 including `context_cycle_review`. Counting `#[tool(...)]` annotations in `tools.rs` yields 11 handlers. The discrepancy may be a documentation count error or `context_cycle_review` may be defined in a separate file. The implementation agent must verify the exact count before authoring the README tool reference table. Recommendation: if 11 tools exist, state 11; if 12 exist, state 12.

**OQ-02 — MicroLoRA / unimatrix-adapt Description Level**: The README currently describes MicroLoRA at a technical depth (InfoNCE loss, EWC++ regularization, contrastive learning). SCOPE.md says the README should cover "what users experience, not how it works internally" and "architecture section is minimal." However, MicroLoRA is explicitly called out in the SCOPE.md Core Capabilities framing as a differentiator. The implementation agent must determine whether MicroLoRA's technical details belong in Core Capabilities (user-facing value framing) or should be omitted. Recommendation: mention MicroLoRA by name as "adaptive embeddings that tune to project-specific usage patterns" without the technical details (InfoNCE, EWC++).

**OQ-03 — `unimatrix-learn` Crate**: The workspace contains a `unimatrix-learn` crate not present in the current README's 8-crate table and not documented in PRODUCT-VISION.md's crate list. Its `lib.rs` exports ML infrastructure and neural models. The implementation agent must verify its role and include it in the crate table (FR-08c requires the correct count of 9 crates).

---

## Knowledge Stewardship

- Queried: `/query-patterns` for documentation agent and delivery protocol patterns -- no results; no prior documentation-focused features with stored patterns in Unimatrix at time of specification authoring. Patterns established by this feature (documentation agent behavior, README accuracy checklist discipline) are candidates for retrospective extraction.
