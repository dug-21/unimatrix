# ASS-063: Unimatrix as Protocol/Workflow Harness — FINDINGS

**Spike**: ass-063
**Date**: 2026-05-29
**Approach**: synthesis (5 research tracks)
**Confidence**: directional

---

## Executive Summary

Unimatrix can absorb workflow definitions into its typed graph and serve per-step instructions to LLMs without fundamental redesign. The token savings are real but secondary (19-29% of protocol overhead, 2-6% of total context budget). The primary value is compliance enforcement and quality improvement — eliminating SM-level protocol drift and reducing subagent depth from 2 to 1. However, the five research tracks surface a critical identity tension: the graph and execution models (RQ-1, RQ-2) assume Unimatrix enforces gates and controls sequencing, while the vision analysis (RQ-7) argues this crosses the product boundary into orchestration. This synthesis resolves that tension toward a narrowed "workflow-literate" model with structural gate support but not behavioral enforcement, paired with a thin external coordinator for multi-LLM dispatch. The recommended path is proceed-with-narrowed-scope.

---

## Per-RQ Findings

### RQ-1: Graph as Workflow Model

**Answer**: The existing typed graph can represent workflow DAGs with targeted extensions — no schema migration required.

**Key evidence**: The current graph supports directed typed edges, multi-hop traversal, cycle detection, and 16 relation types stored as free-form strings. Three new entry categories (`workflow`, `step`, `gate`) and three new edge types (`HasStep`, `GatedBy`, `Requires`) are sufficient. Workflow composition works through cross-workflow `Requires` edges. Step and gate content schemas are JSON conventions stored in the existing `content` field — no storage layer changes. The existing traversal modes (`neighbors`, `path`, `subgraph`, `filter`, `inverse`) already support all needed workflow queries.

**Effort**: ~20 lines of Rust change to `RelationType` enum (3 new match arms in `as_str`/`from_str`). Zero schema migrations.

**Recommendation**: Extend the graph with 3 categories and 3 edge types. Define content schemas as documented conventions, not enforced at storage layer.

---

### RQ-2: Workflow Execution Semantics

**Answer**: Three models evaluated — passive (LLM self-navigates with hints), guided (Unimatrix returns structured step payloads and enforces sequencing), and active (Unimatrix initiates LLM sessions). The guided model is architecturally sound and the most defensible for a first version.

**Key evidence**: The MCP protocol is request-response with the LLM as initiator. Active dispatch (Model C) would require outbound HTTP clients, provider API keys, and session lifecycle management — turning Unimatrix into an orchestration engine. The guided model (Model B) proposes 5 MCP tools (`workflow_start`, `workflow_complete_step`, `workflow_gate_result`, `workflow_status`, `workflow_abort`) and 3 new SQL tables for mutable execution state (`workflow_runs`, `workflow_step_runs`, `workflow_gate_runs`). State gating enforces compliance: `workflow_complete_step` only returns the next eligible step; there is no API to skip a gate.

**Tension with RQ-7**: The guided model's gate enforcement (Unimatrix decides pass/fail, blocks progression) crosses the vision boundary identified in RQ-7. See "Key Tensions & Resolution" below for the resolved position.

**Recommendation**: Implement the tool surface from the guided model but with advisory semantics initially. See resolution below.

---

### RQ-3: LLM Token Reduction (Quantitative)

**Answer**: Per-step delivery reduces SM-level protocol consumption by 70-80%. Total systemic savings are 19-29% of protocol/agent overhead (3,500-11,400 tokens per session), representing 2-6% of a 200K context budget.

**Key evidence**: Protocol files consume 3,469-6,483 tokens per session type at the SM level. Including agent definitions across all subagent spawns, total overhead ranges from 12,800 (research) to 39,800 (delivery) tokens. At any given step, only 15-20% of the full protocol is relevant (10% step-specific + 10% cross-cutting). The delivery protocol is most dramatic: a Stage 3b agent needs ~1,050 tokens out of the full 6,483 (16%).

Agent definition redundancy compounds the cost: `uni-validator` (12,554 bytes) contains 4 gate check sets but each spawn needs only one. CLAUDE.md (~1,216 tokens) loads redundantly into every subagent. In a delivery session with 8+ subagents, that is ~10,000 tokens of repeated project-level instructions.

**Recommendation**: Token reduction alone does not justify the infrastructure investment. It is a positive side effect of workflow control, not the primary driver.

---

### RQ-4: Subagent Bypass Path

**Answer**: The hybrid model captures the most value — Unimatrix controls sequencing for dependent steps (run at top-level, no depth penalty), allows parallel spawns at depth-1 (one level shallower than today's depth-2) for independent work.

**Key evidence**: Current architecture runs SM at depth-1, all specialists at depth-2. Observed depth-2 degradation: reduced instruction-following fidelity, lower self-correction, variable protocol compliance. The protocols themselves contain compensatory mechanisms proving drift occurs — "Do NOT skip this step" in bold, "MANDATORY BLOCK" in all-caps, Quick Reference sections that repeat the entire flow because the SM loses track.

Latency trade-offs for a design session (7 steps, 5 sequential, 2 parallel): current ~30 min, fully sequential ~45 min, hybrid ~35 min. For mostly-sequential sessions (bugfix), latency difference is negligible.

**Recommendation**: Adopt hybrid model. Sequential steps at top-level with Unimatrix providing per-step instructions. Parallel agents at depth-1 only for genuinely independent work. 5-17% latency cost; compliance drift elimination is the primary value.

---

### RQ-5: Multi-LLM Routing

**Answer**: Feasible today using existing provider tooling. All three major CLI agents (Claude Code, Codex CLI, Gemini CLI) support programmatic non-interactive invocation with JSON output, working directory specification, and MCP server connections.

**Key evidence**: Claude Code supports `claude -p` with `--mcp-config`, `--output-format json`, and session resume. Codex CLI supports `codex exec` with `--json`, `--cd`, and full MCP integration including running as an MCP server itself. Gemini CLI supports `gemini -p` with `--output-format json` and MCP via settings config. All operate on the same filesystem (git working tree), providing natural artifact transfer.

Critical constraint: MCP Sampling is deprecated in the 2026-07-28 release candidate. An MCP server cannot ask the LLM to do work — it can only respond to requests. Active dispatch through MCP is architecturally blocked.

The minimum viable model is passive guidance + thin coordinator: Unimatrix provides workflow state via MCP tools; a small coordinator script (~100-200 lines) dispatches steps to the appropriate CLI agent with MCP server connection; filesystem + Unimatrix graph handle context transfer.

**Recommendation**: Use passive guidance model. Unimatrix does not initiate LLM sessions. A thin external coordinator dispatches. Zero custom provider integration required.

---

### RQ-6: Visual Workflow UI

**Answer**: ReactFlow (@xyflow/react) with dagre layout is the clear framework choice. MIT license, 1.2 MB, purpose-built for node-based workflow editors, used by Stripe/Zapier, 8.55M weekly npm installs.

**Key evidence**: 12 frameworks surveyed. tldraw eliminated (commercial $6k/yr license). Excalidraw eliminated (freeform, not structured DAG). Mermaid eliminated (76 MB, read-only). Litegraph.js eliminated (archived). Cytoscape.js is strong for read-only but poor for interactive editing. JointJS viable but more complex and MPL-2.0 licensed.

Production validation: n8n (50k+ GitHub stars) uses Vue Flow (xyflow family). LangGraph Studio uses React. Dagster uses React + dagre. All successful workflow UIs share: graph canvas + detail sidebar, status by node color, automatic layout with manual override, connection ports, minimap.

Build effort: Tier A (read-only visualization) 2-3 person-weeks, Tier B (interactive editing) 5-7 person-weeks cumulative, Tier C (full execution overlay) 10-14 person-weeks cumulative. Tier A deploys as ~200-250 KB gzipped static assets served by the Rust daemon, no Node.js in production.

Important alternative discovered: MCP Apps (2026-07-28 RC) allow MCP servers to ship interactive HTML UIs in sandboxed iframes. Unimatrix could serve workflow visualization as an MCP App rather than bundling a standalone web UI.

**Recommendation**: Use ReactFlow + dagre. Start at Tier A. ReactFlow's data model supports incremental upgrade to Tier B/C without rewrite.

---

### RQ-7: Vision Boundary

**Answer**: Full workflow orchestration crosses the product identity boundary. There is a coherent middle ground: "workflow-literate knowledge engine" — Unimatrix stores workflow definitions as knowledge, serves per-step instructions on demand, and records step completion, but leaves execution control and agent dispatching to the LLM or an external coordinator.

**Key evidence**: The current vision's static/dynamic partition is load-bearing — the entire intelligence pipeline (phase-conditioned ranking, session enrichment, proactive delivery, planned GNN) optimizes for surfacing the right knowledge at the right moment. The claude-flow competitive analysis (entry #190) explicitly validated: "Keep Unimatrix as knowledge engine. Adopt claude-flow's orchestration patterns."

The proposal's three key features each pull toward orchestration: per-step instruction delivery makes Unimatrix the state machine, gate enforcement makes it the workflow controller, agent routing makes it a scheduler. Historical precedent (Terraform, Elasticsearch/ELK, Notion) shows that when a knowledge tool adds workflow control, control features attract most development attention and the knowledge features become the less-maintained layer. Counter-examples (Git, PostgreSQL) maintain identity by doing one thing well and letting others compose on top.

The litmus test: if a feature requires Unimatrix to *initiate an action* (spawn an agent, call an API, enforce a gate), it has crossed the line. If a feature requires Unimatrix to *answer a question* ("what should I do next?", "what are this step's instructions?"), it is within bounds.

**Recommendation**: Adopt workflow-literate knowledge engine framing. Store workflow definitions in the graph. Serve per-step instructions on request. Record step completion. Do not enforce gates or dispatch agents from within Unimatrix.

---

## Feasibility Matrix

| RQ | Topic | Feasibility | Effort | Dependencies | Key Risk |
|---|---|---|---|---|---|
| RQ-1 | Graph as Workflow Model | **Feasible** | S | None — extends existing graph | Category allowlist (crt-031) may need update |
| RQ-2 | Workflow Execution Semantics | **Feasible-with-caveats** | M-L | RQ-1; identity resolution (RQ-7) | Scope creep from guided to active model |
| RQ-3 | Token Reduction | **Feasible** | S | RQ-1, RQ-2 | Savings secondary (2-6% of total budget) |
| RQ-4 | Subagent Bypass | **Feasible** | M | RQ-2 (workflow tools needed for sequencing) | Parallel step encoding adds graph complexity |
| RQ-5 | Multi-LLM Routing | **Feasible-with-caveats** | M | RQ-1, RQ-2; external coordinator needed | Gemini CLI lacks session resume; MCP Sampling deprecated |
| RQ-6 | Visual Workflow UI | **Feasible-with-caveats** | L-XL (Tier B/C) | W2-1 container; REST/WS API for UI | New capability area (no frontend in project today) |
| RQ-7 | Vision Boundary | N/A (identity question) | — | — | Resolved: narrowed scope preserves identity |

---

## Key Tensions & Resolution

### Tension 1: Gate Enforcement — Guided Model vs. Workflow-Literate Model

**The conflict**: RQ-2 (FINDINGS-GRAPH) proposes Unimatrix as the authority that enforces gates — `workflow_complete_step` blocks progression until gate conditions are met; `workflow_gate_result` controls whether the workflow advances, reworks, or escalates. RQ-7 (FINDINGS-VISION) argues that gate enforcement crosses the identity boundary — Unimatrix should answer "what's next?" but not decide whether the LLM may proceed.

**Evidence from both sides**:
- For enforcement: The SCOPE.md problem statement identifies compliance drift as the primary pain point. The RQ-2 analysis states "Gate enforcement is the critical differentiator — without it, the system is just a more expensive way to store protocol files." The RQ-4 evidence shows SM drift is real (MANDATORY BLOCK markers, Quick Reference redundancy).
- Against enforcement: The RQ-7 analysis shows that gate enforcement makes Unimatrix "the SM" — it is making workflow-control decisions, not knowledge decisions. The static/dynamic partition is architecturally load-bearing. Historical precedent shows knowledge tools that add control features drift toward becoming control tools.

**Resolution**: Adopt a **two-layer architecture** that preserves the identity boundary while enabling enforcement:

1. **Unimatrix layer (knowledge)**: Stores workflow definitions, serves per-step instructions, records step completion and gate results, provides `workflow_status` for any consumer. It *stores* gate criteria and *records* gate outcomes. It answers questions: "what is the next step?", "what are this gate's criteria?", "what happened in this run?" The `workflow_next` tool returns the topologically next step based on recorded completions — it does not refuse to serve information.

2. **Coordinator layer (control)**: A thin external component (the ~100-200 line coordinator from RQ-5, or the LLM itself) reads gate criteria from Unimatrix, evaluates them, records results back to Unimatrix, and decides whether to advance, rework, or escalate. Gate enforcement happens outside Unimatrix.

This preserves the litmus test: Unimatrix answers questions but does not initiate actions or block progression. The coordinator enforces. If the coordinator is a 200-line shell script, it is not a second product — it is a deployment artifact, like a Makefile. Unimatrix remains composable: users with their own orchestration (LangChain, CrewAI) can use Unimatrix for workflow knowledge without adopting its enforcement semantics.

**Practical impact on proposed MCP tools**: The 5 tools from RQ-2 survive with adjusted semantics:
- `workflow_start` — unchanged (creates run, returns first step)
- `workflow_complete_step` — records completion and outputs; returns the next step based on graph topology and recorded completions. Does NOT refuse to serve the next step if a gate is pending — it returns the gate criteria alongside the next step, annotated with a `gate_pending: true` flag. The caller decides whether to proceed.
- `workflow_gate_result` — records gate outcome. Does NOT block progression. Returns the rework step or next step as information.
- `workflow_status` — unchanged (read-only)
- `workflow_abort` — unchanged (records termination)

The enforcement guarantee comes from the coordinator or LLM checking `gate_pending` before executing the next step. A well-behaved caller enforces gates; a misbehaving caller can bypass them. This is the same trust model as the current protocol files (advisory), but the structure makes compliance dramatically easier — the caller gets explicit gate criteria and a binary flag, not a buried paragraph in a 6,000-token protocol.

### Tension 2: Effort Justification vs. Token Savings

**The conflict**: RQ-3 shows token savings of 2-6% of total context budget — not transformative. RQ-2 and RQ-4 require significant infrastructure (5 MCP tools, 3 SQL tables, coordinator script).

**Resolution**: Token savings are not the justification. The investment is justified by:
1. **Compliance improvement** (RQ-4): Eliminating SM depth penalty and providing structured step delivery. This is the primary value driver.
2. **Multi-LLM enablement** (RQ-5): No path to multi-provider workflows without shared workflow state.
3. **Observability** (RQ-6): No path to workflow visualization without workflow definitions in the graph.

Token savings are a side effect, not a justification.

---

## Recommended Phasing

### Phase 1: Workflow Graph Foundation (effort: S, dependencies: none)
- Add 3 categories (`workflow`, `step`, `gate`) and 3 edge types (`HasStep`, `GatedBy`, `Requires`) to the graph
- Define JSON content schemas for step and gate entries as documented conventions
- Encode one existing protocol (design session) as workflow/step/gate entries
- Validate with graph traversal queries

### Phase 2: Workflow MCP Tools (effort: M, dependencies: Phase 1)
- Implement 5 workflow tools with advisory semantics (no enforcement)
- Add 3 SQL tables for mutable run state
- Convert the design session protocol to use `workflow_start` / `workflow_complete_step` / `workflow_status` flow
- Measure: does per-step delivery actually improve compliance vs. full protocol loading?

### Phase 3: Hybrid Execution Model (effort: M, dependencies: Phase 2)
- Build the thin coordinator script for sequential top-level execution with Unimatrix step delivery
- Test with design and delivery sessions
- Encode remaining protocols (delivery, bugfix, research) as workflow entries
- Measure: wall-clock time, compliance, quality vs. current swarm model

### Phase 4: Multi-LLM Routing (effort: M, dependencies: Phase 3)
- Extend coordinator to dispatch steps to Claude Code, Codex CLI, or Gemini CLI based on step `agent_type`
- Define routing rules (e.g., "code review" steps to Codex, "implementation" steps to Claude)
- Test with a real multi-provider delivery session

### Phase 5: Visual Workflow UI — Tier A (effort: L, dependencies: Phase 2, W2-1)
- ReactFlow + dagre read-only visualization
- WebSocket for real-time status
- Static assets served from Rust daemon in W2-1 container

### Deferred
- Visual editing (Tier B/C): Defer until Tier A proves value. 5-14 person-weeks incremental.
- Active dispatch (Model C from RQ-2): Defer indefinitely. The passive/guided model with external coordinator is sufficient and preserves product identity.
- MCP Apps as alternative UI delivery: Monitor the 2026-07-28 MCP RC. May supersede or complement the bundled web UI.

---

## Minimum Viable Version

The smallest useful increment is **Phase 1 + Phase 2 with a single workflow (design session)**.

This delivers:
- Workflow definitions stored in the knowledge graph (queryable, versionable, composable)
- Per-step instruction delivery via `workflow_start` / `workflow_complete_step` (70-80% SM protocol reduction)
- Run state tracking via `workflow_status` (observability without the UI)
- Gate criteria served as knowledge (the LLM or human evaluates; Unimatrix records)

This does NOT deliver:
- Multi-LLM routing (requires Phase 3-4)
- Visual UI (requires Phase 5)
- Full enforcement (by design — advisory semantics)
- Subagent bypass (requires Phase 3 coordinator)

The minimum viable version validates whether per-step delivery improves compliance without requiring the full coordinator or UI. If it does not improve compliance meaningfully, the project can stop at Phase 2 without wasted investment in Phases 3-5.

---

## Recommendation

**Proceed-with-narrowed-scope.**

Narrow the scope to the "workflow-literate knowledge engine" framing:
- Store workflow definitions in the graph (RQ-1: feasible, low effort)
- Serve per-step instructions and record execution state via MCP tools (RQ-2: feasible, advisory semantics)
- Build a thin external coordinator for multi-LLM dispatch (RQ-5: feasible, keeps Unimatrix as knowledge layer)
- Defer the visual UI to post-W2-1 (RQ-6: feasible but large effort, not on critical path)
- Do not cross into active orchestration, gate enforcement, or agent dispatching within Unimatrix (RQ-7)

The investment is justified primarily by compliance improvement (RQ-4) and multi-LLM enablement (RQ-5), not token savings (RQ-3). Phase the work so that each increment is independently valuable and the project can stop at any phase boundary without wasted effort.

---

## Unanswered Questions

*Merged from all 5 research tracks:*

1. **Claude Agent SDK queue-based dispatch**: Detailed API documentation for queue-based dispatch was not fully available during the RQ-5 investigation. Needed before Phase 4 implementation. *(Source: FINDINGS-MULTI-LLM)*
2. **Codex CLI MCP server mode tool surface**: Exact tools exposed when Codex runs as MCP server not fully documented. Relevant to Phase 4 agent-to-agent integration. *(Source: FINDINGS-MULTI-LLM)*
3. **Gemini CLI session persistence**: No session resume capability documented. Limits multi-step workflows with Gemini as provider. *(Source: FINDINGS-MULTI-LLM)*
4. **Cross-provider token cost comparison**: Needed for intelligent routing decisions but requires pricing analysis outside this scope. *(Source: FINDINGS-MULTI-LLM)*
5. **REST/GraphQL API surface for UI**: Exact API design for the visual UI (REST vs GraphQL, WebSocket contract) not in scope for feasibility study. Required before Phase 5. *(Source: FINDINGS-UI)*
6. **W2-1 static file serving**: Does the daemon already serve static files, or does this capability need to be added? *(Source: FINDINGS-UI)*
7. **Category allowlist impact**: Entry #3775 references `CategoryAllowlist` (crt-031) that may enforce valid categories. Adding `workflow`, `step`, `gate` may require allowlist configuration. *(Source: FINDINGS-GRAPH)*
8. **Session state convergence**: `SessionState` in `session.rs` tracks `current_phase`, `rework_events`, `current_goal`. The proposed `workflow_runs` tables overlap. The relationship must be resolved: either `SessionState` becomes a read-through cache of `workflow_runs`, or is deprecated in favor of persistent workflow state. *(Source: FINDINGS-GRAPH)*
9. **Template variable resolution security**: Instruction templates with `{variable}` placeholders resolved from the `context` parameter create template injection risk if context contains user-supplied values. *(Source: FINDINGS-GRAPH)*

---

## Out-of-Scope Discoveries

*Merged and deduplicated from all 5 research tracks:*

1. **MCP Apps (2026-07-28 RC)**: MCP servers can ship interactive HTML UIs in sandboxed iframes. Could serve workflow visualization as an MCP App rather than bundling a standalone web UI, potentially reducing or eliminating the Phase 5 build effort. *(Sources: FINDINGS-MULTI-LLM, FINDINGS-UI)*

2. **Agent definition redundancy optimization**: `uni-validator` (12,554 bytes) contains 4 gate check sets but each spawn needs only one (~3,000 bytes). Same for `uni-tester` and `uni-risk-strategist`. Splitting definitions or delivering role-specific instructions would reduce per-spawn overhead by ~67%. Independent of workflow harness. *(Source: FINDINGS-ECONOMICS)*

3. **CLAUDE.md redundant loading**: ~1,216 tokens loaded into every agent context. 8+ subagents in delivery = ~10,000 tokens of redundancy. Could be eliminated if Unimatrix controls per-step instructions. *(Source: FINDINGS-ECONOMICS)*

4. **Quick Reference sections as compensatory bloat**: Every protocol's Message Map (~40-60 lines) exists because SM loses track of earlier sections. Unnecessary in per-step delivery model. *(Source: FINDINGS-ECONOMICS)*

5. **Gate validator replacement**: Currently depth-2 validator agents reading artifacts. If Unimatrix stores gate criteria, lightweight structural checks (file existence, cargo commands) could run without agent spawning — eliminating 3 subagent spawns and ~13,000 tokens per delivery session. *(Source: FINDINGS-ECONOMICS)*

6. **Workflow versioning via supersession chains**: Workflow/step/gate entries support supersession naturally. Active runs continue on original version; new runs pick up latest. Needs validation during design. *(Source: FINDINGS-GRAPH)*

7. **Protocol versioning gap**: No versioning mechanism for workflow definitions if they move into the graph. The knowledge integrity chain (hash-chained corrections, confidence evolution) is designed for independently-evolving entries; a protocol is an atomic unit whose steps cannot be independently superseded. Needs architectural resolution. *(Source: FINDINGS-VISION)*

8. **Vision guardian process update**: If the boundary moves, the vision guardian agent, vision document, and README all need coordinated updates to prevent the guardian from flagging workflow features as vision violations. *(Source: FINDINGS-VISION)*

9. **Parallel step execution model limitation**: MCP is strictly request-response. Workflow engine can declare parallelism but cannot enforce it — the LLM decides how to execute parallel-eligible steps. *(Source: FINDINGS-GRAPH)*

10. **Dagster's Rust/WASM layout**: Ported graph layout to Rust/WASM for 10k+ asset graphs. Worth noting for future knowledge graph visualization beyond workflow DAGs. *(Source: FINDINGS-UI)*

11. **LangGraph Studio time-travel debugging**: Rewind to any checkpoint, edit state, fork execution. Requires checkpoint-based execution model — compatible with the proposed `workflow_runs` tables. *(Source: FINDINGS-UI)*

12. **Anthropic Managed Agents**: Hosted REST API alternative to CLI dispatch for Claude steps. Potentially relevant to Phase 4. *(Source: FINDINGS-MULTI-LLM)*

13. **n8n bidirectional MCP integration**: Could serve as a visual workflow editor dispatching to Unimatrix-managed agents, potentially offering an alternative to building a custom UI. *(Source: FINDINGS-MULTI-LLM)*

14. **Temporal's durable execution pattern**: "Deterministic orchestrator + non-deterministic workers" maps directly to "Unimatrix workflow graph + LLM step execution." Activities must be idempotent — a design constraint for workflow step definitions. *(Source: FINDINGS-MULTI-LLM)*
