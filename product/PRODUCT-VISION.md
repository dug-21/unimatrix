# Unimatrix — Product Vision & Roadmap

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

This combination is architectural — it requires commitment from the data model up. The defensible position is: **Trust + Lifecycle + Integrity + Learning + Invisible Delivery**, delivered as a self-contained embedded engine with zero cloud dependency.

**Cross-domain portability (ASS-009):** The core engine is domain-agnostic. Domain-specific behavior is confined to four server-level configuration items (category allowlist, server instructions, agent bootstrap, content scanning patterns). The value proposition applies to any domain where knowledge evolves, requires trust, and benefits from lifecycle management. See `product/research/ass-009/`.

---

## What We've Built

42 features shipped across 5 phases. ~1,600+ tests. 12 MCP tools. SQLite-backed with in-memory HNSW vector index. Hook-driven delivery pipeline operational.

### Storage & Schema (Nexus — `nxs`)

SQLite storage engine with normalized schema (24-column entries table, junction tables, SQL indexes). HNSW vector index (384-dim, all-MiniLM-L6-v2 embeddings). Started on redb, migrated to SQLite for analytical query support. Schema v9. 9 features: nxs-001 through nxs-009.

### MCP Server (Vinculum — `vnc`)

rmcp 0.16 SDK, stdio transport. 12 tools: `context_{search, lookup, get, store, correct, deprecate, status, briefing, quarantine, enroll, retrospective}`. Unified service layer (SearchService, StoreService, BriefingService, ConfidenceService) with SecurityGateway (S1-S5). Content scanning (~50 injection + PII patterns). Agent registry with trust levels and capability checks. PID lifecycle hardening. 9 features: vnc-001 through vnc-009.

### Learning & Drift (Cortical — `crt`)

Six-factor additive confidence formula (base, usage, freshness, helpfulness, correction, trust — weights sum to 0.92, co-access 0.08 at query time). Wilson score helpfulness with min-5-votes guard. Contradiction detection. Co-access boosting. Coherence gate (lambda health metric from 4 dimensions). Adaptive embedding pipeline (MicroLoRA + prototype adjustment on frozen ONNX). Status-aware retrieval with topology-derived penalties for deprecated/superseded entries, calibrated (crt-013). Neural extraction pipeline (Signal Classifier + Convention Scorer MLPs) with continuous self-retraining from utilization feedback. 10 features: crt-001 through crt-008, crt-010, crt-011, crt-013.

### Orchestration & Delivery (Collective — `col`)

Hook-driven delivery: automatic context injection on every prompt (UserPromptSubmit), compaction resilience (PreCompact), closed-loop confidence feedback (PostToolUse/Stop), full session lifecycle persistence. Retrospective pipeline with 21 detection rules across 4 hotspot categories and historical baseline comparison. Observation data unified in SQLite. Rule-based knowledge extraction engine (5 rules + 6-check quality gate) with automatic background maintenance. Intelligence pipeline end-to-end validated (col-015). 13 features: col-001, col-002, col-002b, col-006 through col-010, col-010b, col-012 through col-015.

### Agent Integration (Alcove — `alc`)

Agent enrollment tool with protected agents, self-lockout prevention, strict capability parsing. Three-layer architecture established: Skills (platform-native), Agent defs (platform-native), Knowledge (Unimatrix). 2 features: alc-001 (research), alc-002.

---

## What's Next

### Milestone: Intelligence Sharpening — COMPLETE

Fixed, validated, and tuned the self-learning intelligence pipeline. 6/7 features shipped (crt-012 remaining, P2). Confidence signals accurate (crt-011), quarantine state restorable (vnc-010), feature attribution handles suffixed IDs (col-014), metrics normalized to SQL columns (nxs-009), retrieval calibrated (crt-013), full pipeline validated end-to-end (col-015).

### Milestone: Activity Intelligence

Connect the observation pipeline to make activity data queryable, attributable, and analyzable. The hook pipeline captures 3,200+ events/day but sessions have no topic attribution, user prompts are discarded, and query text is never stored. This milestone fixes the activity data model and enables cross-session analysis — from restored retrospectives to knowledge effectiveness measurement to embedding tuning data export. Research: `product/research/ass-018/`.

Introduces `topic` as the universal grouping concept — aligning knowledge-side `entries.topic` with activity-side session attribution. Domain-agnostic (works for SDLC features, research areas, operational runbooks).

**Wave 1 — Fix the data pipeline (parallel):**

- [ ] **col-017: Hook-Side Topic Attribution** — Hook extracts topic signals from tool inputs per-event (file paths, prompt text). Server accumulates signals per session, resolves dominant topic on SessionClose. Persists attribution results. Retrospective fast path works. New column: `observations.topic_signal`.
- [ ] **col-018: UserPromptSubmit Dual-Route** — Store user prompts as observations AND dispatch to ContextSearch. Currently prompts are the richest topic/intent signal but are discarded from the observation record.
- [ ] **col-019: PostToolUse Response Capture** — Fix field name mismatch causing `response_size` and `response_snippet` to be NULL for all 5,136+ PostToolUse rows. Unblocks 8+ detection rules and context-load metrics. (#164)

**Wave 2 — Connect & capture (depends on Wave 1):**

- [ ] **nxs-010: Activity Schema Evolution** — New `topic_deliveries` table (groups sessions by topic with aggregate counters) and `query_log` table (search query text + result metadata). Schema v10. Backfill existing unattributed sessions.
- [ ] **col-020: Multi-Session Retrospective** — Retrospective spans all sessions for a topic. New cross-session metrics: context reload rate, knowledge reuse, session efficiency trends, rework session count. Updates topic_deliveries aggregates.

**Wave 3 — Intelligence & export (depends on Wave 2):**

- [ ] **vnc-011: Retrospective ReportFormatter** — Markdown-format retrospective output for LLM consumers. Session table, finding collapse (related hotspots → single finding), actionability tagging (`[actionable]`/`[expected]`/`[informational]`), narrative collapse, baseline filtering to outliers only. ~80% token reduction from current JSON default. JSON preserved via `format: "json"`. No dependencies on Wave 1/2. (#91)
- [ ] **crt-018: Knowledge Effectiveness Analysis** — Per-entry utility scoring from injection_log + session outcomes. Confidence calibration validation. Dead knowledge detection. Surfaces via `context_status`.
- [ ] **crt-019: Search Quality & Gap Detection** — Zero-result query analysis, query reformulation detection, result utilization rate. Identifies knowledge gaps. Surfaces via `context_status`.
- [ ] **col-021: Query Data Export** — Export (query, results, outcome) triples for embedding model tuning. New tool or CLI subcommand.

### Milestone: Graph Enablement

Depends on Activity Intelligence. Introduce petgraph for topology-derived scoring and multi-hop traversal. Research complete (ASS-017, `product/research/ass-017/`). Deep analysis in `product/research/ass-017/ANALYSIS.md`. Technical risk: LOW.

Dependency: `petgraph = { version = "0.8", default-features = false, features = ["stable_graph"] }` in `unimatrix-engine`. New module `graph.rs` alongside existing `confidence.rs` and `coaccess.rs`. Graph built per-query from store edges (Option A — always fresh, ~1-2ms at 500 entries). Cached graph (Option B) deferred until profiling shows need.

**Phase 1 — Supersession Graph (replaces hardcoded penalties):**

- [ ] **crt-014: Supersession Graph & Topology-Derived Penalties** — Build directed supersession graph from `supersedes`/`superseded_by` fields. Replace hardcoded `DEPRECATED_PENALTY` (0.7x) and `SUPERSEDED_PENALTY` (0.5x) constants in `confidence.rs:50-57` with `graph_penalty(node, graph) -> f64` that scores based on successor depth, active reachability, fan-out, and leaf status. Enable multi-hop successor resolution in `search.rs:214-262` (replaces ADR-003 single-hop limit — chains A→B→C now resolve to terminal active C). Add cycle detection via `is_cyclic_directed` as integrity check during `maintain=true`. Supersedes ADR-003 (single-hop, #483) and ADR-005 (hardcoded penalties, #485). crt-013 behavior-based tests (ADR-003 #703) designed to survive this transition. Touches unimatrix-engine (new `graph.rs` module, ~200-300 lines), unimatrix-server (search wiring, ~50 lines). P1.

**Phase 2 — Knowledge Topology Metrics (gated on Phase 1 validation + col-015):**

- [ ] **crt-015: Coherence Gate Graph Topology** — Replace HNSW stale-node ratio in coherence gate `graph_quality` dimension (weight 0.30, `coherence.rs:74-80`) with true topological metric from connected component analysis via `petgraph::algo::connected_components`. Knowledge silos (isolated subgraphs) and well-linked clusters become measurable. Requires col-015 validation framework to calibrate λ impact. Touches unimatrix-engine, unimatrix-server (`coherence.rs`). P2.
- [ ] **crt-016: Co-Access Transitivity** — Build undirected co-access graph from CO_ACCESS table. Transitive boost with decay: direct pair gets full boost, 2-hop gets dampened boost via `petgraph::algo::dijkstra` with co-access count as edge weight. Replaces `compute_search_boost()` in `coaccess.rs:80-95`. ADR-002 (crt-013, #702) explicitly names this as the intended replacement for the scalar boost. Gated on col-015 evaluation of MicroLoRA vs scalar boost signal overlap — if MicroLoRA already captures transitive signal, this may be unnecessary. Risk: noise amplification (analysis warns dampening factor required). Touches unimatrix-engine (`coaccess.rs`). P2.
- [ ] **crt-017: Contradiction Cluster Detection** — Build undirected graph from contradiction pairs detected by crt-003 heuristic. Connected components reveal contradiction clusters — groups of entries that collectively disagree. More actionable than current pairwise alerts in `contradiction.rs`. Value scales with knowledge base size; currently low contradiction density. Touches unimatrix-server (`contradiction.rs`). P3.

**Phase 3 — Unified Knowledge Graph (gated on Phase 2 validation, entry count >1000):**

Merge supersession + co-access + correction edges into single graph. Enables knowledge decay propagation, semantic neighborhood enrichment for briefing, correction chain quality scoring (chain length, branching, convergence), and Graphviz export via `petgraph::dot::Dot`. Scoped as individual features when Phase 2 validates the approach.

**Additional candidates:**
- crt-009: Advanced models (Duplicate Detector, Pattern Merger, optional LLM tier)
- #34: NLI model integration for conflict heuristic

### Milestone: Platform Hardening & Release

Depends on Activity Intelligence + Graph Enablement. Stabilize the platform for distribution and parallel development workflows.

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

**Release enablement:**
- vnc-005: Config externalization (multi-domain deployment)
- nan-*: CLI binary, Docker, CI integration, release automation
- Versioning and distribution automation

### Future Horizons

Not committed. Directional priorities after the three milestones above.

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
