# ASS-063: Unimatrix as Protocol/Workflow Harness — Feasibility Study

**Date**: 2026-05-28
**Type**: Feasibility / Impact study
**Trigger**: Operational pain — LLM protocol compliance, subagent quality ceiling, multi-LLM coordination need, workflow opacity

---

## Problem Statement

Agentic workflows today are governed by text-file protocols that the LLM loads, interprets, and self-navigates. This has three compounding problems:

1. **Token cost and compliance drift**: Protocols consume significant context. Under pressure, LLMs skip steps, truncate outputs, or invent shortcuts. The protocol is advisory — enforcement is aspirational.
2. **Subagent quality ceiling**: Claude degrades subagent capabilities at depth. Multi-agent swarms lose quality where the protocol demands it most — at specialist boundaries.
3. **Single-LLM lock-in**: Workflows hardcode one provider's agent model. Using Codex for review while Claude implements requires manual coordination with no shared workflow state.

The hypothesis: Unimatrix's typed graph and MCP tool surface could serve as the execution backbone — replacing advisory protocol files with graph-stored workflow definitions that Unimatrix actively controls. The LLM receives per-step instructions rather than loading full protocols, and Unimatrix enforces sequencing, gates, and agent routing.

A secondary hypothesis: a visual UI (canvas-style, bundled in the W2-1 container) could make workflows inspectable, editable, and observable in real time.

---

## Research Questions

### RQ-1: Graph as Workflow Model
Can the existing typed graph (categories, typed edges, traversal modes) represent workflow DAGs — steps, gates, agent assignments, input/output contracts? What new categories, edge types, and entry schemas are needed? How do workflow definitions compose (e.g., design workflow reuses scoping steps from research workflow)?

### RQ-2: Workflow Execution Semantics
What does "Unimatrix controls the workflow" mean concretely? Options range from passive (LLM queries for next step) to active (Unimatrix dispatches instructions). What MCP tools are needed (e.g., `workflow_next`, `workflow_complete_step`, `workflow_status`)? How are step completion, gate pass/fail, rework loops, and workflow abort represented and enforced?

### RQ-3: LLM Token Reduction (Quantitative)
Measure current protocol token cost per session type (design, delivery, bugfix, research) from recent transcripts. Estimate reduction from per-step instruction delivery vs. full protocol loading. Determine whether the reduction is material enough to justify the infrastructure investment.

### RQ-4: Subagent Bypass Path
How does Unimatrix-controlled workflow avoid Claude's subagent limitations? Does sequential top-level execution (Unimatrix tells the agent what to do next) produce better results than spawned subagents following an embedded protocol? What are the trade-offs — latency, context continuity, ability to parallelize independent work?

### RQ-5: Multi-LLM Routing
Technical feasibility of dispatching workflow steps to different LLM providers. What does the interface look like per provider (MCP for Claude, API for Codex CLI/Gemini CLI)? How does artifact and context state transfer between providers? What's the minimum viable routing model — and does it require Unimatrix to initiate LLM sessions, or just inform them?

### RQ-6: Visual Workflow UI
Technology options for a canvas-style workflow visualization and editor. Target: bundled in the W2-1 container image, served alongside the daemon — no separate infrastructure for the developer to manage. Evaluate: (a) read-only visualization of workflow state, (b) interactive workflow definition editing, (c) real-time execution status overlay. Survey existing open-source canvas/graph-editor frameworks (ReactFlow, tldraw, Excalidraw, xyflow, etc.) for fit. What's the realistic build effort for each tier?

### RQ-7: Vision Boundary
The current vision explicitly states "Unimatrix is not an orchestration engine." Does this pivot change the product identity, or is there a coherent hybrid — a knowledge engine that also understands workflow structure and can guide execution through it? What is gained and what is lost? Is "workflow-aware knowledge engine that can also serve as workflow harness" a natural extension of "workflow-aware knowledge engine," or a different product?

---

## Approach

Breadth-first investigation across all seven RQs. Each should produce:
- Feasibility assessment: **feasible** / **feasible-with-caveats** / **not feasible**
- Effort estimate (T-shirt: S/M/L/XL)
- Dependencies on existing or planned work
- Key risks or unknowns that would require deeper investigation

RQ-3 should include actual token measurements from recent session transcripts.
RQ-6 should include a concrete framework survey with selection criteria.
RQ-5 should distinguish between what's achievable with MCP alone vs. what requires custom integration per provider.

---

## Output

`FINDINGS.md` with:
1. Per-RQ feasibility assessment
2. Combined feasibility matrix
3. Recommended phasing — which elements to pursue first, which to defer
4. Minimum viable version definition — what's the smallest useful increment?
5. Explicit recommendation: proceed / proceed-with-narrowed-scope / defer

---

## Constraints

- This is a feasibility study, not a design. No implementation artifacts.
- The investigation should be honest about build cost — especially for the UI surface (RQ-6), which is entirely new territory for the project.
- Multi-LLM routing (RQ-5) should assess what's possible today with existing provider tooling, not what would require provider cooperation.
- The W2-1 container is the assumed deployment vehicle for any UI component. The UI investigation should assume it ships post-W2-1, bundled in the container.

---

## Vision Alignment

This research evaluates whether the vision boundary should expand. The investigation is warranted by real operational pain. "The boundary should not move" is a valid finding. The research does not presuppose the answer.
