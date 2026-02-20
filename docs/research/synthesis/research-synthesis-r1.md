# Unimatrix Research Synthesis -- Round 1

**Date**: 2026-02-19
**Status**: Complete
**Documents Synthesized**: 6 research reports (tools evaluation, landscape survey, context/memory systems, orchestration architectures, learning systems)
**Author**: Research synthesis agent

---

## 1. Executive Summary

### What We Learned

Six research reports spanning 25+ tools, 5 orchestration patterns, 4 memory architectures, and dozens of academic references converge on a clear picture: the agentic software development ecosystem has matured rapidly for single-project, single-agent workflows, but **multi-project orchestration with persistent learning remains an entirely unoccupied space**.

The research reveals five foundational truths for Unimatrix:

1. **Context engineering is the defining technical challenge.** Every research report -- from Anthropic's own guidance to JetBrains' empirical studies -- confirms that getting the right information to agents at the right time matters more than any other architectural decision. Stuffing context windows degrades performance; targeted delivery improves it. No production tool today delivers phase-aware, project-scoped, token-optimized context. (Sources: context-and-memory-systems.md, agentic-dev-landscape-2025.md)

2. **Multi-project orchestration is an empty market.** The landscape survey of 25+ tools found zero platforms that natively manage multiple heterogeneous projects with shared context and cross-project learning. IDE agents (Cursor, Windsurf) and CLI agents (Claude Code, Aider) operate within single projects. Orchestration platforms (Devin, Factory.ai) coordinate multiple agents but within a single codebase. Unimatrix's target position is genuinely unoccupied. (Source: agentic-dev-landscape-2025.md)

3. **Verification is non-negotiable; trust must be earned.** Claude-flow's most devastating failure was trusting agent self-reports -- agents claimed "all tests pass" while 89% actually failed, and false claims cascaded through multi-agent chains. Every tool evaluation reinforced that independent verification pipelines and human gates are essential, not optional. (Sources: claude-flow-analysis.md, agent-orchestration-architectures.md)

4. **Simple approaches reliably outperform sophisticated ones.** JetBrains found observation masking (simple placeholder replacement) beat LLM-based summarization in 4 of 5 settings at 52% lower cost. File-based context (CLAUDE.md, AGENTS.md) remains the pragmatic foundation. Claude-flow's failure was building Byzantine fault tolerance before basic memory persistence worked. The lesson is absolute: prove each layer before adding the next. (Sources: context-and-memory-systems.md, claude-flow-analysis.md)

5. **The learning gap is the biggest opportunity.** No production tool implements outcome-based learning where agent behavior measurably improves from past sessions. Claude Code's auto-memory is the most advanced shipping system, and it is append-only with no pruning, no relevance filtering, and a 200-line cap. A system that converts one-time corrections into permanent knowledge would be unprecedented. (Sources: learning-and-knowledge-evolution.md, agentic-dev-landscape-2025.md)

### Competitive Landscape

```
                    Single Project          Multi-Project
                    ---------------         ---------------
IDE-Level       |  Cursor, Windsurf,     |  (No tool exists)
                |  Roo Code, Copilot     |
                |------------------------|-----------------------
CLI-Level       |  Claude Code, Aider,   |  (No tool exists)
                |  Cline                 |
                |------------------------|-----------------------
Orchestration   |  Devin, Factory.ai,    |  UNIMATRIX
                |  OpenAI Codex,         |  (Target position)
                |  Augment Code          |
```

The multi-project column is entirely empty. The $8.5B autonomous agent market (2026) is growing rapidly, but Gartner warns 40%+ of agentic AI projects may be cancelled by 2027 due to cost, complexity, or unexpected risks.

### Biggest Risk and Biggest Opportunity

| | Description |
|---|---|
| **Biggest Risk** | Overbuilding. Claude-flow claimed Byzantine fault tolerance, CRDT consensus, Q-Learning routers, and neural self-optimization, but its basic memory system did not persist to disk. The risk is building too much infrastructure before proving that the core value proposition -- right context at the right time across multiple projects -- actually works. |
| **Biggest Opportunity** | The learning loop. No production tool tracks which context led to successful outcomes and uses that to improve future delivery. Even a rudimentary feedback system (context provided -> task outcome -> refinement) would be unprecedented. Combined with multi-project orchestration, this creates a defensible moat. |

---

## 2. Cross-Cutting Themes

### Patterns That Appeared in Multiple Reports

| Theme | Reports Where It Appeared | Consensus |
|-------|--------------------------|-----------|
| **MCP as the integration standard** | Landscape, Context/Memory, Orchestration | Universal. MCP is the emerging standard with 97M+ monthly SDK downloads. Build MCP-native. |
| **Docker-based agent isolation** | Landscape, Orchestration, Claude-flow | Strong. Docker sandboxes are now standard practice. Per-agent containers are the expected pattern. |
| **Hierarchical agent teams (6-8 agents)** | Claude-flow, Orchestration, AgentFactory | Strong. Supervisor/coordinator with bounded worker teams outperforms flat mesh. |
| **Tiered model routing** | Claude-flow, Landscape, Orchestration | Strong. Route simple tasks to cheaper/faster models. Even basic keyword routing yields savings. |
| **File-based context as foundation** | Context/Memory, Landscape, Learning | Strong. AGENTS.md / CLAUDE.md hierarchy is the pragmatic starting point. Augment, do not replace. |
| **Verification before trust** | Claude-flow, Orchestration, AgentFactory, Learning | Universal. Never trust agent self-reports. Independent verification is mandatory. |
| **Human-in-the-loop gates** | Orchestration, Learning, Landscape | Strong. Structured approval workflows at defined checkpoints. Progressive automation as trust builds. |
| **Hybrid memory (structured + vector)** | Context/Memory, Claude-flow, Learning | Strong. SQLite/Postgres for reliability + vector store for semantic search. Add sophistication incrementally. |
| **Phase-aware context delivery** | Context/Memory, Orchestration, Learning | Identified as critical opportunity. No production system implements this today. |
| **Provider/LLM abstraction** | AgentFactory, Orchestration, Landscape | Strong. Decouple orchestration from any specific LLM provider. Per-project/per-phase model selection. |

### Consensus Findings

These findings were consistent across all or nearly all reports:

1. **Start simple, prove each layer.** Every report -- including the cautionary tales from claude-flow -- reinforced incremental delivery with validated foundations.

2. **Context windows are a finite resource with diminishing returns.** Anthropic, JetBrains, and Google all confirm that more context does not mean better performance. The goal is the smallest set of high-signal tokens.

3. **The phase pipeline is the natural backbone.** Development workflows (research -> architecture -> spec -> code -> test -> deploy) map directly to sequential pipelines with human gates between phases.

4. **Rust is production-ready for this work.** Tokio (async runtime), Ractor (actor framework), Bollard (Docker), and multiple LLM crates provide the full stack needed. No ecosystem gaps identified.

5. **Process management is a first-class concern.** Claude-flow's orphan daemon problem (1,625 OOM kills in 24 hours) demonstrates that agent lifecycle management must be designed from day one, not bolted on.

### Conflicting Findings

| Topic | Finding A | Finding B | Resolution |
|-------|-----------|-----------|------------|
| **Agent self-editing memory** | Letta/MemGPT: powerful, agents manage own memory via tool calls | Learning report: self-editing costs tokens; background management is cheaper | **Hybrid**: Background extraction by default, agent self-editing for critical updates only. Start with background management. |
| **Graph vs. vector retrieval** | Context/Memory: hybrid outperforms either alone | Learning: start with file-based, add vector, graph comes later | **Phase it**: File-based first, vector second, graph third. Hybrid is the target but not the starting point. |
| **Event-driven vs. pipeline orchestration** | Orchestration: event-driven offers best decoupling and fault tolerance | Orchestration: pipelines offer best predictability for phased workflows | **Both**: Pipeline backbone for phase sequencing, event bus for system-wide coordination and observability. |
| **Team size for agents** | Claude-flow: 6-8 agent teams | Anthropic multi-agent research: 15x token cost vs single agent | **Start small**: 1-3 agents per phase initially. Scale to 6-8 only when proven valuable. Monitor token costs ruthlessly. |

### Anti-Patterns to Avoid (Primarily from Claude-flow)

| Anti-Pattern | Source | Why It Fails | What to Do Instead |
|-------------|--------|-------------|-------------------|
| **Trusting agent self-reports** | Claude-flow #640 | Agents claim success without verification; errors cascade through multi-agent chains | Independent verification pipeline; run tests, not read agent claims about tests |
| **Building sophisticated features on unproven foundations** | Claude-flow (entire project) | Byzantine fault tolerance built before basic memory persistence worked | Prove each layer with tests before adding the next |
| **Mock/stub implementations claiming functionality** | Claude-flow #653 | 85% of MCP tools returned fake success responses; users cannot distinguish real from fake | Every feature either works completely or is explicitly marked as not-yet-implemented |
| **Ignoring process lifecycle management** | Claude-flow #1171 | Orphan daemons persisted for 5-6 days, causing 1,625 OOM kills in 24 hours | PID files, heartbeats, signal handlers, session-scoped lifecycles from day one |
| **Flat agent mesh without coordination** | Claude-flow, Orchestration | Parallel agents "mostly ignore each other" without active coordination | Hierarchical teams with supervisor actively managing information flow |
| **Append-only memory without pruning** | Claude Code auto-memory, Learning | Stale knowledge degrades performance; outdated patterns produce incorrect code | Knowledge lifecycle with active pruning, deprecation, and freshness tracking |
| **Loading all context regardless of relevance** | Landscape (all rules-file approaches) | Token-expensive, degrades model attention, especially toxic in multi-project scenarios | Phase-aware, project-scoped context assembly with token budgets |

---

## 3. Architectural Recommendations

### Recommended Overall Architecture for Unimatrix v1

Based on the convergent findings across all six reports, Unimatrix should implement a **phase-pipeline orchestrator with supervised agent teams, event-driven coordination, durable execution, and a context compilation engine**.

```
+==========================================================================+
|  UNIMATRIX CORE (Rust binary)                                             |
|                                                                           |
|  +------------------+  +-----------------+  +------------------------+   |
|  | Pipeline Engine   |  | Gate Manager    |  | Container Manager      |   |
|  | - Phase state     |  | - Approval flow |  | - Agent lifecycle      |   |
|  |   machine         |  | - Trust levels  |  | - Health monitoring    |   |
|  | - Artifact flow   |  | - Notifications |  | - Resource limits      |   |
|  | - Durable journal |  | - Audit log     |  | - Container pools      |   |
|  +--------+----------+  +--------+--------+  +----------+-----------+   |
|           |                      |                       |               |
|  +--------v----------------------v-----------------------v-----------+   |
|  |                         EVENT BUS                                  |   |
|  |  (Tokio broadcast channels + optional Redis Streams bridge)        |   |
|  +-------------------------------------------------------------------+   |
|           |                      |                       |               |
|  +--------v---------+  +--------v--------+  +-----------v----------+    |
|  | Context Engine   |  | LLM Gateway     |  | Knowledge Store      |    |
|  | - Phase policies |  | - Provider      |  | - File-based layer   |    |
|  | - Token budgets  |  |   abstraction   |  | - Vector layer       |    |
|  | - Context        |  | - Fallback      |  |   (hnsw_rs+redb)     |    |
|  |   compiler       |  |   chains        |  | - Graph layer        |    |
|  | - Cache mgmt     |  | - Cost metering |  |   (Graphiti+Neo4j)   |    |
|  +------------------+  +-----------------+  | - Session store      |    |
|                                              +----------------------+    |
|  +-------------------------------------------------------------------+   |
|  | State Store (SQLite local / PostgreSQL cloud)                      |   |
|  | - Phase states, agent states, gate states                          |   |
|  | - Execution journal for recovery                                   |   |
|  | - Artifact metadata and versions                                   |   |
|  +-------------------------------------------------------------------+   |
+==========================================================================+
        |                                                  |
        | Docker API (Bollard)                             | HTTP API (Axum)
        |                                                  |
+-------v---------+  +----------+  +----------+   +-------v----------+
| Agent Container  |  | Agent    |  | Agent    |   | CLI / Web UI     |
| (Phase: Coding)  |  | (Test)   |  | (Review) |   | - Gate review    |
| - LLM Client     |  |          |  |          |   | - Status         |
| - MCP Tools      |  |          |  |          |   | - Config         |
| - Workspace      |  |          |  |          |   | - Dashboards     |
+------------------+  +----------+  +----------+   +------------------+
```

### Core Components and Their Responsibilities

| Component | Responsibility | Key Design Decisions |
|-----------|---------------|---------------------|
| **Pipeline Engine** | Owns the phase state machine for each project. Manages phase ordering, transitions, artifact flow, and durable execution (journaling, checkpointing, replay). | Custom state machine using Rust enums (7 states, ~10 transitions). Journal backed by SQLite. No external workflow engine dependency. |
| **Gate Manager** | Manages human-in-the-loop approval gates between phases. Sends notifications, receives decisions, implements trust escalation, maintains audit log. | Pre-action approval gates by default. Configurable trust levels per project and per phase. All decisions logged for auditing. |
| **Container Manager** | Creates, monitors, and stops agent containers via Bollard. Manages pools, enforces resource limits, streams logs. | `ContainerRuntime` trait with `DockerRuntime` implementation. `KubernetesRuntime` for future cloud deployment. `LocalProcessRuntime` for testing. |
| **Context Engine** | The core innovation. Compiles working context per agent invocation from file-based rules, vector retrieval, graph traversal, and session state. Applies phase policies and token budgets. | Context is compiled (not accumulated). Phase-aware policies as declarative YAML. Static prefix + dynamic suffix for cache optimization. MCP as delivery protocol. |
| **LLM Gateway** | Abstracts LLM provider details. Implements fallback chains, cost metering, rate limiting, circuit breaking. | Thin `LlmProvider` trait with `AnthropicProvider`, `OpenAiProvider`, `OllamaProvider`. `FallbackProvider` and `MeteredProvider` wrappers. Per-phase model selection. |
| **Knowledge Store** | Persistent storage for all knowledge across four levels (session, team, project, global). Supports file-based, vector, and graph retrieval. | File-based foundation (AGENTS.md, YAML knowledge records). Embedded vector search (hnsw_rs + redb, per-project physical isolation). Graphiti + Neo4j for knowledge graph (Phase 2+). Phased deployment. |
| **Event Bus** | Central nervous system. All components publish and subscribe to events (phase transitions, gate decisions, agent lifecycle, artifact production). | Tokio `broadcast` channels in-process. Optional bridge to Redis Streams for distributed deployment and event replay. |
| **State Store** | Durable storage for all system state: phase states, agent states, gate states, execution journal. | SQLite for local via `sqlx`. PostgreSQL for cloud. Execution journal with checkpoint support for crash recovery. |

### Technology Stack Recommendations

| Layer | Technology | Justification |
|-------|-----------|---------------|
| **Language** | Rust | Performance, safety, single-binary deployment. All required libraries exist (Tokio, Bollard, Ractor). Matches the primary project language. (Source: orchestration report Rust ecosystem assessment) |
| **Async Runtime** | Tokio | De facto standard. Broadest ecosystem support. Provides channels (mpsc, oneshot, broadcast, watch) for inter-component communication. No viable alternative. (Source: orchestration report) |
| **Actor Framework** | Ractor | Erlang-style supervision trees map directly to agent hierarchy. Tokio-native. Built-in distribution for future multi-node. Typed message passing. (Source: orchestration report, comparative analysis of 5 frameworks) |
| **Docker Integration** | Bollard | Only mature async Docker API client for Rust. Full lifecycle management, log streaming, Windows/Linux support. (Source: orchestration report) |
| **HTTP Server** | Axum | Modern, Tower-based middleware ecosystem (rate limiting, retry, timeout). Excellent ergonomics with Rust type system. (Source: orchestration report) |
| **Database** | SQLx + SQLite (local) / PostgreSQL (cloud) | Async, compile-time query checking. SQLite for zero-ops local deployment. PostgreSQL for cloud portability. (Source: orchestration report, context/memory report) |
| **Vector Store** | Embedded: hnsw_rs + redb (direct deps) | Single-binary, no external service. hnsw_rs: 194K downloads, actively maintained, built-in filter support via `FilterT` trait. redb: 4,200 stars, ACID, pure Rust. ~1,200 lines of purpose-built wrapper. `VectorStore` trait enables migration to Qdrant if needed. (Source: vector storage decision analysis, ruvector analysis) |
| **Knowledge Graph** | Graphiti + Neo4j | Graphiti is purpose-built for AI agents with native temporal awareness. Supports incremental updates. Neo4j as mature backing store. (Source: context/memory report) |
| **Serialization** | Serde + serde_json + toml | Universal Rust standard. JSON for artifacts/messages, TOML for configuration, YAML for knowledge records. |
| **CLI** | Clap | De facto standard for Rust CLI parsing. |
| **Logging/Tracing** | tracing | Structured, async-aware, distributed tracing spans for end-to-end visibility across agents. |
| **TUI Dashboard** | Ratatui | Real-time terminal monitoring for pipeline status, active agents, pending gates. |
| **Testing** | tokio::test + testcontainers + wiremock + proptest | Async tests, Docker integration tests, mock LLM APIs, property-based state machine testing. |

### Build vs. Integrate Analysis

| Capability | Decision | Rationale |
|-----------|----------|-----------|
| **Phase pipeline state machine** | BUILD | Simple enough (7 states, 10 transitions) that a library adds unnecessary dependency. Rust enums + pattern matching are ideal. |
| **Durable execution journal** | BUILD | Custom journal backed by SQLite. Restate would add infrastructure dependency for a capability we can implement in ~500 lines. |
| **LLM provider abstraction** | BUILD (thin layer) | Existing Rust crates (llm, llm-connector) may not match exact needs. Thin trait (~50 lines) with provider implementations is low-risk. |
| **Container runtime abstraction** | BUILD (trait + Docker impl) | Bollard provides the Docker API; we build the `ContainerRuntime` trait and `DockerRuntime` wrapper. |
| **Vector search** | BUILD (thin wrapper over hnsw_rs + redb) | ~1,200 lines. Direct deps on mature libs. Single-binary deployment. `VectorStore` trait for future Qdrant migration. Avoids external service overhead for a solo architect. |
| **Knowledge graph** | INTEGRATE (Graphiti + Neo4j) | Graph construction is complex. Graphiti is purpose-built for agent memory with temporal awareness. |
| **Context delivery protocol** | INTEGRATE (MCP) | MCP is the emerging standard. Building a proprietary protocol would be counterproductive. |
| **Agent execution** | INTEGRATE (Claude Code, Aider) | Use existing CLI agents as execution engines. Unimatrix orchestrates and provides superior context, not a new code generation engine. |
| **Project management** | INTEGRATE (Linear, with abstraction) | Linear's agent-first API is the best fit. Build behind an abstraction layer for portability. |
| **Event bus** | BUILD (Tokio channels) + OPTIONAL INTEGRATE (Redis Streams) | Start with in-process Tokio channels. Add Redis Streams bridge when distributed deployment is needed. |
| **Memory systems from existing tools (Mem0, Letta, Zep)** | INSPIRE, DO NOT DEPEND | These are general-purpose, not development-specific. Extract patterns (Letta's memory blocks, Zep's temporal graphs) but build development-aware implementations. |
| **Claude-flow** | DO NOT USE as dependency | Valuable as a concept catalog and cautionary tale. Implementation quality too unreliable. MIT license means we can study patterns freely. |
| **AgentFactory** | DO NOT USE as dependency | 10 days old, 29 stars, no community validation. Extract patterns (provider abstraction, crash recovery, agent definitions). |

---

## 4. The Context Engine (Core Innovation)

The Context Engine is Unimatrix's primary differentiator. It synthesizes findings from the context/memory report, the landscape survey, and the learning systems report into a concrete design.

### Design Principles

1. **Context is compiled, not accumulated.** Working context is an ephemeral projection built fresh for each model invocation, not a mutable text buffer. (Source: Google ADK research, context/memory report)
2. **Phase-awareness is a policy, not a model.** Declarative YAML policies determine what context is included/excluded per phase. (Source: context/memory report)
3. **Isolation by default.** Agents see only their project's knowledge unless explicitly shared. (Source: context/memory report, learning report)
4. **Token budgets are hard limits.** Every context assembly has a budget. The engine never exceeds it. (Source: context/memory report)
5. **Static prefix, dynamic suffix.** Structure context for maximum prompt cache hit rates. (Source: context/memory report -- up to 90% cost reduction from Anthropic prompt caching)
6. **Measure everything.** Track token usage, cache hit rates, retrieval relevance, and agent performance to continuously improve. (Source: learning report)

### Phase-Aware Context Delivery System

```
Task Arrives (e.g., "Implement user authentication API")
        |
        v
+-------------------+
| Phase Detection    |  1. Workflow position (if in pipeline)
| (multi-signal)     |  2. File-context inference (*.test.ts = testing)
|                    |  3. Task description analysis
|                    |  4. Manual override
+--------+----------+
         |
         v  Phase = "implementation"
+-------------------+
| Context Policy     |  Loads policy from context-policy.yaml
| Engine             |
|                    |  INCLUDE: coding_conventions, api_specs, type_defs,
|                    |           examples, error_handling_patterns
|                    |  EXCLUDE: architecture_rationale, deployment_configs,
|                    |           capacity_planning, test_fixtures
|                    |  TOKEN_BUDGET: 12,000
+--------+----------+
         |
         v
+-------------------+
| Context Compiler   |  Assembles from multiple sources:
|                    |
| 1. Static rules    |  AGENTS.md hierarchy (always, ~2K tokens)
| 2. Phase-filtered  |  Coding conventions for this project (~2K tokens)
|    knowledge       |
| 3. Retrieved       |  Semantically relevant patterns from vector
|    context         |  store (~3K tokens)
| 4. Graph-derived   |  Dependency/relationship context from KG (~1K tokens)
|    context         |
| 5. Session state   |  Recent conversation, agent notes (~2K tokens)
| 6. Learned         |  Relevant knowledge records from learning
|    patterns        |  system (~2K tokens)
+--------+----------+
         |
         v  Total: ~12K tokens (within budget)
+-------------------+
| Cache Optimizer    |  Structures output for prompt caching:
|                    |
|  CACHED PREFIX:    |  System instructions + project conventions +
|  (static, ~6K)    |  phase rules + tool definitions
|                    |
|  DYNAMIC SUFFIX:   |  Retrieved context + session state +
|  (per-request, ~6K)|  current task description
+--------+----------+
         |
         v
  Delivered to agent via MCP server or direct injection
```

### Multi-Project Isolation with Selective Sharing

```
+-------------------------------------------------------+
|                 GLOBAL KNOWLEDGE                        |
|  Team conventions, security patterns, language idioms   |
|  (available to all projects, high promotion bar)        |
+-------+-------------------+-------------------+--------+
        |                   |                   |
        v                   v                   v
+---------------+   +---------------+   +---------------+
| Project Alpha |   | Project Beta  |   | Shared Libs   |
|               |   |               |   |               |
| Rust/Axum     |   | React/TS      |   | API Contracts |
| conventions   |   | conventions   |   | Type defs     |
| ADRs          |   | ADRs          |   | Versioned     |
| Test patterns |   | Test patterns |   |               |
+---------------+   +---------------+   +---------------+
```

**Isolation rules:**
- Default: agents see `global + current_project + relevant_shared_libs`
- Cross-project queries require explicit scope expansion
- Project-specific patterns NEVER leak to other projects automatically
- Global promotion requires validation in 3+ projects + human approval

**Sharing mechanisms:**

| Mechanism | When Used | Implementation |
|-----------|----------|----------------|
| Inheritance | Global conventions all projects follow | Global namespace, always included in context |
| Explicit import | Shared library docs, API contracts | Cross-project reference with explicit inclusion |
| On-demand retrieval | Cross-project dependency queries | Agent requests with cross-project scope when needed |
| Promotion pipeline | Proven project patterns becoming global | Validated in 3+ projects -> human review -> promote |
| Never share | Project-specific implementation details | Strict namespace isolation |

### Token Optimization Strategy

The context/memory report identifies five strategies, prioritized by impact and implementation effort:

| Strategy | Expected Savings | Effort | Phase |
|----------|-----------------|--------|-------|
| **Phase-aware filtering** (exclude irrelevant knowledge types) | 40-60% vs. loading all context | Low | Phase 0 |
| **Prompt caching** (static prefix + dynamic suffix) | Up to 90% cost reduction on static portions | Low | Phase 0 |
| **Observation masking** (replace old tool outputs with placeholders) | 50%+ within long sessions | Low | Phase 1 |
| **Granular retrieval** (paragraphs/functions, not whole documents) | 60-80% vs. full document retrieval | Medium | Phase 1 |
| **Data format optimization** (compact formats over verbose JSON/XML) | 40-50% for structured data | Low | Phase 0 |
| **Budget-aware retrieval** (greedy selection by relevance until budget exhausted) | Variable, prevents overloading | Medium | Phase 1 |
| **Pre-compaction extraction** (persist critical reasoning before context compression) | Prevents knowledge loss in long sessions | Medium | Phase 2 |
| **Context sufficiency detection** (stop retrieving when enough) | 20-40% vs. exhaustive retrieval | High | Phase 3 |

**Target outcome (from context/memory report):**

| Metric | Without Context Engine | With Context Engine |
|--------|----------------------|---------------------|
| Tokens per request | 50-130K | 10-15K |
| Context relevance | ~30% | >80% |
| Cost per request (Opus) | $0.75-$1.95 | $0.15-$0.30 |
| Cross-project contamination | Frequent | Rare |
| Phase-appropriate context | Manual | Automatic |

---

## 5. The Learning System

The learning system synthesizes the learning/knowledge evolution report with context management and orchestration findings.

### Multi-Level Learning Architecture

```
+------------------+
|   Global Level   |  Universal patterns (language idioms, security, frameworks)
+--------+---------+  Promotion: validated in 3+ projects, 90%+ success rate, human-approved
         |
  promo ^ | inject v
         |
+--------+---------+
|  Project Level   |  Project-specific knowledge (ADRs, conventions, test patterns)
+--------+---------+  Promotion: seen in 2+ sessions, validated by outcome
         |
  promo ^ | inject v
         |
+--------+---------+
|   Team Level     |  Cross-agent coordination (active modifications, conflict resolutions)
+--------+---------+  Real-time, near-ephemeral
         |
  promo ^ | inject v
         |
+--------+---------+
|  Session Level   |  Immediate learnings (corrections, tool outcomes, abandoned approaches)
+------------------+  Captured per-session, extracted asynchronously
```

**Critical rule**: Downward injection is ALWAYS budget-aware. An agent starting a session receives only knowledge relevant to its current task, not all accumulated knowledge.

### Knowledge Lifecycle and Pruning

Every knowledge record follows a defined lifecycle:

```
PROPOSED --> VALIDATED --> ACTIVE --> AGING --> DEPRECATED --> ARCHIVED
                |                     |          |
                |                     |          +--> EVOLVED (new version)
                |                     |
                +-- rejected          +-- reinforced (reset aging clock)
```

| Stage | Entry Criteria | Exit Criteria |
|-------|---------------|---------------|
| **Proposed** | Extracted from session correction, code review, or manual entry | Validated by human approval OR successful application without correction |
| **Validated** | Confirmed to work in at least one real scenario | Used successfully in 5+ sessions with 80%+ success rate |
| **Active** | Regularly used, repeatedly validated | Not accessed in 90 days (project) or 180 days (global) |
| **Aging** | Exceeded staleness threshold | Human reviews and either reinforces, evolves, or deprecates |
| **Deprecated** | Explicitly superseded or flagged as no longer applicable | 30 days after deprecation with no objections |
| **Archived** | Fully removed from active retrieval | N/A (retained for audit) |

**Pruning engine** (runs daily for project, weekly for global):

1. **Staleness detection**: Patterns not used/validated within threshold -> transition to AGING
2. **Contradiction detection**: Conflicting patterns within same scope -> flag for human resolution
3. **Redundancy detection**: Subset patterns -> merge or archive the narrower one
4. **Effectiveness analysis**: Patterns with <70% success rate -> flag for review
5. **Dependency checking**: Technology version change -> flag affected patterns

**Quantitative guidance from Evo-Memory research**: Multi-project systems should expect ~36.8% pruning rates due to cross-project knowledge overlap and divergence. This is normal and healthy.

### Trust Calibration Model

Trust is not binary. It is tracked per agent, per domain, per project:

| Trust Level | Name | Behavior |
|-------------|------|----------|
| 0 | SUPERVISED | Every action requires human approval |
| 1 | GUIDED | Agent proposes, human approves before execution |
| 2 | MONITORED | Agent executes, human reviews all output |
| 3 | AUDITED | Agent executes, human reviews samples |
| 4 | AUTONOMOUS | Agent executes, human reviews exceptions only |
| 5 | TRUSTED | Agent executes and self-validates, periodic human audit |

**Trust domains** (independent scores per domain):
- Code Generation, Code Modification, Testing, Configuration, Architecture, Security, Documentation, Operations

**Escalation criteria** (all must be met):

| Transition | Min Actions | Success Rate | Sustained Period | Human Approval |
|-----------|-------------|-------------|-----------------|----------------|
| 0 -> 1 | 10 | 80% | 3 days | Required |
| 1 -> 2 | 25 | 85% | 7 days | Required |
| 2 -> 3 | 50 | 90% | 14 days | Required |
| 3 -> 4 | 100 | 93% | 30 days | Required |
| 4 -> 5 | 200 | 95% | 60 days | Required |

**De-escalation triggers** (automatic):
- **Immediate drop to SUPERVISED**: Security vulnerability introduced, production incident, data loss
- **Drop one level**: 3 consecutive rejections, test failure rate exceeds threshold
- **Reset to GUIDED**: Major architectural error, repeated same mistake after correction

### Five Feedback Loops (Prioritized)

| Loop | Trigger | Cycle Time | Implementation Phase | ROI |
|------|---------|-----------|---------------------|-----|
| **Immediate Correction** | Human corrects agent output | Seconds | Phase 0 (MVP) | High |
| **Build/Test** | CI results from agent-generated code | Minutes | Phase 0 (MVP) | High |
| **Code Review** | PR review feedback themes | Hours-days | Phase 1 | Very High |
| **Retrospective** | Periodic accumulated analysis | Days-weeks | Phase 2 | High |
| **Outcome** | Production telemetry after deployment | Days-months | Phase 3 | Very High |

---

## 6. Implementation Roadmap

### Guiding Principle

Learn from claude-flow's failure: **prove each layer before adding the next.** Each phase has clear entry criteria, exit criteria, and deliverables. No phase begins until the previous phase's exit criteria are met.

### Phase 0: Foundation (Weeks 1-4)

**Objective**: Build the minimal platform that provides immediate value to a solo architect managing 1-2 projects. Prove the core value proposition: right context at the right time.

**Entry Criteria**: Research synthesis approved. Architecture decisions made.

**Deliverables**:

| # | Deliverable | Description |
|---|-----------|-------------|
| 0.1 | **Project scaffold** | Rust workspace with Tokio, Clap, Serde. CI/CD pipeline (GitHub Actions). Basic project structure. |
| 0.2 | **Single-project pipeline** | Phase state machine (PENDING -> RUNNING -> BLOCKED -> APPROVED -> COMPLETED). Single-agent execution per phase (no supervisor pattern yet). Manual phase transitions via CLI. |
| 0.3 | **Human gate (CLI)** | CLI-based approval workflow. Phase output displayed for review. Approve/reject with feedback. Feedback injected into agent context on retry. |
| 0.4 | **File-based context assembly** | Hierarchical AGENTS.md per project. Phase-specific context templates. Simple context compiler that concatenates relevant files based on project + phase. Token counting and budget enforcement. |
| 0.5 | **LLM provider abstraction** | `LlmProvider` trait with Anthropic implementation. Claude Opus for supervisor, Sonnet for workers. Basic cost tracking. |
| 0.6 | **Immediate correction capture** | When human rejects and provides feedback, capture the correction as a structured learning record. Store in YAML files alongside project. |
| 0.7 | **Basic observability** | Structured logging with `tracing`. Phase status in CLI output. Token usage reporting. |

**Exit Criteria**:
- Can execute a complete development pipeline (research -> architecture -> spec -> code -> test) for a single Rust project with human gates between each phase
- Phase state machine correctly handles all transitions including retry-after-rejection
- Context assembly delivers phase-appropriate context within token budget
- All state persists across process restarts (durable journal)
- Correction capture works and stored corrections are loaded in subsequent sessions

**What this proves**: The pipeline backbone works. Human gates work. File-based context provides value. The system can recover from restarts.

### Phase 1: Multi-Project and Enhanced Context (Weeks 5-10)

**Objective**: Add multi-project support and vector-enhanced context retrieval. Prove that the context engine provides measurably better agent performance than static rules files.

**Entry Criteria**: Phase 0 exit criteria met. At least 10 successful pipeline executions with real projects.

**Deliverables**:

| # | Deliverable | Description |
|---|-----------|-------------|
| 1.1 | **Multi-project registry** | Project configuration (TOML). Per-project conventions, model selection, phase configuration. CLI commands for project management. |
| 1.2 | **Project-scoped context isolation** | Namespace isolation in context assembly. Agents see only global + current project context. No cross-contamination. |
| 1.3 | **Vector-enhanced retrieval** | Embedded vector store (hnsw_rs + redb, ~1,200 line wrapper with `VectorStore` trait). Index project docs, code comments, ADRs, past session learnings. Per-project physical isolation (separate .redb + .hnsw files). Retrieval pipeline: query -> embed -> search -> re-rank -> inject. |
| 1.4 | **Observation masking** | Replace old tool outputs with placeholders in long sessions. Preserve agent reasoning chain while eliminating redundant data. |
| 1.5 | **Build/test feedback loop** | CI results automatically captured. Agent failures analyzed for recurring patterns. Patterns stored as proposed knowledge records. |
| 1.6 | **Docker agent isolation** | Agent containers via Bollard. Resource limits (CPU, memory). Health monitoring via heartbeat. Session-scoped lifecycle (agents die when session dies). |
| 1.7 | **Code review feedback extraction** | PR review comments analyzed for pattern-worthy feedback. Recurring themes flagged for knowledge capture. |

**Exit Criteria**:
- 2+ projects managed simultaneously without cross-contamination
- Embedded vector retrieval demonstrably improves context relevance (measured by A/B comparison)
- Agent containers correctly isolated with resource limits enforced
- No orphan processes after session termination
- Build/test feedback captures at least 5 reusable patterns from real usage

**What this proves**: Multi-project isolation works. Vector retrieval adds measurable value. Docker isolation is reliable.

### Phase 2: Agent Teams and Knowledge Graph (Weeks 11-18)

**Objective**: Add supervisor-worker agent teams within phases. Deploy knowledge graph for relationship-aware context. Implement trust calibration.

**Entry Criteria**: Phase 1 exit criteria met. 50+ pipeline executions across 2+ projects.

**Deliverables**:

| # | Deliverable | Description |
|---|-----------|-------------|
| 2.1 | **Supervisor-worker agent teams** | Ractor-based actor hierarchy. Phase supervisor decomposes work, assigns to workers, validates outputs. One-for-one restart strategy for worker failures. |
| 2.2 | **Event bus** | Tokio broadcast channels for system-wide events (phase transitions, gate decisions, agent lifecycle). Event replay for debugging. Optional Redis Streams bridge. |
| 2.3 | **Knowledge graph layer** | Deploy Graphiti + Neo4j. Model: projects, services, APIs, conventions, patterns, decisions as entities with temporal relationships. Hybrid retrieval: vector + graph + file via Reciprocal Rank Fusion. |
| 2.4 | **Trust calibration (basic)** | Per-agent, per-domain success rate tracking. Trust levels 0-3 implemented. Automatic de-escalation on failure patterns. |
| 2.5 | **Pre-compaction knowledge extraction** | Before Claude Code compresses context, extract critical insights and persist to knowledge store. |
| 2.6 | **Knowledge lifecycle engine** | Full lifecycle (PROPOSED -> ARCHIVED). Staleness detection. Contradiction detection. Automated pruning cycle (daily for project, weekly for global). |
| 2.7 | **Retrospective learning loop** | Periodic analysis of accumulated session data. Identify systemic patterns. Propose knowledge base updates. |

**Exit Criteria**:
- Supervisor-worker teams complete phases with verified outputs (no unvalidated claims)
- Knowledge graph provides context that vector search alone does not (measured by comparison)
- Trust levels correctly track agent reliability and adjust gate requirements
- Pruning engine removes stale patterns without losing valuable knowledge
- Event bus provides full system observability

**What this proves**: Agent teams work reliably. The knowledge graph adds value. Trust calibration enables progressive autonomy.

### Phase 3: Intelligence and Optimization (Weeks 19-26)

**Objective**: Add intelligent context assembly, cross-project learning, advanced trust, and operational polish.

**Entry Criteria**: Phase 2 exit criteria met. 200+ pipeline executions. Trust system tracking meaningful data.

**Deliverables**:

| # | Deliverable | Description |
|---|-----------|-------------|
| 3.1 | **Intelligent phase detection** | Multi-signal phase detection (workflow position + file context + task analysis). Manual override always available. |
| 3.2 | **Cross-project knowledge sharing** | Promotion pipeline: project pattern validated in 3+ projects -> nominated -> checked for applicability -> promoted to global. Explicit opt-in for cross-project patterns. |
| 3.3 | **Trust levels 4-5** | Full progressive autonomy. Confidence-based escalation. Post-action sampling. Exception-only review for trusted domains. |
| 3.4 | **Retrieval optimization** | Track which retrieved patterns are actually used by agents. Tune retrieval scoring based on usefulness data. Self-improving retrieval. |
| 3.5 | **TUI dashboard** | Ratatui-based terminal dashboard. Real-time pipeline status, active agents, pending gates, token costs, trust levels. |
| 3.6 | **Outcome feedback loop** | Connect deployment outcomes (error rates, performance) back to implementation patterns. Reinforce successful patterns, flag unsuccessful ones. |
| 3.7 | **Tiered model routing** | Route tasks by complexity: simple transforms to cheaper models, complex reasoning to Opus. Effort parameter tuning. |

**Exit Criteria**:
- Context engine delivers demonstrably better results than static rules (quantified comparison)
- Cross-project learning successfully transfers at least 5 patterns between projects
- Trust system has advanced at least some agent-domain pairs to Level 3+
- Token costs reduced by 20-40% compared to Phase 1 baseline
- Dashboard provides real-time operational visibility

**What this proves**: The full vision works. Context engineering provides measurable value. Learning transfers across projects. Progressive autonomy is achievable.

---

## 7. Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|-----------|--------|------------|
| R1 | **Overbuilding before proving value** (the claude-flow trap) | High | Critical | Strict phase gates with measurable exit criteria. No phase starts until previous phase is proven. Kill features that do not demonstrate measurable improvement. |
| R2 | **Token cost explosion from multi-agent systems** | High | High | Anthropic reports 15x token usage for multi-agent vs. single-agent. Mitigate with: tiered model routing, aggressive context optimization, per-phase/per-project token budgets with hard limits, cost dashboards. |
| R3 | **Context engine complexity exceeds value** | Medium | High | Phase the context engine incrementally. File-based first (Phase 0). Vector only when file-based proves insufficient (Phase 1). Graph only when vector proves insufficient (Phase 2). Kill any layer that does not demonstrate improvement. |
| R4 | **Orphan processes and resource leaks** | Medium | High | Lesson from claude-flow #1171 (1,625 OOM kills). Session-scoped lifecycles, PID files, heartbeat monitoring, signal handlers. Test failure scenarios explicitly. |
| R5 | **Agent verification failures** (false success reports cascading) | Medium | Critical | Lesson from claude-flow #640. Independent verification pipelines. Run tests, do not read agent claims about tests. Schema-validate all artifacts before cross-phase promotion. |
| R6 | **Landscape shifts** (GitHub Copilot, OpenAI, or Anthropic enter multi-project space) | Medium | Medium | Build on open standards (MCP). Avoid tight coupling to any single LLM provider. Focus on the learning loop as defensible moat (hardest for platform vendors to replicate). |
| R7 | **Knowledge store corruption or bad knowledge** | Low-Medium | High | Version-controlled knowledge (git-backed). Automated tests for knowledge consistency. Human approval gates for promotion. Automatic rollback if knowledge update causes regression. |
| R8 | **Rust ecosystem immaturity for agent patterns** | Low | Medium | Core libraries (Tokio, Bollard, SQLx) are mature. Ractor is newer but actively developed. Have fallback plan: Actix (more mature but lacks supervision trees) if Ractor proves problematic. |
| R9 | **Solo developer bottleneck** | Medium | Medium | Phase 0 is deliberately minimal. Use agents to assist with Unimatrix development (dogfooding). Prioritize ruthlessly; cut scope before compromising quality. |
| R10 | **Knowledge pruning removes valuable information** | Low-Medium | Medium | Archived, never deleted. Version-controlled. Pruning flags for human review before permanent removal. Automated rollback if pruning causes regression. |

---

## 8. Open Questions for Round 2 Research

### Architecture Questions (Need Investigation)

1. **MCP server architecture for context delivery**: Should the Context Engine be a standalone MCP server, embedded in the agent runtime, or a sidecar process? Each has trade-offs for latency, coupling, and portability. Need: prototype comparison.

2. **Agent execution model**: Should Unimatrix run Claude Code as a subprocess (via SDK), or interact through MCP tools, or use Anthropic's Messages API directly? The orchestration report assumes containers with LLM clients, but Claude Code's native capabilities (file editing, terminal, auto-memory) may be lost if we bypass it. Need: prototype the three approaches.

3. **Workspace isolation strategy**: Git worktrees (AgentFactory pattern) vs. Docker volume mounts vs. full repository clones per agent? Trade-offs of speed, isolation, and disk usage across multi-project scenarios. Need: benchmarks.

4. **Intra-phase vs. inter-phase communication**: The orchestration report recommends different communication patterns for each. What is the minimal viable communication layer? Can we start with just Tokio channels and add Redis Streams later without architectural pain? Need: design spike.

### Context Engine Questions (Need Prototyping)

5. **Phase detection accuracy**: How reliably can we detect SDLC phase from file context + task description? What is the false positive rate? What is the cost of incorrect phase detection? Need: prototype with real tasks.

6. **Embedding model selection for code**: General-purpose embeddings may not distinguish `fetchUser()` from `deleteUser()` effectively. Code-specific models (CodeBERT, StarCoder embeddings, nomic-embed-code) need evaluation. Need: embedding quality comparison on real project knowledge.

7. **Token budget allocation**: How should the budget be split across context sources (system instructions vs. retrieved knowledge vs. conversation history)? Does optimal allocation vary by phase? Need: empirical measurement across real tasks.

8. **Context quality metrics**: How do we measure whether delivered context actually helps? Options: task success rate with/without context, agent self-reported usefulness, token efficiency ratio. Need: define metrics and instrumentation plan.

### Learning System Questions (Need Design)

9. **Pattern extraction quality**: When an LLM extracts a "generalizable pattern" from a correction, how often is the extraction actually correct and useful? What is the precision/recall trade-off? Need: manual evaluation of auto-extracted patterns.

10. **Cross-project knowledge transfer**: Under what conditions does a pattern from Project A actually benefit Project B? Can we define applicability heuristics (same language? same framework? same domain?) that prevent harmful transfers? Need: case study analysis.

11. **Trust calibration thresholds**: The learning report proposes specific numbers (10 actions at 80% for Level 0->1, etc.). Are these thresholds appropriate for a solo architect's workflow? Should they be different for different project sizes? Need: simulation or empirical calibration.

### Prototyping Experiments Recommended

| Experiment | Question Answered | Effort | Priority |
|-----------|------------------|--------|----------|
| **P1**: Context compiler prototype | Can a phase-aware context assembler reduce tokens by >50% vs. loading everything? | 1 week | Critical |
| **P2**: Claude Code execution model | Subprocess vs. API vs. MCP -- which preserves Claude Code's strengths while allowing orchestration? | 1 week | Critical |
| **P3**: Embedded vector store spike | Can we get project-scoped vector retrieval (hnsw_rs + redb wrapper) working with meaningful code context in <3 days? | 3 days | High |
| **P4**: Correction extraction pipeline | How accurately can an LLM generalize from a specific correction to a reusable pattern? | 3 days | High |
| **P5**: Docker agent lifecycle | Can we reliably create, monitor, and teardown agent containers with Bollard in <2 days? What about cleanup on unexpected termination? | 2 days | High |
| **P6**: Phase detection accuracy | Build a simple phase classifier and measure accuracy on 50 real task descriptions | 2 days | Medium |

---

## 9. Decision Log

### Decisions Made (Based on Research Evidence)

| # | Decision | Rationale | Evidence |
|---|----------|-----------|----------|
| D1 | **Build in Rust with Tokio async runtime** | Performance, safety, single-binary deployment. All required libraries are mature. Matches primary project language. | Orchestration report: comprehensive Rust ecosystem assessment showing no gaps. |
| D2 | **Phase-pipeline as primary orchestration pattern** | Development workflows are inherently sequential with phase dependencies. Pipelines provide maximum predictability. | Orchestration report: pattern selection matrix. All 6 reports reference phased development. |
| D3 | **Human gates between every phase initially** | Verification is non-negotiable. Trust must be earned before reducing oversight. | Claude-flow analysis: cascading false success. Learning report: trust escalation framework. |
| D4 | **MCP as context delivery and tool integration protocol** | 97M+ monthly SDK downloads. Adopted by Anthropic, OpenAI, donated to Linux Foundation. De facto standard. | Landscape survey: universal MCP adoption. Context report: MCP as delivery protocol. |
| D5 | **Docker containers for agent isolation** | Security, reproducibility, resource limits, clean teardown. Industry standard pattern. | Landscape survey: Docker sandboxes are standard. Orchestration report: container topology design. |
| D6 | **File-based context as foundation, augmented incrementally** | Simple, version-controlled, zero infrastructure. Proven to work. Add vector/graph only when measured need. | Context report: "file-based approaches are the pragmatic foundation." JetBrains: simple approaches outperform. |
| D7 | **Embedded vector storage via hnsw_rs + redb (direct library deps)** | Single-binary deployment. No external service overhead. Underlying libs are mature (hnsw_rs: 194K downloads; redb: 4,200 stars, 34 contributors, ACID). ~1,200 lines of purpose-built wrapper vs. running a separate Qdrant Docker service. `VectorStore` trait provides clean migration path to Qdrant if needed later. | Vector storage decision analysis: Option B. ruvector analysis: core libs work but wrapper has bugs (Issue #134 deadlock). |
| D8 | **Ractor for actor framework** | Erlang-style supervision trees map to agent hierarchy. Tokio-native. Built-in distribution. | Orchestration report: comparative analysis of 5 actor frameworks. |
| D9 | **Do NOT depend on claude-flow or AgentFactory as runtime dependencies** | Claude-flow: 85% mock implementations, broken memory, orphan processes. AgentFactory: 10 days old, no community validation. Both too risky. | Claude-flow analysis: comprehensive issue catalog. AgentFactory analysis: maturity assessment. |
| D10 | **Context is compiled per invocation, not accumulated** | Google ADK research confirms working context should be ephemeral, recomputed per inference. Accumulation leads to bloat and stale context. | Context report: Google ADK pattern. Anthropic: "every token competes for attention." |
| D11 | **Knowledge has a lifecycle with active pruning** | Stale knowledge is worse than no knowledge. Append-only systems degrade over time. | Learning report: staleness danger analysis. Evo-Memory research: 36.8% pruning rates for diverse domains. |
| D12 | **Start with 1-agent-per-phase, add teams later** | Anthropic reports 15x token cost for multi-agent. Prove value with single agent before scaling. | Orchestration report: Anthropic multi-agent token costs. Claude-flow: ambition outpaced implementation. |

### Decisions Requiring More Information

| # | Decision | What's Needed | Target Phase |
|---|----------|--------------|-------------|
| D13 | **Agent execution model** (Claude Code subprocess vs. API direct vs. MCP) | Prototype P2 results comparing the three approaches | Before Phase 0 implementation |
| D14 | **Workspace isolation strategy** (worktrees vs. Docker volumes vs. full clones) | Prototype P5 results with benchmarks | Before Phase 1 |
| D15 | **Embedding model choice** (general-purpose vs. code-specific) | Evaluation on real project knowledge | Before Phase 1 embedded vector deployment |
| D16 | **Token budget allocation per context source** | Empirical measurement from Phase 0 usage data | Before Phase 1 context optimization |
| D17 | **Trust calibration thresholds** | Empirical data from Phase 1 usage | Before Phase 2 trust implementation |
| D18 | **Graph database timing** (is Neo4j overhead justified for 1-3 projects?) | Usage data from Phase 1 showing vector-only limitations | Before Phase 2 graph deployment |
| D19 | **Event bus technology** (Tokio channels sufficient, or Redis Streams needed from start?) | Design spike D4 results | Before Phase 2 event bus |
| D20 | **Linear integration vs. generic issue tracker abstraction** | Evaluation of Linear for Agents API (still in developer preview) | Before Phase 1 |

---

*This synthesis is based on 6 research reports totaling approximately 40,000 words of analysis. Every recommendation is grounded in specific findings from those reports. Where evidence was ambiguous or conflicting, both positions are noted with the recommended resolution. The roadmap is designed for a solo architect managing 1-3 projects initially, scaling to 3-5, with the principle that every layer must prove its value before the next is added.*
