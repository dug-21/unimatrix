# Pre-Roadmap Spike: The Plan for the Plan

**Date**: 2026-02-20
**Status**: Active
**Follows**: Research Synthesis R1, Feature Roadmap v0, Vector Storage Decision
**Produces**: Information required to write the definitive implementation roadmap

---

## Context

Six research reports and a 40,000-word synthesis established Unimatrix's architecture, technology stack, and competitive positioning. A feature roadmap (v0.1-v0.5) was drafted based on a "build from scratch with direct library dependencies" decision.

That roadmap was challenged on product management grounds: **the MCP tool interface is the load-bearing contract.** Every layer above the backend — orchestration, context compilation, learning, Claude Code integration — depends on it. Designing that interface incrementally across five versions risks retrofitting, and retrofitting cascades.

The counter-argument: understanding the backend deeply (what operations are natural, what the constraints are) and the frontend deeply (how Claude Code actually consumes MCP context, what behaviors can be driven) produces the design confidence needed to specify the interface once and correctly.

This document defines the research and prototyping necessary to write that specification, and therefore the real roadmap.

---

## What We Know (Decisions Made)

These decisions are settled and not revisited by this spike:

| Decision | Choice | Source |
|----------|--------|--------|
| Language | Rust + Tokio | Research Synthesis D1 |
| Vector storage | hnsw_rs + redb (direct library deps) | Vector Storage Decision |
| Embeddings (initial) | OpenAI text-embedding-3-small | Feature Roadmap v0.1 |
| Embeddings (target) | Local via ort + all-MiniLM-L6-v2 | Feature Roadmap v0.2 |
| Integration protocol | MCP (stdio transport) | Research Synthesis D4 |
| HTTP framework | Axum | Research Synthesis |
| Serialization | serde + serde_json + bincode (vectors) + toml (config) | Research Synthesis |
| Deployment | Single binary, local-first, Docker packaging | Feature Roadmap |
| Not vendoring ruvector | Operational/maintenance risk too high | Vector Storage Decision, ruvector analysis |
| Not using Qdrant | Over-engineered for single-user; VectorStore trait preserves migration path | Vector Storage Decision |

## What We Don't Know (This Spike Answers)

### Track 1: Backend Capabilities

**Goal**: Understand the natural operation shapes of hnsw_rs + redb at the level of detail needed to design the MCP tool interface confidently.

**Not building production code.** Building throwaway test harnesses that answer specific questions.

#### 1A. hnsw_rs Capability Spike

| Question | Why It Matters for Interface Design |
|----------|-------------------------------------|
| Does `FilterT` support pre-filtering during search (not post-filter)? | Determines whether `memory_search` takes inline filter params or needs a separate filtered-search flow |
| What does `search()` return? (IDs + distances? Ranked? Configurable k?) | Shapes the `memory_search` response schema |
| Does `insert_parallel()` work reliably for batch operations? | Determines whether `memory_import` can be fast or needs sequential insertion |
| What does `file_dump()` / reload look like? Format? Speed? Atomic? | Determines persistence model and whether we need redb for index state or just metadata |
| Can the index handle mixed dimensionality or is it fixed at creation? | Determines whether switching embedding models (OpenAI 1536d -> local 384d) requires index rebuild |
| What's the actual memory profile at 1K, 10K, 100K entries? | Informs per-project resource limits and whether we need quantization planning |
| How does the `DistCosine` vs `DistL2` choice affect retrieval quality for text embeddings? | Determines whether distance metric should be configurable per-project |

**Deliverable**: Capability matrix documenting each operation, its parameters, return types, and constraints. Written as a reference for interface design.

**Estimated effort**: 1-2 days.

#### 1B. redb Storage Patterns

| Question | Why It Matters for Interface Design |
|----------|-------------------------------------|
| Can redb do range queries on timestamps efficiently? | Determines whether `memory_list` supports "entries since X" natively or needs secondary indexing |
| Multiple named tables in one DB vs. separate DB files per table? | Determines physical storage layout for metadata, vectors, lifecycle state |
| What's the transaction model? Single writer + multiple readers? | Determines whether concurrent search + insert is safe, which affects tool behavior under load |
| How do typed tables work? Can we store structured metadata (not just blobs)? | Determines metadata schema richness — flat key-value or structured types |
| What's the practical size limit before performance degrades? | Informs when "project too large" warnings should trigger |

**Deliverable**: Storage pattern guide documenting table layout, transaction patterns, and query capabilities. Written as a reference for persistence layer design.

**Estimated effort**: 1 day.

#### 1C. Learning Model Assessment

Read sona's ReasoningBank, LoRA, and EWC++ code (not to vendor, but to understand).

| Question | Why It Matters for Interface Design |
|----------|-------------------------------------|
| What does ReasoningBank's `store_pattern` / `find_patterns` API look like? | Informs whether our learning tools should mirror this shape or use a simpler model |
| Does K-means++ clustering produce useful groupings for development knowledge? | Determines whether `memory_search` should return cluster/category information |
| What does the trajectory model (begin/step/end/reward) actually track? | Informs whether we need explicit trajectory tools or implicit session tracking |
| Is confidence scoring meaningful for code knowledge? (decay rates, promotion thresholds) | Determines whether confidence is a first-class field in search results |
| Would a simpler metadata state machine (active/aging/deprecated + correction links) cover 90% of the value? | Key decision: sona-style ML learning vs. lifecycle metadata learning |

**Deliverable**: Learning model comparison — sona's approach vs. metadata lifecycle approach. Recommendation for which model to design the interface around.

**Estimated effort**: 1 day (code reading, not implementation).

---

### Track 2: Frontend / Client Behavior Research

**Goal**: Understand exactly how Claude Code discovers, invokes, and uses MCP tool responses — and what patterns successfully drive Claude to use tools proactively (not just when explicitly asked).

This is the least-researched area and arguably the highest-risk. A perfectly designed backend is worthless if Claude doesn't use it effectively.

#### 2A. MCP Protocol Deep Dive

| Question | Why It Matters |
|----------|----------------|
| What's the exact JSON-RPC lifecycle? (initialize → tools/list → tools/call → ...) | Must implement correctly; shapes server architecture |
| How does Claude Code render tool responses in its context? | Determines optimal response format (markdown? structured JSON? both?) |
| What's the token cost of tool descriptions in the system prompt? | Every tool description competes for context budget; informs how many tools we can afford |
| Does Claude Code support MCP resources (passive context) or only tools (active invocation)? | Resources could enable passive context injection without tool calls |
| Can tool descriptions include usage hints that influence when Claude invokes them? | Critical for driving proactive behavior |
| How does Claude Code handle tool errors? Retry? Surface to user? | Determines error response design |
| Is there a Rust MCP SDK we should use, or do we implement the protocol directly? | Build vs. integrate for the transport layer |

**Method**: Read MCP specification. Read Claude Code MCP documentation. Build a minimal hello-world MCP server in Rust that Claude Code can connect to. Observe behavior.

**Deliverable**: MCP integration guide documenting the protocol flow, response format best practices, and tool description patterns.

**Estimated effort**: 2 days (includes building a minimal test server).

#### 2B. Context Injection Patterns

| Question | Why It Matters |
|----------|----------------|
| When Claude calls `memory_search`, how does the response content influence subsequent generation? | Determines how to format search results for maximum utilization |
| Does response length affect utilization? (Do long responses get ignored?) | Determines whether we should return full entries or summaries |
| How do multiple tool calls in sequence interact? (search → get → use) | Determines whether we need compound tools or can rely on Claude chaining calls |
| What response format does Claude utilize best? (plain text? markdown? JSON with explanation?) | Directly determines response schema |
| How does MCP tool context interact with CLAUDE.md instructions? | Determines whether memory results reinforce or conflict with static instructions |
| Can we use tool responses to inject "system-like" instructions? (e.g., "Based on project conventions, you should...") | Determines whether memory can actively guide behavior, not just provide information |

**Method**: Build test MCP server with various response formats. Run real tasks with Claude Code connected. Observe which formats Claude utilizes most effectively.

**Deliverable**: Context injection playbook — what response formats work, what gets ignored, optimal result count and length.

**Estimated effort**: 2-3 days (requires iterative testing with Claude Code).

#### 2C. Claude Configuration Surface & Behavioral Driving

This is the core product question — but it's deeper than "can we drive Claude's behavior?" It's: **can we make it easy for a user to configure Claude agents to reliably talk to Unimatrix, without becoming an expert in Claude's config hierarchy?**

##### The Problem

Claude Code has multiple context injection mechanisms, each with different scoping rules, load conditions, and interaction behaviors:

| Mechanism | Scope | When Loaded | Format |
|-----------|-------|-------------|--------|
| `CLAUDE.md` (project root) | All sessions in project | Always | Markdown, free-form |
| `CLAUDE.md` (subdirectories) | Sessions touching that subtree | Conditionally | Markdown, free-form |
| `.claude/agents/*.md` | Agent-specific (Task tool subagents) | When agent spawned | Markdown with YAML frontmatter |
| `.claude/rules/*.md` | Contextual (glob-matched to files) | When matching files are in context | Markdown |
| Hooks (`settings.json`) | Pre/post tool execution | On specific tool calls | Shell commands |
| Commands (`.claude/commands/`) | User-invoked shortcuts | On explicit invocation | Prompt templates |
| Skills | Loaded via Skill tool | On matching request | Expanded prompts |
| MCP tool descriptions | Always visible to Claude | On tools/list | JSON schema + description text |

These mechanisms relate to each other in complex, poorly-documented ways. Precedence rules are unclear. It's difficult for a user to predict, given a set of config files on disk, exactly what context Claude will see for a given task. Files are scattered across multiple directories with different naming conventions.

##### What Unimatrix Must Solve

A fundamental goal: **make it trivial to configure Claude agents to request context from Unimatrix at the right moments.** The user should not need to:

- Manually author complex CLAUDE.md instructions
- Understand .claude/rules/ glob matching to get phase-specific behavior
- Write hooks to trigger memory searches on tool calls
- Maintain multiple agent definitions with repeated memory instructions
- Debug why Claude sometimes searches memory and sometimes doesn't

Instead, Unimatrix should provide a minimal, reliable configuration surface — ideally generated or managed — that places the right instructions in the right locations across Claude's config hierarchy.

##### Research Questions

**Mapping the config hierarchy:**

| Question | Why It Matters |
|----------|----------------|
| What are the exact precedence rules when CLAUDE.md, rules, agent defs, and MCP tool descriptions provide conflicting guidance? | Must know which mechanism "wins" to place instructions strategically |
| Which mechanisms persist across conversation turns vs. are one-shot? | Determines whether memory-search instructions need reinforcement |
| Do .claude/rules/ actually fire reliably when matched? What are the glob semantics? | Determines whether rules are viable for phase-specific memory behavior |
| Can agent definitions (.claude/agents/) include MCP tool usage instructions that reliably drive behavior? | Determines whether agent defs are the right injection point for subagent workflows |
| How do hooks interact with MCP tool calls? Can a hook on `Read` trigger a memory search before the read? | Determines whether hooks can automate "check memory first" behavior |

**Driving proactive behavior:**

| Behavior We Want | Research Question |
|------------------|-------------------|
| Claude searches memory when starting any task | Which config mechanism most reliably drives "always search memory before starting work"? CLAUDE.md? Tool description? Agent def? |
| Claude stores key decisions after completing work | Can end-of-session behavior be driven by config, or does it require a hook? |
| Claude recognizes corrections and records them | Can we detect correction patterns ("no, do X instead") via instructions alone, or do we need hook-based detection? |
| Claude checks conventions before generating code | Is this best driven by CLAUDE.md global instruction, or by rules/ files scoped to source code? |
| Claude uses retrieved context accurately | How reliably does Claude follow patterns returned by memory search vs. hallucinating over them? |
| Subagents (via Task tool) inherit memory behavior | Do agent definitions in .claude/agents/ reliably carry memory instructions into subagent contexts? |

**Designing the configuration surface:**

| Question | Why It Matters |
|----------|----------------|
| What is the MINIMUM config a user needs to add for reliable Unimatrix integration? | Core UX question — "claude mcp add unimatrix" should be most of the setup |
| Can MCP tool descriptions alone drive sufficient behavior (no CLAUDE.md changes needed)? | If yes, setup is literally just adding the MCP server. If no, we need to generate config. |
| Should Unimatrix provide a `unimatrix init` command that generates the right CLAUDE.md entries, rules, and agent defs? | Determines whether config management is part of our product surface |
| Can we use a single CLAUDE.md append (5-10 lines) that covers 90% of cases? | The sweet spot: minimal config, reliable behavior |
| Which behaviors are impossible to drive via config alone and require Unimatrix-side logic? (e.g., should the MCP server proactively return context on initialize, not wait for tool calls?) | Determines what must be server-side vs. config-side |

##### Method

1. **Audit**: Document every Claude Code config mechanism, its exact loading behavior, scope, and interaction rules. Use Claude Code's own documentation and observed behavior.
2. **Map**: For each target behavior (search on task start, store on completion, etc.), identify which config mechanism(s) could drive it.
3. **Test**: Build candidate configurations (minimal CLAUDE.md additions, rules files, tool descriptions) and test each against real development tasks. Measure reliability.
4. **Minimize**: Find the smallest configuration surface that drives reliable behavior. Determine what Unimatrix can generate vs. what users must maintain.

##### Deliverables

- **Claude config mechanism audit** — complete map of every mechanism, its scope, precedence, and interaction rules
- **Behavioral driving playbook** — for each target behavior, the recommended config mechanism, exact text, and measured reliability
- **Unimatrix config surface design** — the minimal user-facing configuration (what goes where, what Unimatrix generates, what the user writes)

**Estimated effort**: 3-4 days (requires systematic testing across multiple config mechanisms).

---

### Track 3: Interface Specification

**Goal**: Using the outputs of Track 1 and Track 2, write the complete MCP tool specification — the contract that everything depends on.

This happens AFTER Tracks 1 and 2, not in parallel.

#### What the Specification Contains

For every tool:

```
Tool: memory_search
Ship Version: v0.1
Description: [exact text Claude Code sees — this IS the prompt engineering]

Parameters:
  query: string (required) — natural language search query
  k: integer (optional, default 10) — max results to return
  filter: object (optional) — structured filter
    phase: enum [architecture, coding, testing, deployment, debugging] (optional)
    status: enum [active, aging, deprecated] (optional, default: active)
    tags: string[] (optional) — require all specified tags
    since: datetime (optional) — entries modified after this time
  max_tokens: integer (optional) — stop returning results when cumulative tokens exceed budget

Response:
  results: array of:
    id: string
    content: string
    similarity: float (0-1)
    metadata: object
      tags: string[]
      category: string
      phase: string (nullable)
      status: string
      confidence: float
      created_at: datetime
      last_used_at: datetime
    correction: object (nullable, v0.4+)
      original_id: string
      what_was_wrong: string
      what_is_right: string
  total_found: integer
  tokens_used: integer

Notes:
  - Phase filter uses hnsw_rs FilterT for pre-filtering (not post-filter)
  - Token budget counts content field lengths
  - Results ordered by similarity descending
  - Deprecated entries excluded unless status filter explicitly includes them
```

Every tool gets this treatment. The specification includes:

- **v0.1 tools**: memory_store, memory_search, memory_get, memory_list, memory_delete
- **v0.2 tools**: project_create, project_list, project_switch, memory_count
- **v0.3 tools**: (no new tools — phase filter added to memory_search params)
- **v0.4 tools**: memory_correct, memory_promote, memory_deprecate
- **v0.5 tools**: context_compile, session_summary, status
- **Orchestration tools** (future): pipeline_status, gate_approve, gate_reject, agent_status

**Key principle**: v0.1 tools are designed knowing v0.5's parameters exist. Parameters added in later versions are present in the schema from day one (nullable/optional), even if the backend doesn't implement them yet. This prevents interface breaks.

**Deliverable**: Complete MCP tool specification document. This becomes the contract.

**Estimated effort**: 2-3 days (after Tracks 1 and 2 complete).

---

## Schedule

All three tracks can partially overlap. Track 3 depends on Track 1 and Track 2 outputs.

```
Week 1:
  Days 1-2:  Track 1A (hnsw_rs spike) + Track 2A (MCP protocol deep dive) [parallel]
  Day  3:    Track 1B (redb patterns) + Track 1C (sona assessment) [parallel]
  Days 4-5:  Track 2B (context injection testing) — needs test MCP server from 2A

Week 2:
  Days 1-3:  Track 2C (Claude config audit + behavioral driving) — needs 2A + 2B outputs
  Days 4-5:  Track 3 begins (interface specification) — uses all prior outputs

Week 3:
  Days 1-2:  Track 3 completion (interface specification)

Buffer: 2 days for unknowns, rabbit holes, or deeper investigation.
```

**Total: ~14 working days (3 weeks with buffer)**

Track 2C expanded from 2 days to 3 because the Claude config audit (mapping every mechanism, testing precedence and interaction rules) is systematic work that can't be rushed. Getting this right is what makes the difference between "users must understand Claude's config hierarchy" and "users run one command and it works."

---

## Deliverables Summary

| # | Deliverable | Produced By | Used By |
|---|-------------|-------------|---------|
| D1 | hnsw_rs capability matrix | Track 1A | Track 3 (interface spec) |
| D2 | redb storage pattern guide | Track 1B | Track 3 (interface spec) |
| D3 | Learning model comparison (sona vs. metadata lifecycle) | Track 1C | Track 3 (interface spec), roadmap |
| D4 | MCP integration guide | Track 2A | Track 2B, Track 2C, Track 3 |
| D5 | Context injection playbook (response formats, sizes, utilization) | Track 2B | Track 2C, Track 3 |
| D6a | Claude config mechanism audit (every mechanism, scope, precedence, interactions) | Track 2C | Track 3, roadmap, product design |
| D6b | Behavioral driving playbook (which config drives which behavior, measured reliability) | Track 2C | Track 3, roadmap, CLAUDE.md design |
| D6c | Unimatrix config surface design (minimal user config, what we generate, what users write) | Track 2C | Track 3, roadmap, UX design |
| D7 | **Complete MCP tool specification** | Track 3 | **The roadmap. Everything after this.** |

D7 is the critical output. It is the interface contract that the roadmap is built to deliver. Once D7 exists, the roadmap becomes: "in what order do we implement these tools and the backend behind them?" — a sequencing question, not a design question.

---

## Exit Criteria

This spike is complete when:

1. We can answer "what does `memory_search` look like, exactly — parameters, response, behavior?" with full confidence, not just for memory_search but for every tool through orchestration.
2. We know how Claude Code will use the tools — not theoretically, but from observed behavior during testing.
3. We know what the backend can and cannot do efficiently — not from documentation, but from running code against hnsw_rs and redb.
4. The learning model decision is made: sona-style ML vs. metadata lifecycle. Based on evidence, not preference.
5. We know exactly which Claude config mechanisms drive reliable Unimatrix usage, and the minimal config surface is designed — a user shouldn't need to understand the .claude/ directory hierarchy to get value from Unimatrix.
6. The interface spec (D7) exists, is reviewed, and is accepted as the contract.

After this spike, the roadmap writes itself. The versions become "which tools ship when" against a fixed interface, with backend implementation as an internal concern that never surfaces to the client contract. And critically, the roadmap includes the user-facing config story — what a user does to set up Unimatrix, not just what the server does internally.

---

## What This Spike Does NOT Produce

- Production code (all spike code is throwaway / test harnesses)
- The implementation roadmap (that's the next step, after D7)
- Orchestration design (that builds on top of D7)
- Final CLAUDE.md instructions (informed by D6, but not finalized until real usage)

---

## Risks to This Spike

| Risk | Impact | Mitigation |
|------|--------|------------|
| hnsw_rs has a showstopper limitation we don't discover until Track 1A | Delays Track 3; may force reconsideration of Qdrant | Day 1-2 timeline means fast feedback; VectorStore trait preserves migration path |
| Claude Code MCP behavior is less controllable than expected | Reduces value of proactive memory; may require different product model | Track 2C tests this directly; better to know now than after building |
| Rust MCP SDK doesn't exist or is immature | Must implement JSON-RPC stdio from scratch; adds ~2 days | Track 2A discovers this on day 1 |
| Interface spec becomes too large / too abstract | Spec that nobody reads is useless | Limit to tools through v0.5 + orchestration sketch. No further. |
| Spike extends past 2.5 weeks | Delays roadmap | Hard time-boxes per track. Document what you know, flag what you don't, move on. |
