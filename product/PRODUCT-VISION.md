# Unimatrix — Product Vision & Feature Roadmap

## Vision

Unimatrix is a self-learning expertise engine for multi-agent software development. It captures the knowledge that emerges from doing work — decisions, patterns, conventions, and lessons — and makes it trustworthy, retrievable, and ever-improving. Agents ask Unimatrix "how do I do X?" and get answers that reflect the team's accumulated expertise, not stale documentation.

**The boundary is clear:** Files define the process (SDLC/governance — what must be done, by whom, and when). Unimatrix holds the expertise (how our team does it well). Workflow choreography, role definitions, and phase sequencing stay in `.claude/` files where Claude Code expects them. Knowledge that evolves through feature delivery — coding patterns, interface contracts, testing procedures, architectural decisions — lives in Unimatrix where it can be searched, scored, corrected, and improved.

## Core Value Proposition

Agent memory systems remember. Unimatrix ensures what agents remember is **trustworthy, correctable, and auditable** — and gets better with every feature delivered.

The 10x story is not semantic search (ubiquitous) or local-first deployment (niche). It is the **auditable knowledge lifecycle**: hash-chained correction histories with attribution, confidence evolution from real usage signals, and contradiction detection across the knowledge base. When an agent asks "how do I write integration tests?", the answer reflects what has actually worked, what has been corrected, and what the team has learned — not what someone wrote in a wiki six months ago.

This combination is architectural — it requires commitment from the data model up. Competitors cannot retrofit hash-chained correction histories, confidence scoring, or contradiction detection without fundamental restructuring. The defensible position is: **Trust + Lifecycle + Integrity + Learning**, delivered as a self-contained embedded engine with zero cloud dependency.

**Cross-domain portability note (ASS-009):** The core engine is domain-agnostic. The `EntryRecord` schema, `QueryFilter` model, correction chains, and security fields impose no domain coupling. Domain-specific behavior is confined to four server-level configuration items (category allowlist, server instructions, agent bootstrap, content scanning patterns). This means the value proposition above applies to any domain where knowledge evolves, requires trust, and benefits from lifecycle management — not just software development. See `product/research/ass-009/` for the full analysis.

---

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
| Config Externalization | `vnc-004` | Extract domain-specific constants into `ServerConfig` loaded from `~/.unimatrix/config.toml` or per-project config file. Four items externalized: (1) initial category allowlist (replace `INITIAL_CATEGORIES` const), (2) server instructions text (replace `SERVER_INSTRUCTIONS` const), (3) default agent bootstrap (replace hardcoded `bootstrap_defaults()`), (4) content scanning pattern extensions (additive to built-in patterns). Fall back to current dev-focused defaults when no config file present — zero breaking changes. Enables multi-domain deployment (SRE, product management, scientific research, etc.) by swapping a config file, not rebuilding the binary. **Does not block M4** — can be implemented in parallel or between features. See `product/research/ass-009/` for domain opportunity analysis. |

**Ships**: Agents can search, store, correct, and receive briefings. Knowledge accumulates across features. Config externalization enables multi-domain deployment.

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
| Confidence Evolution | `crt-002` | Helpfulness factor added to confidence formula: `confidence = base * usage * freshness * correction * helpfulness`. Before usage data, factor = 1.0 (neutral). Confidence boost (+0.03/access), time decay (-0.005/hr), floor at 0.1. **Gaming resistance note (from crt-001 design):** The multiplicative formula should be replaced with an additive weighted composite of independent signals, each clamped to [0,1], to bound the impact of gaming any single factor. Usage factor must use log-transform (not linear access_count). Helpfulness factor must use Wilson score lower bound (not naive ratio) with a **minimum sample size** (e.g., n >= 5 votes) before deviating from the neutral prior (0.5) — this defends against both boosting (helpful-flag stuffing) and active suppression (systematic unhelpful voting to degrade entry quality). See `product/research/ass-008/USAGE-TRACKING-RESEARCH.md` for full analysis and recommended formula. |
| Contradiction Detection | `crt-003` | Flag entries with high embedding similarity (>0.85) but conflicting content. Surface during `context_status`. Similar to ReasoningBank's contradiction pipeline — cheap, high value. **Security alignment**: This is also the primary defense against semantic poisoning — the highest-severity knowledge integrity risk (see `product/research/mcp-security/`). Extend with embedding consistency checks (re-embed and compare to detect relevance hijacking) and entry quarantine status in StatusIndex. |
| Co-Access Boosting | `crt-004` | Track entries frequently retrieved together. Boost co-accessed entries in search results. Lightweight version of PageRank on access graph — 80% of value, 20% of complexity. |
| Coherence Gate | `crt-005` | Unified structural health metric (λ) monitoring knowledge base coherence across four dimensions and gating autonomous self-maintenance. **Dimension 1 — Confidence staleness**: `EntryRecord.confidence` is computed at mutation time and stored as f32, but the freshness component (`e^(-age/168h)`) decays with real time. Entries that haven't been touched become increasingly overconfident. crt-005 introduces lazy confidence refresh — on `context_status` or `context_search`, entries whose stored confidence age exceeds a staleness threshold (configurable, default 24h) are recomputed. No schema change — reuses existing `confidence` field. **Dimension 2 — HNSW graph degradation**: Re-embeds (from `context_correct` and embedding consistency checks) add new points to the hnsw_rs graph but leave old points as stale routing nodes. `stale_count()` is tracked (nxs-002) but never triggers cleanup. crt-005 adds graph compaction — when stale ratio exceeds a threshold (default 10%), rebuild affected graph regions. Piggybacked on `context_status` calls (same pattern as crt-004 co-access staleness cleanup). **Dimension 3 — Embedding consistency**: Extends the opt-in `check_embedding_consistency` from crt-003 into a coherence signal. Track the ratio of entries failing the 0.99 self-similarity threshold. A rising inconsistency ratio indicates model drift or content corruption. **Dimension 4 — Contradiction density**: Track the ratio of quarantined entries to active entries. A rising ratio signals knowledge base quality degradation. **Coherence metric**: Composite λ score (0.0–1.0) combining all four dimensions, exposed as `coherence` field in `StatusReport`. Individual dimension scores also exposed for diagnostics. **Maintenance gating**: When λ drops below configurable threshold (default 0.8), `context_status` response includes maintenance recommendations. Maintenance operations execute inline during `context_status` calls — no background threads, no timers, no new async patterns. **Ordering note**: Should complete before col-002 (Retrospective Pipeline) — col-002 draws conclusions from knowledge quality signals; stale confidence and degraded HNSW graphs produce misleading retrospective insights. **Mathematical foundation**: Structural de-alignment via irrational constants (Weyl equidistribution theorem) informs the design of thresholds that avoid binary-aligned resonance in scoring — see `product/research/ass-012/` for full analysis. |
| Adaptive Embedding | `crt-006` | 4-stage adaptation pipeline on frozen ONNX embeddings: MicroLoRA (rank 2-8, ~3K params for rank=4 on 384d) → Prototype Adjustment → Episodic Augmentation → adapted 384d vector. **Training signal**: Co-access pairs from crt-004 via contrastive learning (InfoNCE loss) — entries frequently retrieved together get pulled closer in embedding space. Batch processing: accumulate pairs, process when buffer reaches 16, use other pairs as negatives. **Domain prototypes**: Online running-mean centroids per category/topic with soft pull (alpha=0.1*similarity). Unimatrix categories (decision, pattern, convention) provide natural prototype labels. **EWC++ regularization**: Diagonal Fisher approximation prevents catastrophic forgetting when domain shifts — preserves prior adaptation while learning new relationships. **Architecture**: Pipeline sits between unimatrix-embed (ONNX, frozen) and unimatrix-vector (HNSW). f32 throughout — no precision boundary changes (ADR-001 f64 scoring boundary unaffected). MicroLoRA forward pass: `output = input + scale * (input @ A @ B)`. **Coherence gate impact**: crt-005 embedding consistency dimension (0.99 self-similarity threshold) must compare against adapted embeddings, not raw re-embeds — adapted vectors intentionally diverge from raw ONNX output. **What this does NOT change**: ONNX embedder (frozen), HNSW index (hnsw_rs), distance metric (DistDot), vector dimensionality (384). **Ordering**: After crt-005 (coherence monitoring required for adapted embeddings), before col-002 (better embeddings improve retrospective quality). See Unimatrix entry #181 for d-ruvector source analysis. |

**Ships**: Knowledge quality improves automatically. Unused entries fade. Helpful entries strengthen. Contradictions surface. Structural coherence is monitored and self-maintained. Embedding space adapts to the project's actual usage patterns.

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
| Thin-Shell Agent Pattern | `alc-010` | Agent files slim their **knowledge content** (conventions, duties, standards) to Unimatrix — retrieved via `context_briefing` at runtime. **Workflow structure** (phase sequencing, gate transitions, conditional branching) stays in agent files — briefing returns an unordered bag of entries, not a choreography. Agent files become ~80-100 lines: identity, workflow choreography, orientation directive (`context_briefing`), self-check gates, outcome reporting. |
| Migration Assistant | `alc-011` | Analyze existing agent files. Identify content that duplicates or contradicts Unimatrix entries. Suggest what can be extracted. Preview thin-shell version. Track migration status per agent. Accessible via `mtx-006` (Control Manager) or CLI. |

**On hold (2026-02-27):** Workflow sequencing cannot be served from Unimatrix — `context_briefing` returns knowledge as an unordered set, not as ordered steps. Agent definitions must retain their workflow choreography (phase ordering, gate logic, conditional transitions). Thin-shell migration is limited to extracting **knowledge content** (conventions, duties, cross-cutting standards), not workflow structure. A future capability (workflow-aware retrieval or procedure sequencing in briefing) would be needed to go thinner. See alc-001 (Knowledge Bootstrap) for the current integration approach.

**Ships**: Agent maintenance burden drops for knowledge content. Workflow structure remains author-maintained in agent files.

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
M1: Foundation (nxs)         ✅ COMPLETE
 └─► M2: MCP Server (vnc)   ✅ COMPLETE (vnc-001/002/003)
      ├─► vnc-004: Config Externalization  ← does not block M4, parallel track
      ├─► M4: Learning & Drift (crt)  ✅ COMPLETE (crt-001/002/003/004)
      │    ├─► col-001: Outcome Tracking  ← ships first (data collection)
      │    ├─► crt-005: Coherence Gate    ← ships after col-001 (data quality)
      │    │    ├─► crt-006: Adaptive Embedding  ← ships after crt-005 (needs coherence + co-access)
      │    │    └─► col-002: Retrospective Pipeline  ← ships after crt-005 (data interpretation)
      │    │         └─► col-003/004: Process Proposals + Feature Lifecycle
      │    │              └─► M6: Real-Time Interface (mtx)
      │    │                   └─► M7: Multi-Project (dsn)
      │    └─► col-003: Process Proposal Workflow  ← parallel track (CLI plumbing)
      └─► M3: Agent Integration (alc)    ← deferred, see note
           └─► M8: Thin-Shell Migration (alc)

M9: Build & Deploy (nan) — parallel track, ships incrementally alongside M2+
```

**Milestone reordering note (2026-02-24):** M3 (Agent Integration) is deferred after M4/M5. M3's features (CLI init command, starter kit) formalize external adoption — but the *intent* (agents using Unimatrix) is achieved manually via CLAUDE.md and agent file edits. M4/M5 deliver higher value (learning, contradiction detection, process proposals) and only depend on M2 + agents actively using the tools, not on M3's automation. M3 will formalize the manual integration when external adoption matters.

## Phase-to-Proposal Mapping

| Milestone | Proposal A territory | Proposal C territory |
|-----------|---------------------|---------------------|
| M1-M2 | Core A — knowledge store + MCP | — |
| M4 | Bridge — adds tracking infrastructure | First C capabilities active |
| M5 | — | Full C — retrospective, proposals, process learning |
| M3 | Formalized agent integration (deferred) | — |
| M6 | — | Beyond C — visual management layer |
| M7 | — | Beyond C — multi-project scale |
| M8 | — | Beyond C — thin-shell agent pattern |
