# Unimatrix — Product Vision & Roadmap

## Vision

Unimatrix is a self-learning knowledge integrity engine. It captures knowledge that emerges from doing work — in any domain — and makes it trustworthy, correctable, and ever-improving. It delivers the right knowledge at the right time.

## Story

Unimatrix began in agentic software delivery, where the problem was specific: AI agents forget, contradict each other, and confidently repeat mistakes. We built a knowledge engine where nothing is merely stored — everything is attributed, hash-chained for integrity, scored by real usage, and correctable with full provenance. Agents stopped relitigating decisions. Knowledge started improving with every delivery.

That foundation became a platform. A typed knowledge graph formalizes relationships — not just what agents retrieve together, but why: support, contradiction, supersession, dependency. A confidence system learns from actual usage rather than manual calibration, adapting weights and decay rates to each domain's signal patterns. Contradiction detection is semantic. Any event source — hooks, webhooks, automated pipelines — feeds the learning layer without agent cooperation. Any knowledge-intensive domain — environmental monitoring, SRE operations, scientific research, regulatory compliance — runs on the same engine, configured not rebuilt. Secured with OAuth, containerized, serving any number of repositories from a single instance. The integrity chain runs through all of it: hash-chained corrections, immutable audit log, trust-attributed provenance — tamper-evident from first write to last.

## Core Value Proposition

The defensible position is **Trust + Lifecycle + Integrity + Learning + Delivery** — in any domain, delivered as a self-contained embedded engine with zero infrastructure dependency.

The 10x story: hash-chained correction histories with attribution, confidence evolution from real usage signals, semantic contradiction detection, and automatic injection of the right knowledge into every agent prompt — without tool calls or agent cooperation. Knowledge arrives as ambient context. It improves with every use. The integrity chain is the moat.

**Cross-domain portability (ASS-009, ASS-022):** The core engine is domain-agnostic. Domain-specific behavior is confined to configuration (category allowlist, server instructions, agent bootstrap, freshness parameters). The same binary serves software delivery, environmental monitoring, SRE operations, regulatory compliance. See `product/research/ass-022/`.

---

## What We've Built

54 features shipped across 6 phases. ~2,400+ tests. 12 MCP tools. SQLite-backed with in-memory HNSW vector index. Hook-driven delivery pipeline operational.

### Storage & Schema (Nexus — `nxs`)

SQLite storage engine with normalized schema (24-column entries table, junction tables, SQL indexes). HNSW vector index (384-dim, all-MiniLM-L6-v2 embeddings). Started on redb, migrated to SQLite for analytical query support. Schema v9. 9 features: nxs-001 through nxs-009.

### MCP Server (Vinculum — `vnc`)

rmcp 0.16 SDK, stdio transport. 12 tools: `context_{search, lookup, get, store, correct, deprecate, status, briefing, quarantine, enroll, retrospective}`. Unified service layer (SearchService, StoreService, BriefingService, ConfidenceService) with SecurityGateway (S1-S5). Content scanning (~50 injection + PII patterns). Agent registry with trust levels and capability checks. PID lifecycle hardening. Retrospective ReportFormatter with markdown output, finding collapse, actionability tagging, and ~80% token reduction (vnc-011). 10 features: vnc-001 through vnc-009, vnc-011.

### Learning & Drift (Cortical — `crt`)

Six-factor additive confidence formula (base, usage, freshness, helpfulness, correction, trust — weights sum to 0.92, co-access 0.08 at query time). Wilson score helpfulness with min-5-votes guard. Contradiction detection. Co-access boosting. Coherence gate (lambda health metric from 4 dimensions). Adaptive embedding pipeline (MicroLoRA + prototype adjustment on frozen ONNX). Status-aware retrieval with topology-derived penalties for deprecated/superseded entries, calibrated (crt-013). Neural extraction pipeline (Signal Classifier + Convention Scorer MLPs) with continuous self-retraining from utilization feedback. Knowledge effectiveness analysis with per-entry utility scoring, confidence calibration validation, and dead knowledge detection (crt-018). Confidence signal activation — Wilson score on sparse votes (Laplace prior), weight rebalancing, trust_source-differentiated base_score, 0.75/0.25 similarity/confidence blend (crt-019). Topology-aware supersession: petgraph stable_graph in-tree, topology-derived graph penalties replacing hardcoded constants, multi-hop successor resolution (crt-014). Effectiveness-driven retrieval: effectiveness scores wired into search re-ranking and briefing assembly, auto-quarantine below utility threshold (crt-018b). 14 features: crt-001 through crt-008, crt-010, crt-011, crt-013, crt-014, crt-018, crt-018b, crt-019.

### Orchestration & Delivery (Collective — `col`)

Hook-driven delivery: automatic context injection on every prompt (UserPromptSubmit), compaction resilience (PreCompact), closed-loop confidence feedback (PostToolUse/Stop), full session lifecycle persistence. Retrospective pipeline with 21 detection rules across 4 hotspot categories and historical baseline comparison. Observation data unified in SQLite. Rule-based knowledge extraction engine (5 rules + 6-check quality gate) with automatic background maintenance. Intelligence pipeline end-to-end validated (col-015). 13 features: col-001, col-002, col-002b, col-006 through col-010, col-010b, col-012 through col-015.

### Agent Integration (Alcove — `alc`)

Agent enrollment tool with protected agents, self-lockout prevention, strict capability parsing. Three-layer architecture established: Skills (platform-native), Agent defs (platform-native), Knowledge (Unimatrix). 2 features: alc-001 (research), alc-002.

### Build, Deploy & CI (Nanoprobes — `nan`)

Export/import CLI subcommands with full data model preservation (nan-001, nan-002). Two onboarding skills: `/uni-init` (CLAUDE.md wiring) and `/uni-seed` (guided repo knowledge extraction) (nan-003). npm/npx distribution with platform-specific binaries, schema migration, and mechanical wiring of MCP server + hooks (nan-004). Comprehensive capability-first README and uni-docs delivery protocol step (nan-005). 5 features: nan-001 through nan-005.

---

## What's Next

### Milestone: Intelligence Sharpening — COMPLETE

Fixed, validated, and tuned the self-learning intelligence pipeline. 6/7 features shipped (crt-012 remaining, P2). Confidence signals accurate (crt-011), quarantine state restorable (vnc-010), feature attribution handles suffixed IDs (col-014), metrics normalized to SQL columns (nxs-009), retrieval calibrated (crt-013), full pipeline validated end-to-end (col-015).

### Milestone: Activity Intelligence — COMPLETE

Connected the observation pipeline to make activity data queryable, attributable, and analyzable. Introduces `topic` as the universal grouping concept — aligning knowledge-side `entries.topic` with activity-side session attribution.

**Wave 1 — Fix the data pipeline:**

- [x] **col-017: Hook-Side Topic Attribution** — Hook extracts topic signals from tool inputs per-event (file paths, prompt text). Server accumulates signals per session, resolves dominant topic on SessionClose. Persists attribution results. Retrospective fast path works. New column: `observations.topic_signal`.
- [x] **col-018: UserPromptSubmit Dual-Route** — Store user prompts as observations AND dispatch to ContextSearch. Currently prompts are the richest topic/intent signal but are discarded from the observation record.
- [x] **col-019: PostToolUse Response Capture** — Fix field name mismatch causing `response_size` and `response_snippet` to be NULL for all 5,136+ PostToolUse rows. Unblocks 8+ detection rules and context-load metrics. (#164)

**Wave 2 — Connect & capture:**

- [x] **nxs-010: Activity Schema Evolution** — New `topic_deliveries` table (groups sessions by topic with aggregate counters) and `query_log` table (search query text + result metadata). Schema v10. Backfill existing unattributed sessions.
- [x] **col-020: Multi-Session Retrospective** — Retrospective spans all sessions for a topic. New cross-session metrics: context reload rate, knowledge reuse, session efficiency trends, rework session count. Updates topic_deliveries aggregates.

**Wave 3 — Intelligence & export:**

- [x] **vnc-011: Retrospective ReportFormatter** — Markdown-format retrospective output for LLM consumers. ~80% token reduction. JSON preserved via `format: "json"`. (#91)
- [x] **crt-018: Knowledge Effectiveness Analysis** — Per-entry utility scoring from injection_log + session outcomes. Confidence calibration validation. Dead knowledge detection. (#205, PR #207)

### Milestone: Platform Hardening & Release — COMPLETE

First multi-repo deployments enabled. npm/npx distribution, backup/restore, initialization, packaging, and documentation.

- [x] **nan-001: Knowledge Export** — Full knowledge base dump: entries, correction chains, hash history, co-access pairs, audit log. Text-only (no embeddings — derived data). Format preserves the full data model for lossless restore. CLI subcommand.
- [x] **nan-002: Knowledge Import** — Restore from export dump. Re-embed all entries on import (guarantees consistency with current model). Hash chain integrity validation. Schema version compatibility check. CLI subcommand.
- [x] **nan-003: Onboarding Skills** — Two skills for new repo onboarding. `/uni-init`: appends Unimatrix block to CLAUDE.md (skill inventory, category conventions, usage instructions), scans agent defs and recommends concrete changes. `/uni-seed`: optional conversational repo exploration — auto-scans structure at Level 0, human-directed deeper dives at Level 1+, stores foundational knowledge entries.
- [x] **nan-004: Versioning & Packaging** — npm/npx distribution of Rust binary. Platform-specific binary compilation (linux x64, darwin arm64/x64). npm package that downloads the right binary. Schema migration on startup. Semantic versioning. Includes mechanical wiring: settings.json (MCP server + hooks), ONNX model pre-download, schema pre-creation, skill file installation.
- [x] **nan-005: Documentation & Onboarding** — Comprehensive README: features, capabilities, MCP tool reference (how/why to use each), benefits, constraints, workflow guidance, skills reference. Documentation agent added to protocols — automatically updates docs with new features, capabilities, and tips after each shipped feature.

### Milestone: Search Quality Enhancements — COMPLETE

Addressed the confidence differentiation gap (ASS-017). All four high-priority items shipped.

- [x] **crt-019: Confidence Signal Activation** — Wilson score on sparse votes, weight rebalancing, trust_source-differentiated base_score, 0.75/0.25 blend.
- [x] **crt-018b: Effectiveness-Driven Retrieval** — Effectiveness scores wired into re-ranking and briefing; auto-quarantine below utility threshold.
- [x] **crt-014: Topology-Aware Supersession** — petgraph in-tree, topology-derived penalties, multi-hop successor resolution.
- ~~crt-017: Contradiction Cluster Detection~~ — Superseded by W1-2 (NLI model). Building clusters on cosine heuristic before NLI replaces it adds no lasting value.
- ~~crt-020: Implicit Helpfulness from Outcome Signals~~ — Superseded by W3-1 (GNN). GNN reads behavioral signals directly from observation pipeline as training labels; materializing them into helpful_count is redundant.

### Milestone: Graph Enablement — SUPERSEDED

crt-015 (coherence gate topology) and crt-016 (co-access transitivity) would build on the current narrow-edge graph. W1-1 (Typed Relationship Graph) replaces that graph model with typed, persisted edges — making these features obsolete before they ship. Phase 3 (unified knowledge graph) IS W1-1 in the ubiquity roadmap. No further work here; this milestone is absorbed into Wave 1.

### Milestone: Wave 0 — Foundation — NEXT

Prerequisites for domain agnosticism, scalability, and secure deployment. All three can run in parallel. Full roadmap: `product/research/ass-022/06-roadmap-to-ubiquity.md`.

- [ ] **W0-1: Two-Database Split** — Separate `knowledge.db` (integrity chain: entries, audit_log, agent_registry, vector_map) from `analytics.db` (learning layer: graph_edges, co_access, observations, sessions, confidence_weights). Single async write queue draining analytics.db. MCP hot path writes only to knowledge.db — zero write contention with background intelligence work. Prerequisite for all Wave 1 additions.

- [ ] **W0-2: Session Identity via Env Var** — `UNIMATRIX_SESSION_AGENT` in settings.json → default agent_id for all tool calls. Auto-enroll session agent at startup with `[Read, Write, Search]`. `PERMISSIVE_AUTO_ENROLL` converted to env-var (default false). Tracked: GH #293.

- [ ] **W0-3: Config Externalization** — categories, boosted_categories, freshness_half_life_hours, server instructions → `~/.unimatrix/config.toml`. Eliminates all hardcoded domain coupling. 1 day of work; highest-leverage single change for domain agnosticism. Tracked as vnc-005.

### Milestone: Wave 1 + Wave 2 — Intelligence & Deployment (after Wave 0)

*Waves 1 and 2 run in parallel. Full scope in `product/research/ass-022/06-roadmap-to-ubiquity.md`.*

**Wave 1 — Intelligence Foundation:**
- [ ] **W1-1: Typed Relationship Graph** — Upgrade `StableGraph<u64, ()>` to `StableGraph<u64, RelationEdge>`. Persist to GRAPH_EDGES in analytics.db. RelationTypes: Supersedes, Contradicts, Supports, CoAccess, Prerequisite.
- [ ] **W1-2: Embedded NLI Model** — DeBERTa-v3-small ONNX (~180MB). Runs post-store async. Replaces cosine heuristic. Outputs Contradicts/Supports edges to typed graph.
- [ ] **W1-3: Observation Pipeline Generalization** — HookType → pluggable ObservationEvent schema. Domain packs register event types. Required for any non-Claude-Code signal source.

**Wave 2 — Deployment:**
- [ ] **W2-1: Container Packaging** — Dockerfile + docker-compose. Named volume (knowledge.db, analytics.db, models/, config.toml). Backup = volume snapshot.
- [ ] **W2-2: HTTP Transport** — Streamable HTTP alongside stdio. Same 12 tools. `--transport http` flag. Bearer token auth against AGENT_REGISTRY.
- [ ] **W2-3: OAuth Middleware** — OAuth 2.0 client credentials. Scopes map to capabilities. sub claim → agent_id. Multi-project routing via token claims.

### Milestone: Wave 3 — Adaptive Intelligence (gated on usage data)

*Requires Wave 1 complete + sufficient usage data (50+ helpfulness votes or 2-4 weeks active observation pipeline events).*

- [ ] **W3-1: GNN Confidence Learning** — 2-layer Graph Attention Network (~400KB ONNX). Learns `[w_base, w_usage, w_fresh, w_help, w_corr, w_trust]` + freshness_half_life per deployment from helpfulness signals + behavioral observation patterns. Replaces hardcoded weight constants.
- [ ] **W3-2: Knowledge Synthesis** — Maintenance-tick distillation of 3+ clustered entries into single synthesized entry. Gated: deploy when KB exceeds ~200 clustered entries on any topic.

### Future Considerations

Not committed. Includes deferred features, infrastructure debt, and directional priorities.

**Deferred features:**

- **col-021: Query Data Export** — Export (query, results, outcome) triples for embedding model tuning. New tool or CLI subcommand. Deferred; requires sufficient labeled data volume to be useful.
- **crt-009: Advanced models** — Duplicate Detector, Pattern Merger, optional LLM tier. Deferred pending neural pipeline maturity.
- **#34: NLI model integration** — Conflict heuristic upgrade from embedding similarity to natural language inference. Deferred; current heuristic sufficient at current contradiction density.
- **vnc-005: Config externalization** — Multi-domain deployment via external config file. Needed for second-repo deployments; deferred to after first external adopter.

**Infrastructure debt:**

- #122 — zombie cargo test processes blocking workspace testing
- #4 — Box::leak in VectorIndex (hnsw_rs lifetime workaround)
- #97 — direct transaction coupling in server
- #131 — detect_project_root fails in git worktrees
- #66 — UDS spurious warnings on fire-and-forget

**Test infrastructure:**

- #93 — server reliability integration test suite
- #71 — col-007 partial test coverage
- #70 — latency benchmark infrastructure

**Quality-of-life:**

- #42 — outcome stats computation optimization
- #89 — OperationalEvent structured logging

**Strategic directions (post-Graph Enablement):**

| Area | Key Features | Why |
|------|-------------|-----|
| **Semantic Routing** | col-011: Prompt→agent matching via embeddings | Advisory agent selection from knowledge, not keywords |
| **Thin-Shell Agents** | alc-010/011: Slim agent files, migration assistant | Hooks deliver knowledge at runtime; agent files shed static content |
| **Real-Time Interface** | mtx-*: Dashboard, knowledge explorer, prompt debugger | Human oversight layer — deprioritized vs delivery features |
| **Multi-Project** | dsn-*: Project isolation, cross-project knowledge, export/import | Scale to multiple concurrent repos |

### Research Threads

Open questions, not yet features:

- **Code pattern observation** — Can tool call sequences across sessions reveal emergent coding patterns? Fits col-002 retrospective detection rules. Needs spike to validate signal-to-noise. (Unimatrix #549). Activity Intelligence (col-021 export) enables offline analysis of tool sequences.
- **Graph visualization** — petgraph exports to Graphviz DOT. Lightweight path to knowledge topology visualization without full mtx-* UI investment.
- **Competitive positioning** — FastBuilder.ai analysis (Unimatrix #547) confirmed Unimatrix's approach (confidence decay, contradiction detection) is fundamentally stronger than enforcing architectural compliance. No action needed; validates direction.
- **Activity intelligence deep analysis** — ASS-018 identified 16 use cases across 5 tiers enabled by connected observation data. Tiers 4-5 (predictive signals, proactive knowledge serving, rework early warning) are future candidates beyond the Activity Intelligence milestone. See `product/research/ass-018/USE-CASES.md`.

---

## Security Cross-Cutting Concerns

Security is integrated into existing features across the product, not isolated into a separate phase. Full analysis: `product/research/mcp-security/`.

### Threat Landscape

Unimatrix faces amplified versions of standard MCP security risks because it is a **cumulative knowledge engine** — a single poisoned entry propagates across feature cycles. OWASP classifies memory/context poisoning (ASI06) as a top agentic AI risk. MCP's architectural weaknesses amplify attack success by 23-41%.

### Security by Layer

| Layer | Capabilities |
|-------|-------------|
| **Schema** | `created_by`, `modified_by`, `content_hash`, `previous_hash`, `version`, `feature_cycle`, `trust_source` |
| **Identity** | AGENT_REGISTRY with trust levels (System/Privileged/Internal/Restricted), capability checks per tool call |
| **Audit** | Append-only AUDIT_LOG — request_id, session_id, agent_id, operation, target_ids, outcome |
| **Input** | Content scanning (~50 injection patterns + PII), input validation (max lengths, pattern matching, no control chars) |
| **Output** | Framing on read tools to distinguish data from instructions |
| **Integrity** | Contradiction detection, embedding consistency checks, entry quarantine, hash-chained correction histories |
| **Feedback** | Confidence evolution from real signals, Wilson score with gaming resistance, co-access from actual usage |

### Agent Identity Evolution

```
stdio (current):  agent_id tool parameter → AGENT_REGISTRY → capability check → execute
_meta (future):   _meta.agent_id on MCP request → same pipeline
HTTPS (future):   OAuth 2.1 bearer token claims → same pipeline
```

---

## Architecture Notes

### Phase Prefixes

Phase prefixes (`nxs`, `vnc`, `crt`, `col`, `alc`, `mtx`, `dsn`, `nan`) are used for commit messages, branch names, and issue tracking. Milestones are goal-oriented and may pull features from multiple phases.

| Phase | Prefix | Focus |
|-------|--------|-------|
| Nexus | `nxs` | Storage, vectors, embedding, schema |
| Vinculum | `vnc` | MCP server |
| Cortical | `crt` | Learning & drift |
| Collective | `col` | Orchestration & delivery |
| Alcove | `alc` | Agent management |
| Matrix | `mtx` | UI & dashboards |
| Designation | `dsn` | Multi-project identity |
| Nanoprobes | `nan` | Build, deploy, CI |
| Assimilate | `ass` | Research spikes |

### Key Architecture Decisions

- **SQLite** (normalized, schema v9) replaced redb for analytical query support. HNSW stays in-memory — sqlite-vec rejected (brute-force only, no ANN).
- **f64 scoring pipeline** end-to-end. Embeddings remain f32 (ONNX boundary). Precision boundary lives in `VectorIndex::map_neighbours_to_results`.
- **Hook-driven delivery** via `unimatrix-server hook` subcommand. Single binary routes all Claude Code lifecycle events. Unix domain socket transport between hooks and running MCP server.
- **Service layer** (vnc-006–009) abstracts business logic from transport. All storage access through services, not direct SQL from transport layers.
- **Self-learning pipeline**: observation hooks → SQLite persistence → rule-based extraction (col-013) → neural extraction (crt-007/008) → quality gates → auto-stored entries with `trust_source: "auto"`. Continuous self-retraining from utilization feedback.
