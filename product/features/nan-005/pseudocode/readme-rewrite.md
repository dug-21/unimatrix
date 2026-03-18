# Pseudocode: readme-rewrite

## Purpose

Complete rewrite of `/README.md` as the canonical external-facing product document. 11 sections, capability-first ordering per ADR-001. Every numeric claim sourced from verified codebase values (see OVERVIEW.md Verified Facts table).

Target: 450-650 lines. Single file. No `docs/` directory (ADR-001).

---

## Section 1: Hero (5-8 lines)

### Content

Opening paragraph (2-3 sentences):
- "A self-learning knowledge engine for multi-agent software development."
- Key differentiator sentence: captures decisions, patterns, conventions, lessons from real work; delivers context automatically via hooks; confidence scoring evolves from usage signals.
- "Built in Rust. Zero cloud dependency. Ships as a single binary MCP server."

Second paragraph (2-3 sentences):
- Acknowledgments: credits claude-flow and ruvnet. Preserves existing acknowledgment language from current README (FR-01e).
- Specific callout: "hook-driven delivery architecture" and "auditable knowledge lifecycle" as the pairing that differentiates Unimatrix.

### Constraints
- No crate names in hero.
- No internal terminology (no "redb", no table counts, no formula weights).
- "Unimatrix" not "the Unimatrix" or "UniMatrix" (NFR-07).

---

## Section 2: Why Unimatrix (10-15 lines)

### Content

Problem framing (1-2 sentences):
- Multi-agent development creates knowledge that lives in context windows and dies with sessions. Decisions get re-made, patterns get re-discovered, lessons get re-learned.

Three differentiators as bullet points:

1. **Auditable Knowledge Lifecycle** — Every entry has a SHA-256 content hash. Corrections create hash-chained supersession links. An append-only audit log records every operation with agent identity and session context. You can trace how any piece of knowledge evolved.

2. **Invisible Delivery** — Agents do not need to ask for context. Hook-driven integration (Cortical Implant) injects relevant expertise into every prompt automatically via Claude Code's lifecycle hooks. Knowledge reaches agents without their cooperation.

3. **Self-Learning** — Confidence scoring evolves from real usage signals: access frequency, helpfulness votes, correction quality, creator trust, freshness decay, and co-access patterns. Entries that help get boosted; entries that mislead get downranked. Adaptive embeddings (MicroLoRA) tune search to project-specific usage patterns.

### Constraints
- No formula weights or coefficients (SCOPE.md non-goal, ALIGNMENT-REPORT WARN #2).
- MicroLoRA mentioned by name but described in user-facing terms only.

---

## Section 3: Core Capabilities (60-80 lines)

### Content

Brief intro sentence: "Unimatrix provides these capabilities out of the box."

Each capability is an H3 subsection with 3-6 lines of user-facing description.

#### 3.1 Self-Learning Knowledge Engine
- Captures decisions, patterns, conventions, procedures, lessons, and outcomes from real feature work.
- 8 knowledge categories ensure entries surface in the right context.
- Confidence scoring combines usage signals, correction quality, creator trust, freshness, helpfulness, and co-access patterns into a composite score that evolves automatically.
- No manual curation required — the system learns what is useful from how knowledge is accessed and rated.

#### 3.2 Adaptive Embeddings (MicroLoRA)
- All-MiniLM-L6-v2 ONNX model runs locally — no API calls, no cloud dependency.
- MicroLoRA layer adapts frozen embeddings to project-specific usage patterns.
- Search relevance improves over time as the system learns which entries are accessed together.
- 384-dimension vectors with HNSW index for fast approximate nearest-neighbor search.

#### 3.3 Semantic Search with Confidence-Aware Ranking
- Natural language queries return entries ranked by a combination of semantic similarity, confidence score, and co-access affinity.
- Filters by topic, category, tags, and status narrow results without losing semantic ranking.
- Near-duplicate detection (cosine similarity >= 0.92) prevents redundant entries at write time.
- Provenance boosting: lesson-learned entries get a small ranking boost in search results.

#### 3.4 Hook-Driven Invisible Delivery (Cortical Implant)
- Automatic context injection on every prompt via `UserPromptSubmit` hook.
- 5 hook events: `UserPromptSubmit`, `PreCompact`, `PreToolUse`, `PostToolUse`, `Stop`.
- Compaction resilience: `PreCompact` hook preserves critical context before Claude Code's context window compaction.
- Closed-loop feedback: `Stop` hook records session outcomes for confidence evolution.
- Sub-50ms round-trip budget per hook event. Disk-backed event queue for graceful degradation.
- Single binary — the `hook` subcommand connects to the running MCP server via Unix domain socket IPC.

#### 3.5 Retrospective Analysis
- Analyzes session telemetry for a completed feature cycle.
- 21 detection rules across 4 categories: agent behavior, friction points, session health, and scope indicators.
- Historical baselines with outlier detection surface anomalies.
- Evidence synthesis produces actionable findings with supporting data.
- Lessons and patterns extracted from retrospectives are stored back in the knowledge base with de-duplication via correction chains.

#### 3.6 Contradiction Detection
- Pairwise heuristic detection across the knowledge base identifies entries that may conflict.
- Contradictions surface in `context_status` health reports.
- Detected contradictions reduce the coherence health metric (lambda), prompting review.

#### 3.7 Correction Chains with Audit Trails
- `context_correct` creates a new entry and deprecates the original, linking them with SHA-256 content hashes (`previous_hash` chain).
- The append-only audit log records every operation: store, correct, deprecate, quarantine, enroll — with agent identity, session context, and operation outcome.
- Correction chains are tamper-evident: any break in the hash chain is detectable.

#### 3.8 Coherence Gate (Lambda Health Metric)
- Lambda is a composite health metric [0.0, 1.0] computed from four dimensions:
  - Confidence freshness — are entries' confidence scores up to date?
  - Graph quality — is the vector index structurally sound?
  - Contradiction density — how many unresolved contradictions exist?
  - Embedding consistency — do entries have valid, current embeddings?
- When lambda drops below 0.8, maintenance is recommended.
- Background tick handles maintenance automatically (confidence refresh, graph compaction, co-access cleanup).

#### 3.9 Content Scanning
- Every `context_store` and `context_correct` call scans content for:
  - Injection patterns (~25+ patterns): prompt injection attempts, system prompt overrides, encoded payloads.
  - PII patterns (6+ patterns): emails, phone numbers, API keys, credentials.
- Flagged content is rejected with a descriptive error before storage.

#### 3.10 Agent Trust Hierarchy
- 4-tier trust model: System > Privileged > Internal > Restricted.
- 4 capabilities: `read`, `write`, `search`, `admin`.
- Unknown agents auto-enroll as Restricted (read + search only) on first contact.
- Protected agents: `system` and `human` cannot be modified. Self-lockout prevention: an admin cannot remove their own Admin capability.
- `context_enroll` (Admin-only) manages agent trust levels and capabilities at runtime.

#### 3.11 Knowledge Effectiveness Analysis
- Per-entry utility scoring from injection logs and session outcomes.
- Confidence calibration validation — does predicted quality match actual usefulness?
- Dead knowledge detection — entries that are never accessed after initial storage.

### Constraints
- No formula weights, no scoring coefficients, no HNSW construction parameters (NFR-05, SCOPE.md non-goals).
- Describe capabilities from user perspective ("what you can do"), not implementation ("what was built") (FR-02b).
- MicroLoRA: "adaptive embeddings that tune to project-specific usage patterns" — no InfoNCE, no EWC++ (ALIGNMENT-REPORT WARN #2).

---

## Section 4: Getting Started (50-70 lines)

### Content

#### 4.1 Install via npm (PRIMARY)

```
npm install @dug-21/unimatrix
```

Prerequisite: Node.js >= 18.

Note: The npm package includes pre-built binaries for Linux x64. The embedding model downloads automatically on first run (or via `npx unimatrix model-download`).

#### 4.2 Build from Source (SECONDARY)

Prerequisites:
- Rust 1.89+ (edition 2024)
- ONNX Runtime 1.20.x shared library

Platform-specific ONNX Runtime instructions:

**macOS (Homebrew)**:
```
brew install onnxruntime
```

**Linux (manual)**:
```
# Download from https://github.com/microsoft/onnxruntime/releases
# Extract and set ORT_DYLIB_PATH or install to /usr/lib
```

**Devcontainer**: ONNX Runtime pre-installed. No setup needed.

Build:
```bash
cargo build --release --workspace
```

Binary at `target/release/unimatrix-server`.

#### 4.3 Configure MCP Server

Add to `.claude/settings.json`:

```json
{
  "mcpServers": {
    "unimatrix": {
      "command": "npx",
      "args": ["unimatrix"]
    }
  }
}
```

Or for build-from-source:

```json
{
  "mcpServers": {
    "unimatrix": {
      "command": "/path/to/unimatrix-server"
    }
  }
}
```

#### 4.4 Configure Hooks

Add to `.claude/settings.json`:

```json
{
  "hooks": {
    "UserPromptSubmit": [{ "command": "npx unimatrix hook UserPromptSubmit" }],
    "PreCompact": [{ "command": "npx unimatrix hook PreCompact" }],
    "PreToolUse": [{ "command": "npx unimatrix hook PreToolUse" }],
    "PostToolUse": [{ "command": "npx unimatrix hook PostToolUse" }],
    "Stop": [{ "command": "npx unimatrix hook Stop" }]
  }
}
```

#### 4.5 Cold Start

A fresh knowledge base returns empty results. Use `/unimatrix-seed` to populate foundational knowledge entries. Use `/unimatrix-init` to configure CLAUDE.md awareness and get agent recommendations.

#### 4.6 First Use Examples (FR-03d — at least 3)

**Search for existing knowledge:**
```
context_search(query: "error handling patterns", category: "pattern")
```

**Store a new decision:**
```
context_store(
  content: "Use SQLite for local storage — zero cloud dependency, single-file database.",
  topic: "nxs-008",
  category: "decision",
  title: "Storage backend choice"
)
```

**Get an orientation briefing:**
```
context_briefing(role: "developer", task: "implement new MCP tool", feature: "vnc-012")
```

### Constraints
- npm path is PRIMARY (nan-004 shipped). Build-from-source is SECONDARY (FR-03a, FR-03b).
- Cross-reference `/unimatrix-init` and `/unimatrix-seed` — do not restate their content (ADR-004, C-06).
- Hook command paths must be syntactically valid and safe (no shell metacharacters) — security risk from RISK-TEST-STRATEGY.

---

## Section 5: Tips for Maximum Value (20-30 lines)

### Content

All 7 constraints from FR-10a as numbered bullet points:

1. **Start a new session per feature cycle.** Context window pollution across features reduces knowledge quality. Each feature cycle (e.g., `col-015`) should use a fresh Claude Code session.

2. **Use feature cycle naming.** Phase prefix + number: `col-015`, `nan-005`, `vnc-012`. Used in commits, branches, issue tracking, and as the `feature_cycle` parameter in MCP tool calls.

3. **Follow commit message format.** `{prefix}: {description} (#{issue})` — see `/uni-git` for the prefix table.

4. **Category discipline matters.** The right category determines retrieval quality. Decisions (`decision`) are not conventions (`convention`); procedures (`procedure`) are not patterns (`pattern`). Miscategorized entries surface in wrong contexts during semantic search.

5. **Hook latency budget.** Hooks have a sub-50ms round-trip budget. Heavy blocking operations in hook handlers degrade the user experience.

6. **Cold start: use `/unimatrix-seed`.** A fresh knowledge base returns empty search results. `/unimatrix-seed` populates foundational entries before relying on search.

7. **Near-duplicate detection.** Entries with cosine similarity >= 0.92 to existing entries are rejected as duplicates. Rephrase if a legitimate distinct entry is rejected.

### Constraints
- Reference `/unimatrix-init` and `/unimatrix-seed` by name — do not restate content (ADR-004, FR-10b).
- No internal workflow (protocols, swarms) documented here (ADR-004).

---

## Section 6: MCP Tool Reference (50-70 lines)

### Content

Intro line: "Unimatrix exposes 11 MCP tools. All tools accept `format: "summary" | "markdown" | "json"` as a common parameter (FR-04c)."

Table format: `| Tool | Purpose | When to Use |`

Each row includes the exact tool name from `#[tool(name = "...")]` in tools.rs.

| Tool | Purpose | When to Use |
|------|---------|-------------|
| `context_search` | Search for relevant context using natural language. Returns semantically similar entries ranked by relevance. | When you need to find patterns, conventions, or decisions related to a concept. Use when you do NOT know exactly what you are looking for. Key params: `query` (required), `category`, `topic`, `tags`, `k` (default 5), `helpful`. |
| `context_lookup` | Look up context entries by exact filters. Returns entries matching topic, category, tags, status, or ID. | When you KNOW what you are looking for — a specific feature's entries, a category listing, or a known ID. Key params: `topic`, `category`, `tags`, `id`, `status`, `limit` (default 10). |
| `context_get` | Get a specific context entry by its ID. | When you have an entry ID from a previous search or lookup result and need the full content. Key params: `id` (required), `helpful`. |
| `context_store` | Store a new context entry with duplicate detection and content scanning. | When you discover a pattern, convention, decision, or lesson worth preserving. Key params: `content` (required), `topic` (required), `category` (required), `tags`, `title`, `feature_cycle`. |
| `context_correct` | Correct an existing entry. Deprecates the original and creates a new entry with a hash-chain link. | When an entry contains wrong or outdated information that should be superseded (not just hidden). Key params: `original_id` (required), `content` (required), `reason`. |
| `context_deprecate` | Mark an entry as outdated. Entry remains accessible but excluded from default search/lookup. | When knowledge is no longer relevant but should not be deleted (historical record). Key params: `id` (required), `reason`. |
| `context_quarantine` | Quarantine or restore an entry. Quarantined entries are excluded from search and lookup. **Admin only.** | When an entry is suspicious, invalid, or harmful and should be isolated. Use `action: "restore"` to undo. Key params: `id` (required), `action` ("quarantine" or "restore"), `reason`. |
| `context_status` | Get knowledge base health metrics. Shows entry counts, distributions, correction chains, coherence score, security metrics. **Admin only.** | When you need to assess knowledge base health. `maintain` parameter is accepted but silently ignored — background tick handles maintenance automatically (FR-04e). Key params: `topic`, `category`, `check_embeddings`. |
| `context_briefing` | Get an orientation briefing for a role and task. Includes role conventions and task-relevant context. | At the start of any task to get oriented. Gated on `mcp-briefing` feature flag (FR-04d). Key params: `role` (required), `task` (required), `feature`, `max_tokens` (default 3000, range 500-10000). |
| `context_enroll` | Enroll or update an agent's trust level and capabilities. **Admin only.** | When managing agent permissions. Protected agents (`system`, `human`) cannot be modified. Self-lockout prevention active. Key params: `target_agent_id` (required), `trust_level` (required), `capabilities` (required). |
| `context_cycle_review` | Analyze observation data for a work cycle. Parses session telemetry, detects hotspots, computes metrics. | After a work cycle completes, to extract patterns and lessons. Key params: `feature_cycle` (required), `evidence_limit`, `format` ("markdown" default, "json"). |

Post-table notes:
- **search vs. lookup**: `context_search` uses semantic similarity (natural language). `context_lookup` uses exact filters (topic, category, tags, status). Use search when exploring; use lookup when you know what you want.
- **correct vs. deprecate vs. quarantine**: `context_correct` supersedes with a new version (hash-chained). `context_deprecate` marks as outdated (no replacement). `context_quarantine` isolates from all results (Admin-only, reversible).

### Constraints
- Exactly 11 rows (verified count). Not 12 (SCOPE.md claim incorrect, ALIGNMENT-REPORT WARN #1).
- Tool names use exact registered form: `context_search` not `contextSearch` (NFR-07).
- `maintain` documented as silently ignored (FR-04e, R-03).
- `context_briefing` feature flag note (FR-04d).
- Admin-only tools noted: `context_status`, `context_quarantine`, `context_enroll` (FR-04b).

---

## Section 7: Skills Reference (30-40 lines)

### Content

Intro line: "Unimatrix includes 14 Claude Code skills. Skills are platform-native `/command` files installed via the npm package or by copying `.claude/skills/` directories to the target repository (FR-05c)."

Note: "Skills that interact with the MCP server require the server to be running and configured (FR-05d)."

Table format: `| Skill | Purpose | When to Use |`

| Skill | Purpose | When to Use |
|-------|---------|-------------|
| `/query-patterns` | Search for patterns, procedures, and conventions before work. (MCP) | Before designing or implementing any component. |
| `/store-adr` | Store an architectural decision record in Unimatrix. (MCP) | After each design decision during architecture work. |
| `/store-pattern` | Store a reusable implementation pattern. (MCP) | When you discover a gotcha, trap, or reusable solution. |
| `/store-procedure` | Store or update a technical procedure (how-to). (MCP) | During retrospectives when a technique has evolved. |
| `/store-lesson` | Store a lesson learned from a failure or unexpected issue. (MCP) | After bugfixes, gate failures, or rework cycles. |
| `/record-outcome` | Record a feature or bugfix outcome. (MCP) | At the end of every session (design, delivery, bugfix, retrospective). |
| `/knowledge-search` | Interactive semantic search across knowledge. (MCP) | When exploring a topic or looking for related entries. |
| `/knowledge-lookup` | Interactive deterministic lookup by exact filters. (MCP) | When you know what you want — a specific feature, category, or ID. |
| `/review-pr` | PR security review and merge readiness check. | After delivery or bugfix opens a PR. Can be invoked standalone. |
| `/retro` | Post-merge retrospective — extract patterns, procedures, lessons. (MCP) | After a feature PR is merged. |
| `/uni-git` | Git workflow conventions (branch naming, commit prefixes, PR templates). | For consistent git conventions. Contributor/developer-focused. |
| `/release` | Version bump, changelog generation, tag, and release pipeline. | When creating a new release. |
| `/unimatrix-init` | Initialize Unimatrix in a repository — CLAUDE.md setup + agent recommendations. | First-time setup of a repo to use Unimatrix. |
| `/unimatrix-seed` | Populate foundational knowledge through human-directed exploration. (MCP) | After installation, to seed the knowledge base before relying on search. |

### Constraints
- Exactly 14 rows (verified count).
- Skills use invocation form with leading slash: `/query-patterns` not `query-patterns` (NFR-07).
- `/uni-git` included with note that it is contributor/developer-focused (OQ-04 resolution).
- Skills requiring MCP server marked with (MCP) suffix (FR-05d).

---

## Section 8: Knowledge Categories (25-35 lines)

### Content

Intro: "Unimatrix uses 8 built-in knowledge categories. Category discipline matters for retrieval quality — miscategorized entries surface in wrong contexts during semantic search (FR-06c)."

Table format: `| Category | Description | Example |`

| Category | Description | Example |
|----------|-------------|---------|
| `outcome` | Session completion records — what shipped, how it went. | "col-015 delivered. All gates passed. PR #42 merged." |
| `lesson-learned` | Lessons from failures, gate rejections, unexpected issues. | "Always verify hook latency after adding new UDS handlers — we hit 200ms in col-008." |
| `decision` | Architectural and design decisions (ADRs). | "Use SQLite for local storage — single-file, zero cloud dependency, bundled via rusqlite." |
| `convention` | Project conventions and rules agents should follow. | "All MCP tool handlers follow the execution order: identity -> capability -> validation -> category -> scanning -> business logic -> format -> audit." |
| `pattern` | Reusable implementation patterns, gotchas, and solutions. | "Do not hold Store lock across async boundaries — use spawn_blocking for all Store calls." |
| `procedure` | Step-by-step technical procedures (how-to). | "How to add a new MCP tool: 1. Define params struct, 2. Implement handler, 3. Add validation, 4. Add audit event." |
| `duties` | Role duties for context_briefing orientation. | "Architect duties: read SCOPE.md, decompose into components, define integration surface, produce ADRs." |
| `reference` | General reference material. | "ONNX Runtime 1.20.x compatibility matrix for supported platforms." |

Post-table note: "The category allowlist is runtime-extensible via `add_category()`, but the 8 built-in categories cover the primary use cases (FR-06d)."

### Constraints
- All 8 categories from INITIAL_CATEGORIES in categories.rs: outcome, lesson-learned, decision, convention, pattern, procedure, duties, reference.
- Category names use exact strings as used in `context_store` calls (NFR-07).

---

## Section 9: CLI Reference (25-35 lines)

### Content

Intro: "The `unimatrix` binary (or `npx unimatrix`) serves as both the MCP server and the hook handler."

#### Default mode (no subcommand)
Starts the MCP server over stdio. This is what the MCP server configuration invokes.

#### Subcommands

Table format: `| Subcommand | Description | Key Flags |`

| Subcommand | Description | Key Flags |
|------------|-------------|-----------|
| `hook <EVENT>` | Handle a Claude Code lifecycle hook event. Reads JSON from stdin, connects to the running server via UDS. Designed for use in `.claude/settings.json` hook configuration, not direct user invocation (FR-07c). | Event name as positional arg. |
| `export` | Export the knowledge base to JSONL format. No running server required. | `--output <PATH>` (defaults to stdout) |
| `import` | Import a knowledge base from a JSONL export file. Re-embeds entries and rebuilds vector index. | `--input <PATH>` (required), `--skip-hash-validation`, `--force` (drop existing data) |
| `version` | Print version and exit. With `--project-dir`, also initializes the database. | `--project-dir <PATH>` |
| `model-download` | Download the ONNX embedding model to cache. Used by npm postinstall. | None |

#### Global Flags

| Flag | Description |
|------|-------------|
| `--project-dir <PATH>` | Override automatic project root detection. |
| `--verbose` / `-v` | Enable debug-level logging to stderr. |

### Constraints
- 5 subcommands verified from Command enum in main.rs: Hook, Export, Import, Version, ModelDownload.
- Hook subcommand note per FR-07c.

---

## Section 10: Architecture Overview (20-40 lines)

### Content

Intro: "Unimatrix is a 9-crate Rust workspace that ships as a single binary."

#### Storage
SQLite local database (`unimatrix.db`). 19 tables. Schema version 11. Zero cloud dependency — all data stays on your machine.

#### Vector Search
384-dimension HNSW vector index (in-memory, persisted to disk). Dot product similarity.

#### Embedding
Local ONNX model (all-MiniLM-L6-v2) via ONNX Runtime. No API calls. MicroLoRA adaptive layer tunes embeddings to project-specific usage.

#### Hook Integration
Single binary. The `hook` subcommand communicates with the running MCP server via Unix domain socket (UDS) IPC. Sub-50ms round-trip budget.

#### MCP Transport
stdio transport via rmcp. Claude Code connects to the binary as an MCP server.

#### Data Layout
```
~/.unimatrix/{project-hash}/
  unimatrix.db               # SQLite knowledge database (schema v11)
  unimatrix.pid              # PID file with flock advisory lock
  unimatrix.sock             # Unix domain socket for hook IPC
  vector/
    unimatrix-vector.hnsw2   # HNSW graph
    unimatrix-vector.meta    # Index metadata
~/.cache/unimatrix/models/   # ONNX model files (downloaded once)
```

#### Crate Workspace (9 crates)

| Crate | Responsibility |
|-------|---------------|
| `unimatrix-store` | SQLite storage engine — entries, indexes, audit log, migrations |
| `unimatrix-vector` | HNSW vector index — build, search, persist, compact |
| `unimatrix-embed` | ONNX embedding pipeline — model loading, tokenization, inference |
| `unimatrix-core` | Core traits, domain types, async wrappers, query filters |
| `unimatrix-engine` | Shared business logic — confidence scoring, project paths, search ranking |
| `unimatrix-adapt` | Adaptive embedding pipeline — MicroLoRA training, state persistence |
| `unimatrix-observe` | Observation pipeline — hotspot detection, metric computation, retrospective analysis |
| `unimatrix-learn` | Shared ML infrastructure — training reservoirs, EWC++ state, neural models, model versioning |
| `unimatrix-server` | MCP server — tool handlers, hook IPC, agent registry, audit, content scanning |

### Constraints
- 20-40 lines target (NFR-05).
- No crate internals, formula weights, HNSW construction parameters, table schemas, detection rule logic (FR-08e).
- SQLite not redb (FR-08a). Correct file names: `.db` not `.redb` (FR-01b).
- 9 crates verified from workspace (includes unimatrix-learn).

---

## Section 11: Security Model (20-30 lines)

### Content

#### Trust Hierarchy
4-tier model: System > Privileged > Internal > Restricted.
- Unknown agents auto-enroll as Restricted on first contact (read + search only).
- `context_enroll` (Admin-only) promotes or modifies agent trust and capabilities.
- Protected agents: `system` and `human` cannot be modified.
- Self-lockout prevention: an admin cannot remove their own Admin capability.

#### Capabilities
4 capabilities gate tool access: `read`, `write`, `search`, `admin`.

#### Content Scanning
Every write operation (`context_store`, `context_correct`) scans content for:
- Injection patterns (~25+): prompt injection, system prompt overrides, encoded payloads.
- PII patterns (6+): emails, phone numbers, API keys, credentials.
Flagged content is rejected before storage.

#### Audit Trail
Append-only audit log records every operation with:
- Agent identity (who performed the action)
- Session context (which session, feature cycle)
- Operation outcome (success/failure)

#### Hash-Chained Corrections
SHA-256 content hashes with `previous_hash` links create tamper-evident correction chains. Any break in the chain is detectable.

### Constraints
- User-facing summary only. No implementation details (FR-09a through FR-09f).
- MUST NOT mention OAuth, HTTPS transport, or `_meta` agent identity — these are not implemented (FR-09g, R-10).
- No "will", "planned", "future" language about security capabilities (R-08).

---

## Section 12: Acknowledgments (preserved from current README)

### Content

Preserve the existing acknowledgments paragraph crediting claude-flow and ruvnet. This is already in the hero section — if kept there, no separate section needed. If the implementation agent prefers a separate section at the bottom, that is also acceptable. The key requirement is that the acknowledgment text is preserved (FR-01e).

---

## Structural Requirements

- H2 (`##`) for main sections, H3 (`###`) for subsections (NFR-03).
- Tables for reference data: tools, skills, categories, CLI (NFR-03).
- No "TODO", "TBD", placeholder text, or aspirational language (NFR-04).
- "Unimatrix" not "UniMatrix" or "the Unimatrix" throughout (NFR-07).
- Tool names: `context_search` not `contextSearch` (NFR-07).
- Skill names: `/query-patterns` not `query-patterns` (NFR-07).
- "SQLite" consistently — no "sqlite", "SQLITE", or "redb" (NFR-07).

---

## Fact Verification Checklist (Completed)

| Claim | Verification Command | Verified Value |
|-------|---------------------|---------------|
| MCP tool count | `grep -c '#[tool(' tools.rs` | **11** |
| Skill count | `ls .claude/skills/ \| wc -l` | **14** |
| Crate count | `ls crates/` | **9** |
| Schema version | `grep CURRENT_SCHEMA_VERSION migration.rs` | **11** |
| SQLite table count | `grep -c 'CREATE TABLE IF NOT EXISTS' db.rs` | **19** |
| Rust version | `grep rust-version Cargo.toml` | **1.89** |
| npm package name | `jq .name packages/unimatrix/package.json` | **@dug-21/unimatrix** |
| Test count | `grep -r '#[test]' crates/ \| wc -l` | **2131+** |
| Storage backend | `grep rusqlite unimatrix-store/Cargo.toml` | **rusqlite 0.34 bundled** |
| Database filename | project.rs `unimatrix.db` | **unimatrix.db** |
| Hook events | hook.rs grep | **UserPromptSubmit, PreCompact, PreToolUse, PostToolUse, Stop** |
| `maintain` behavior | tools.rs comment | **Silently ignored (col-013)** |
| CLI subcommands | Command enum in main.rs | **hook, export, import, version, model-download** |
| Node.js requirement | package.json engines | **>=18** |

---

## Key Test Scenarios

1. `grep -r "redb\|\.redb" README.md` returns no matches (R-01).
2. MCP Tool Reference table has exactly 11 rows matching `#[tool(` count (R-04).
3. Skills Reference table has exactly 14 rows matching `ls .claude/skills/` count (R-11).
4. `context_status` entry does NOT say `maintain=true` triggers maintenance (R-03).
5. Security section does NOT mention OAuth, HTTPS transport, or `_meta` (R-10).
6. `grep -n "will\|planned\|future\|coming soon" README.md` returns no content-bearing matches (R-08).
7. All 7 operational constraints from FR-10a findable by keyword (R-05 from RISK-TEST-STRATEGY).
8. Acknowledgments paragraph preserved (R-13).
9. All 8 category names from categories.rs present (AC-11).
10. Data layout shows `unimatrix.db` not `unimatrix.redb` (AC-10).
11. Crate list has 9 entries (AC-10).
12. Total line count under 800 (ADR-001 threshold).
