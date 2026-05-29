# FINDINGS: Vision & Identity Analysis — RQ-7

**Spike**: ass-063
**Date**: 2026-05-29
**Approach**: investigation
**Confidence**: directional

---

## Findings

### Q: The current vision explicitly states "Unimatrix is not an orchestration engine." Does this pivot change the product identity, or is there a coherent hybrid — a knowledge engine that also understands workflow structure and can guide execution through it? What is gained and what is lost? Is "workflow-aware knowledge engine that can also serve as workflow harness" a natural extension of "workflow-aware knowledge engine," or a different product?

**Answer**: The proposed pivot represents a genuine product identity change, not a natural extension. However, there is a coherent middle ground that preserves the knowledge engine identity while absorbing the most valuable aspects of workflow harnessing. The boundary line falls at a specific point: storing workflow definitions in the graph and serving per-step instructions is a natural extension; dispatching agents and controlling execution flow is a different product.

**Evidence**:

#### 1. The Current Vision Boundary Is Precisely Defined and Load-Bearing

The vision statement in `product/PRODUCT-VISION.md` (lines 11-21) makes an explicit architectural distinction:

> "Workflow definitions, agent definitions, and skill definitions are static — they live in your tooling and change infrequently. Architecture decisions, patterns, and lessons-learned are dynamic — they evolve with every feature, every delivery, every failure. Unimatrix was designed to manage the dynamic layer."

This static/dynamic partition is not decorative. It is the core architectural principle that has governed every shipped feature. Unimatrix's entire intelligence pipeline — phase-conditioned ranking (WA-1), session context enrichment (WA-2), proactive delivery (WA-4), the planned GNN (W3-1) — optimizes for surfacing the right *knowledge* at the right *moment*. The "right moment" is determined by workflow position, but the workflow itself is externally controlled.

The claude-flow competitive analysis (Unimatrix entry #190, 2026-03-01) explicitly validated this separation: "claude-flow is an orchestration shell [...] Unimatrix is a knowledge engine [...] They occupy different layers." The recommendation was: "Keep Unimatrix as knowledge engine. Adopt claude-flow's orchestration patterns" — specifically, adopt the *injection patterns* (UserPromptSubmit, PreCompact hooks), not the orchestration control plane.

#### 2. What Unimatrix Already Does That Is Workflow-Adjacent

The codebase reveals that Unimatrix is substantially more workflow-aware than a pure knowledge engine, but stops short of control:

- **Session lifecycle tracking** (`sessions.rs`): Records session_id, feature_cycle, agent_role, started_at, ended_at, status (Active/Completed/TimedOut/Abandoned), outcome, compaction_count, total_injections.
- **Cycle events** (`context_cycle` tool): Tracks start/phase-end/stop with topic, phase names, goal text. This is workflow *observation*, not workflow *control*. The handler literally responds with "Acknowledged" — it records, it does not direct.
- **Observation pipeline** (`observations.rs`): Records every hook event with session_id, tool, input, response_size. 23 detection rules across 4 categories analyze this data retrospectively.
- **Signal queue** (`signal.rs`): Captures confidence signals (Helpful/Flagged) per entry per session for learning feedback.
- **Phase-conditioned delivery**: `context_briefing` uses the current phase and cycle goal to prioritize knowledge. UDS injection is phase-conditioned. The GNN session context vector (W3-1) includes `current_phase`, `category_histogram`, `cycle_position_normalized`.

Crucially, all of this workflow awareness flows *inward* — Unimatrix observes the workflow and uses that context to improve knowledge delivery. No information flows *outward* to control the workflow. The `context_cycle` tool (lines 3251-3395 of `tools.rs`) is an acknowledgment endpoint, not a control endpoint.

#### 3. The Identity Spectrum — Where Does the Proposal Sit?

Four positions along the spectrum:

**(a) Pure knowledge engine** — Stores and retrieves structured knowledge. No workflow awareness. Unimatrix was never here.

**(b) Workflow-aware knowledge engine** — Understands workflow structure, uses it to improve retrieval, but does not control execution. This is where Unimatrix sits today.

**(c) Knowledge engine with workflow harness** — Stores workflow definitions AND actively guides execution through them. This is what the SCOPE.md describes: "graph-stored workflow definitions that Unimatrix actively controls [...] The LLM receives per-step instructions rather than loading full protocols, and Unimatrix enforces sequencing, gates, and agent routing."

**(d) Orchestration engine with knowledge** — Execution control is primary, knowledge is secondary. The claude-flow model.

The proposal sits at position (c). The critical question is whether (c) is a stable equilibrium or an unstable transition state between (b) and (d).

#### 4. Why (c) Is Unstable

The proposal's three key features — per-step instruction delivery, gate enforcement, agent routing — each pull toward (d):

- **Per-step instruction delivery**: If Unimatrix serves the next step's instructions, it must know the workflow graph, current position, and completion criteria. It becomes the state machine. The LLM no longer self-navigates; it receives instructions. This is orchestration by another name.

- **Gate enforcement**: If Unimatrix decides whether a step passes or fails and controls the transition, it is making workflow-control decisions, not knowledge decisions. The `context_cycle(type: "phase-end")` today is an *observation* that the SM reports. If Unimatrix validates the gate condition, it becomes the SM.

- **Agent routing**: If Unimatrix dispatches steps to different LLMs or agents, it is a scheduler. The multi-LLM routing question (RQ-5) directly requires this — Unimatrix must know which provider to call, manage sessions, transfer context. This is the definitional function of an orchestration engine.

Each feature individually might be justified as "the knowledge engine that also does X." Together, they constitute an orchestration control plane that happens to also store knowledge.

#### 5. Why the Static/Dynamic Boundary Matters

The vision document's static/dynamic partition reflects a genuine architectural insight about change rates:

- **Protocols change infrequently**: The delivery protocol (`.claude/protocols/uni/uni-delivery-protocol.md`) has been stable since early in the project. Gate structures, agent roles, and step ordering evolve slowly.
- **Knowledge changes continuously**: ADRs, patterns, lessons — these emerge from every feature cycle and must be curated, corrected, and versioned.

Systems that manage both fast-changing and slow-changing data tend to optimize for one at the expense of the other. Unimatrix's entire integrity chain (hash-chained corrections, confidence evolution, contradiction detection, supersession) is designed for the dynamic layer. Applying this machinery to static workflow definitions is either unnecessary (they don't need confidence scoring or contradiction detection) or harmful (treating protocol steps as correctable knowledge entries when they should be version-controlled artifacts).

#### 6. What Is Gained

- **Enforcement over advisory**: Protocols today are advisory text that LLMs interpret. Compliance drift is real — the SCOPE.md documents this as a trigger. A harness that enforces step ordering and gate conditions solves a genuine problem.
- **Token reduction**: Loading a full protocol (~2000-4000 tokens) vs. receiving the current step's instructions (~200-400 tokens) is a material saving. This is the strongest practical argument.
- **Workflow observability**: If the workflow graph lives in Unimatrix, the observation pipeline can see workflow structure directly, not just infer it from hook events. `context_cycle_review` becomes dramatically more useful.
- **Multi-LLM routing**: The current architecture cannot coordinate across providers. A central harness with provider adapters solves this.

#### 7. What Is Lost

- **Simplicity**: Unimatrix is a 9-crate Rust workspace shipping as a single binary. Adding workflow execution, agent dispatching, multi-provider adapters, and a workflow DAG engine likely doubles the codebase.
- **Clear product boundary**: "Knowledge engine, not orchestration engine" is instantly comprehensible. "Workflow-aware knowledge engine that can also serve as workflow harness" requires explanation. Product identity matters for adoption.
- **"Do one thing well"**: The knowledge engine is deep. 14 MCP tools, typed knowledge graph, HNSW vector index, NLI cross-encoder, confidence scoring, observation pipeline, MicroLoRA adaptive embeddings. Every new capability competes for attention.
- **Composability**: Today, Unimatrix works with any workflow system — Claude Code, Gemini CLI, Codex CLI, or a human-driven process. If Unimatrix becomes the workflow system, it may exclude alternatives. Users who already have workflow orchestration (LangChain, CrewAI, AutoGen) cannot adopt just the knowledge engine without also adopting the orchestration layer.
- **The vision's own guard rails**: The product has a vision guardian agent (`uni-vision-guardian.md`) that validates every feature against `PRODUCT-VISION.md`. 18+ features have been checked against it. This boundary has been enforced consistently. Removing it removes a governance mechanism that has caught scope creep.

#### 8. The Coherent Middle Ground

There is a position between (b) and (c) that addresses the real pain points without crossing into orchestration:

**"Workflow-literate knowledge engine"** — Unimatrix stores workflow definitions as knowledge (a new category or entry type), understands their graph structure, and can serve the right step's instructions on request — but does not control execution, enforce gates, or dispatch agents.

The LLM or external coordinator calls `workflow_current_step(topic: "crt-042")` and gets back instructions for the current step. The LLM calls `workflow_step_complete(step_id: 42, outcome: "pass")` and Unimatrix records the transition and serves the next step. The LLM remains the executor.

This framing:
- Solves token reduction (per-step delivery, not full protocol loading)
- Preserves the knowledge engine identity (workflow definitions are just another knowledge type in the graph)
- Keeps the static/dynamic boundary (workflow definitions are stored but not run through the knowledge integrity chain — they are version-controlled separately)
- Does NOT require agent dispatching, provider adapters, or execution control
- Works with any LLM or orchestrator that can call MCP tools
- Leaves gate enforcement to the LLM/orchestrator, but provides the gate criteria as knowledge

The difference: Unimatrix answers "what should I do next?" but does not answer "do this." The LLM remains the decision-maker and executor. Unimatrix is a navigator, not a driver.

#### 9. Precedent Analysis

The tool-that-knows vs. tool-that-coordinates tension is well-established:

- **Terraform**: Started as infrastructure-as-code (declarative definitions). Added an execution engine. Terraform became an orchestration tool; the "infrastructure knowledge" aspect is secondary. The scope expanded permanently.
- **Elasticsearch**: Started as a search engine. Kibana added dashboards. Logstash added ingestion. The "ELK stack" became a complete observability platform. The search engine identity is diluted.
- **Notion**: Started as a note-taking tool with database features. Added workflow views, automations, timelines. Now competes with project management tools. Users who wanted a knowledge tool now use a project management tool.

The pattern: when a knowledge/information tool adds workflow control, the control features attract most development attention because they are more immediately impactful and more frequently requested. Over time, the knowledge features become the less-maintained layer.

Counter-examples: **Git** stores code and understands branching/merging but does not build or deploy. **PostgreSQL** stores data and understands relationships but does not orchestrate applications. These tools maintain their identity by being extremely good at their core function and letting other tools compose on top of them. Unimatrix is closer to this model today.

**Recommendation**: Adopt the "workflow-literate knowledge engine" framing. Store workflow definitions in the graph and serve per-step instructions on demand. Do not cross into execution control, gate enforcement, or agent dispatching. The specific litmus test: if a feature requires Unimatrix to *initiate an action* (spawn an agent, call an API, enforce a gate), it has crossed the line. If a feature requires Unimatrix to *answer a question* ("what should I do next?", "what are this step's instructions?", "did the previous step pass?"), it is within bounds.

---

## Unanswered Questions

None for RQ-7. The question is fully addressed. The recommendation's viability depends on findings from RQ-1 (graph feasibility for workflow DAGs) and RQ-2 (execution semantics), which are handled by parallel researchers.

---

## Out-of-Scope Discoveries

- **Protocol versioning gap**: The current system stores protocols as static `.md` files in `.claude/protocols/`. If workflow definitions move into the graph, there is no versioning mechanism for them. The knowledge integrity chain (hash-chained corrections, supersession) is designed for entries that evolve independently. A protocol is an atomic unit; its steps cannot be independently superseded. This architectural constraint needs resolution before any workflow-in-graph implementation.

- **Vision guardian process update required**: Regardless of outcome, the vision guardian agent, the vision document, and the README all need coordinated updates if the boundary moves. Skipping this would cause the guardian to flag every workflow-related feature as a vision violation.

---

## Recommendations Summary

- **RQ-7**: Do not adopt full workflow orchestration. Adopt a "workflow-literate knowledge engine" framing — store workflow definitions as graph-structured knowledge, serve per-step instructions on request, record step completion, but leave execution control and agent dispatching to the LLM or an external coordinator. Litmus test: Unimatrix answers questions about workflow state; it does not initiate actions. This addresses the real pain (token cost, compliance drift) without crossing the identity boundary into orchestration.
