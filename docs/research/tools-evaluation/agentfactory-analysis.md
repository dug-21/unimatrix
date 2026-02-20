# AgentFactory Analysis

**Repository**: [supaku/agentfactory](https://github.com/supaku/agentfactory)
**License**: MIT
**Language**: TypeScript 5.0+
**Date of Analysis**: 2026-02-19

---

## Executive Summary

AgentFactory is an open-source, TypeScript-based platform that orchestrates multiple AI coding agents (Claude, Codex, Amp) through a factory-inspired pipeline: development, QA, and acceptance. Created by Supaku and first published on February 9, 2026, it is a young but actively developed project with 71 commits across a well-structured monorepo of 7 packages. The project has 29 GitHub stars and 3 forks as of analysis date.

The core metaphor -- a software factory with assembly lines, work stations, and floor managers -- maps directly to the concerns of multi-project agentic development. AgentFactory demonstrates mature thinking about agent lifecycle management, crash recovery, provider abstraction, and work-type specialization, even though the project itself is early-stage. Several of its design patterns are directly applicable to the Unimatrix platform, particularly its provider abstraction layer, state recovery system, and agent definition templates.

However, AgentFactory is tightly coupled to Linear as its issue tracker and is primarily designed for single-repository workflows rather than multi-project orchestration. It would serve better as a design reference and source of pattern inspiration than as a dependency or integration target.

---

## Architecture Overview

### Monorepo Structure

AgentFactory is organized as a pnpm + Turborepo monorepo with seven packages:

| Package | npm Name | Purpose |
|---------|----------|---------|
| `core` | `@supaku/agentfactory` | Orchestrator, provider abstraction, crash recovery |
| `linear` | `@supaku/agentfactory-linear` | Linear issue tracker integration, agent sessions |
| `server` | `@supaku/agentfactory-server` | Redis work queue, session storage, worker pool |
| `cli` | `@supaku/agentfactory-cli` | CLI tools: orchestrator, workers, Linear CLI |
| `nextjs` | `@supaku/agentfactory-nextjs` | Next.js route handlers, webhook processor |
| `dashboard` | (internal) | Fleet overview, pipeline kanban, session details UI |
| `create-app` | `@supaku/create-agentfactory-app` | Project scaffolding tool |

### Core Architecture Diagram

```
+---------------------------------------------------+
|                   Orchestrator                      |
|  +----------+  +----------+  +----------+          |
|  | Agent 1  |  | Agent 2  |  | Agent 3  |          |
|  | (Claude) |  | (Codex)  |  | (Claude) |          |
|  | DEV:#123 |  | QA:#120  |  | DEV:#125 |          |
|  +----+-----+  +----+-----+  +----+-----+          |
|       |              |              |                |
|  +----+-----+  +----+-----+  +----+-----+          |
|  | Worktree |  | Worktree |  | Worktree |          |
|  | .wt/#123 |  | .wt/#120 |  | .wt/#125 |          |
|  +----------+  +----------+  +----------+          |
+---------------------------------------------------+
        |                    |
   +----+----+         +----+----+
   | Linear  |         |   Git   |
   |   API   |         |   Repo  |
   +---------+         +---------+
```

### Distributed Worker Architecture

For horizontal scaling, AgentFactory supports a Redis-backed distributed worker pool:

```
+----------------+     +---------+     +----------------+
| Webhook Server |---->|  Redis  |<----| Worker Node 1  |
| (receives      |     |  Queue  |     | (claims work)  |
|  issues)       |     |         |     +----------------+
+----------------+     |         |     +----------------+
                       |         |<----| Worker Node 2  |
                       |         |     | (claims work)  |
                       +---------+     +----------------+
```

### Key Architectural Decisions

1. **Git worktrees for isolation**: Each agent operates in its own git worktree, preventing cross-contamination between concurrent tasks. Worktrees are named with issue identifiers and work-type suffixes (e.g., `SUP-294-QA`).

2. **Provider abstraction**: The `AgentProvider` interface decouples the orchestrator from any specific AI agent SDK. Providers implement `spawn()` and `resume()` methods returning a normalized `AgentHandle` with a unified event stream.

3. **Assembly-line pipeline**: Issues flow through defined stages (Backlog -> Started -> Finished -> Delivered -> Accepted), each mapped to a specialized work type with its own agent definition.

4. **State persistence via filesystem**: Agent state is persisted to a `.agent/` directory within each worktree, containing `state.json`, `heartbeat.json`, and `todos.json`. This enables crash recovery without a central database.

---

## Key Design Patterns

### 1. Provider Abstraction Pattern

The most transferable pattern in AgentFactory is its provider abstraction layer. All AI agent interactions are mediated through two interfaces:

```typescript
interface AgentProvider {
  readonly name: 'claude' | 'codex' | 'amp'
  spawn(config: AgentSpawnConfig): AgentHandle
  resume(sessionId: string, config: AgentSpawnConfig): AgentHandle
}

interface AgentHandle {
  sessionId: string | null
  stream: AsyncIterable<AgentEvent>
  injectMessage(text: string): Promise<void>
  stop(): Promise<void>
}
```

Events are normalized into a common type union (`AgentEvent`) covering: `init`, `system`, `assistant_text`, `tool_use`, `tool_result`, `tool_progress`, `result`, and `error`. This means the orchestrator never interacts with provider-specific SDKs directly.

**Provider selection** is configurable at three levels of granularity:
- Global default: `AGENT_PROVIDER=claude`
- Per work-type: `AGENT_PROVIDER_QA=codex`
- Per project: `AGENT_PROVIDER_SOCIAL=amp`

**Relevance to Unimatrix**: This pattern is directly applicable. A multi-project platform needs provider abstraction to support heterogeneous agent configurations across different projects and phases.

### 2. Agent Lifecycle Management

Agent lifecycle is tracked through the `AgentProcess` interface with these states:

```
starting -> running -> completed | failed | stopped | incomplete
```

The lifecycle includes:

- **Creation**: `spawnAgentForIssue()` creates a git worktree, initializes the `.agent/` directory, generates a work-type-specific prompt, and delegates to the provider.
- **Monitoring**: A heartbeat system (default 10-second interval, 30-second timeout) tracks agent liveness. An inactivity timeout (default 5 minutes, configurable per work type) stops stalled agents.
- **Crash Recovery**: On restart, the system scans for worktrees with stale heartbeats, reads persisted state, and builds recovery prompts that include previous progress context. Recovery attempts are capped (default: 3).
- **Completion**: Agents emit structured result markers (`<!-- WORK_RESULT:passed -->` or `<!-- WORK_RESULT:failed -->`), which the orchestrator parses to determine status transitions.
- **Teardown**: Worktree cleanup is managed exclusively by the orchestrator, never by agents. A safety mechanism preserves worktrees when PR creation fails to prevent data loss.

**Relevance to Unimatrix**: The crash recovery pattern with state persistence and recovery prompts is particularly valuable. The principle of "orchestrator owns teardown, agents never self-cleanup" is a strong safety pattern for multi-project environments.

### 3. Work-Type Specialization (Agent Definitions)

AgentFactory defines five agent roles through markdown-based "agent definitions" placed in `.claude/agents/`:

| Definition | Stage | Responsibility |
|-----------|-------|---------------|
| `developer.md` | Development | Implement features, create PRs |
| `qa-reviewer.md` | QA | Validate implementation, run tests |
| `coordinator.md` | Coordination | Orchestrate parallel sub-issues |
| `acceptance-handler.md` | Acceptance | Validate, merge PRs, cleanup |
| `backlog-writer.md` | Planning | Transform plans into Linear issues |

Each definition includes:
- YAML frontmatter (name, description, tools, model)
- Detailed step-by-step workflow instructions
- Validation commands scoped to affected packages
- Critical constraints (e.g., "never clean up own worktree")
- Required structured result markers for orchestrator parsing

The **coordinator** pattern is especially notable: it manages parallel sub-issue execution with dependency graphs, using Claude Code's Task system to track progress across concurrent sub-agents.

**Relevance to Unimatrix**: The template system maps well to Unimatrix's need for different development phases (architecture, coding, testing, CI/CD). The approach of role-specific markdown instructions with structured output contracts is a proven pattern for agent specialization.

### 4. Configuration Management

Configuration operates at multiple levels:

1. **Environment variables**: `LINEAR_ACCESS_TOKEN`, `AGENT_PROVIDER`, `REDIS_URL`, timeouts, auto-trigger flags.
2. **Programmatic config**: The `OrchestratorConfig` interface with typed options for concurrency, worktree paths, streaming, timeouts, and sandbox mode.
3. **Per-work-type overrides**: Timeout and provider settings can be customized per work type (e.g., QA gets longer inactivity timeouts).
4. **Route factory**: `createAllRoutes(config)` produces 21+ Next.js route handlers from a single configuration object, covering webhooks, OAuth, workers, sessions, and cleanup.

**Relevance to Unimatrix**: The layered configuration approach (env vars -> programmatic config -> per-work-type overrides) is a good model for multi-project configuration where defaults need to be overridable at project and phase levels.

### 5. Work Queue and Distributed Processing

The Redis-backed work queue uses:
- **Sorted set** for priority ordering: `score = (priority * 1e13) + timestamp`
- **Hash** for O(1) item lookup: `sessionId -> JSON work item`
- **Atomic claims** via `SETNX` with TTL: prevents race conditions between workers

Work items carry rich metadata: session ID, issue identifier, priority, prompt, work type, source session ID (for QA tracking), and project name (for worker routing).

The queue supports: queueing, peeking, claiming, releasing, re-queuing with priority boosts, and legacy migration from a simpler list-based implementation.

**Relevance to Unimatrix**: The priority-queue pattern with atomic claims is directly transferable to multi-project work distribution. The `projectName` field for worker routing hints at the kind of project-scoped dispatching Unimatrix would need.

### 6. Linear Integration as Session Manager

The `AgentSession` class in `@supaku/agentfactory-linear` manages:
- Lifecycle methods: `start()`, `complete()`, `fail()`, `awaitInput()`
- Activity streaming: thoughts, actions, tool results visible in Linear
- Plan management: task checklists with progress states
- Sub-issue coordination: dependency-aware parallel execution
- Issue relations: blocking, related, duplicate relationships
- External resource linking: PR URLs, environment URLs

**Relevance to Unimatrix**: While Unimatrix may not use Linear, the session management abstraction -- with its lifecycle methods, activity streaming, and plan tracking -- represents a reusable pattern for any issue/project management integration.

---

## Relevance to Multi-Project Development

### Directly Applicable Patterns

1. **Provider abstraction** enables heterogeneous agent configurations per project and phase. Unimatrix could extend this to support project-specific provider selections.

2. **Agent definitions as templates** provide a proven model for encoding phase-specific behavior (architecture review, coding, testing, CI/CD) in composable markdown documents.

3. **State persistence and crash recovery** via filesystem-based state in worktrees offers a robust pattern for long-running multi-project operations.

4. **Work queue with project routing** already includes `projectName` as a routing dimension -- this could be extended to full multi-project dispatch.

5. **Coordinator pattern** for dependency-aware parallel execution across sub-issues demonstrates how to decompose complex work into parallel agent tasks with dependency graphs.

### Gaps for Multi-Project Use

1. **Single-repository assumption**: AgentFactory's worktree model assumes a single git repository. Multi-project orchestration requires cross-repository awareness.

2. **Linear coupling**: Deep integration with Linear as the sole issue tracker limits portability. Unimatrix would need an issue tracker abstraction layer.

3. **No cross-project context sharing**: Agents working in different worktrees have no mechanism to share architectural decisions, API contracts, or configuration across projects.

4. **No project dependency graph**: While sub-issues have dependency support, there is no concept of inter-project dependencies (e.g., shared library changes affecting downstream consumers).

5. **No multi-tenancy**: The current model assumes a single team/organization. Multi-project platforms often need tenant isolation.

---

## Strengths and Weaknesses

### Strengths

- **Clean provider abstraction**: The `AgentProvider`/`AgentHandle`/`AgentEvent` interface trio is well-designed and genuinely provider-agnostic. Adding new agent providers requires implementing only two methods.

- **Robust crash recovery**: The heartbeat + state persistence + recovery prompt system is production-quality. The cap on recovery attempts and worktree preservation on PR failure show operational maturity.

- **Assembly-line metaphor**: The factory metaphor creates clear mental models for work stages, making the system intuitive for teams to adopt and extend.

- **Safety-first agent management**: Multiple safety guards prevent destructive operations: agents cannot delete worktrees, force-push is blocked in the permission handler, and the orchestrator exclusively manages teardown.

- **Well-structured monorepo**: Clean separation between core, server, linear, CLI, and Next.js packages with independent versioning.

- **Active development**: Daily commits with meaningful improvements (safety guards, performance optimization, CLI tooling) indicate active maintenance.

- **Operational visibility**: Dashboard with fleet overview, pipeline kanban, and per-session cost tracking provides production-grade monitoring.

### Weaknesses

- **Early-stage maturity**: With 29 stars, 71 commits, and a creation date of February 9, 2026 (10 days old at analysis time), this is a very new project. API stability is not guaranteed.

- **No community adoption signal**: Zero open issues, no external contributors visible, and npm download data not publicly available suggest minimal external usage.

- **Tight Linear coupling**: The linear package is deeply intertwined with the core orchestrator. Replacing Linear would require significant refactoring.

- **Single-repo limitation**: The git worktree approach inherently assumes a monorepo or single-repo workflow. Multi-repo support would require architectural changes.

- **Limited testing visibility**: While `pnpm test` is documented, the test suite scope and coverage are not visible from the public repository.

- **Documentation depth**: While the README is comprehensive, per-package API documentation appears limited. The `docs/configuration.md` is the only substantive documentation file beyond the README.

- **No plugin/extension system**: Agent definitions are static markdown files. There is no plugin architecture for extending the pipeline with custom stages or hooks.

---

## Recommendations for Unimatrix

### Adopt These Patterns

1. **Provider abstraction layer**: Implement a similar `AgentProvider` / `AgentHandle` / `AgentEvent` interface hierarchy in Unimatrix. This is the most immediately reusable pattern -- it cleanly decouples orchestration logic from agent-specific SDKs and allows per-project/per-phase provider selection.

2. **Agent definition templates**: Use markdown-based agent definitions with YAML frontmatter for configuring phase-specific agent behavior. Extend the concept to include multi-project context sections (shared architecture decisions, cross-project API contracts).

3. **Structured result markers**: Adopt the `<!-- WORK_RESULT:passed/failed -->` convention (or a more structured equivalent like JSON output schemas) for agents to communicate outcomes to the orchestrator in a machine-parseable way.

4. **Crash recovery with state persistence**: Implement filesystem-based state tracking in work directories with heartbeat monitoring and recovery prompts. The pattern of persisting enough state to reconstruct context on resume is essential for long-running multi-project operations.

5. **Orchestrator-managed teardown**: Enforce the principle that agents never manage their own lifecycle boundaries. The orchestrator should exclusively handle creation and destruction of agent workspaces.

### Extend These Patterns for Multi-Project

1. **Cross-repository worktree management**: Replace AgentFactory's single-repo worktree model with a multi-repo workspace manager that can clone, branch, and isolate workspaces across multiple repositories.

2. **Issue tracker abstraction**: Where AgentFactory is tightly coupled to Linear, Unimatrix should define an abstract issue/task interface supporting multiple backends (Linear, GitHub Issues, Jira, internal systems).

3. **Inter-project dependency graph**: Extend the sub-issue dependency model to operate across project boundaries. An agent working on a shared library should be aware of downstream consumers that may be affected.

4. **Project-scoped configuration**: Extend the per-work-type configuration override pattern to include per-project overrides: different providers, timeouts, concurrency limits, and agent definitions per managed project.

5. **Cross-project context bus**: Implement a mechanism for agents across different projects to share architectural decisions, API contracts, and configuration changes -- something AgentFactory's isolated worktree model does not support.

### Avoid These Approaches

1. **Do not adopt the Linear dependency**: Unimatrix's multi-project scope requires issue tracker abstraction from day one. Building on AgentFactory's Linear-specific session management would create migration debt.

2. **Do not use AgentFactory as a runtime dependency**: Given its 10-day age and lack of community validation, it is too risky to depend on as a library. Extract patterns and reimplement them within Unimatrix's architecture.

3. **Do not replicate the single-stage coordinator model**: AgentFactory's coordinator handles sub-issues within a single parent context. Unimatrix needs a higher-order coordinator that operates across projects, managing inter-project dependencies rather than just intra-issue parallelism.

---

## Appendix: Key Source Files Reference

| File | Purpose |
|------|---------|
| `packages/core/src/orchestrator/orchestrator.ts` | Main orchestration engine: agent spawning, monitoring, completion handling |
| `packages/core/src/providers/types.ts` | Provider abstraction interfaces: `AgentProvider`, `AgentHandle`, `AgentEvent` |
| `packages/core/src/orchestrator/types.ts` | Orchestrator types: `OrchestratorConfig`, `AgentProcess`, lifecycle events |
| `packages/core/src/orchestrator/state-recovery.ts` | Crash recovery: heartbeat checking, state persistence, recovery prompts |
| `packages/core/src/providers/claude-provider.ts` | Claude Code SDK integration with permission handling |
| `packages/linear/src/agent-session.ts` | Linear session lifecycle: start, complete, fail, activity streaming |
| `packages/server/src/work-queue.ts` | Redis-backed priority queue with atomic claims |
| `examples/agent-definitions/developer.md` | Development agent role template |
| `examples/agent-definitions/qa-reviewer.md` | QA agent role template |
| `examples/agent-definitions/coordinator.md` | Sub-issue coordination agent template |
| `examples/agent-definitions/acceptance-handler.md` | Acceptance and PR merge agent template |
| `examples/agent-definitions/backlog-writer.md` | Planning-to-issues agent template |
