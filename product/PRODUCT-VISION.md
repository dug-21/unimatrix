# Unimatrix — Product Vision & Feature Roadmap

## Vision

Unimatrix is a self-learning context engine that serves as the knowledge backbone for multi-agent development orchestration — accumulating conventions, decisions, patterns, and process intelligence across feature cycles, then delivering the right context to the right agent at the right workflow moment. Over time, it evolves from a knowledge store into a workflow-aware system that proposes process improvements from evidence, supports multiple concurrent projects, and provides a real-time interface for human visibility and control.

## Strategic Approach

Start with Proposal A (Knowledge Oracle) — a focused, testable knowledge store. Evolve incrementally toward Proposal C (Workflow-Aware Hybrid) — adding usage tracking, outcome analysis, retrospective intelligence, and eventually thin-shell agent files. Each milestone is independently shippable and provable. The schema pre-seeds all known future fields from day 1, covering M2–M5 without schema changes. When new fields are added (M6+), a `schema_version` counter triggers automatic scan-and-rewrite migration on database open — fast at Unimatrix scale.

---

## Feature Roadmap

### Milestone 1: Foundation (Nexus Phase — `nxs`)

**Goal**: Ship a working knowledge store that agents can read from and write to via MCP.

| Feature | Prefix | Summary |
|---------|--------|---------|
| Embedded Storage Engine | `nxs-001` | redb-backed entry store with 8 tables (ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP, COUNTERS). bincode v2 serialization. EntryRecord schema pre-seeds all known future fields (M2–M5). Schema versioning via COUNTERS table with scan-and-rewrite migration when fields are added. |
| Vector Index | `nxs-002` | hnsw_rs integration — 384-dimension embeddings (all-MiniLM-L6-v2), DistDot, 16 max connections, ef_construction=200. VECTOR_MAP bridge table between entry IDs and hnsw data IDs. |
| Embedding Pipeline | `nxs-003` | Local embedding generation via ONNX runtime or API-based fallback. Title+content concatenation strategy. Batch embedding on import. |
| Core Traits & Domain Adapters | `nxs-004` | Storage traits (EntryStore, VectorStore, IndexStore) in core crate. Domain adapter pattern — implementations in domain modules. `spawn_blocking` with `Arc<Database>` for async. |

**Ships**: Functional storage + retrieval backend. No MCP yet — internal API only.

---

### Milestone 2: MCP Server (Vinculum Phase — `vnc`)

**Goal**: Expose the knowledge engine to Claude Code via MCP stdio transport.

| Feature | Prefix | Summary |
|---------|--------|---------|
| MCP Server Core | `vnc-001` | rmcp 0.16 SDK, stdio transport. Server `instructions` field for behavioral driving (70-85% agent compliance). Auto-init on first `context_store`. Project isolation via `~/.unimatrix/{project_hash}/`. |
| v0.1 Tools | `vnc-002` | `context_search` (semantic, query-driven, returns top-k with similarity scores), `context_lookup` (deterministic, metadata-driven, category/topic/tags filters), `context_store` (with near-duplicate detection at 0.92 threshold), `context_get` (full entry by ID). Dual response format: compact markdown in `content`, JSON in `structuredContent`. |
| v0.2 Tools | `vnc-003` | `context_correct` (supersede with correction chain), `context_deprecate` (mark irrelevant), `context_status` (health metrics — counts, age distribution, stale entries, duplicate candidates), `context_briefing` (compiled orientation — lookup duties + conventions + search task-relevant patterns in one call, <2000 token target). |

**Ships**: Agents can search, store, correct, and receive briefings. Knowledge accumulates across features.

---

### Milestone 3: Agent Integration (Alcove Phase — `alc`)

**Goal**: Establish the behavioral driving chain so agents reliably use Unimatrix without manual prompting.

| Feature | Prefix | Summary |
|---------|--------|---------|
| CLAUDE.md Integration | `alc-001` | `unimatrix init` CLI command appends Unimatrix block to CLAUDE.md. Reinforces server instructions (~90% compliance). Documents category conventions (outcome, lesson-learned, decision, convention, pattern, procedure). |
| Agent Orientation Pattern | `alc-002` | Agent definition template with `## Orientation` section containing `context_briefing` call. `## Outcome Reporting` section for end-of-task `context_store`. Three-layer behavioral chain: server instructions → CLAUDE.md → agent file. |
| Starter Kit | `alc-003` | Repo template with generic agents (architect, developer, tester, validator), standard protocols (planning, implementation), and Unimatrix-aware agent structure. Reduces new project setup pain. |

**Ships**: New projects get Unimatrix integration out of the box. Agents orient and report without custom per-project configuration.

---

### Milestone 4: Learning & Drift (Cortical Phase — `crt`)

**Goal**: Turn passive knowledge accumulation into active learning — the bridge from Proposal A to C.

| Feature | Prefix | Summary |
|---------|--------|---------|
| Usage Tracking | `crt-001` | USAGE_LOG table — every retrieval logged with `(entry_id, timestamp, agent_role, feature_id, tool, helpful)`. FEATURE_ENTRIES multimap links features to entries used. Populate `usage_count`, `helpful_count`, `last_used_at` on EntryRecord. |
| Confidence Evolution | `crt-002` | Helpfulness factor added to confidence formula: `confidence = base * usage * freshness * correction * helpfulness`. Before usage data, factor = 1.0 (neutral). Confidence boost (+0.03/access), time decay (-0.005/hr), floor at 0.1. |
| Contradiction Detection | `crt-003` | Flag entries with high embedding similarity (>0.85) but conflicting content. Surface during `context_status`. Similar to ReasoningBank's contradiction pipeline — cheap, high value. |
| Co-Access Boosting | `crt-004` | Track entries frequently retrieved together. Boost co-accessed entries in search results. Lightweight version of PageRank on access graph — 80% of value, 20% of complexity. |

**Ships**: Knowledge quality improves automatically. Unused entries fade. Helpful entries strengthen. Contradictions surface.

---

### Milestone 5: Orchestration Engine (Collective Phase — `col`)

**Goal**: Workflow orchestration as a first-class capability — phase gates, wave management, outcome tracking.

| Feature | Prefix | Summary |
|---------|--------|---------|
| Outcome Tracking | `col-001` | OUTCOME_INDEX table — `(feature_hash, entry_id)` for outcome entries. Convention: agents store `category: "outcome"` with structured tags (`gate:3a`, `phase:implementation`, `result:pass`). |
| Retrospective Pipeline | `col-002` | `context_retrospective` tool — aggregates outcomes across features, detects patterns (e.g., "4+ agent waves correlate with higher bug rates"), generates `process-proposal` entries with evidence. `PendingReview` status variant. |
| Process Proposal Workflow | `col-003` | CLI: `unimatrix proposals` (list pending), `unimatrix approve <id>` (promote to active process knowledge), `unimatrix reject <id>` (record rejection as learning signal). Approved proposals become entries with `category: "process"`, `status: Active`. |
| Feature Lifecycle | `col-004` | Feature-scoped context: `context_briefing` with `feature` param returns feature-specific decisions + cross-feature patterns. Gate status tracking — which gates passed/failed for active features. |

**Ships**: System proposes process improvements from evidence. Human approves with one command. Process knowledge evolves across feature cycles. This is the Proposal A → C transition.

---

### Milestone 6: Real-Time Interface (Matrix Phase — `mtx`)

**Goal**: Visual interface for human oversight, knowledge management, and workflow visibility.

| Feature | Prefix | Summary |
|---------|--------|---------|
| Dashboard Core | `mtx-001` | Web-based dashboard (local, no cloud). Feature list view — all features with status, agent count, entry count, gate results. Real-time updates via WebSocket or SSE from MCP server events. |
| Knowledge Explorer | `mtx-002` | Browse, search, and filter all entries. Correction chain visualization (supersedes/superseded_by graph). Confidence trends over time. Category/topic breakdown charts. Entry detail view with full metadata and usage history. |
| Feature Drilldown | `mtx-003` | Per-feature view: which entries were created, which were retrieved, gate pass/fail timeline, outcome summaries. Cross-feature comparison (bug rates, agent counts, entry helpfulness). |
| Process Proposal Manager | `mtx-004` | Visual review of pending process proposals. Evidence display with cross-feature data. Approve/reject/modify inline. History of approved and rejected proposals with outcomes. |
| Prompt Debugger | `mtx-005` | Inspect what `context_briefing` returned for any agent invocation. See which entries were selected, why (similarity scores, confidence), and what was excluded (token budget). Replay briefings with modified parameters. |
| Control Manager | `mtx-006` | View and manage the relationship between `.claude/` files and Unimatrix entries. Identify drift — where file content contradicts stored knowledge. Thin-shell migration assistant — shows what can safely move to Unimatrix. |

**Ships**: Humans have full visibility into what agents know, how knowledge evolves, and where the process is working or failing. The "single pane of glass" for multi-agent orchestration.

---

### Milestone 7: Multi-Project & Identity (Designation Phase — `dsn`)

**Goal**: Support multiple concurrent projects with isolation, shared knowledge where appropriate, and project identity management.

| Feature | Prefix | Summary |
|---------|--------|---------|
| Project Registry | `dsn-001` | Central registry of projects — each with its own `{project_hash}` data directory. Project metadata: name, path, created date, last accessed. `project_id` field on EntryRecord (requires schema migration — add field + scan-and-rewrite existing entries). Auto-detection of project root from git or file markers. |
| Project Isolation | `dsn-002` | Strict isolation by default — each project's entries, vectors, and indexes are separate. No cross-project leakage. Separate hnsw_rs indexes per project. |
| Cross-Project Knowledge | `dsn-003` | Opt-in shared knowledge layer. Entries tagged `scope: "global"` are visible across projects. Global conventions (e.g., "always use conventional commits") stored once, available everywhere. Project-specific overrides via correction chain. |
| Config & Export | `dsn-004` | `unimatrix export --project <name>` — full JSON dump for backup or migration. `unimatrix import` into new project. Config file (`~/.unimatrix/config.toml`) for global settings (embedding model, confidence parameters, decay rates). |

**Ships**: Teams working on multiple repos get isolated knowledge per project with the option to share universal conventions. Portable knowledge across environments.

---

### Milestone 8: Thin-Shell Migration (Alcove Phase — `alc`)

**Goal**: Gradually slim agent files as expertise moves to Unimatrix. Optional, per-agent, no big bang.

| Feature | Prefix | Summary |
|---------|--------|---------|
| Thin-Shell Agent Pattern | `alc-010` | Agent files shrink to ~45 lines: identity (name, type, scope), design principles (stable philosophy), orientation directive (`context_briefing`), self-check gates, outcome reporting. All dynamic content (conventions, duties, process knowledge) served by Unimatrix at runtime. |
| Migration Assistant | `alc-011` | Analyze existing agent files. Identify content that duplicates or contradicts Unimatrix entries. Suggest what can be extracted. Preview thin-shell version. Track migration status per agent. Accessible via `mtx-006` (Control Manager) or CLI. |

**Ships**: Agent maintenance burden drops. Knowledge lives in one place. Agent files become stable identity documents that rarely change.

---

### Milestone 9: Build & Deploy (Nanites Phase — `nan`)

**Goal**: Production packaging, distribution, and CI integration.

| Feature | Prefix | Summary |
|---------|--------|---------|
| CLI Binary | `nan-001` | `unimatrix` CLI — single binary distribution (Rust). Commands: init, status, export, import, proposals, approve, reject, rebuild-index. Cross-platform (Linux, macOS, Windows). |
| Docker Packaging | `nan-002` | Container image with MCP server + CLI. Dev container integration for VS Code / Codespaces. Pre-built with embedding model. |
| CI Integration | `nan-003` | GitHub Actions for Unimatrix — export knowledge base on release, validate entry health in CI, flag stale entries in PR checks. |
| Release Automation | `nan-004` | Versioned releases with changelog. Schema migration infrastructure (scan-and-rewrite on version bump, triggered automatically on database open). Homebrew/cargo install distribution. |

**Ships**: Installable product. One command to add Unimatrix to any project.

---

## Milestone Dependency Graph

```
M1: Foundation (nxs)
 └─► M2: MCP Server (vnc)
      ├─► M3: Agent Integration (alc)
      │    └─► M8: Thin-Shell Migration (alc)
      └─► M4: Learning & Drift (crt)
           └─► M5: Orchestration Engine (col)
                └─► M6: Real-Time Interface (mtx)
                     └─► M7: Multi-Project (dsn)

M9: Build & Deploy (nan) — parallel track, ships incrementally alongside M2+
```

## Phase-to-Proposal Mapping

| Milestone | Proposal A territory | Proposal C territory |
|-----------|---------------------|---------------------|
| M1-M3 | Core A — knowledge store + MCP + agent integration | — |
| M4 | Bridge — adds tracking infrastructure | First C capabilities active |
| M5 | — | Full C — retrospective, proposals, process learning |
| M6 | — | Beyond C — visual management layer |
| M7 | — | Beyond C — multi-project scale |
| M8 | — | Beyond C — thin-shell agent pattern |
