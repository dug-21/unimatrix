# Unimatrix — Product Vision & Feature Roadmap

## Vision

Unimatrix is a self-learning expertise engine for multi-agent software development. It captures the knowledge that emerges from doing work — decisions, patterns, conventions, and lessons — and makes it trustworthy, retrievable, and ever-improving. Agents ask Unimatrix "how do I do X?" and get answers that reflect the team's accumulated expertise, not stale documentation. But agents don't even need to ask — Unimatrix **delivers knowledge automatically** via Claude Code's hook system. Relevant expertise is injected into every prompt, survives context compaction, and feeds confidence signals back without explicit agent action.

**The boundary is clear — three legs:**
- **Files** define the process (SDLC/governance — what must be done, by whom, and when)
- **Unimatrix** holds the expertise (how our team does it well — searchable, scored, correctable, improving)
- **Hooks** connect them — delivering expertise automatically via Claude Code lifecycle events

Workflow choreography, role definitions, and phase sequencing stay in `.claude/` files where Claude Code expects them. Knowledge that evolves through feature delivery — coding patterns, interface contracts, testing procedures, architectural decisions — lives in Unimatrix. Hooks bridge the gap: context injection on every prompt, compaction resilience, confidence feedback, and session lifecycle — all without agent cooperation.

## Core Value Proposition

Agent memory systems remember. Unimatrix ensures what agents remember is **trustworthy, correctable, and auditable** — and gets better with every feature delivered.

The 10x story is not semantic search (ubiquitous) or local-first deployment (niche). It is the **auditable knowledge lifecycle** combined with **invisible delivery**: hash-chained correction histories with attribution, confidence evolution from real usage signals, contradiction detection across the knowledge base — and automatic injection of the right knowledge into every agent prompt without tool calls or agent cooperation. When an agent asks "how do I write integration tests?", the answer reflects what has actually worked, what has been corrected, and what the team has learned — not what someone wrote in a wiki six months ago. But the agent doesn't even need to ask: the knowledge arrives as ambient context, injected by hooks before the agent sees the prompt.

This combination is architectural — it requires commitment from the data model up. Unimatrix strives for perfection.  The defensible position is: **Trust + Lifecycle + Integrity + Learning + Invisible Delivery**, delivered as a self-contained embedded engine with zero cloud dependency. The combination of trust and lifecycle becomes transformative when delivery is automatic — knowledge reaches agents as ambient context.

**Cross-domain portability note (ASS-009):** The core engine is domain-agnostic. The `EntryRecord` schema, `QueryFilter` model, correction chains, and security fields impose no domain coupling. Domain-specific behavior is confined to four server-level configuration items (category allowlist, server instructions, agent bootstrap, content scanning patterns). This means the value proposition above applies to any domain where knowledge evolves, requires trust, and benefits from lifecycle management — not just software development. See `product/research/ass-009/` for the full analysis.

---

## Strategic Approach

Start with Proposal A (Knowledge Oracle) — a focused, testable knowledge store. Evolve incrementally toward Proposal C (Workflow-Aware Hybrid) — adding usage tracking, outcome analysis, retrospective intelligence, and eventually thin-shell agent files. Each milestone is independently shippable and provable. The schema pre-seeds all known future fields from day 1, covering M2–M5 without schema changes. When new fields are added (M6+), a `schema_version` counter triggers automatic scan-and-rewrite migration on database open — fast at Unimatrix scale.

**Learning from Genius (2026-03-01):** Research comparing Unimatrix with claude-flow (Ruflo v3.5) revealed complementary strengths. Unimatrix built a sophisticated knowledge engine — redb storage, HNSW vectors, confidence evolution, contradiction detection — but agents must explicitly call `context_briefing` to benefit, and most don't. claude-flow solved the delivery problem via Claude Code lifecycle hooks (context injection on every prompt, compaction resilience) but the knowledge layer is more difficult to navigate for some.  The strategic move: adopt claude-flow's hook-driven delivery patterns with Unimatrix's engine (Which also has its roots in Ruv's Ruvector). See Unimatrix entries #190, #191 for the full analysis.

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
| Server Process Reliability | `vnc-004` | PID file lifecycle hardening via RAII `PidGuard` with `fs2` advisory locking (flock). Process identity verification (`/proc/{pid}/cmdline` on Linux) before SIGTERM of stale processes. `DatabaseLocked` error variant replaces `process::exit(1)` — all exit paths use Rust error propagation. Poison recovery for `CategoryAllowlist` RwLock (`.unwrap_or_else(\|e\| e.into_inner())`). Eliminates cascading process lifecycle failures during extended sessions. Bug fix for #52. |
| Config Externalization | `vnc-005` | Extract domain-specific constants into `ServerConfig` loaded from `~/.unimatrix/config.toml` or per-project config file. Four items externalized: (1) initial category allowlist (replace `INITIAL_CATEGORIES` const), (2) server instructions text (replace `SERVER_INSTRUCTIONS` const), (3) default agent bootstrap (replace hardcoded `bootstrap_defaults()`), (4) content scanning pattern extensions (additive to built-in patterns). Fall back to current dev-focused defaults when no config file present — zero breaking changes. Enables multi-domain deployment (SRE, product management, scientific research, etc.) by swapping a config file, not rebuilding the binary. **Does not block M4** — can be implemented in parallel or between features. See `product/research/ass-009/` for domain opportunity analysis. |

**Ships**: Agents can search, store, correct, and receive briefings. Knowledge accumulates across features. Server process reliability ensures stable long-running sessions. Config externalization enables multi-domain deployment.

---

### Milestone 3: Agent Integration (Alcove Phase — `alc`)

**Goal**: Agent identity, enrollment, and behavioral driving so agents reliably use Unimatrix.

| Feature | Prefix | Summary |
|---------|--------|---------|
| Knowledge Bootstrap | `alc-001` | Research spike: how Unimatrix integrates into Claude workflow. Established the three-layer architecture (Skills as platform-native `/command`, Agent defs as platform-native Task spawning, Knowledge in Unimatrix entries). Key finding: `context_briefing` returns unordered knowledge, not choreography — workflow sequencing must stay in agent defs. Reactive protocol delivery (v3) designed but deferred. See `product/research/ass-011/`. |
| Agent Enrollment Tool | `alc-002` | `context_enroll` MCP tool (11th tool) — Admin-level agents can enroll new agents or update trust levels and capabilities at runtime. Protected agents ("system", "human") cannot be modified. Self-lockout prevention. Strict parsing with case-insensitive trust levels/capabilities. Fixes #46 (spawned agents blocked from writes). |
| CLAUDE.md Integration | `alc-003` | `unimatrix init` CLI command appends Unimatrix block to CLAUDE.md. Reinforces server instructions (~90% compliance). Documents category conventions. |
| Starter Kit | `alc-004` | Repo template with generic agents (architect, developer, tester, validator), standard protocols (planning, implementation), and Unimatrix-aware agent structure. Reduces new project setup pain. |

**Status (2026-03-01):** alc-001 complete (research). alc-002 complete (#46, PR #55). alc-003/004 deferred — agent integration is achieved manually via CLAUDE.md and agent file conventions.

**Ships**: Agent enrollment and identity management. Manual integration patterns established. CLI formalization (alc-003/004) deferred until external adoption matters.

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

**Goal**: Automatic knowledge delivery via hooks, process intelligence from observation, and workflow orchestration. The delivery features (col-006–011) are the immediate priority — the "nervous system" connecting Unimatrix's engine to agents automatically.

#### Completed Features

| Feature | Prefix | Summary |
|---------|--------|---------|
| Outcome Tracking | `col-001` | OUTCOME_INDEX table — `(feature_hash, entry_id)` for outcome entries. Convention: agents store `category: "outcome"` with structured tags (`gate:3a`, `phase:implementation`, `result:pass`). |
| Retrospective Pipeline | `col-002` | Observation-driven retrospective capability. `unimatrix-observe` crate: JSONL parser, content-based feature attribution, rule-based hotspot detection (21 rules across 4 categories), MetricVector computation, report generation. `context_retrospective` MCP tool (12th tool). Per-session JSONL telemetry via Claude Code hooks (PreToolUse, PostToolUse, SubagentStart, SubagentStop). Four hotspot categories: agent (context load, lifespan, file breadth, re-reads, mutation spread, compile cycles, edit bloat), friction (permission retries, sleep workarounds, search-via-bash, output parsing struggle), session (timeouts, cold restarts, coordinator respawns, post-completion work, rework events), scope (source file count, design artifacts, ADR count, post-delivery issues, phase duration outliers). Bootstrapped thresholds with convergence toward mean+1.5σ. Metric vector stored in Unimatrix (category: "observation"). See `product/research/ass-013/`. |
| Detection & Baselines | `col-002b` | Extends col-002 with 18 additional detection rules (completing the full 21-rule library) and historical baseline comparison. Baseline computation (mean + stddev across stored MetricVectors, phase-specific grouping, 1.5σ outlier flagging). Four arithmetic guard modes (Normal, Outlier, NoVariance, NewSignal). Minimum 3 MetricVectors required for baseline. Enhances `context_retrospective` report — no new MCP tools. |

#### Hook-Driven Delivery Features — IMMEDIATE PRIORITY

| Feature | Prefix | Summary |
|---------|--------|---------|
| Hook Transport Layer ("Cortical Implant") | `col-006` | Research spike + implementation. Single `unimatrix-hook` binary — the **cortical implant** — acts as the universal router for all Claude Code lifecycle hooks. Inspired by claude-flow's router pattern: one binary, configured once in `.claude/settings.json`, dispatches all hook events internally (UserPromptSubmit → context injection, PreCompact → compaction resilience, PostToolUse → confidence feedback, etc.). **Transport**: How does the cortical implant communicate with the running Unimatrix MCP server? The MCP connection is owned by Claude Code's stdio pipe — hooks can't share it. Options: (1) Unix domain socket listener alongside stdio in unimatrix-server, (2) cortical implant opens redb directly for reads and queues writes, (3) HTTP listener, (4) named pipe / shared memory. **Router benefits**: (a) simplifies Claude configuration — one binary handles all events instead of N separate scripts, (b) single point of security — validates caller identity, checks process lineage, ensures the connection targets the correct Unimatrix instance for this project, (c) centralized transport — connection pooling / socket reuse across hook invocations within a session. Must support both synchronous query (hook needs results to inject into stdout) and fire-and-forget (hook records an event). Performance target: <50ms round-trip. Foundation for all col-007–011 features. Architecture defined by ASS-014 research spike (`product/research/ass-014/`). |
| Automatic Context Injection | `col-007` | UserPromptSubmit hook that queries Unimatrix for knowledge relevant to the current prompt. Semantic search against active entries, formats top 3-5 matches with confidence scores, prints to stdout for injection into Claude's context. Every prompt gets enriched with relevant knowledge automatically — no agent action needed. Token budget awareness (<500 tokens per injection). Uses hook transport from col-006. |
| Compaction Resilience | `col-008` | PreCompact hook that calls context_briefing for the active session's role and task context. Injects critical knowledge (active decisions, conventions, current feature context) into the compacted window via stdout. Ensures agents don't lose Unimatrix context on compaction. Leverages vnc-003's existing briefing infrastructure (<2000 token target). May also inject session state (current task, active files). |
| Closed-Loop Confidence | `col-009` | PostToolUse and Stop/TaskCompleted hooks that close the confidence evolution feedback loop without agent cooperation. **Asymmetric signals**: successful session → bulk `helpful=true` for injected entries (auto-applied via confidence pipeline); rework detected → entries flagged for human review in retrospective pipeline (never auto-downweighted — only explicit MCP votes touch `unhelpful_count`). **Schema v4**: adds SIGNAL_QUEUE table (14th) and `next_signal_id` counter — owned by col-009, consistent with prior convention (crt-001 owned USAGE_LOG, crt-005 owned confidence f64 migration). Dual-consumer signal processing: confidence consumer drains `Helpful` signals into `helpful_count`; retrospective consumer routes `Flagged` signals to `entries_analysis` in col-002 report. Session-scoped dedup (max 1 helpful vote per entry per session). Stale session sweep for unreliable-SessionStop recovery — orphaned in-memory sessions processed before eviction. Operates on col-008's in-memory `SessionState` for injection history; server restart mid-session loses that session's signals (accepted tradeoff — col-010 provides persistent recovery). |
| Session Lifecycle & Observation | `col-010` | Full session lifecycle persistence and structured observation pipeline. **Schema v5**: adds SESSIONS table (15th), INJECTION_LOG table (16th), and `session_id: Option<String>` field on `EntryRecord`. SessionStart/End hooks fully wired in `.claude/settings.json`. Upgrades col-008's in-memory `SessionState` to persistent `SessionRecord` — survives server restart, recoverable after missed SessionEnd. **Col-002 integration**: `from_structured_events()` entry point replaces JSONL-based session detection with explicit start/end signals and session-scoped feature attribution. Auto-generated session outcome entries via col-001 (`category: outcome`, `type: session`). Stale session sweep: sessions with no `ended_at` after 24h marked `TimedOut` during maintenance. Telemetry GC (30-day cleanup). |
| Semantic Agent Routing | `col-011` | UserPromptSubmit hook that matches prompt against stored agent duties, patterns, and historical outcomes using 384d semantic embeddings. Unlike keyword regex, finds best-fit agent by querying `category: "duties"` + `category: "outcome"` entries, ranks by confidence + similarity. Connects col-001 outcomes to agent selection. Advisory — prints recommendation, does not spawn. |

#### Process Intelligence Features — After Delivery

| Feature | Prefix | Summary |
|---------|--------|---------|
| Process Proposal Workflow | `col-003` | CLI: `unimatrix proposals` (list pending), `unimatrix approve <id>` (promote to active process knowledge), `unimatrix reject <id>` (record rejection as learning signal). Approved proposals become entries with `category: "process"`, `status: Active`. |
| Feature Lifecycle | `col-004` | Feature-scoped context: `context_briefing` with `feature` param returns feature-specific decisions + cross-feature patterns. Gate status tracking — which gates passed/failed for active features. |
| Auto-Knowledge Extraction | `col-005` | Derive durable project knowledge from observation telemetry automatically — without consuming agent context window. Three extraction tiers: **Tier 1** (high confidence) structural conventions from file creation patterns — safe to auto-extract after cross-feature validation. **Tier 2** (medium) procedural knowledge from ordered tool-call sequences — require 3+ features showing identical sequence before promotion. **Tier 3** (medium-high) dependency graphs from read-before-edit chains. Depends on col-002 having accumulated 5+ feature metric vectors. See `product/research/ass-013/auto-knowledge.md`. |

**Status (2026-03-02):** col-001 ✅, col-002 ✅ (#56, PR #58), col-002b ✅ (#57, PR #60), col-006 ✅ (#63, PR #64), col-007 ✅ (#67, PR #68), col-008 ← IN PROGRESS (#69). Hook delivery (col-006–011) is the immediate priority — the "nervous system" connecting Unimatrix's engine to agents automatically. col-003/004 follow after delivery features. col-005 blocked until 5+ feature retrospectives accumulated.

**Ships**: Automatic knowledge delivery via hooks — every agent prompt enriched, compaction resilient, confidence feedback closed-loop. System observes agent behavior, identifies process hotspots from evidence, and proposes improvements. Knowledge base self-populates from observation after sufficient iteration. This is the Proposal A → C transition.

---

### Milestone 6: Thin-Shell Migration (Alcove Phase — `alc`)

**Goal**: Gradually slim agent files as expertise moves to Unimatrix. Optional, per-agent, no big bang.

| Feature | Prefix | Summary |
|---------|--------|---------|
| Thin-Shell Agent Pattern | `alc-010` | Agent files slim their **knowledge content** (conventions, duties, standards) to Unimatrix — retrieved via `context_briefing` at runtime. **Workflow structure** (phase sequencing, gate transitions, conditional branching) stays in agent files — briefing returns an unordered bag of entries, not a choreography. Agent files become ~80-100 lines: identity, workflow choreography, orientation directive (`context_briefing`), self-check gates, outcome reporting. |
| Migration Assistant | `alc-011` | Analyze existing agent files. Identify content that duplicates or contradicts Unimatrix entries. Suggest what can be extracted. Preview thin-shell version. Track migration status per agent. Accessible via `mtx-006` (Control Manager) or CLI. |

**Promoted (2026-03-01):** Once hooks deliver knowledge automatically (col-007 context injection, col-008 compaction resilience), agent files no longer need baked-in knowledge. Thin-shell migration becomes "just delete the static knowledge sections from agent files" — hooks deliver it at runtime. Workflow sequencing still cannot be served from Unimatrix (`context_briefing` returns an unordered set), so agent definitions retain their workflow choreography. Thin-shell migration remains limited to extracting **knowledge content** (conventions, duties, cross-cutting standards). See alc-001 (Knowledge Bootstrap) for the integration approach.

**Ships**: Agent maintenance burden drops for knowledge content. Workflow structure remains author-maintained in agent files.

---

### Milestone 7: Real-Time Interface (Matrix Phase — `mtx`)

**Goal**: Visual interface for human oversight, knowledge management, and workflow visibility.

| Feature | Prefix | Summary |
|---------|--------|---------|
| Dashboard Core | `mtx-001` | Web-based dashboard (local, no cloud). Feature list view — all features with status, agent count, entry count, gate results. Real-time updates via WebSocket or SSE from MCP server events. **Live activity stream**: Tail per-session JSONL files from `~/.unimatrix/observation/` to show agent activity in real-time — tool calls, subagent spawns, file access patterns. Consumes the same telemetry infrastructure as col-002 (observation hooks). Nice-to-have visibility during swarm runs ("is my swarm healthy right now?"), distinct from col-002's retrospective analysis ("what should I improve next time?"). |
| Knowledge Explorer | `mtx-002` | Browse, search, and filter all entries. Correction chain visualization (supersedes/superseded_by graph). Confidence trends over time. Category/topic breakdown charts. Entry detail view with full metadata and usage history. |
| Feature Drilldown | `mtx-003` | Per-feature view: which entries were created, which were retrieved, gate pass/fail timeline, outcome summaries. Cross-feature comparison (bug rates, agent counts, entry helpfulness). |
| Process Proposal Manager | `mtx-004` | Visual review of pending process proposals. Evidence display with cross-feature data. Approve/reject/modify inline. History of approved and rejected proposals with outcomes. |
| Prompt Debugger | `mtx-005` | Inspect what `context_briefing` returned for any agent invocation. See which entries were selected, why (similarity scores, confidence), and what was excluded (token budget). Replay briefings with modified parameters. |
| Control Manager | `mtx-006` | View and manage the relationship between `.claude/` files and Unimatrix entries. Identify drift — where file content contradicts stored knowledge. Thin-shell migration assistant — shows what can safely move to Unimatrix. |

**Deprioritized (2026-03-01):** The primary consumer of knowledge shifts from humans browsing a UI to hooks injecting context automatically. Dashboards remain valuable for oversight but are no longer the primary delivery mechanism.

**Ships**: Humans have full visibility into what agents know, how knowledge evolves, and where the process is working or failing. The "single pane of glass" for multi-agent orchestration.

---

### Milestone 8: Multi-Project & Identity (Designation Phase — `dsn`)

**Goal**: Support multiple concurrent projects with isolation, shared knowledge where appropriate, and project identity management.

| Feature | Prefix | Summary |
|---------|--------|---------|
| Project Registry | `dsn-001` | Central registry of projects — each with its own `{project_hash}` data directory. Project metadata: name, path, created date, last accessed. `project_id` field on EntryRecord (requires schema migration — add field + scan-and-rewrite existing entries). Auto-detection of project root from git or file markers. |
| Project Isolation | `dsn-002` | Strict isolation by default — each project's entries, vectors, and indexes are separate. No cross-project leakage. Separate hnsw_rs indexes per project. |
| Cross-Project Knowledge | `dsn-003` | Opt-in shared knowledge layer. Entries tagged `scope: "global"` are visible across projects. Global conventions (e.g., "always use conventional commits") stored once, available everywhere. Project-specific overrides via correction chain. |
| Config & Export | `dsn-004` | `unimatrix export --project <name>` — full JSON dump for backup or migration. `unimatrix import` into new project. Config file (`~/.unimatrix/config.toml`) for global settings (embedding model, confidence parameters, decay rates). |

**Ships**: Teams working on multiple repos get isolated knowledge per project with the option to share universal conventions. Portable knowledge across environments.

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
HTTPS (M7+):  OAuth 2.1 bearer token claims → same pipeline
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
 └─► M2: MCP Server (vnc)   ✅ COMPLETE (vnc-001/002/003/004)
      ├─► vnc-005: Config Externalization  ← parallel track
      ├─► alc-002: Agent Enrollment  ✅ COMPLETE
      ├─► M4: Learning & Drift (crt)  ✅ COMPLETE (crt-001/002/003/004/005/006)
      │    ├─► col-001: Outcome Tracking  ✅ COMPLETE
      │    ├─► col-002/002b: Retrospective  ✅ COMPLETE
      │    │    └─► col-005: Auto-Knowledge  ← blocked: needs 5+ retrospectives
      │    ├─► M5: Orchestration Engine (col)  ← IMMEDIATE PRIORITY
      │    │    ├─► ASS-014: Cortical Implant Architecture  ← research spike
      │    │    │    └─► col-006: Hook Transport  ✅ COMPLETE
      │    │    │         ├─► col-007: Context Injection  ✅ COMPLETE
      │    │    │         │    └─► col-008: Compaction Resilience  ← IN PROGRESS
      │    │    │         │         └─► col-009: Confidence Feedback  (schema v4: SIGNAL_QUEUE)
      │    │    │         │              └─► col-010: Session Lifecycle  (schema v5: SESSIONS, INJECTION_LOG)
      │    │    │         └─► col-011: Agent Routing  ← independent
      │    │    ├─► col-003/004: Process Proposals + Feature Lifecycle
      │    │    └─► M6: Thin-Shell Migration (alc)  ← enabled by delivery
      │    └─► M7: Real-Time Interface (mtx)  ← deprioritized
      │         └─► M8: Multi-Project (dsn)
      └─► M3: Agent Integration (alc-003/004)  ← deferred

M9: Build & Deploy (nan) — parallel track
```

**Milestone reordering note (2026-02-24):** M3 remaining features (CLI init, starter kit) formalize external adoption — but the *intent* (agents using Unimatrix) is achieved manually via CLAUDE.md and agent file edits. M4/M5 deliver higher value and only depend on M2 + agents actively using the tools. M3 will formalize the manual integration when external adoption matters.

**col-002 dependency note (2026-02-28):** col-002 primarily analyzes tool-call telemetry from observation hooks — independent of knowledge base quality. It depends only on M2 (metric vector storage) and the hook infrastructure. col-005 (Auto-Knowledge Extraction) depends on col-002 having accumulated 5+ feature retrospectives to validate extraction patterns.

**col-002b completion note (2026-03-01):** col-002 + col-002b ship the full retrospective pipeline with 21 detection rules and historical baseline comparison. The pipeline is deployed but has not yet accumulated real observation data (hook JSONL format bug #61 fixed same day). First real retrospective data will accumulate from the next feature cycle onward.

**Hook delivery rationale (2026-03-01):** Competitive analysis of claude-flow (Ruflo v3.5) revealed that Unimatrix built a sophisticated knowledge engine but lacked automatic delivery. claude-flow solved the delivery problem via Claude Code lifecycle hooks but has a broken/theater backend. The Collective phase expands to include hook-driven delivery (col-006–011) as the immediate priority — adopting claude-flow's delivery patterns with Unimatrix's real engine. This is the "nervous system" connecting the brain to agents. Consumption features (Dashboard, Multi-Project) deprioritized; Thin-Shell promoted (hooks make it trivial). See Unimatrix entries #190, #191 for full analysis.

**col-009/col-010 scoping revision (2026-03-02):** ASS-014 originally proposed a separate infrastructure-only "col-010a" feature (schema migration only) shipping before col-009 to provide persistent session tables. Revised during col-009 scoping: col-009 owns its own schema migration (SIGNAL_QUEUE table, schema v4), consistent with prior convention (crt-001 owned USAGE_LOG, crt-005 owned the confidence f64 migration). Infrastructure-without-consumer creates a testing gap — SIGNAL_QUEUE is best validated by its writer, not by projected use in a prior feature. col-010 ships independently as the full session lifecycle + col-002 integration feature (schema v5: SESSIONS, INJECTION_LOG, `session_id` on EntryRecord). No separate col-010a. See `product/research/ass-014/findings/feature-scoping.md` for the revised sizing and dependency graph.

## Phase-to-Proposal Mapping

| Milestone | Proposal A territory | Proposal C territory |
|-----------|---------------------|---------------------|
| M1-M2 | Core A — knowledge store + MCP | — |
| M4 | Bridge — adds tracking infrastructure | First C capabilities active |
| M5 (col-001/002) | — | Retrospective intelligence |
| **M5 (col-006–011)** | — | **Automatic delivery — the A→C transition** |
| M5 (col-003/004) | — | Process proposals, feature lifecycle |
| M3 | Formalized agent integration (deferred) | — |
| M6 | — | Thin-shell agents (enabled by hooks) |
| M7 | — | Visual management layer |
| M8 | — | Multi-project scale |
