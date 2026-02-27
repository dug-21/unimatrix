# ASS-011: Workflow State Machine & Hook-Driven Orchestration

**Type:** Research Spike (Design Investigation)
**Date:** 2026-02-25
**Predecessors:**
- `product/research/claude-int/ANALYSIS.md` — raw message interception analysis
- `product/research/ass-010/SCOPE.md` — observation pipeline (signal quality validation)
- `product/features/col-001/SCOPE.md` — outcome tracking (structured tags, OUTCOME_INDEX)

**Purpose:** Gather the technical facts, measured capabilities, and design constraints necessary to make formal recommendations about hook-driven orchestration, workflow state machines, agent identity, dynamic context delivery, and observation-based learning. This spike produces evidence, not architecture. Design decisions follow from findings.

**Outcome (2026-02-27):** Spike validated hook capabilities (Phase 1) and produced a revised architecture through iterative design review (Phase 2). Key conclusion: Unimatrix does NOT orchestrate — `.claude/` files own orchestration (unchanged), hooks passively observe, Unimatrix serves project context and connects the dots across sessions via `agent_id` + `feature_cycle`. Hook-based metadata injection (silent interface keys) was evaluated and deferred — the risks of hidden MCP parameters vs the incremental value did not justify implementation at this time. The existing `agent_id` parameter (already in MCP schema, agent-reported) combined with the `feature` parameter provides sufficient linkage for the flow without silent injection. See `findings/` for detailed results.

---

## Motivation

### The Complexity Problem

Configuring a Claude Code project for multi-agent work requires managing multiple instruction layers:

| Layer | Location | Loaded When | Typical Size |
|-------|----------|-------------|-------------|
| CLAUDE.md | Project root | Every invocation | 100-200 lines |
| Agent definitions | `.claude/agents/` | Per subagent type | 100-300 lines each |
| Protocols | `.claude/protocols/` | Referenced by agents | 200-400 lines each |
| Rules | `.claude/rules/` | Pattern-matched on files | 50-100 lines each |
| MCP server instructions | MCP init response | Every invocation | 50+ lines |
| Skills | `.claude/skills/` | On invocation | Variable |

An agent invocation loads 600-1050 lines of static instructions before it reads a single project file. This content is the same regardless of what phase the agent is in, what task it's performing, or what actually helps. The instruction surface:

- **Competes for context window** — static instructions consume tokens that could be used for project code and dynamic context
- **Drifts and contradicts** — multiple files covering overlapping concerns go stale independently
- **Is not portable** — moving to a new repo means copying and adapting dozens of files
- **Is not learnable** — the system cannot optimize which instructions are useful because they're baked into files, not served dynamically

### The Opportunity

Claude Code hooks (documented as of 2026) provide 18 lifecycle event types with capabilities far beyond passive observation:

- **PreToolUse** can modify tool parameters and inject context before execution
- **SubagentStart** can inject context into subagents at spawn time
- **PostToolUse** can inject feedback and replace MCP tool output after execution
- **SessionStart** can set environment variables and inject session context

These capabilities, combined with a workflow state machine inside Unimatrix, could:

1. **Replace static instruction files** with dynamic, phase-aware context delivery
2. **Enforce security boundaries** deterministically (not via LLM compliance)
3. **Track workflow state** without explicit agent reporting
4. **Optimize context delivery** through observation of what actually helps
5. **Enable portability** — the workflow definition becomes the portable artifact

### The Vision

```
YAML Workflow Definition (human-authored, portable)
        │
        ▼
Unimatrix State Machine (tracks current phase, active agents, gate results)
        │
        ├──► SubagentStart hook: inject role×phase context (replaces agent files)
        ├──► PreToolUse hook: inject agent identity, enforce scope (replaces security layer)
        ├──► PostToolUse hook: observe outcomes, provide real-time feedback
        └──► context_briefing: serve tailored knowledge (replaces protocols/rules)
```

The LLM executes work. Everything else — identity, scope, phase tracking, security enforcement, context delivery — is deterministic infrastructure.

---

## Research Questions

### RQ-1: Hook Control Plane Viability

Can the hook system serve as a reliable bidirectional control plane between Unimatrix and Claude Code agents?

**Sub-questions:**

**RQ-1a: Context injection fidelity.** When SubagentStart injects `additionalContext`, does the agent reliably receive and act on it? Is the injected content distinguishable from the agent's original prompt? Does it survive context compaction? What is the effective token budget for injected context before it degrades agent performance?

**RQ-1b: Tool input modification reliability.** When PreToolUse returns `updatedInput` for MCP tool calls, does the modification reach the tool server correctly? Are there edge cases (missing fields, type mismatches, partial updates) that cause failures? Does the agent see the original or modified parameters in its conversation?

**RQ-1c: MCP output replacement.** When PostToolUse returns `updatedMCPToolOutput` for `context_briefing`, does the agent see only the replaced output? Can this be used to enrich briefings with workflow-state-specific content without modifying the MCP server?

**RQ-1d: Real-time feedback delivery.** When PostToolUse returns `additionalContext` after a Write/Edit/Bash call, does the agent incorporate it in subsequent reasoning? Is this mechanism reliable enough for course correction (e.g., "your code diverges from convention X")?

**RQ-1e: Hook latency budget.** What is the practical latency budget for hooks that call external processes (e.g., a Unimatrix CLI)? At what point does hook latency degrade the agent experience? Measure across PreToolUse (blocks execution), PostToolUse (blocks response), and SubagentStart (blocks spawn).

**Validation approach:** Build minimal test hooks for each mechanism. Run them during a real feature session. Measure reliability, latency, and agent behavioral response.

---

### RQ-2: Unimatrix ↔ Hook Communication

How do hooks access Unimatrix's state machine and knowledge base?

**Sub-questions:**

**RQ-2a: Direct redb access.** Can a hook script invoke a CLI binary that opens the same redb database file? redb supports concurrent readers with a single writer. If the MCP server holds a write lock, can a CLI tool read? What are the locking semantics?

**RQ-2b: CLI tool design.** If direct access works, what does a minimal `unimatrix-cli` look like? Commands needed: `state get` (current workflow state), `state transition` (advance phase), `identity issue` (generate agent token), `briefing compile` (generate context for role×phase). How much of the existing server crate can be reused?

**RQ-2c: Sidecar file protocol.** Alternative: hooks write to/read from a lightweight state file (JSON/YAML) that Unimatrix also reads/writes. Simpler than CLI, but introduces consistency risks. When is this appropriate vs. direct DB access?

**RQ-2d: MCP tool calls from hooks.** Can a hook script invoke MCP tools on the running Unimatrix server? The MCP server uses stdio transport — stdin/stdout are owned by the Claude Code process. A hook cannot directly call MCP. Options: (a) separate MCP client in the hook, (b) HTTP sidecar endpoint on the MCP server, (c) CLI tool with shared DB. Evaluate trade-offs.

**RQ-2e: Latency constraints.** Hook timeout defaults are generous (600s for commands), but PreToolUse hooks block tool execution. A CLI tool that opens redb, reads state, and exits must complete in <50ms to be imperceptible. Benchmark realistic scenarios.

**Validation approach:** Prototype a minimal CLI tool that reads from the redb database. Measure open/read/close latency. Test concurrent access with the running MCP server.

---

### RQ-3: Workflow State Machine Design

What does the workflow state machine look like inside Unimatrix?

**Sub-questions:**

**RQ-3a: YAML workflow definition schema.** Design the schema for defining workflows, phases, roles, gates, scope constraints, and transition rules. Must be human-readable, human-editable, and machine-parseable. Consider: how do conditional transitions work (e.g., "if gate fails, loop back to implementation")? How do parallel phases work? How do you express scope constraints per role×phase?

**RQ-3b: State representation.** What state does the machine track? Candidates: current phase, active agents (with IDs), gate results, phase start/end timestamps, outcome references. Where is this stored — a new redb table? A YAML state file? Both? How does state survive server restarts?

**RQ-3c: Transition triggers.** What triggers phase transitions? Options: (a) explicit gate pass detected via PostToolUse observation, (b) agent calls a `workflow_advance` tool, (c) hook detects completion patterns (e.g., gate report written). How much should be automatic vs. explicit?

**RQ-3d: Relationship to col-001.** col-001 built OUTCOME_INDEX with structured tags (type, gate, phase, result). The workflow state machine needs gate results. Should it read from OUTCOME_INDEX, or should the state machine be the primary source and OUTCOME_INDEX become a secondary index? How does the `feature_cycle` field map to workflow instances?

**RQ-3e: Multi-workflow support.** Can multiple workflows run concurrently (e.g., feature work + bug fix)? How are they isolated? How does the state machine handle a session switching between workflows?

**Validation approach:** Design the YAML schema. Model 2-3 real workflow instances (the Unimatrix design-delivery protocol, a bug fix, a research spike). Verify the schema can express all transition patterns.

---

### RQ-4: Agent Identity & Security

Can hooks provide deterministic agent identity and scope enforcement without LLM cooperation?

**Sub-questions:**

**RQ-4a: Identity injection via PreToolUse.** When a PreToolUse hook on `mcp__unimatrix__context_*` tools injects `agent_id` via `updatedInput`, the LLM never controls identity. Design: the SubagentStart hook assigns an identity token (e.g., `{workflow}-{phase}-{role}-{session}`), writes it to a session state file. The PreToolUse hook reads the state file and injects the identity into every MCP tool call. Does this work end-to-end? Does the agent see the injected ID in tool responses?

**RQ-4b: Scope enforcement via PreToolUse.** The workflow YAML defines scope per role×phase (allowed file paths, allowed tools, allowed topics/categories). A PreToolUse hook reads the workflow definition + current state, checks whether the requested tool call is in scope, and blocks (exit 2) with explanation if not. How granular can enforcement be? Can it check Write/Edit file paths against allowed patterns? Can it check `context_store` topic/category against allowed values?

**RQ-4c: Identity lifecycle.** When does an identity get created and when does it expire? Options: (a) SubagentStart creates identity, SubagentStop destroys it — scoped to agent lifetime. (b) SessionStart creates a session-level identity, agents inherit it. (c) Unimatrix issues a signed token (HMAC or similar) that hooks verify — prevents forgery even if an agent tries to override. What level of security is appropriate for single-user local-first deployment vs. future multi-user scenarios?

**RQ-4d: Trust without SDK.** The current AGENT_REGISTRY uses self-reported agent_id with trust levels (System, Privileged, Internal, Restricted). With hook-injected identity, the trust model changes — identity is verified by infrastructure, not self-reported. Does this eliminate the need for trust tiers entirely, or do tiers still serve a purpose (e.g., restricting what a test agent can write)?

**RQ-4e: Audit trail.** With hook-injected identity, every MCP tool call has a verified agent_id. Does this change the AUDIT_LOG design? Should the audit log record "hook-verified" vs. "self-reported" identity provenance?

**Validation approach:** Build a PreToolUse hook that injects agent_id into MCP tool calls. Verify the identity reaches Unimatrix. Test scope enforcement by blocking out-of-scope Write calls.

---

### RQ-5: Dynamic Context Delivery (Enhanced Briefing)

Can `context_briefing` + hooks replace static agent/protocol/rule files?

**Sub-questions:**

**RQ-5a: What's in the thin shell?** If agent files shrink to ~20 lines (identity + briefing call), what MUST remain in the static file vs. what can be served dynamically? Candidates for static: role name, scope declaration, first action (call briefing). Everything else: duties, conventions, gate criteria, file structure expectations — served by briefing.

**RQ-5b: Briefing as primary interface.** Redesign `context_briefing` to accept workflow context: `briefing(role, task, phase, feature)`. The briefing compiles: (a) role duties for this phase (from workflow definition), (b) relevant conventions (from knowledge base, filtered by role×phase), (c) current workflow state (what gates passed, what's pending), (d) outcome history (from observation). How large is the compiled briefing? Target: <1500 tokens with >80% relevance.

**RQ-5c: SubagentStart as briefing trigger.** Instead of the agent calling `context_briefing` as its first action, the SubagentStart hook injects a compiled briefing via `additionalContext`. The agent starts with context already loaded — no explicit tool call needed. Does this work? Does the injected context have the same effect as a tool response?

**RQ-5d: PostToolUse as dynamic enrichment.** After specific tool calls (Write, Edit, Bash), PostToolUse hook checks work against relevant conventions and provides real-time feedback via `additionalContext`. This replaces static "always follow convention X" instructions with contextual "your code does/doesn't match convention X." Does the agent respond to this feedback? Is it more effective than static instructions?

**RQ-5e: CLAUDE.md reduction.** What can be removed from CLAUDE.md if briefing handles conventions, workflows, and agent guidance? Target: CLAUDE.md shrinks to project identity (name, structure, vision) + non-negotiable rules (anti-stub, no root files). Everything else moves to Unimatrix.

**RQ-5f: Portability test.** If the workflow definition + knowledge base are the portable artifacts, what does "unimatrix init" look like for a new repo? What minimal static files are generated? How much of the current `.claude/` directory becomes unnecessary?

**Validation approach:** Take the current `uni-rust-dev` agent definition. Extract everything that could be served dynamically. Build a SubagentStart hook that injects a compiled briefing. Run a real implementation task with the thin shell vs. the full agent definition. Compare output quality and token usage.

---

### RQ-6: Observation Data — Storage, Processing, and Learning

Where should observation data live, how should it be processed, and what learning mechanisms should consume it?

**Sub-questions:**

**RQ-6a: Observation data placement.** ass-010 currently writes raw tool I/O to spool files outside Unimatrix (`~/.unimatrix/observation/spool/`). Evaluate the trade-offs of alternative placements: (a) external spool files (current), (b) a dedicated redb table inside Unimatrix, (c) a separate redb database, (d) a hybrid (real-time signals in redb, raw data in spool). Factors: queryability via MCP tools, dashboard visibility (M6), storage volume, latency of writes during PostToolUse hooks, data retention policy.

**RQ-6b: Real-time signal extraction.** Can the PostToolUse hook extract key signals inline (test pass/fail from Bash output, file paths from Write/Edit, gate results from report content) without excessive latency? What is the cost of inline extraction vs. deferred processing?

**RQ-6c: Utilization tracking.** Compare `context_briefing` output (what was served) against subsequent Write/Edit output (what the agent did). Can this comparison happen in real-time (PostToolUse on Write/Edit checks against last briefing) or must it be batch? What level of matching is feasible inline (substring, concept overlap, embedding similarity)?

**RQ-6d: Feedback loop timing.** The learning loop: serve briefing → observe utilization → adjust next briefing. What is the minimum cycle time? Can it operate within a single session (briefing A serves entry X → agent ignores X → briefing B demotes X)? Or is the cycle cross-session?

**RQ-6e: State machine observation.** Can PostToolUse observations trigger state machine transitions? Example: hook observes test pass → signals state machine → next agent gets updated phase context. What reliability and latency does this achieve?

**RQ-6f: Learning mechanism spectrum.** Observation data can be consumed by multiple learning mechanisms, from simple to sophisticated: (a) rule-based extraction (regex on test output), (b) statistical aggregation (utilization rates over N sessions), (c) agent-based analysis (LLM processes spool), (d) trained models (neural networks learn from observation corpus). Document the data requirements, latency characteristics, and infrastructure needs of each tier. The spike does NOT choose a mechanism — it documents what each requires so that the design phase can make an informed decision based on project stage and goals.

**Validation approach:** Measure observation data volumes from ass-010 spool. Prototype inline signal extraction in a PostToolUse hook and measure latency. Document the data schema requirements for each learning tier.

---

## Implementation Plan

### Phase 1: Hook Capability Validation (2-3 days)

Build minimal proof-of-concept hooks to validate RQ-1:

1. **SubagentStart hook** — inject `additionalContext` with a known string. Verify agent receives it.
2. **PreToolUse hook on MCP tools** — inject `agent_id` via `updatedInput`. Verify it reaches Unimatrix.
3. **PostToolUse hook** — inject `additionalContext` after a Bash call. Verify agent references it.
4. **PostToolUse hook on MCP** — replace `context_briefing` output via `updatedMCPToolOutput`. Verify agent sees replacement.
5. Measure latency for each hook type across 20+ invocations.

Output: `findings/hook-capabilities.md` — what works, what doesn't, latency measurements.

### Phase 2: Communication & State Prototype (2-3 days)

Validate RQ-2 and RQ-3:

1. **CLI prototype** — minimal Rust binary that opens redb read-only, reads workflow state, returns JSON. Measure latency.
2. **Concurrent access test** — CLI reads while MCP server writes. Verify no conflicts.
3. **YAML schema draft** — model the Unimatrix design-delivery protocol as YAML. Model a bug fix workflow. Model a research spike.
4. **State file prototype** — workflow state as a JSON file that hooks read/write. Compare to redb-based state.

Output: `findings/communication.md`, `findings/workflow-schema.md`

### Phase 3: Identity & Security Prototype (1-2 days)

Validate RQ-4:

1. **Identity injection** — SubagentStart writes identity to state file. PreToolUse injects into MCP calls. End-to-end test.
2. **Scope enforcement** — PreToolUse blocks Write calls to out-of-scope paths. Test with real agent session.
3. **Audit verification** — confirm AUDIT_LOG records hook-injected identity correctly.

Output: `findings/identity-security.md`

### Phase 4: Dynamic Briefing Prototype (2-3 days)

Validate RQ-5:

1. **Thin shell experiment** — strip an agent to identity-only, inject briefing via SubagentStart hook. Run a real task.
2. **Compare** — same task with full agent definition vs. thin shell + injected briefing. Measure: token usage, output quality, completion time.
3. **CLAUDE.md reduction** — identify which lines can be removed if briefing handles them. Test with reduced CLAUDE.md.

Output: `findings/dynamic-briefing.md`, `findings/claude-md-reduction.md`

### Phase 5: Synthesis & Formal Recommendations (1-2 days)

Combine all findings into evidence-based recommendations. Each recommendation must cite specific measurements and test results from Phases 1-4. The synthesis does not advocate for a predetermined architecture — it presents what the evidence supports, what it doesn't, and where trade-offs exist.

Recommendations to produce:
1. **Hook control plane** — which mechanisms are viable, which aren't, measured capabilities and limitations
2. **Communication architecture** — recommended path for hook↔Unimatrix communication with latency data
3. **Workflow state machine** — feasibility assessment, schema viability, storage recommendation
4. **Agent identity & security** — recommended model with trust analysis
5. **Dynamic context delivery** — measured token reduction, quality comparison, what must remain static
6. **Observation data strategy** — recommended placement, processing approach, and learning mechanism tiers with data requirements for each
7. **M5 architecture recommendation** — how findings affect col-002+ design, which features change and how
8. **Future learning path** — what observation data infrastructure enables for neural model integration (documenting requirements, not designing the models)

Output: `findings/synthesis.md` — formal recommendations grounded in evidence.

---

## Success Criteria

The spike succeeds if it produces **evidence-based recommendations with measured data** for each research question. Success is not a binary GO/NO-GO — it's clarity on what works, what doesn't, and what each option costs.

### Per-Capability Measurement Targets

These targets define what "viable" means for each capability. Meeting or missing a target is a data point, not a verdict:

| Capability | Measurement | Viable Threshold | Ideal Threshold |
|-----------|-------------|-----------------|-----------------|
| Context injection (RQ-1a) | Agent acts on injected content | >70% of injections | >90% |
| Tool input modification (RQ-1b) | Modified params reach MCP server | 100% (correctness) | 100% |
| MCP output replacement (RQ-1c) | Agent sees replaced output | 100% (correctness) | 100% |
| Feedback delivery (RQ-1d) | Agent references feedback | >50% of feedback | >70% |
| Hook↔Unimatrix round-trip (RQ-2) | Latency of CLI/state read | <100ms | <50ms |
| Identity injection (RQ-4a) | Identity reaches AUDIT_LOG | 100% (correctness) | 100% |
| Scope enforcement (RQ-4b) | Out-of-scope calls blocked | 100% (correctness) | 100% |
| Thin-shell quality (RQ-5) | Output quality vs. full agent | Comparable | Better (fewer tokens, same quality) |
| Inline signal extraction (RQ-6b) | Latency per PostToolUse | <100ms | <30ms |

### Spike Deliverables

The spike is complete when:
1. Every RQ has a findings document with measured results
2. Each capability is classified as: **Validated** (meets viable threshold), **Marginal** (close to threshold, needs refinement), or **Not Viable** (below threshold or fundamentally limited)
3. The synthesis document produces formal recommendations citing specific findings
4. Open questions #1-7 are answered with evidence
5. Observation data strategy recommends placement and documents requirements per learning tier

---

## Constraints

- **No production code changes during research.** This spike produces findings documents, prototype hooks, and a CLI prototype. No changes to existing crates.
- **Prototype hooks are disposable.** They validate capabilities, not production quality. Production hooks will be designed based on findings.
- **CLI prototype may become a real component.** If RQ-2 validates direct redb access, the CLI becomes the foundation for hook↔Unimatrix communication. Design it as a potential keeper.
- **YAML schema is a draft.** It will be refined during col-002 (or whatever replaces col-002) design. The spike validates that YAML can express the workflow, not that the schema is final.
- **ass-010 continues independently.** The observation spool keeps collecting data. This spike may adjust what ass-010 captures based on findings.

---

## Relationship to Roadmap

```
M4 (Learning & Drift)    ✅ COMPLETE (crt-001 through crt-004)
  │
  ├── col-001 (Outcome Tracking)     ✅ COMPLETE
  │
  ├── ass-010 (Observation Pipeline)  ● ACTIVE (data collection)
  │
  └── ass-011 (THIS SPIKE)           ● STARTING
        │
        ▼
    GO/PARTIAL/NO-GO decision
        │
        │
        ▼
    Formal Recommendations (evidence-based)
        │
        ├── Per-capability recommendations (each RQ produces independent findings)
        ├── Architecture recommendation for M5 redesign (informed by all RQs)
        ├── Observation data strategy (placement, processing, learning tiers)
        └── Security model recommendation (identity, enforcement, trust)
```

### Features Potentially Affected

If findings support hook-driven orchestration, the following planned features may be redesigned, simplified, or subsumed. The spike does NOT predetermine these outcomes — findings will indicate which features are affected and how:

| Feature | What Findings Would Affect It |
|---------|-------------------------------|
| col-002 (Retrospective Pipeline) | RQ-6: Where observation data lives and how it's processed determines whether col-002 is Rust pipeline, agent analysis, learned model, or hybrid |
| col-003 (Process Proposals) | RQ-3: If state machine tracks process patterns, col-003's scope changes |
| col-004 (Feature Lifecycle) | RQ-3: If state machine handles lifecycle, col-004 may be subsumed |
| alc-001 (CLAUDE.md Integration) | RQ-5e: If CLAUDE.md reduces, alc-001's scope changes |
| alc-002 (Agent Orientation) | RQ-5c: If SubagentStart delivers context, orientation pattern changes |
| alc-010 (Thin-Shell Pattern) | RQ-5a: If hooks deliver context dynamically, thin shells emerge naturally |

### What Carries Forward Unchanged

| Component | Why It Stays |
|-----------|-------------|
| Storage engine (nxs-001) | Fundamental |
| Vector index (nxs-002) | Semantic search still needed |
| Embedding pipeline (nxs-003) | Embeddings still needed |
| Core traits (nxs-004) | API contracts |
| MCP server (vnc-001/002/003) | Agent interface to knowledge base |
| Contradiction detection (crt-003) | Knowledge integrity |
| OUTCOME_INDEX (col-001) | Structured outcome data; state machine reads from it |
| Usage tracking (crt-001) | Foundation for utilization measurement |

### What May Simplify Over Time

| Component | Potential Simplification |
|-----------|------------------------|
| CO_ACCESS table (crt-004) | RQ-6: If observation data captures co-retrieval patterns, atomic pair tracking may be redundant |
| FEATURE_ENTRIES (crt-001) | RQ-6: If observation captures entry-to-feature linkage, dedicated table may simplify |
| 6-factor confidence (crt-002) | RQ-6c/6f: If observed utilization proves to be a higher-fidelity signal, confidence formula may simplify |
| AGENT_REGISTRY trust tiers | RQ-4d: If hook-verified identity is viable, self-reported trust model may change |

---

## Open Questions (To Be Answered by Spike)

1. **Hook execution context for subagents.** Do hooks fire for subagent tool calls with the subagent's context, or the parent's? If subagent, does the SubagentStart-assigned identity persist across the subagent's tool calls?

2. **Context compaction interaction.** When Claude compacts context (PreCompact event), does injected `additionalContext` from hooks get compacted/lost? If so, critical workflow context could disappear mid-session.

3. **Hook ordering.** If multiple hooks match the same event, what is the execution order? Can one hook's output feed into another's input? This matters if identity injection and scope enforcement are separate hooks.

4. **Agent-type hooks for analysis.** The "agent" hook type spawns a subagent with tool access. Could this be used for real-time convention checking (PostToolUse agent hook analyzes code against conventions)? What is the latency and cost?

5. **Prompt-type hooks for lightweight decisions.** The "prompt" hook type makes a single-turn LLM call. Could this be used for nuanced scope enforcement (e.g., "is this Write call within the spirit of the workflow phase?")? Cost vs. deterministic enforcement trade-off.

6. **CLI binary distribution.** If hooks need a CLI tool, how is it distributed? Built from the workspace? Installed separately? Does this create a bootstrap problem (hooks need CLI, CLI needs Unimatrix, Unimatrix needs hooks)?

7. **State machine recovery.** If the state file gets corrupted or deleted mid-workflow, how does the system recover? Can state be reconstructed from OUTCOME_INDEX + observation spool?

8. **Neural model foundation.** The long-term vision includes trained models inside Unimatrix that learn from observation data (not just rule-based extraction or agent analysis). What data schema, storage format, and access patterns would a future training pipeline require? RQ-6f should document these requirements even though model training is out of scope for this spike. The goal is to ensure the observation data infrastructure doesn't foreclose the neural learning path.

9. **Observation data as a first-class Unimatrix concern.** If observation data moves inside Unimatrix (RQ-6a), it becomes queryable via MCP tools and visible to dashboards (M6). This has implications for data volume, retention policy, and the MCP tool surface. What are the trade-offs of making observation data a peer of knowledge entries vs. keeping it as a separate internal data store?
