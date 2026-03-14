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

54 features shipped across 6 phases. ~2,400+ tests. 12 MCP tools. SQLite-backed with in-memory HNSW vector index. Hook-driven delivery pipeline operational.

### Storage & Schema (Nexus — `nxs`)

SQLite storage engine with normalized schema (24-column entries table, junction tables, SQL indexes). HNSW vector index (384-dim, all-MiniLM-L6-v2 embeddings). Started on redb, migrated to SQLite for analytical query support. Schema v9. 9 features: nxs-001 through nxs-009.

### MCP Server (Vinculum — `vnc`)

rmcp 0.16 SDK, stdio transport. 12 tools: `context_{search, lookup, get, store, correct, deprecate, status, briefing, quarantine, enroll, retrospective}`. Unified service layer (SearchService, StoreService, BriefingService, ConfidenceService) with SecurityGateway (S1-S5). Content scanning (~50 injection + PII patterns). Agent registry with trust levels and capability checks. PID lifecycle hardening. Retrospective ReportFormatter with markdown output, finding collapse, actionability tagging, and ~80% token reduction (vnc-011). 10 features: vnc-001 through vnc-009, vnc-011.

### Learning & Drift (Cortical — `crt`)

Six-factor additive confidence formula (base, usage, freshness, helpfulness, correction, trust — weights sum to 0.92, co-access 0.08 at query time). Wilson score helpfulness with min-5-votes guard. Contradiction detection. Co-access boosting. Coherence gate (lambda health metric from 4 dimensions). Adaptive embedding pipeline (MicroLoRA + prototype adjustment on frozen ONNX). Status-aware retrieval with topology-derived penalties for deprecated/superseded entries, calibrated (crt-013). Neural extraction pipeline (Signal Classifier + Convention Scorer MLPs) with continuous self-retraining from utilization feedback. Knowledge effectiveness analysis with per-entry utility scoring, confidence calibration validation, and dead knowledge detection (crt-018). 11 features: crt-001 through crt-008, crt-010, crt-011, crt-013, crt-018.

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

### Milestone: Search Quality Enhancements — NEXT

Addresses the confidence differentiation gap identified in ASS-017 (KB health analysis, `product/research/ass-017/`). 88% of entries score in a 0.08-wide confidence band — confidence contributes ≤0.012 to re-ranking, 2.5x weaker than the co-access boost. This milestone improves what `context_search` and `context_briefing` return through targeted formula recalibration, topology-derived penalties, effectiveness-driven retrieval, and implicit feedback closure.

Two parallel tracks. Ship independently, respect order within each track.

**Track A — Confidence & Effectiveness:**

- [ ] **crt-019: Confidence Signal Activation** — Three coordinated changes: (1) Lower `MINIMUM_SAMPLE_SIZE` from 5 to 2 or replace hard cutoff with Laplace prior — Wilson score activates on sparse votes rather than permanently returning 0.5 neutral; (2) rebalance stored weights — shift dead weight from `W_BASE`/`W_HELP` toward `W_USAGE`/`W_TRUST` where actual signal exists; (3) differentiate `base_score` by trust_source — auto-sourced active entries start lower (≈0.35) rather than flat 0.5 for all Active entries. Raise `SEARCH_SIMILARITY_WEIGHT` blend from 0.85/0.15 → 0.75/0.25, gated on confidence producing real score spread in calibration tests. Bump `MAX_CONFIDENCE_REFRESH_BATCH` 100 → 500 with duration guard. Pulls in #199 (deliberate retrieval weighted signal: `context_get`/`context_lookup` count as stronger signal than search-hit) and #202 (wire `helpful: true` in query skills so votes actually flow). Touches `unimatrix-engine/src/confidence.rs`, `unimatrix-server/src/services/usage.rs`, query skills. P1.

- [ ] **crt-018b: Effectiveness-Driven Retrieval** — Wire effectiveness scores from crt-018 into search re-ranking and briefing assembly. Boost Effective entries, penalize Ineffective/Noisy. Auto-quarantine entries consistently below utility threshold after N cycles. See #206 for full scope and priority ordering. Depends on: crt-019 (confidence spread established before adding another signal). P1.

- [ ] **crt-020: Implicit Helpfulness from Outcome Signals** — Close the feedback loop for automated pipelines that never pass `helpful: true`. Join `injection_log` with resolved session outcomes post-close. For entries injected in successful sessions: add 1 implicit helpful vote. For rework/abandoned sessions: add 0.5 implicit unhelpful vote (half-weight — failure may not be the entry's fault). Deduped per session per entry. Run as a background tick operation. Uses existing `helpful_count`/`unhelpful_count` fields — no schema change. Depends on: crt-019 (formula calibrated to use votes), crt-018 (session outcome infrastructure). P2.

**Track B — Topology & Correctness:**

- [ ] **crt-014: Topology-Aware Supersession** — Add `petgraph` (`stable_graph` feature only) to `unimatrix-engine`. Build directed supersession DAG per query from `supersedes`/`superseded_by` fields (Option A: per-query, always-fresh, ~1-2ms at current entry count). Replace hardcoded `DEPRECATED_PENALTY` (0.7×) and `SUPERSEDED_PENALTY` (0.5×) constants with `graph_penalty(node, graph) -> f64` — topology-derived from successor count, chain depth, active reachability, and fan-out (partial supersession gets softer penalty; 2-hop-outdated gets harsher). Enable full multi-hop successor resolution in `search.rs` — chains A→B→C now follow to terminal active node C, replacing the single-hop limit from ADR-003. Add `is_cyclic_directed` integrity check. Supersedes ADR-003 and ADR-005. ~200-300 lines in engine (`graph.rs`), ~50 in server search wiring. P1.

- [ ] **crt-017: Contradiction Cluster Detection** — Build undirected contradiction graph from pairwise detection output after each extraction tick. `connected_components` reveals cluster membership — groups of entries that collectively disagree, more actionable than individual pairs. Surface cluster-level contradiction summary in `context_status`. Quarantine recommendation targets the cluster, not individual entries. Does not include co-access transitivity (deferred). Depends on: crt-014 (petgraph already in-tree). P2.

**Dependency graph:**

```
Track A (confidence)          Track B (topology)
─────────────────────         ──────────────────
crt-019                       crt-014
  ├─► crt-018b                  └─► crt-017
  └─► crt-020
```

### Milestone: Graph Enablement

Depends on Search Quality Enhancements (petgraph already in-tree via crt-014). Deeper topology metrics and knowledge graph evolution. Research complete (ASS-017). Technical risk: LOW.

- [ ] **crt-015: Coherence Gate Graph Topology** — Replace HNSW stale-node ratio in coherence gate `graph_quality` dimension (weight 0.30) with true topological metric from connected component analysis via `petgraph::algo::connected_components`. Knowledge silos (isolated subgraphs) and well-linked clusters become measurable. Requires lambda calibration after crt-014 validates the approach. Touches `unimatrix-engine`, `unimatrix-server/coherence.rs`. P2.

- [ ] **crt-016: Co-Access Transitivity** — Build undirected co-access graph from CO_ACCESS table. Transitive boost with decay: direct pair gets full boost, 2-hop gets dampened boost via `petgraph::algo::dijkstra`. Replaces scalar boost in `coaccess.rs`. Gated on crt-020 outcome — if implicit helpfulness already captures transitivity signal adequately, may be unnecessary. Risk: noise amplification; dampening factor required. P2.

**Phase 3 — Unified Knowledge Graph (gated on Phase 2 validation, entry count > 1000 active):**

Merge supersession + co-access + correction edges into single graph. Enables knowledge decay propagation, semantic neighborhood enrichment for `context_briefing` (graph neighborhood of semantic matches promoted even if embedding similarity is lower), correction chain quality scoring (chain length, branching, convergence), and Graphviz export via `petgraph::dot::Dot`. Scoped as individual features when Phase 2 validates the approach.

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
