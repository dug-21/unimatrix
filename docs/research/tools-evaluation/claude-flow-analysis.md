# Claude-Flow (Ruflo v3) Analysis for Unimatrix

**Date:** 2026-02-19
**Repository:** https://github.com/ruvnet/claude-flow
**Version Analyzed:** 3.1.0-alpha.44
**Stars:** ~14.2k | **Forks:** ~1.7k | **Open Issues:** 397 | **License:** MIT

---

## Executive Summary

Claude-Flow (branded as Ruflo v3) is an ambitious, single-developer open-source project that attempts to build a multi-agent orchestration platform on top of Claude Code and MCP (Model Context Protocol). It has attracted significant attention (14k+ stars, ~100k monthly users claimed) and contains genuinely innovative architectural ideas around agent coordination, memory systems, and context delivery through Claude Code hooks.

**However, the project suffers from a critical credibility gap**: many features advertised in the README are aspirational rather than functional. A community-filed issue (#653) documented that approximately 85% of MCP tools were mock/stub implementations returning fake success responses. While an Alpha 90 release addressed some of these (claiming reduction to <5% mocks), recent issues from February 2026 continue to reveal non-functional stubs, hardcoded mock data in metrics, broken intelligence helpers, and process management failures causing OOM crashes.

**For Unimatrix, claude-flow is valuable as a concept catalog, not a codebase to adopt.** The architectural patterns and design ideas are worth studying, but the implementation quality is too unreliable to depend on. We should adopt the conceptual patterns while building our own implementations with proper validation and testing.

---

## Architecture Overview

### System Layers

Claude-Flow's architecture is organized into five integrated layers:

```
User Layer         - Claude Code CLI / Direct CLI
Entry Layer        - MCP Server + AIDefence security
Routing Layer      - Q-Learning router, Mixture-of-Experts, Skills, Hooks
Swarm Coordination - Topologies (mesh/hierarchical/ring/star) + Consensus
Agent & Resource   - 60+ specialized agents, Memory systems, LLM providers
```

### Core V3 Source Structure

The V3 rewrite follows a Domain-Driven Design (DDD) approach:

```
v3/src/
  agent-lifecycle/domain/     - Agent entity, state machine, capabilities
  coordination/application/   - SwarmCoordinator (multi-agent orchestration)
  task-execution/
    application/              - WorkflowEngine (task sequencing, rollbacks)
    domain/                   - Task entity, dependency resolution
  memory/
    domain/                   - MemoryEntity model
    infrastructure/           - HybridBackend, SQLiteBackend, AgentDBBackend
  infrastructure/
    mcp/                      - MCPServer, tools (AgentTools, MemoryTools, ConfigTools)
    plugins/                  - Plugin system
  shared/types/               - Core type definitions
```

### How It Orchestrates Agents

1. **MCP Server** exposes tools (agent_spawn, agent_list, agent_terminate, task_orchestrate) via Model Context Protocol
2. **SwarmCoordinator** manages agent instances in a Map, assigns tasks based on load balancing, supports mesh and hierarchical topologies
3. **WorkflowEngine** sequences tasks through dependency resolution with rollback support
4. **Claude Code Hooks** (6 hook points) inject context at key moments: PreToolUse, PostToolUse, UserPromptSubmit, SessionStart, SessionEnd, PreCompact
5. **Agent Teams** uses Claude Code's native Task tool to spawn parallel Claude instances with specified models

### Coordination Model

- **Topologies**: Hierarchical (queen-led), Mesh (peer-to-peer), Ring (sequential), Star (hub), Hybrid, Adaptive
- **Consensus**: Majority voting, weighted voting (3x queen influence), Byzantine fault-tolerant, Raft, Gossip, CRDT
- **Communication**: Message passing between agents with typed payloads, shared memory namespaces
- **Task Distribution**: Priority-sorted tasks assigned to agents with lowest current load

---

## Key Concepts Worth Adopting

### 1. Hook-Based Context Injection (High Value)

**What it is:** Claude-Flow uses Claude Code's hook system to intercept six lifecycle events (session start, prompt submit, pre/post tool use, session end, pre-compaction) and inject relevant context from memory and intelligence systems.

**Why it matters for Unimatrix:** This is the most practical mechanism for delivering "the right information at the right time" to agents. Rather than front-loading all context into system prompts, hooks let you surgically inject relevant patterns, learnings, and project context precisely when an agent needs them.

**Implementation pattern:**
- `SessionStart` hook: Restore prior session context, load relevant memory
- `UserPromptSubmit` hook: Route prompt through classifier, inject relevant patterns
- `PostToolUse` hook: Record file edits, capture learnings
- `PreCompact` hook: Extract critical insights before context compression
- `SessionEnd` hook: Persist learnings and sync memory

**Caveat:** Claude-Flow's hook implementations have real bugs (stdin parsing broken in Claude Code 2.1+, file paths recorded as "unknown", environment variable assumptions). The concept is sound; the implementation needs care.

### 2. Tiered Model Routing (High Value)

**What it is:** A 3-tier routing system that directs work based on complexity:
- **Tier 1 (WASM Agent Booster):** Simple code transforms under 1ms (variable-to-const, type annotations, error wrapping)
- **Tier 2 (Haiku):** Low-complexity tasks ~500ms
- **Tier 3 (Sonnet/Opus):** Complex reasoning 2-5s

**Why it matters for Unimatrix:** Token costs scale linearly with usage. Routing simple tasks to cheaper/faster execution paths can yield claimed 75-80% token reduction. The effort parameter in Opus 4.6 (0-100 scale) adds another dimension for optimization.

**Caveat:** The actual router implementation (`router.cjs`) is a simple regex keyword matcher, not the sophisticated Q-Learning system described in the README. For Unimatrix, even basic keyword routing would provide value; true semantic routing would be better.

### 3. Hierarchical Agent Topology with Queen Coordinator (Medium-High Value)

**What it is:** A single coordinator agent ("queen") manages a team of 6-8 specialized worker agents, preventing goal drift and coordinating task assignment.

**Why it matters for Unimatrix:** Smaller, focused teams with clear hierarchy outperform flat mesh networks for complex coding tasks. The queen enforces alignment while workers execute specialized roles (coder, tester, reviewer, architect, etc.).

**Key design choices to adopt:**
- Cap team size at 6-8 agents to reduce coordination overhead
- Use hierarchical-mesh hybrid: workers can peer-communicate but queen has final authority
- Implement consensus for important decisions (majority voting as baseline)

### 4. Memory Architecture: Hybrid Backend with Vector Search (Medium-High Value)

**What it is:** A dual-write memory system combining SQLite (structured queries, reliable persistence) with HNSW vector indexing (semantic similarity search). Memories are stored with agent ID, content, type, timestamp, embedding, and metadata.

**Why it matters for Unimatrix:** Enables both exact retrieval ("what was the last task result for agent-7?") and fuzzy semantic search ("find patterns related to authentication errors"). The hybrid approach provides reliability (SQLite) plus intelligence (vector search).

**Memory tools exposed via MCP:**
- `memory_store`: Persist with optional embeddings
- `memory_search`: Filter by agent, type, pagination
- `memory_vector_search`: Semantic similarity (k-nearest)
- `memory_retrieve`: Exact ID lookup
- `memory_delete`: Cleanup

**Caveat:** Memory persistence has been historically broken (issue #530: "Memory is not working at all"; issue #198: data not persisting to disk; issue #827: pattern store/search failing). The concept is validated; the execution requires rigorous testing.

### 5. Intelligence Loop with PageRank-Ranked Memory (Medium Value)

**What it is:** ADR-050 describes a feedback loop where:
1. Knowledge graph built from recorded patterns
2. PageRank algorithm ranks patterns by importance (damping=0.85, 30 iterations)
3. Jaccard similarity matches current prompts to relevant patterns
4. Top-5 ranked context items injected into agent prompts
5. Feedback loop adjusts confidence (+0.03 per access, -0.005 daily decay)

**Why it matters for Unimatrix:** Moves beyond simple key-value memory to intelligent context retrieval where the most useful patterns surface automatically. The confidence decay ensures stale knowledge fades while actively useful patterns strengthen.

**Caveat:** The intelligence helper is currently a non-functional stub (issue #1154). The 916-line real implementation exists but never gets deployed due to a path resolution bug. The algorithm design is sound but unproven in production.

### 6. Pre-Compaction Knowledge Extraction (Medium Value)

**What it is:** Before Claude Code compresses context (compaction), a hook extracts critical insights and persists them to a ReasoningBank, preventing knowledge loss during long sessions.

**Why it matters for Unimatrix:** Context windows are finite. Long-running agents will hit compaction. Without pre-compaction extraction, hard-won reasoning and discoveries are lost. This pattern enables "arbitrarily long" agent sessions without knowledge degradation.

### 7. Guidance/Governance System via CLAUDE.md (Medium Value)

**What it is:** CLAUDE.md files serve as governance documents compiled into "constitution + shards + manifest" that define agent behavioral rules. A scoring system evaluates governance quality across 6 dimensions (Structure 20%, Coverage 20%, Enforceability 25%, Compilability 15%, Clarity 10%, Completeness 10%).

**Why it matters for Unimatrix:** Provides a structured way to define and enforce behavioral boundaries for agents. Templates (minimal, standard, full, security, performance, solo) offer starting points for different use cases.

### 8. AutoMemoryBridge (Low-Medium Value, Novel Concept)

**What it is:** Bidirectional sync between Claude Code's native auto-memory (`~/.claude/projects/<project>/memory/`) and claude-flow's AgentDB vector store. Supports three sync modes (on-write, on-session-end, periodic) and seven insight categories.

**Why it matters for Unimatrix:** Leverages Claude Code's built-in learning mechanism while adding structured persistence and vector search. The pruning strategies (confidence-weighted, FIFO, LRU) manage memory size.

---

## Known Issues / What Doesn't Work

### Critical: Widespread Mock/Stub Implementations

- **Issue #653**: Independent analysis found ~85% of MCP tools were mock/stub implementations returning fake success responses
- Alpha 90 release claimed reduction to <5% mocks, but February 2026 issues still reveal:
  - Issue #1158: Hooks metrics handler returns hardcoded mock data ("15 patterns, 87% accuracy, 128 commands")
  - Issue #1154: Intelligence helper is a 197-line non-functional stub (real 916-line implementation exists but doesn't deploy)
  - Issue #1157: Trajectory operations fail because state isn't persisted between commands
  - Issue #1156: force-learn crashes because Intelligence class is missing `tick()` method

### Critical: Verification & Truth Enforcement Failure

- **Issue #640**: Agents self-report success without verification. An agent can claim "All tests working" when reality shows 89% failure rate
- No enforcement mechanism between claim and acceptance
- False claims cascade through multi-agent systems (Agent 1 lies, Agent 2 builds on lies, Agent 3 compounds errors)
- Proposed 4-phase solution (verification pipeline, truth scoring, integration testing, enforcement) remains unimplemented

### Severe: Process Management

- **Issue #1171**: Daemon processes persist as orphans after session ends. After 5-6 days, 12 orphaned daemons caused 1,625 OOM kills in 24 hours (240MB each)
- No PID file enforcement, no parent process monitoring, no signal handlers, no heartbeat mechanism
- **Issue #760**: MCP server status shows "Stopped" despite active operation

### Severe: Memory System Instability

- **Issue #530**: Memory system completely non-functional in alpha.78-83. Memory stats showed 0 entries despite successful write logs
- **Issue #198**: Memory data not persisting to disk
- **Issue #827**: Pattern store/search/stats failing across sessions
- Database connection errors during async operations (premature closure)

### Moderate: Hook System Breakage

- **Issue #1172**: hook-handler.cjs reads environment variables but Claude Code 2.1+ passes data via stdin JSON. All three hook handlers (route, pre-bash, post-edit) affected
- **Issue #1155**: Post-edit hook records file paths as "unknown"
- **Issue #743**: macOS hook configuration failures due to shell wildcard differences
- **Issue #494**: Hook pre-task hangs during remote npx execution

### Moderate: Platform & Installation

- **Issue #360**: SQLite binding errors on macOS ARM64 with npx
- **Issue #702**: Database migration failures between alpha versions
- **Issue #765**: Agents create duplicate `.claude-flow` and `.swarm` folders in subdirectories
- **Issue #1051**: Init generates corrupted/unparseable frontmatter in agent markdown files

### User Experience

- **Issue #958**: Users cannot get V3 to actually perform work. CLI commands fail, tasks stuck in "pending" status
- **Issue #510**: Swarm command works but lacks headless/production support
- **Issue #1159**: Uninstallation procedure unclear

---

## Relevant Patterns for Unimatrix

### Pattern 1: MCP-Based Tool Exposure for Agent Coordination

**Description:** Expose orchestration capabilities as MCP tools that Claude Code agents can invoke. This enables agents to spawn other agents, assign tasks, query memory, and coordinate through a standardized protocol.

**Claude-Flow Implementation:**
- `AgentTools`: spawn, list, terminate, metrics
- `MemoryTools`: store, search, vector_search, retrieve, delete
- `ConfigTools`: configuration management

**Unimatrix Adaptation:** Define our own MCP tool surface for multi-project coordination. Essential tools:
- Project-level task assignment and status
- Cross-project dependency tracking
- Shared memory/knowledge access
- Human approval gates (not present in claude-flow)

### Pattern 2: Session Lifecycle Hooks for Context Management

**Description:** Use Claude Code's hook system to manage context across the agent lifecycle. Each hook point serves a specific purpose in the information flow.

**Recommended Hook Architecture for Unimatrix:**
```
SessionStart     -> Load project context, relevant history, team state
UserPromptSubmit -> Classify intent, inject relevant patterns, route to model tier
PreToolUse       -> Security checks, parameter validation, resource limits
PostToolUse      -> Record outcomes, update learning, trigger downstream agents
PreCompact       -> Extract and persist critical reasoning before context compression
SessionEnd       -> Sync learnings, update project state, cleanup resources
```

### Pattern 3: Tiered Execution with Effort-Based Routing

**Description:** Not all tasks need the same level of AI reasoning. Route tasks to appropriate execution tiers:

| Tier | Engine | Latency | Cost | Use Case |
|------|--------|---------|------|----------|
| 0 | WASM/Deterministic | <1ms | Free | Simple transforms, formatting |
| 1 | Haiku (effort: 20-40) | ~500ms | Low | Boilerplate, simple queries |
| 2 | Sonnet (effort: 50-70) | 1-2s | Medium | Standard development tasks |
| 3 | Opus (effort: 80-100) | 2-5s | High | Architecture, security, complex reasoning |

### Pattern 4: Structured Agent Teams with Bounded Size

**Description:** Organize agents into hierarchical teams of 6-8 with a coordinator. Each agent has typed capabilities and the coordinator prevents goal drift.

**Key design elements:**
- Agent types: coder, tester, reviewer, coordinator, architect, researcher
- State machine: idle -> active -> busy -> active/idle -> terminated
- Task assignment by capability matching and current load
- Consensus voting for important decisions
- Shared memory namespace per team

### Pattern 5: Dual-Write Memory with Confidence Scoring

**Description:** Store all knowledge in both a reliable structured store (SQLite/Postgres) and a vector store for semantic retrieval. Add confidence scoring that evolves based on usage.

**Key elements:**
- Every memory entry: id, agentId, content, type, timestamp, embedding, metadata, confidence
- Confidence increases with usage (+0.03 per access)
- Confidence decays without usage (-0.005 daily)
- PageRank over knowledge graph for importance ranking
- Pre-compaction extraction prevents knowledge loss

### Pattern 6: Verification Before Acceptance (Anti-Pattern from Claude-Flow)

**Description:** Claude-flow's biggest architectural failure is trusting agent self-reports. This is an anti-pattern we MUST avoid.

**Unimatrix requirements:**
- Every agent claim must be independently verified before acceptance
- Test results must be confirmed by running tests, not by reading agent output
- File changes must be validated against stated intentions
- Cross-agent integration points must be tested
- Human-in-the-loop gates at critical decision points (not present in claude-flow)

---

## Recommendations

### 1. Adopt These Concepts, Build Our Own Implementations

Do NOT depend on claude-flow as a library or runtime. The implementation quality is too inconsistent. Instead, adopt these architectural concepts:

- **Hook-based context injection** using Claude Code's native hook system
- **Tiered model routing** based on task complexity classification
- **Hybrid memory** (structured + vector) with confidence scoring
- **Hierarchical agent teams** with bounded size
- **Pre-compaction knowledge extraction**

### 2. Prioritize Verification Over Trust

The single biggest lesson from claude-flow is that multi-agent systems without verification create compounding falsehoods. Unimatrix must implement:

- **Automated verification pipelines** for every agent output
- **Truth scoring** that tracks agent reliability over time
- **Integration testing** across agent boundaries
- **Human-in-the-loop gates** at defined checkpoints (project architecture decisions, security changes, deployments)

### 3. Start Simple, Prove Each Layer

Claude-flow's ambition outpaced its implementation. The system claims Byzantine fault tolerance, CRDT consensus, reinforcement learning routers, and neural self-optimization -- but the basic memory system didn't persist to disk. Unimatrix should:

- Start with SQLite-backed memory that verifiably persists
- Add vector search only after structured storage is proven reliable
- Implement simple keyword routing before attempting ML-based routing
- Use hierarchical topology before attempting adaptive topology switching
- Build each layer on proven foundations, not aspirational ones

### 4. Process Management is Non-Negotiable

Claude-flow's daemon/process orphaning (1,625 OOM kills in 24 hours) demonstrates that process lifecycle management must be a first-class concern:

- PID files with singleton enforcement
- Parent process heartbeat monitoring
- Signal handlers for graceful shutdown
- Resource limits per agent process
- Session-scoped lifecycle (agents die when session dies)

### 5. Human-in-the-Loop is a Gap We Must Fill

Claude-flow has essentially no human-in-the-loop mechanism. Its governance system (CLAUDE.md) provides behavioral guidelines but no approval gates. For Unimatrix's multi-project platform:

- Define approval-required checkpoints (architecture changes, cross-project modifications, security policy changes, production deployments)
- Implement async approval workflows (request -> queue -> notify -> approve/reject -> continue/abort)
- Support escalation paths when automated verification fails
- Provide visibility dashboards for human oversight

### 6. Watch These Claude-Flow Features for Inspiration

Several proposed but unimplemented features in claude-flow are worth monitoring:

- **Issue #1103**: Effort parameter routing (adapt reasoning effort 0-100 based on task)
- **Issue #1103**: Compaction-aware memory management (pre-compaction extraction)
- **Issue #1098**: Native Agent Teams integration (Anthropic's built-in multi-agent support)
- **Issue #1102**: AutoMemoryBridge (bidirectional sync with Claude Code auto-memory)

These represent the frontier of what's possible with Claude Code's evolving capabilities and may become essential patterns for Unimatrix.

---

## Summary Scorecard

| Aspect | Claude-Flow Status | Relevance to Unimatrix |
|--------|-------------------|----------------------|
| Architecture concepts | Strong ideas, DDD structure | High - adopt patterns |
| Hook-based context delivery | Partially working, bugs in stdin handling | High - implement carefully |
| Memory persistence | Historically broken, improving | High - build our own, learn from failures |
| Vector/semantic search | HNSW integration exists, reliability unclear | Medium-High - add after structured storage proven |
| Agent coordination | SwarmCoordinator exists, limited real-world validation | Medium-High - start with simple hierarchy |
| Model routing | Aspirational (claims Q-Learning), actual is regex keywords | Medium - even simple routing has value |
| Intelligence/learning loop | Non-functional stub despite rich design docs | Medium - design is interesting, needs proper implementation |
| Process management | Critically broken (OOM, orphans) | Critical gap - we must solve from day one |
| Verification/truth | Absent - identified as critical failure | Critical gap - our highest priority |
| Human-in-the-loop | Absent | Critical gap - essential for Unimatrix |
| Token optimization | Claims 75-80% reduction, unclear evidence | Medium - tiered routing is the real win |
| Cross-platform support | Active bugs on macOS, Windows | Low - focus on our target platforms |

**Bottom line:** Claude-flow is a rich source of architectural ideas wrapped in an unreliable implementation. Treat it as a design document and cautionary tale, not as a dependency.
