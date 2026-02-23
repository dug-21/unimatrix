# Unimatrix — Product Vision & Feature Roadmap

## Vision

Unimatrix is a self-learning context engine that serves as the knowledge backbone for multi-agent development orchestration — accumulating conventions, decisions, patterns, and process intelligence across feature cycles, then delivering the right context to the right agent at the right workflow moment. Over time, it evolves from a knowledge store into a workflow-aware system that proposes process improvements from evidence, supports multiple concurrent projects, and provides a real-time interface for human visibility and control.

## Strategic Approach

Start with Proposal A (Knowledge Oracle) — a focused, testable knowledge store. Evolve incrementally toward Proposal C (Workflow-Aware Hybrid) — adding usage tracking, outcome analysis, retrospective intelligence, and eventually thin-shell agent files. Each milestone is independently shippable and provable. The schema pre-seeds all known future fields from day 1, covering M2–M5 without schema changes. When new fields are added (M6+), a `schema_version` counter triggers automatic scan-and-rewrite migration on database open — fast at Unimatrix scale.

Security is a cross-cutting concern woven into existing features, not a separate milestone. Foundational security fields are added to EntryRecord in nxs-004 (before MCP writes entries). Agent identity, audit logging, input validation, and capability checks are integrated into the Vinculum phase. Advanced defenses (contradiction detection, anomaly detection, behavioral analysis) align with the Cortical phase. See [Security Cross-Cutting Concerns](#security-cross-cutting-concerns) and `product/research/mcp-security/` for the full analysis.

---

## Feature Roadmap

### Milestone 1: Foundation (Nexus Phase — `nxs`)

**Goal**: Ship a working knowledge store that agents can read from and write to via MCP.

| Feature | Prefix | Summary |
|---------|--------|---------|
| Embedded Storage Engine | `nxs-001` | redb-backed entry store with 8 tables (ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP, COUNTERS). bincode v2 serialization. EntryRecord schema pre-seeds all known future fields (M2–M5). Schema versioning via COUNTERS table with scan-and-rewrite migration when fields are added. |
| Vector Index | `nxs-002` | hnsw_rs integration — 384-dimension embeddings (all-MiniLM-L6-v2), DistDot, 16 max connections, ef_construction=200. VECTOR_MAP bridge table between entry IDs and hnsw data IDs. |
| Embedding Pipeline | `nxs-003` | Local embedding generation via ONNX runtime or API-based fallback. Title+content concatenation strategy. Batch embedding on import. |
| Core Traits & Domain Adapters | `nxs-004` | Storage traits (EntryStore, VectorStore, IndexStore) in core crate. Domain adapter pattern — implementations in domain modules. `spawn_blocking` with `Arc<Database>` for async. **Security schema**: Add 7 fields to EntryRecord — `created_by`, `modified_by`, `content_hash` (SHA-256), `previous_hash`, `version` (u32), `feature_cycle`, `trust_source` ("agent"\|"human"\|"system"). Implement scan-and-rewrite migration capability (first schema evolution event — establishes the migration pattern for all future field additions). |

**Ships**: Functional storage + retrieval backend. No MCP yet — internal API only.

---

### Milestone 2: MCP Server (Vinculum Phase — `vnc`)

**Goal**: Expose the knowledge engine to Claude Code via MCP stdio transport.

| Feature | Prefix | Summary |
|---------|--------|---------|
| MCP Server Core | `vnc-001` | rmcp 0.16 SDK, stdio transport. Server `instructions` field for behavioral driving (70-85% agent compliance). Auto-init on first `context_store`. Project isolation via `~/.unimatrix/{project_hash}/`. **Persistence note**: vnc-001 must coordinate graceful shutdown — calling both `Store::compact()` (nxs-001) and `VectorIndex::dump()` (nxs-002) to ensure all data is persisted. Both are explicit-only; neither auto-persists on drop. **Security infrastructure**: AGENT_REGISTRY table (agent_id, trust_level, capabilities, allowed_topics/categories, enrollment metadata). AUDIT_LOG table (append-only — request_id, session_id, agent_id, operation, target_ids, outcome). Agent identification via `agent_id` tool parameter for stdio (design internal plumbing transport-agnostic for future `_meta` field and OAuth 2.1 bearer token support on HTTPS). Unknown agents auto-enroll as Restricted (read-only). |
| v0.1 Tools | `vnc-002` | `context_search` (semantic, query-driven, returns top-k with similarity scores), `context_lookup` (deterministic, metadata-driven, category/topic/tags filters), `context_store` (with near-duplicate detection at 0.92 threshold), `context_get` (full entry by ID). Dual response format: compact markdown in `content`, JSON in `structuredContent`. **Security**: Input validation on all tool params (max lengths, pattern matching, no control chars). Category allowlist enforcement (initial: outcome, lesson-learned, decision, convention, pattern, procedure — extensible at runtime). Content scanning on `context_store` writes (~50 injection patterns + PII detection, native Rust `regex` crate). Output framing on read tools to distinguish data from instructions. Capability check per tool call against AGENT_REGISTRY (Read for search/lookup/get, Write for store). |
| v0.2 Tools | `vnc-003` | `context_correct` (supersede with correction chain), `context_deprecate` (mark irrelevant), `context_status` (health metrics — counts, age distribution, stale entries, duplicate candidates), `context_briefing` (compiled orientation — lookup duties + conventions + search task-relevant patterns in one call, <2000 token target). **Security**: Content scanning on `context_correct` writes. Capability checks (Write for correct/deprecate, Admin for status, Read for briefing). Security metrics in `context_status` — entries by trust_source, entries without attribution, write frequency by agent, content_hash mismatches. |

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
| Usage Tracking | `crt-001` | USAGE_LOG table — every retrieval logged with `(entry_id, timestamp, agent_role, feature_id, tool, helpful)`. FEATURE_ENTRIES multimap links features to entries used. Populate `usage_count`, `helpful_count`, `last_used_at` on EntryRecord. **Security alignment**: Enables write rate limiting per agent and behavioral baseline establishment for anomaly detection. |
| Confidence Evolution | `crt-002` | Helpfulness factor added to confidence formula: `confidence = base * usage * freshness * correction * helpfulness`. Before usage data, factor = 1.0 (neutral). Confidence boost (+0.03/access), time decay (-0.005/hr), floor at 0.1. |
| Contradiction Detection | `crt-003` | Flag entries with high embedding similarity (>0.85) but conflicting content. Surface during `context_status`. Similar to ReasoningBank's contradiction pipeline — cheap, high value. **Security alignment**: This is also the primary defense against semantic poisoning — the highest-severity knowledge integrity risk (see `product/research/mcp-security/`). Extend with embedding consistency checks (re-embed and compare to detect relevance hijacking) and entry quarantine status in StatusIndex. |
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

## Security Cross-Cutting Concerns

Security is integrated into existing features across milestones, not isolated into a separate phase. Full analysis: `product/research/mcp-security/`.

### Threat Landscape Summary

Unimatrix faces amplified versions of standard MCP security risks because it is a **cumulative knowledge engine** — a single poisoned entry propagates across feature cycles. OWASP classifies memory/context poisoning (ASI06) as a top agentic AI risk. Demonstrated attacks (PoisonedRAG, ADMIT, MemoryGraft) achieve 86-90% success rates with minimal poisoning. MCP's architectural weaknesses amplify attack success by 23-41% (arXiv:2601.17549).

### Security by Milestone

| Milestone | Security Responsibilities |
|-----------|--------------------------|
| **M1 (nxs-004)** | Schema fields: `created_by`, `modified_by`, `content_hash`, `previous_hash`, `version`, `feature_cycle`, `trust_source`. Scan-and-rewrite migration capability. |
| **M2 (vnc-001)** | AGENT_REGISTRY table, AUDIT_LOG table (append-only), agent identification flow (self-reported `agent_id` param for stdio, transport-agnostic internal plumbing). |
| **M2 (vnc-002)** | Input validation, content scanning (~50 injection patterns + PII, native Rust), output framing, capability checks (Read/Write per tool). |
| **M2 (vnc-003)** | Security metrics in `context_status`, capability checks on mutations, content scanning on `context_correct`. |
| **M4 (crt-001)** | Write rate limiting, behavioral baselines for anomaly detection. |
| **M4 (crt-003)** | Semantic poisoning defense via contradiction detection, embedding consistency checks, entry quarantine. |
| **M5 (col-004)** | Merkle root computation, trusted snapshots, rollback capability. |
| **Future (HTTPS)** | OAuth 2.1 bearer tokens, TLS, CORS, hard rate limiting. Agent identity upgrades from self-reported to verified. No foundation changes required — internal plumbing is transport-agnostic. |

### Agent Identity Evolution

```
stdio (M2):   agent_id tool parameter → AGENT_REGISTRY → capability check → execute
                ↓ (future, non-breaking)
_meta (M2+):  _meta.agent_id on MCP request → same pipeline
                ↓ (future, non-breaking)
HTTPS (M6+):  OAuth 2.1 bearer token claims → same pipeline
```

### Trust Hierarchy

| Level | Who | Default Capabilities |
|-------|-----|---------------------|
| System | Unimatrix server internals | All operations |
| Privileged | Human user via MCP client | All tools, all topics |
| Internal | Orchestrator agents (e.g., scrum-master) | Read-write, scoped to active feature |
| Restricted | Worker agents, unknown agents | Read-only |

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
