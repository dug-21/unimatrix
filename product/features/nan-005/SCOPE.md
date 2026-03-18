# nan-005: Documentation & Onboarding

## Problem Statement

Unimatrix has shipped 44+ features across 5 phases, has 12 MCP tools, 14 skills, 15 agent definitions, and ~1,700+ tests -- but the project's documentation story has two significant gaps:

1. **The README is internally-focused and already stale.** The existing README.md (~310 lines) was written for developers working on Unimatrix itself. It references redb in several places (architecture table, project structure, data layout) despite the project having migrated to SQLite. It describes 17 tables when the schema has evolved. It does not explain the project's value proposition for someone evaluating adoption, does not document constraints that new users hit immediately (e.g., new sessions per feature cycle, phase-prefixed feature naming), and does not reference the 14 skills that are the primary interaction surface.

2. **Documentation decays silently.** Every shipped feature potentially changes capabilities, tool behavior, constraints, or best practices. There is no mechanism to propagate these changes into documentation. The README already demonstrates this -- it was written once and has drifted from reality. Without a systematic update mechanism, documentation will always lag the product.

The product vision explicitly calls for both: a comprehensive README (features, MCP tool reference, benefits, constraints, workflow guidance, skills reference) and a "documentation agent added to protocols" that automatically updates docs after each shipped feature.

## Goals

1. Rewrite README.md as a comprehensive external-facing document that enables a new user to understand what Unimatrix is, why they would use it, what tools and skills are available, and how to get started -- without reading source code.
2. Create a detailed MCP tool reference documenting every tool's purpose, parameters, when to use it vs. alternatives, and practical examples.
3. Create a skills reference documenting all 14 skills with their purpose and trigger conditions.
4. Document operational constraints and workflow guidance that new users need (session boundaries, feature cycle naming, phase prefixes, category conventions).
5. Add a documentation agent to the design and delivery protocols that updates documentation artifacts after each shipped feature.

## Non-Goals

- **API documentation / rustdoc** -- Internal code documentation is out of scope. This is user-facing product documentation.
- **Tutorial or walkthrough content** -- Step-by-step tutorials for specific workflows. The README provides reference-grade documentation, not guided learning.
- **Documentation website or static site generator** -- Documentation lives in the repository as markdown files. No hosting, no build pipeline.
- **Changelog automation** -- nan-004 already handles CHANGELOG.md generation from conventional commits.
- **Duplicating nan-003 onboarding content** -- `/unimatrix-init` and `/unimatrix-seed` handle per-repo onboarding. nan-005 documents the product itself, not the per-repo setup process.
- **Documenting Unimatrix's internal development workflow** -- The protocols, agent definitions, and swarm orchestration are internal development tools, not user-facing product features.
- **Architecture deep-dives** -- The 8-crate workspace structure, scoring formula weights, and detection rule internals are implementation details. Documentation covers what users need to know (e.g., "confidence scoring ranks entries by quality") not how it works internally.

## Background Research

### Current Documentation State

**README.md (310 lines, exists):** Contains a product description, feature overview (knowledge engine, MCP tools table, confidence scoring details, coherence gate, security, hook-driven delivery, observation pipeline), architecture section (8 crates table, data layout), getting started (prerequisites, build, MCP config, hooks config, usage examples, CLI flags), test instructions, knowledge categories, and project structure. Several sections are stale:
- References redb in the architecture table, project structure, and data layout despite SQLite migration (nxs-008)
- Lists 11 tools in the MCP table (missing `context_cycle_review` which was added in vnc-011, and the table header says 12 but only lists 11 entries -- `context_cycle_review` row is present but description is cut)
- Data layout shows `.redb` file extension
- Crate descriptions reference redb
- Test count says "1,500+" (actual is higher after recent features)

**docs/ directory:** Contains only pre-roadmap planning (`docs/planning/pre-roadmap-spike.md`) and early research artifacts (`docs/research/` with 8 subdirectories of initial landscape analysis). None of this is user-facing documentation.

**Server instructions (embedded in binary):** A single sentence in `server.rs` (SERVER_INSTRUCTIONS constant) that tells agents to search before implementing and store findings. This is the only behavioral guidance baked into the MCP server itself.

### MCP Tools (12 tools, all in `crates/unimatrix-server/src/mcp/tools.rs`)

Each tool has a `name`, `description` (one sentence), and typed parameters with doc comments. Current tool descriptions are brief -- suitable for MCP tool discovery but insufficient for understanding when/why to use each tool:

| Tool | Current Description (abbreviated) | Needs |
|------|----------------------------------|-------|
| `context_search` | Semantic search by natural language | When to use vs. lookup, practical query strategies |
| `context_lookup` | Deterministic retrieval by exact filters | Filter combinations, topic/category/tag patterns |
| `context_get` | Get entry by ID | When to use (follow-up from search/lookup results) |
| `context_store` | Store new context entry | Category guidance, quality expectations, duplicate handling |
| `context_correct` | Correct existing entry (hash chain) | vs. deprecate, correction chain behavior |
| `context_deprecate` | Mark knowledge as outdated | When to deprecate vs. correct vs. quarantine |
| `context_quarantine` | Isolate suspicious entries | Admin-only, restore capability |
| `context_status` | Health metrics and maintenance | maintain=true behavior, coherence dimensions |
| `context_briefing` | Role+task orientation | Token budget, role/task parameters, hook integration |
| `context_enroll` | Manage agent trust/capabilities | Trust hierarchy, capability list, admin-only |
| `context_cycle_review` | Analyze session telemetry | Feature cycle parameter, evidence limit, format options |

### Skills (14 skills in `.claude/skills/`)

| Skill | Purpose |
|-------|---------|
| `/query-patterns` | Search for patterns/conventions before work |
| `/store-adr` | Record architectural decisions |
| `/store-pattern` | Record reusable patterns |
| `/store-procedure` | Record step-by-step techniques |
| `/store-lesson` | Record lessons learned from failures |
| `/record-outcome` | Record session outcomes |
| `/knowledge-search` | Interactive knowledge search |
| `/knowledge-lookup` | Interactive knowledge lookup |
| `/review-pr` | PR security review and merge readiness |
| `/retro` | Run retrospective analysis |
| `/uni-git` | Git workflow conventions |
| `/release` | Version bump and release pipeline |
| `/unimatrix-init` | CLAUDE.md setup + agent recommendations (nan-003) |
| `/unimatrix-seed` | Conversational knowledge seeding (nan-003) |

### Protocol Structure for Documentation Agent

The design protocol (`uni-design-protocol.md`) follows a phased flow: Phase 1 (research/scope) -> Phase 1b (scope risk) -> Phase 2a (arch+spec) -> Phase 2a+ (risk) -> Phase 2b (vision) -> Phase 2c (synthesis) -> Phase 2d (commit/PR). Session ends at Phase 2d.

The delivery protocol (`uni-delivery-protocol.md`) follows: Stage 3a (pseudocode+tests) -> Gate 3a -> Stage 3b (implementation) -> Gate 3b -> Stage 3c (testing) -> Gate 3c -> Phase 4 (delivery: commit, push, PR, review).

A documentation agent would fit as a **post-delivery step in the delivery protocol** (after Phase 4, before outcome recording). It would read the completed feature's artifacts (SCOPE.md, ARCHITECTURE.md, SPECIFICATION.md) and update documentation files accordingly. It could also fit as a **post-PR-review step** if documentation updates should be part of the PR.

Key design question: should the documentation agent update docs within the feature PR (same branch, pre-merge) or as a follow-up commit on main (post-merge)? Within the PR is more traceable and reviewable. Post-merge risks doc updates being forgotten.

### Existing README Factual Errors to Fix

1. Architecture table says `unimatrix-store` uses "redb storage engine" -- should be SQLite
2. Data layout shows `unimatrix.redb` -- should be `unimatrix.db`
3. Project structure says `unimatrix-store/  # redb storage engine` -- should reference SQLite
4. Getting started section says "Rust 1.89+" -- needs verification against current rust-version
5. Crate count says 8 -- need to verify against workspace
6. The "17-table" claim in features needs verification against current schema (v11)

### nan Feature Pattern

Prior nan features (nan-001 through nan-004) follow the standard SCOPE.md pattern with detailed background research based on actual codebase exploration, precise acceptance criteria with AC-IDs, and explicit constraints. nan-001 and nan-002 are CLI subcommands (code features). nan-003 is skill files (markdown). nan-004 is packaging infrastructure (npm + CI). nan-005 is documentation (markdown) + protocol changes (markdown edits).

## Proposed Approach

### Deliverable 1: README.md Rewrite

Restructure README.md for an external audience. Capability-first framing — lead with what users can do, not what was built. Sections:

1. **Hero section** -- What Unimatrix is, core value proposition (2-3 sentences)
2. **Why Unimatrix** -- Problem it solves, key differentiators (auditable lifecycle, invisible delivery, self-learning)
3. **Core Capabilities** -- Self-learning (MicroLoRA, confidence evolution), semantic search with scoring, hook-driven invisible delivery, retrospective analysis, contradiction detection, correction chains with audit trails. Grouped by what users experience, not by crate.
4. **Getting Started** -- npm install (primary, nan-004), build-from-source (secondary), MCP config, hooks config
5. **Tips for Maximum Value** -- New session per feature, workflow naming with `:`, `/unimatrix-seed` for cold start, category discipline, feature cycle conventions
6. **MCP Tool Reference** -- Table with name, purpose, when-to-use guidance (not exhaustive params)
7. **Skills Reference** -- Table with name, purpose, trigger conditions
8. **Knowledge Categories** -- What each category is for, with examples
9. **CLI Reference** -- Subcommands and flags
10. **Architecture** -- Minimal high-level: SQLite local storage, hook integration, MCP transport. No crate-by-crate breakdown.
11. **Security Model** -- Trust hierarchy, content scanning, audit trail (user-facing summary)

The README stays as a single file. It is the canonical entry point. A separate `docs/` directory is not needed at this stage -- the README covers the full surface area.

### Deliverable 2: Documentation Agent in Protocols

Add a documentation update step to the delivery protocol after Phase 4 (PR creation, before outcome recording). The documentation agent:

1. Reads the feature's SCOPE.md and SPECIFICATION.md to understand what was delivered
2. Reads current README.md
3. Identifies sections that need updating (new tools, new skills, changed capabilities, new constraints)
4. Proposes specific edits to README.md
5. Commits documentation updates to the feature branch (part of the same PR)

This is implemented as:
- A new agent definition: `.claude/agents/uni/uni-docs.md`
- A protocol modification: add a documentation step to `uni-delivery-protocol.md` Phase 4
- The agent is lightweight -- it reads artifacts and proposes README edits, not a full rewrite each time

The documentation agent is **optional** in the protocol -- the Delivery Leader invokes it only when the feature changes user-facing behavior (new tools, new skills, changed constraints). Pure internal refactors or test-only features skip it.

### Deliverable 3: Operational Constraints Documentation

A dedicated section in the README covering constraints that new users discover the hard way:

- **Session boundaries**: Each feature cycle should use a new Claude Code session. Context window pollution across features reduces quality.
- **Feature cycle naming**: Phase prefix + number (e.g., `col-015`). Used in commits, branches, issue tracking, and Unimatrix entries.
- **Phase prefixes with colons**: Commit messages use `{phase}: {description} (#{issue})` format (e.g., `impl: add confidence scoring (#30)`).
- **Category discipline**: Use the right category (`decision` vs `convention` vs `pattern` vs `procedure`). Miscategorized entries reduce retrieval quality.
- **Hook latency budget**: Hooks have a ~50ms round-trip budget. Heavy operations in hooks degrade the user experience.
- **Knowledge base cold start**: A fresh knowledge base returns empty results. Use `/unimatrix-seed` to populate foundational entries.

## Acceptance Criteria

- AC-01: README.md is rewritten with sections covering: product description, value proposition, features overview, MCP tool reference, skills reference, getting started, operational guidance, architecture overview, CLI reference, knowledge categories, and security model.
- AC-02: The MCP tool reference section documents all 12 tools with: name, one-line purpose, key parameters (not exhaustive -- the important ones), and a "when to use" note distinguishing it from similar tools (e.g., search vs. lookup, correct vs. deprecate vs. quarantine).
- AC-03: The skills reference section documents all 14 skills with: name, one-line purpose, and trigger condition (when to invoke).
- AC-04: All factual errors in the current README are corrected: redb references replaced with SQLite, data layout updated, crate descriptions updated, test counts updated, tool table corrected.
- AC-05: An operational guidance section documents at minimum: session boundaries, feature cycle naming conventions, phase prefix commit format, category discipline, and knowledge base cold start mitigation.
- AC-06: A `uni-docs` agent definition exists at `.claude/agents/uni/uni-docs.md` with instructions to read feature artifacts and propose README updates.
- AC-07: The delivery protocol (`uni-delivery-protocol.md`) includes a documentation update step in Phase 4 (after PR creation) that spawns `uni-docs` when the feature changes user-facing behavior.
- AC-08: The documentation agent step in the delivery protocol is explicitly optional -- the Delivery Leader determines whether the feature warrants documentation updates based on whether it adds/changes tools, skills, constraints, or user-visible capabilities.
- AC-09: The README getting-started section includes both the npm install path (referencing nan-004) and the build-from-source path, with prerequisites for each.
- AC-10: The architecture section correctly describes the current crate structure, storage backend (SQLite), and data directory layout with accurate file names and paths.
- AC-11: The knowledge categories section explains each of the 8 categories with a one-line description and an example use case, enabling a new user to choose the right category when storing knowledge.
- AC-12: README content is factually accurate against the current codebase -- no aspirational features, no stale references, no placeholder content.

## Constraints

- **README is the sole documentation artifact.** No separate docs site, no generated API docs, no tutorial pages. A single README.md is the entry point. This keeps maintenance tractable for the documentation agent.
- **Documentation agent reads artifacts, not code.** The agent updates README based on feature SCOPE.md and SPECIFICATION.md, not by reading implementation code. This keeps the agent's task bounded and deterministic.
- **Protocol changes are additive.** The delivery protocol modification adds a step; it does not restructure existing phases or gates. The documentation step has no gate -- it is advisory, not blocking.
- **Skills are Claude Code platform-native.** Skill documentation in the README describes what each skill does and when to use it, but cannot replace the SKILL.md files themselves. Users must have skills installed (via nan-004) to invoke them.
- **README accuracy depends on manual discipline until the documentation agent is operational.** The initial README rewrite is a point-in-time snapshot. The documentation agent prevents future drift, but only for features delivered through the protocol.
- **No runtime changes.** nan-005 produces markdown files and protocol edits only. No Rust code changes, no new MCP tools, no schema changes.

## Resolved Questions

1. **Documentation agent placement**: Before `/review-pr`, so doc updates are part of the reviewed PR.
2. **README structure**: Single file. No splitting.
3. **Stale detection**: Incremental per-feature only. No full validation pass.
4. **npm install path**: nan-004 is shipped. Document the npm install path as the primary path.

## Framing Decision (Human Direction)

**The README should emphasize what users can DO, not what was built.** The focus is capabilities and value — not crate lists or internal architecture. Key framing:

- **Core capabilities front and center**: self-learning knowledge engine, MicroLoRA adaptive embeddings, semantic search with confidence scoring, hook-driven invisible delivery, retrospective analysis (full workflow insight), contradiction detection, correction chains with audit trails.
- **Practical tips for maximum value**: start a new session per feature, workflow definitions use phase prefixes ending in `:`, use `/unimatrix-seed` for cold start, category discipline matters for retrieval quality.
- **Architecture section is minimal** — high-level only, not crate-by-crate breakdown. Users care about SQLite (local, no cloud), hook integration, and MCP transport — not internal module boundaries.
- **Tool and skill references focus on "when and why"** — not exhaustive parameter lists.

## Tracking

https://github.com/dug-21/unimatrix/issues/226
