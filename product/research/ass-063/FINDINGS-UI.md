# FINDINGS: Visual Workflow UI Framework Survey (RQ-6)

**Spike**: ass-063
**Date**: 2026-05-29
**Approach**: evaluation + investigation
**Confidence**: validated

---

## Findings

### Q: Technology options for a canvas-style workflow visualization and editor. Target: bundled in the W2-1 container image, served alongside the daemon -- no separate infrastructure for the developer to manage. Evaluate: (a) read-only visualization of workflow state, (b) interactive workflow definition editing, (c) real-time execution status overlay. Survey existing open-source canvas/graph-editor frameworks for fit. What's the realistic build effort for each tier?

---

## 1. Framework Survey

### 1.1 Framework Comparison Matrix

| Framework | License | Unpacked Size | Graph/DAG Native | Custom Nodes | Real-time Updates | Interactive Editing | Layout Engine | Maturity | Active |
|---|---|---|---|---|---|---|---|---|---|
| **ReactFlow (@xyflow/react)** | MIT | 1.19 MB | Yes | Yes (React components) | Yes (state-driven) | Yes | dagre, elkjs (external) | High (2019, v12) | Yes (36.8k stars) |
| **tldraw** | Commercial ($6k/yr) | 12.8 MB | Via starter kit | Yes (custom shapes) | Yes (reactive signals) | Yes | None built-in | High (2021, v5.0) | Yes (47.5k stars) |
| **Excalidraw** | MIT | N/A | No | Limited | No | Freeform only | None | High | Yes |
| **Cytoscape.js** | MIT | 5.7 MB | Yes | Limited (CSS styling) | Yes (events) | Partial | dagre, cose, elk (plugins) | Very High (2012) | Yes (11k stars) |
| **JointJS** | MPL-2.0 | 8.3 MB | Yes | Yes (SVG templates) | Yes | Yes | Built-in basic | Very High (2010) | Yes |
| **Rete.js** | MIT (core) | 226 KB (core) | Yes | Yes (multi-framework) | Yes | Yes | elk (plugin) | Medium (v2.0.6) | Moderate (12.1k stars) |
| **Mermaid.js** | MIT | 76.3 MB | Yes | No | No | No (text-to-diagram) | dagre, elk | High | Yes |
| **D3.js + d3-dag** | ISC/MIT | 593 KB (d3-dag) | Yes (d3-dag) | Manual (SVG) | Manual | Manual | d3-dag built-in | Very High (d3) | Moderate |
| **Litegraph.js** | MIT | N/A (archived) | Yes | Yes | Yes | Yes | None built-in | Low (archived Aug 2025) | No |
| **Graphviz (viz.js WASM)** | MIT | 5.0 MB (incl WASM) | Yes | No | No | No | dot/neato/etc | Very High | Yes |
| **Vue Flow** | MIT | ~1 MB | Yes | Yes | Yes | Yes | dagre, elk (external) | Medium | Yes |
| **Svelte Flow** | MIT | ~0.8 MB | Yes | Yes | Yes | Yes | dagre, elk (external) | Medium | Yes (official xyflow) |

### 1.2 Layout Engine Comparison

| Engine | Unpacked Size | License | DAG Quality | Speed | Configurability |
|---|---|---|---|---|---|
| **@dagrejs/dagre** | 1.19 MB | MIT | Good | Fast | Low |
| **elkjs** | 8.0 MB | EPL-2.0 | Excellent | Slower (async) | Very High |
| **d3-dag** | 593 KB | MIT | Excellent | Medium | Medium |

**Recommendation for layout**: Use **dagre** for initial implementation. Graduate to **elkjs** if complex layout requirements emerge.

---

## 2. Detailed Framework Evaluations

### 2.1 ReactFlow (@xyflow/react) -- RECOMMENDED

**Strengths:**
- Purpose-built for node-based workflow editors. The exact use case.
- Custom nodes are plain React components -- render anything inside a node.
- Built-in: drag, zoom, pan, multi-select, keyboard shortcuts, minimap, controls panel.
- 8.55M weekly npm installs. Used by Stripe, Zapier, Typeform.
- SSR/SSG support documented.
- MIT license with no commercial restrictions. "Pro" tier is optional support/examples.
- Actively maintained: v12.10.2 as of May 2026.

**Weaknesses:**
- React-only.
- No built-in layout engine -- requires dagre or elkjs as external dependency.
- No built-in virtualization for very large graphs (>1000 nodes). Irrelevant for workflow DAGs (5-50 nodes).

**Container deployment:** Build with Vite to static assets. Serve from Rust daemon. No Node.js runtime needed in production.

### 2.2 Cytoscape.js -- STRONG ALTERNATIVE FOR TIER A

**Strengths:**
- Framework-agnostic.
- Best-in-class graph theory library with built-in algorithms.
- ~112 KB gzipped.
- MIT license, actively maintained.

**Weaknesses:**
- Custom node rendering is CSS-based, not component-based.
- Not designed for interactive editing. Editing features require significant custom code.

**Verdict:** Excellent for Tier A (read-only). Poor fit for Tier B/C without substantial custom development.

### 2.3 tldraw -- NOT RECOMMENDED

**Disqualifying factors:**
- **Commercial license required: $6,000 USD/year.** Hard constraint for BSL-1.1 project.
- 12.8 MB unpacked -- 10x larger than ReactFlow.
- Overkill for structured DAG workflows.

### 2.4 Excalidraw -- NOT RECOMMENDED

**Disqualifying factors:**
- Freeform whiteboard, not structured graph editor. No programmatic DAG rendering or automatic layout.
- No custom node types with structured data.

### 2.5 JointJS -- VIABLE BUT COMPLEX

- MPL-2.0 license: weakly copyleft. Compatible with bundling but requires attention.
- Advanced features (keyboard shortcuts, undo/redo) in commercial JointJS+ tier.
- 8.3 MB unpacked. API is lower-level than ReactFlow.

### 2.6 Mermaid.js -- TIER A ONLY, WITH SEVERE CAVEATS

- Read-only. No interactive editing, no real-time state overlay.
- 76.3 MB unpacked -- impractical for container-bundled deployment.
- Potential use as lightweight export format only.

---

## 3. Prior Art: How Existing Tools Visualize Agent Workflows

### 3.1 LangGraph Studio
- Browser-based React app. Graph mode shows nodes with arrows for control flow.
- **Key feature:** Time-travel debugging -- rewind to any checkpoint, edit state, fork execution.

### 3.2 n8n
- Vue 3 SPA built with Vite. **Canvas built on Vue Flow** (xyflow family).
- Validates that the xyflow family is production-viable for workflow editors at massive scale (50k+ GitHub stars).

### 3.3 Windmill
- Svelte + TypeScript frontend. Custom canvas implementation.
- Code-first architecture -- every visual node is backed by executable code.

### 3.4 Dagster (Dagit)
- React + Next.js. SVG rendering with foreignObject. **Uses dagre for layout.**
- Solved 10k+ asset visualization with viewport-aware rendering, hiding edges during pan, IndexedDB caching, and Rust/WASM layout.

### 3.5 ComfyUI
- Vue.js frontend. Originally used Litegraph.js (now absorbed into monorepo).
- Colored-connection pattern for type differentiation worth adopting.

### 3.6 Pattern Summary

All successful workflow UIs share:
1. **Graph canvas + detail sidebar**: Main view is DAG; click node opens properties panel.
2. **Status by node color/border**: Green = complete, blue = running, red = failed, gray = pending.
3. **Automatic layout with manual override**: dagre/elk computes initial positions; users can drag.
4. **Connection ports**: Input/output ports on nodes.
5. **Minimap**: For navigation in larger workflows.

---

## 4. Three-Tier Build Effort Estimation

Based on **ReactFlow** as recommended framework:

### Tier A: Read-Only Visualization

**Scope:** React SPA with ReactFlow, custom node components, dagre auto-layout, WebSocket for real-time status, sidebar detail panel, static assets served from Rust daemon.

**Build effort: 2-3 person-weeks**

Breakdown:
- Project scaffolding (Vite + React + ReactFlow): 0.5 days
- Custom node components (3-4 types): 2-3 days
- dagre layout integration: 1 day
- WebSocket client for status updates: 1-2 days
- Sidebar detail panel: 1-2 days
- Status overlay: 1-2 days
- Static build pipeline + Docker integration: 1 day
- Testing and polish: 2-3 days

### Tier B: Interactive Editing

**Scope:** Everything in Tier A + node palette, connect/delete, property editor, undo/redo, save to Unimatrix, validation.

**Build effort: 5-7 person-weeks** (cumulative)

### Tier C: Full Execution Overlay

**Scope:** Everything in Tier A + B + real-time execution tracking, per-step log streaming, gate approval UI, execution timeline, workflow history/replay, step retry.

**Build effort: 10-14 person-weeks** (cumulative)

---

## 5. Deployment Architecture

```
[W2-1 Docker Container]
  |
  +-- Rust Daemon (unimatrix-server)
  |     |-- MCP endpoint (existing)
  |     |-- REST/GraphQL API (new, for UI)
  |     |-- WebSocket endpoint (new, for real-time)
  |     +-- Static file server (serves UI assets)
  |
  +-- /ui/ (static directory)
        |-- index.html
        |-- assets/
        |     |-- app.[hash].js   (~200-400 KB gzipped)
        |     +-- app.[hash].css
        +-- favicon.ico
```

**Estimated bundle sizes (gzipped, production build):**
- React + ReactDOM: ~45 KB
- @xyflow/react: ~80-100 KB
- dagre: ~30 KB
- Application code: ~30-50 KB
- CSS: ~10-20 KB
- **Total: ~200-250 KB gzipped** (Tier A)

---

## 6. Incremental Strategy

**Can you start with Tier A and graduate without rewriting?**

**Yes, with ReactFlow.** ReactFlow's data model (nodes array + edges array) is the same for read-only and interactive modes. To go from read-only to interactive, add event handlers and UI components. Custom node components built for Tier A reuse unchanged in Tier B.

**Why not start with Mermaid for Tier A?** Two completely different rendering pipelines, no reusable components, Mermaid integration becomes throwaway code. Incremental cost of starting with ReactFlow is ~2-3 extra days in Tier A but saves a complete rewrite at Tier B.

---

## 7. Frontend Framework Decision

ReactFlow requires React. The project currently has no frontend framework.

**Arguments for React:** Largest ecosystem, ReactFlow is strongest fit, n8n and Dagster validate React-ecosystem choices.

**Alternatives:** Svelte Flow (official xyflow, smaller bundle), Vue Flow (what n8n uses, proven at scale), Cytoscape.js (framework-agnostic but sacrifices editing).

**Recommendation:** React + ReactFlow unless existing preference for different framework.

---

## Unanswered Questions

1. **API surface for UI**: REST or GraphQL for CRUD, WebSocket for real-time -- exact design not in scope.
2. **W2-1 static file serving**: Does the daemon already serve static files?
3. **Offline/disconnected mode**: Affects architecture but not framework choice.
4. **Multi-user/multi-session**: Affects WebSocket architecture but not framework choice.

---

## Out-of-Scope Discoveries

1. **Dagster's Rust/WASM layout**: Ported graph layout to Rust/WASM for 10k+ nodes. Worth noting for future knowledge graph visualization.
2. **tldraw's @tldraw/mermaid**: Converts Mermaid diagrams to native tldraw shapes.
3. **LangGraph Studio time-travel debugging**: Rewind/fork execution. Requires checkpoint-based execution.
4. **MCP Apps (from R4 findings)**: Unimatrix could serve workflow visualization as an MCP App in sandboxed iframes rather than bundling a standalone web UI.

---

## Recommendations Summary

- **Framework**: **ReactFlow (@xyflow/react)** with **dagre** for layout. MIT license, 1.2 MB, purpose-built for node-based workflow editors.
- **Eliminated**: tldraw (commercial $6k/yr), Excalidraw (freeform), Mermaid (read-only, 76 MB), Litegraph.js (archived).
- **Tier A**: 2-3 person-weeks. Read-only visualization with real-time status.
- **Tier B**: 5-7 person-weeks cumulative. Interactive editing.
- **Tier C**: 10-14 person-weeks cumulative. Full execution overlay with log streaming and gate approval.
- **Strategy**: Start with Tier A using ReactFlow. Incremental upgrade to Tier B/C without rewrite.
- **Deployment**: Static assets (~200-250 KB gzipped), served by Rust daemon, no Node.js in production.
