# D5b: Context Injection Deep Dive — Multi-Agent, Multi-Phase Reality

**Addendum to**: D5 Context Injection Playbook
**Date**: 2026-02-20
**Status**: Complete
**Triggered by**: The playbook treated Unimatrix as a flat "Claude searches memory" model. The real problem is fundamentally different.

---

## The Wrong Model vs. The Right Model

### Wrong Model (what D5 assumed)

```
Claude ──search──> Unimatrix ──results──> Claude uses them
```

One agent. One query pattern. Static CLAUDE.md tells it when to search. This treats the LLM as deterministic — always the same actor, always asking the same kind of question.

### Right Model (what Unimatrix actually serves)

```
                    ┌─ ndp-scrum-master (coordinating planning wave 2)
                    │    "I need the Wave 1 outputs and the protocol for Wave 2 spawning"
                    │
                    ├─ ndp-architect (designing auth system)
                    │    "I need existing ADRs, integration surfaces, and technology constraints"
                    │
                    ├─ ndp-rust-dev (implementing auth middleware)
Unimatrix ◄─────── ├─ "I need the trait signature, error handling convention, and test pattern"
                    │
                    ├─ ndp-tester (writing auth tests)
                    │    "I need the test strategy, mock patterns, and acceptance criteria"
                    │
                    ├─ ndp-validator (checking implementation)
                    │    "I need the validation protocol, trust thresholds, and prior reports"
                    │
                    └─ ndp-synthesizer (compiling brief)
                         "I need all SPARC artifacts, ADR IDs, and vision variances"
```

17 agent types. Each with different context needs. Each at different workflow positions. Each needing different things from Unimatrix at different moments.

**Unimatrix is not a memory search tool. It's a context-aware oracle that serves the right information to the right agent at the right moment in their workflow.**

---

## The Three Dimensions of Context Injection

Every Unimatrix query has three dimensions:

### 1. WHO — Agent Role Identity

| Agent | What They Need from Unimatrix |
|-------|------------------------------|
| `ndp-scrum-master` | Protocols, wave state, agent registry, shared context, GH Issue status |
| `ndp-architect` | Existing ADRs, integration surfaces, technology decisions, pattern conflicts |
| `ndp-specification` | Domain knowledge, prior specs, feature boundaries, user requirements |
| `ndp-rust-dev` | Trait signatures, error patterns, naming conventions, code patterns |
| `ndp-tester` | Test strategy, mock patterns, acceptance criteria, coverage expectations |
| `ndp-pseudocode` | Architecture decisions, component boundaries, data flow, API shapes |
| `ndp-validator` | Validation protocols, trust thresholds, prior reports, known issues |
| `ndp-synthesizer` | All SPARC artifacts, ADR IDs, vision variances, component map |
| `ndp-parquet-dev` | Bronze layer patterns, WAL conventions, storage schemas |
| `ndp-timescale-dev` | Hypertable patterns, continuous aggregate conventions, ETL patterns |
| Domain scientists | Domain-specific knowledge, standards (EPA, NWS), calibration data |

The same `memory_search("error handling")` should return **different results** depending on who's asking:
- **Architect**: ADR about error handling strategy, tradeoffs considered
- **Rust developer**: `CoreError` enum pattern, `map_err` convention, trait signatures
- **Tester**: Error test patterns, expected error types, mock error generators

### 2. WHERE — Workflow Position

The current system has well-defined workflow stages:

```
Planning Swarm:
  Wave 1: Specification + Architecture (parallel)
  Wave 2: Pseudocode + Test Plans (parallel, needs Wave 1)
  Wave 3: Vision Alignment → Synthesizer → Validator (sequential)

Implementation Swarm:
  Pre-spawn: Brief read, cargo build check
  Wave N: Agent spawning → execution → drift check → validation → GH update
  Post-wave: Learning gate, reflexion, pattern save
```

An agent's context needs change dramatically based on where they are:

| Position | Context Needed |
|----------|----------------|
| Planning Wave 1 start | SCOPE.md contents, existing patterns, prior decisions |
| Planning Wave 2 start | Wave 1 outputs (spec + architecture), existing patterns |
| Implementation pre-spawn | Brief, cargo build status, shared context |
| Implementation mid-task | Component-specific pseudocode, test plan, relevant ADRs |
| Drift check | Brief constraints, acceptance criteria, scope boundaries |
| Validation | All artifacts, trust thresholds, prior validation reports |
| Post-wave learning | What worked, what failed, pattern effectiveness |

### 3. WHAT — The Specific Task at Hand

Beyond role and position, the specific task changes what's relevant:
- "Implementing JWT auth middleware" → needs auth ADRs, Tower middleware pattern, test strategy
- "Implementing database migrations" → needs schema conventions, redb patterns, test fixtures
- "Fixing a concurrency bug" → needs async patterns, tokio conventions, prior debugging notes

---

## How This Changes the Tool Interface

### The Query Must Carry Context

The original `memory_search(query: string, k: int)` is insufficient. A context-aware query looks like:

```
context_search(
  query: "error handling convention",
  role: "rust-dev",                    // WHO
  phase: "implementation",             // WHERE (broad)
  workflow_step: "wave-1-execution",   // WHERE (specific)
  feature: "nxs-001",                  // WHAT (project scope)
  task: "implementing trait error propagation"  // WHAT (specific)
)
```

Unimatrix uses these dimensions to:
1. **Filter**: Only return entries relevant to this role + phase
2. **Rank**: Prioritize role-specific patterns over generic ones
3. **Augment**: Include relevant protocols, not just stored memories
4. **Guide**: Tailor the guidance footer to the agent's situation

### The Response Must Be Role-Appropriate

Same query, different responses:

**For `ndp-architect` asking about error handling:**
```
## Context for Architecture Decision: Error Handling

### Existing ADRs
- ADR-012: Use anyhow for application errors, thiserror for library errors
  Status: Active | Confidence: 0.95
  > Decided 2026-01-15. No conflicts reported.

### Integration Surfaces Affected
- CoreError enum in core/src/error.rs — any new variants must follow existing pattern
- All trait implementations return Result<T, CoreError>

### Technology Constraints
- async error propagation requires Send + Sync bounds on error types
- Tower middleware expects specific error types for HTTP responses

### Prior Conflicts
None recorded for error handling patterns.

---
Review these existing decisions before creating new ADRs.
If your design changes the error handling strategy, deprecate ADR-012 first.
```

**For `ndp-rust-dev` asking about error handling:**
```
## Error Handling Conventions

### Pattern: CoreError Propagation
```rust
use crate::error::CoreError;

pub async fn fetch_data(&self) -> Result<Data, CoreError> {
    let response = self.client.get(url)
        .await
        .map_err(|e| CoreError::Network(format!("fetch failed: {e}")))?;
    // ...
}
```

### Rules
1. Never use `.unwrap()` in production code
2. Use `.map_err()` to add context at every boundary
3. Use `tracing::error!()` with structured fields before propagating
4. New error variants go in `CoreError` enum, not separate error types

### Recent Correction (2026-02-18)
Previous pattern recommended `panic!` for unrecoverable errors.
Superseded: always propagate, let the caller decide recovery.

---
Apply these patterns. If you encounter a case not covered, flag it — don't invent a new error handling approach.
```

**For `ndp-tester` asking about error handling:**
```
## Error Testing Patterns

### What to Test
- Every public function that returns Result must have error path tests
- Test with invalid inputs, network failures, and timeout conditions
- Use CoreError variant matching in assertions, not string matching

### Mock Pattern
```rust
#[test]
async fn test_fetch_data_network_error() {
    let mock_client = MockClient::new()
        .expect_get()
        .returning(|_| Err(reqwest::Error::new(...)));

    let result = service.fetch_data().await;
    assert!(matches!(result, Err(CoreError::Network(_))));
}
```

### Acceptance Criteria
- Each error variant in CoreError has at least one test exercising it
- Integration tests cover error propagation across module boundaries

---
Follow this pattern for all error path tests in your component.
```

---

## The Config Simplification Problem

### Current State: Config Explosion

Right now, context injection is scattered across:

```
CLAUDE.md                              — 1 file, global rules
.claude/rules/rust-workspace.md        — 1 file, conditional rules
.claude/agents/ndp/*.md                — 17 files, role definitions
.claude/protocols/*.md                 — 4 files, workflow definitions
product/features/*/                    — N files, feature artifacts
```

Each agent definition file contains:
- Role identity and scope
- Design principles
- Naming conventions
- Code quality checklists
- Pattern workflow (get-pattern → work → reflexion)
- Swarm participation rules
- Self-check procedures

**The problem**: This is ~5,000+ words of instructions spread across 20+ files. Every agent needs to read its definition, the relevant protocol, CLAUDE.md, and feature artifacts. Context budget is consumed by instructions before the agent even starts working.

### The Unimatrix Solution: Dynamic Context Assembly

Instead of baking everything into static files, Unimatrix **assembles context dynamically** based on who's asking and where they are:

```
Current (static):
  Agent reads 17KB of agent definition
  + 8KB of protocol
  + 2KB of CLAUDE.md
  + NK of feature artifacts
  = 27KB+ of context before any work begins

Unimatrix (dynamic):
  Agent queries: "I'm a rust-dev, implementation wave 1, working on nxs-001 auth middleware"
  Unimatrix returns: ~2KB of precisely relevant context
    - The 3 patterns that apply to this specific task
    - The 2 conventions that constrain this work
    - The 1 ADR that governs this design area
    - The component-specific pseudocode excerpt
    - The acceptance criteria for this component
```

### The Minimal Agent Definition

With Unimatrix, an agent definition shrinks from ~150 lines to ~20:

```markdown
---
name: ndp-rust-dev
type: developer
---

# Unimatrix Rust Developer

You are a Rust developer for Unimatrix.

## Before Starting Work

Query Unimatrix for your context:
- Your role: rust-dev
- Your current phase and task (from your spawn prompt)
- Apply all returned conventions and patterns

## After Completing Work

Report what patterns helped, what didn't, and any new patterns discovered.

## Swarm Participation

When spawned with `Your agent ID:`, report status through coordination layer.
```

Everything else — design principles, naming conventions, error handling patterns, code quality rules, self-check procedures — comes from Unimatrix dynamically. If a convention changes, it changes in one place (Unimatrix's memory), not across 17 agent files.

---

## The Tool Design Implications

### Tool 1: `context_search` (replaces `memory_search`)

Not just keyword search. Contextual query with role and workflow awareness.

```
context_search(
  query: string,              // What they're looking for
  role: string?,              // Agent role (rust-dev, architect, scrum-master...)
  phase: string?,             // Planning, implementation, validation...
  workflow_step: string?,     // Wave 1, drift check, post-completion...
  feature: string?,           // Feature ID for scoping
  include: [string]?,         // Explicit content types: protocols, patterns, conventions, adrs
  max_tokens: int?            // Response budget
)
```

Unimatrix server-side logic:
1. Semantic search over stored knowledge (vector similarity)
2. Filter by role-appropriate content types
3. Filter by phase-appropriate entries
4. Inject relevant protocols for the workflow step
5. Assemble role-appropriate response format
6. Add contextual guidance footer

### Tool 2: `context_briefing` (new — role+situation context assembly)

A higher-level tool for "give me everything I need to start this work":

```
context_briefing(
  role: string,               // Who am I
  task: string,               // What am I doing
  feature: string?,           // Feature ID
  phase: string?,             // Where in the workflow
)
```

Returns a compiled briefing:
- Relevant protocols for this role at this phase
- Active conventions that apply
- Relevant ADRs and decisions
- Component-specific context from feature artifacts
- Known issues and corrections in this area
- Guidance for the specific task

This is the "I just spawned, orient me" tool. Instead of the agent reading 5 files to understand its context, it makes one call and gets a compiled briefing.

### Tool 3: `context_store` (replaces `memory_store`)

Stores with full dimensional metadata:

```
context_store(
  content: string,
  category: string,           // convention, pattern, decision, protocol, correction
  roles: [string],            // Which roles this is relevant to
  phases: [string],           // Which phases this applies to
  tags: [string],
  supersedes: string?,        // ID of entry this corrects/replaces
)
```

### Tool 4: `workflow_state` (new — swarm coordination)

```
workflow_state(
  feature: string,
  action: "get" | "update",
  wave: int?,
  agent_id: string?,
  status: string?,            // For updates: started, progress, complete, blocked
  data: object?,              // Arbitrary state data
)
```

This replaces the current pattern of agents writing to shared memory files. Unimatrix becomes the coordination backbone.

---

## How Agents Actually Use Unimatrix

### Scrum Master at Planning Wave 2 Start

```
Agent: ndp-scrum-master
Query: context_briefing(
  role: "scrum-master",
  task: "spawning wave 2 agents for planning",
  feature: "nxs-001",
  phase: "planning-wave-2"
)

Unimatrix returns:
───────────────────────────────────
## Planning Wave 2 Briefing

### Protocol
Wave 2 agents (pseudocode + test plan) spawn in parallel, ONE message.
Each agent prompt MUST include:
  1. Agent ID: {feature}-agent-N-{role}
  2. Paths to Wave 1 outputs
  3. Instruction to read artifacts before producing output

### Wave 1 Outputs (completed)
- specification/SPECIFICATION.md — produced by ndp-specification (agent-1)
- architecture/ARCHITECTURE.md — produced by ndp-architect (agent-2)
  ADR IDs: ADR-101, ADR-102, ADR-103
  Integration surfaces: 2 identified

### Wave 2 Agents to Spawn
- ndp-pseudocode: produces pseudocode/OVERVIEW.md + per-component files
- ndp-tester: produces test-plan/OVERVIEW.md + per-component files

### Context to Include in Agent Prompts
- SCOPE.md path
- Wave 1 artifact paths (spec + architecture)
- Component list from SPECIFICATION.md: [auth-middleware, user-store, token-service]
───────────────────────────────────
```

The scrum master doesn't need to re-read the planning protocol. Unimatrix compiles exactly what it needs for THIS step.

### Rust Developer Starting Implementation

```
Agent: ndp-rust-dev
Query: context_briefing(
  role: "rust-dev",
  task: "implementing auth middleware using Tower",
  feature: "nxs-001",
  phase: "implementation"
)

Unimatrix returns:
───────────────────────────────────
## Implementation Context: Auth Middleware

### Relevant Conventions
1. **Tower Middleware Pattern**: Implement as a Layer + Service pair.
   Extract from request extensions, not headers directly.
2. **Error Handling**: Use CoreError::Auth variant. Propagate with map_err.
3. **Async Pattern**: Use tower::Service with poll_ready + call.
4. **Testing**: Mock the inner service. Test both valid and expired tokens.

### Architecture Decisions (from ADR-101, ADR-102)
- JWT with RS256, not HS256
- Token validation via jsonwebtoken crate
- Refresh tokens stored in redb, not in-memory

### Component Pseudocode (excerpt)
```
AuthMiddleware::call(request):
  extract token from Authorization header
  validate token (check signature, expiry, claims)
  if valid: inject User into request extensions, call inner service
  if invalid: return 401 with CoreError::Auth
```

### Acceptance Criteria (from AC-003)
- AC-003a: Valid JWT passes through middleware
- AC-003b: Expired JWT returns 401
- AC-003c: Missing token returns 401
- AC-003d: Malformed token returns 400

### Known Issues
None in auth area. Recent correction in error handling (2026-02-18):
use propagation, not panic.

---
Implement following the patterns above. Your component touches:
  core/src/middleware/auth.rs (create)
  core/src/error.rs (add Auth variant)
  core/tests/middleware_auth.rs (create)
───────────────────────────────────
```

2KB of precisely relevant context. No reading 17KB of agent definition. No reading the full protocol. No reading every pseudocode file.

---

## The Architectural Insight

### What This Means for the MCP Interface

The MCP tools aren't just CRUD operations on a memory store. They're a **context assembly engine**. The key server-side capabilities:

1. **Role-aware filtering**: Tag entries with applicable roles. Filter at query time.
2. **Phase-aware ordering**: Entries have phase relevance. A convention about test patterns is more relevant during testing than during architecture design.
3. **Workflow-state integration**: Know what wave, what step, what's completed. Use this to assemble context that includes completed outputs.
4. **Protocol compilation**: Store protocols as entries. Compile relevant protocol excerpts into briefings based on role + step.
5. **Correction chaining**: When a pattern is corrected, the correction appears in context instead of the superseded version.
6. **Cross-agent context routing**: What one agent stores (architect stores ADRs) is retrievable by others (rust-dev gets the ADR that constrains their work), filtered by role relevance.

### What This Means for Config Simplification

The config surface shrinks to:

```
1. claude mcp add unimatrix -- unimatrix-server     (connect the server)
2. unimatrix init                                     (set up the project)
3. One CLAUDE.md line: "Query Unimatrix for context before starting work."
```

Everything else — all 17 agent definitions, all 4 protocols, all conventions, all patterns — lives IN Unimatrix and is served dynamically. Agent definition files become thin shells that just say "I'm a {role}, ask Unimatrix for my instructions."

### What This Means for Track 2C

Track 2C (Claude Config Audit) needs to validate:
1. Can agent definitions be this thin and still work? Does Claude follow role instructions from a tool response as reliably as from an agent definition file?
2. Can the server `instructions` field replace the CLAUDE.md line? ("Always query Unimatrix for context before starting work")
3. Do subagents (spawned via Task tool) inherit MCP server connections from the parent? If not, how does each subagent connect to Unimatrix?
4. Is there a mechanism for the spawn prompt to trigger a Unimatrix query automatically? (Hook on Task tool? Server-side on initialize?)

**Question 3 is critical.** If subagents don't inherit MCP connections, each of the 17 agent types would need its own MCP setup. That breaks the simplification story entirely.

### What This Means for Track 3

The interface specification (D7) needs tools designed around the three dimensions:

| Tool | Purpose | Replaces |
|------|---------|----------|
| `context_search` | Role+phase-aware semantic search | `memory_search` (too flat) |
| `context_briefing` | Compiled orientation for an agent starting work | Agent reading 5 files |
| `context_store` | Store with dimensional metadata | `memory_store` (needs role/phase tags) |
| `workflow_state` | Swarm coordination backbone | File-based shared state |
| `protocol_get` | Retrieve protocol for role+step | Agent reading protocol files |
| `pattern_search` | Search stored patterns (conventions, ADRs) | `/get-pattern` skill |
| `pattern_report` | Report pattern effectiveness (reflexion) | `/reflexion` skill |

---

## Open Questions for Track 2C

1. **Subagent MCP inheritance**: Do Task-spawned subagents get the parent's MCP connections? This is the single most important question for the config simplification story.

2. **Agent definition minimization**: How thin can an agent definition be while still reliably producing role-appropriate behavior? Is "I'm a rust-dev, ask Unimatrix" enough, or does Claude need the full definition to anchor its behavior?

3. **Tool response as role instruction**: Can a `context_briefing` response effectively replace the content of an agent definition file? Claude treats tool results as factual user-role input — is that sufficient authority to drive role-specific behavior, or does it need system-prompt level authority (which only agent definitions provide)?

4. **Automatic briefing on spawn**: Is there a mechanism to automatically query Unimatrix when an agent spawns, without the spawn prompt explicitly telling it to? (MCP init? Hook? Agent definition one-liner?)

5. **Context budget**: A `context_briefing` response for a complex task might be 2-3KB. That's much less than the current 17KB+ of static agent definition, but it still consumes context. Is there an optimal briefing size that provides enough guidance without consuming too much of the agent's working context?

---

## The Unimatrix Value Proposition (Revised)

**Before Unimatrix**: Context is baked into static files. Every agent reads the same 150-line definition. Protocols are read in full even when only one step is relevant. Conventions are scattered. Changes require editing multiple files. No agent has awareness of what other agents know or have done.

**After Unimatrix**: Context is assembled dynamically. Each agent gets precisely the context it needs for its role, its workflow position, and its specific task. Conventions live in one place. Protocol steps are compiled into actionable briefings. What the architect decided becomes what the developer implements becomes what the tester validates — through Unimatrix's cross-agent context routing, not through file copying.

**The question is not "how does Claude search memory."**
**The question is "how does every agent, at every juncture, get exactly the context it needs."**

Unimatrix is the answer to that question.
