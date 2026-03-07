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

36 features shipped across 5 phases. ~1,600+ tests. 10 MCP tools + 1 retrospective tool + 1 agent enrollment tool. SQLite-backed with in-memory HNSW vector index. Hook-driven delivery pipeline operational.

### Storage & Schema (Nexus — `nxs`)

SQLite storage engine with normalized schema (24-column entries table, junction tables, SQL indexes). HNSW vector index (384-dim, all-MiniLM-L6-v2 embeddings). Started on redb, migrated to SQLite for analytical query support. Schema v6. 8 features: nxs-001 through nxs-008.

### MCP Server (Vinculum — `vnc`)

rmcp 0.16 SDK, stdio transport. 12 tools: `context_{search, lookup, get, store, correct, deprecate, status, briefing, quarantine, enroll, retrospective}`. Unified service layer (SearchService, StoreService, BriefingService, ConfidenceService) with SecurityGateway (S1-S5). Content scanning (~50 injection + PII patterns). Agent registry with trust levels and capability checks. PID lifecycle hardening. 9 features: vnc-001 through vnc-009.

### Learning & Drift (Cortical — `crt`)

Six-factor additive confidence formula (base, usage, freshness, helpfulness, correction, trust — weights sum to 0.92, co-access 0.08 at query time). Wilson score helpfulness with min-5-votes guard. Contradiction detection. Co-access boosting. Coherence gate (lambda health metric from 4 dimensions). Adaptive embedding pipeline (MicroLoRA + prototype adjustment on frozen ONNX). Status-aware retrieval with topology-derived penalties for deprecated/superseded entries. Neural extraction pipeline (Signal Classifier + Convention Scorer MLPs) with continuous self-retraining from utilization feedback. 8 features: crt-001 through crt-008, crt-010.

### Orchestration & Delivery (Collective — `col`)

Hook-driven delivery: automatic context injection on every prompt (UserPromptSubmit), compaction resilience (PreCompact), closed-loop confidence feedback (PostToolUse/Stop), full session lifecycle persistence. Retrospective pipeline with 21 detection rules across 4 hotspot categories and historical baseline comparison. Observation data unified in SQLite. Rule-based knowledge extraction engine (5 rules + 6-check quality gate) with automatic background maintenance. 11 features: col-001, col-002, col-002b, col-006 through col-010, col-010b, col-012, col-013.

### Agent Integration (Alcove — `alc`)

Agent enrollment tool with protected agents, self-lockout prevention, strict capability parsing. Three-layer architecture established: Skills (platform-native), Agent defs (platform-native), Knowledge (Unimatrix). 2 features: alc-001 (research), alc-002.

---

## What's Next

### Hardening Pass

Consolidate before expanding. 25 open issues spanning bugs, structural debt, and test gaps accumulated across 36 features. Priority: fix correctness issues, close test coverage gaps, reduce structural debt. Scope TBD — curated from open issue backlog.

Key candidates:
- **Bugs:** Zombie test processes (#122), feature ID validation (#79), session count over-counting (#75), quarantine status transitions (#43)
- **Structural debt:** Direct transaction coupling (#97), observation metrics normalization (#103), training infrastructure dedup (#113), VectorIndex Box::leak (#4)
- **Test gaps:** Server reliability suite (#93), col-007 coverage (#71), latency benchmarks (#70)
- **Quality-of-life:** Status scan optimization (#17), outcome stats performance (#42), text-format retrospective (#91)

### Intelligence Pipeline Testing

Research in progress. The self-learning pipeline (crt-007/008 neural extraction, col-013 rule-based extraction, confidence evolution, co-access boosting) represents a significant amount of intelligence capability that needs end-to-end validation under real conditions. Goal: confirm the pipeline produces trustworthy, useful knowledge autonomously.

### petgraph Integration

Research complete (ASS-017, `product/research/ass-017/`). Replace hardcoded deprecation/supersession penalty constants (0.7x/0.5x) with graph-topology-derived scoring. Enables multi-hop supersession traversal, co-access transitivity, connected component analysis for coherence gate, correction chain quality scoring, and cycle detection as integrity check. Three-phase rollout: supersession graph → co-access graph → unified knowledge graph. Technical risk: LOW.

### Future Horizons

Not committed. Directional priorities based on where the product stands today.

| Area | Key Features | Why |
|------|-------------|-----|
| **Advanced Models** | crt-009: Duplicate Detector, Pattern Merger, Entry Writer Scorer, optional LLM tier | Completes the self-learning pipeline with sophisticated extraction |
| **Semantic Routing** | col-011: Prompt→agent matching via embeddings | Advisory agent selection from knowledge, not keywords |
| **Config Externalization** | vnc-005: `ServerConfig` from TOML | Enables multi-domain deployment without rebuilding |
| **Thin-Shell Agents** | alc-010/011: Slim agent files, migration assistant | Hooks deliver knowledge at runtime; agent files shed static content |
| **Real-Time Interface** | mtx-*: Dashboard, knowledge explorer, prompt debugger | Human oversight layer — deprioritized vs delivery features |
| **Multi-Project** | dsn-*: Project isolation, cross-project knowledge, export/import | Scale to multiple concurrent repos |
| **Packaging** | nan-*: CLI binary, Docker, CI integration, release automation | Installable product distribution |

### Research Threads

Open questions, not yet features:

- **Code pattern observation** — Can tool call sequences across sessions reveal emergent coding patterns? Fits col-002 retrospective detection rules. Needs spike to validate signal-to-noise. (Unimatrix #549)
- **Graph visualization** — petgraph exports to Graphviz DOT. Lightweight path to knowledge topology visualization without full mtx-* UI investment.
- **Competitive positioning** — FastBuilder.ai analysis (Unimatrix #547) confirmed Unimatrix's approach (confidence decay, contradiction detection) is fundamentally stronger than enforcing architectural compliance. No action needed; validates direction.

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

- **SQLite** (normalized, schema v6) replaced redb for analytical query support. HNSW stays in-memory — sqlite-vec rejected (brute-force only, no ANN).
- **f64 scoring pipeline** end-to-end. Embeddings remain f32 (ONNX boundary). Precision boundary lives in `VectorIndex::map_neighbours_to_results`.
- **Hook-driven delivery** via `unimatrix-server hook` subcommand. Single binary routes all Claude Code lifecycle events. Unix domain socket transport between hooks and running MCP server.
- **Service layer** (vnc-006–009) abstracts business logic from transport. All storage access through services, not direct SQL from transport layers.
- **Self-learning pipeline**: observation hooks → SQLite persistence → rule-based extraction (col-013) → neural extraction (crt-007/008) → quality gates → auto-stored entries with `trust_source: "auto"`. Continuous self-retraining from utilization feedback.
