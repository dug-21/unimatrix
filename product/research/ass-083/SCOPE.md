# ASS-083 — Feasibility & Landscape: A Human-Facing UI for Personal Cloud

**Status:** SCOPE (Phase 1 complete — ready for a research session)
**Goal:** `personal-cloud` (primary) · `proactive-delivery` + `self-learning` (secondary — the lens & graph surfaces)
**Type:** Feasibility / landscape assessment (not an implementation spike)
**Phase:** Matrix (`mtx`) — UI & dashboards

---

## The question

Today Unimatrix has two surfaces, both machine-facing: agents *retrieve* knowledge (search/lookup/get) and the engine *delivers* it proactively (injection/briefing). This spike investigates a **third, human-facing surface — a UI on the personal-cloud deployment** — and answers, concretely:

> What is **easy**, what is **hard**, and what is currently a **dream**, across a UI that lets a human (1) view each project's knowledge graph, (2) get a real-time lens on what's happening in each project, and eventually (3) highlight regions of the graph and (4) edit nodes — across **multiple projects, selected one at a time** from the set of known projects.

**Decided constraint (de-risks the hardest workstream):** the UI selects **one active project at a time** from the list of known/registered projects (a project-switcher). An **aggregated cross-project view is explicitly NOT required.** Every view is therefore a normal single-slug session — no cross-project queries, joins, or merged graphs. This keeps per-project data isolation (C3) fully intact and collapses the multi-project problem from "cross-project RBAC/aggregation" down to "enumerate the known projects + authenticate to the selected one."

The deliverable is a ranked feasibility map + a recommended phasing, NOT a design. It tells us what to build first and what to spike deeper before committing.

## Why it matters to the vision

- The vision frames Unimatrix as a knowledge engine with two surfaces. A **read/observability UI is a natural, on-vision extension** — it surfaces what the engine already knows and is doing ("the lens"). It does not make Unimatrix an orchestration engine.
- **Editing via UI is the delicate end.** Every knowledge mutation today is *attributed*, *hash-chained* (Principle 1), and *audit-logged* (Principle 2), and updates go through `context_correct` to preserve provenance. A human editing through a UI raises real questions: who is the attributed author? does it respect the correction chain? does it threaten hash-chain integrity? The spike must treat editing as a provenance/integrity problem, not a CRUD form.
- **Project-switching lightly touches a deliberate decision.** The current model is 1-client:1-project with a per-project bearer token (vnc-034); cross-project RBAC was a *door deliberately left open*, not walked through. The decided one-project-at-a-time constraint keeps each view a clean single-slug session — but a switcher that can *reach* multiple projects still implies the UI can hold or obtain credentials for more than one. That is a far weaker cross-project surface than aggregation, and W2 examines exactly how lightly it can be done.

## Known constraints & prior art (build on these — do not re-derive)

- **Principle 6 — single binary, zero required infra.** The client is an adapter, not infrastructure. The strongest-fit UI ships as **static assets embedded in the existing binary**, served over the **same HTTPS port + bearer** (personal-cloud transport). A separate frontend build/deploy is a vision-tax to be justified, not assumed.
- **Principle 3 — capability checks at the service layer**, after identity resolution, regardless of transport (UDS/stdio/HTTP/OAuth). A UI is just another caller; its identity must resolve to capabilities the same way.
- **Principles 1, 2, 8 — hash chain immutable, audit log append-only, no secrets in any DB.** Hard boundaries for the editing workstream.
- **`context_graph`** already exposes typed depth-1 edges and graph traversal (PPR / `graph_expand`); **vnc-037 (#754)** surfaces an entry's ranked typed edges on `context_get`. These are candidate read-backends for graph viz — assess fit before proposing new query surface.
- **Real-time substrate already exists in part:** the server-side session transcript buffer (vnc-025 #670), `context_cycle_review`, and the observation pipeline (hooks/webhooks feeding the learning layer). The "real-time lens" should be assessed as a *consumer of existing event sources*, not a new pipeline.
- **Auth surface:** HTTPS + bearer, `rmcp allowed_hosts`, per-slug routing (C3). Multi-project UI auth must be reasoned about against this.

## Bounded investigation (workstreams)

**W1 — Packaging & delivery model (the easy↔hard axis).**
Can the UI ship as static assets embedded in the single binary, served over the existing HTTPS+bearer surface, with zero new infra (Principle 6)? Compare: embedded-static vs separate SPA deploy vs no-build server-rendered. Output: the delivery option that preserves "one container, one bearer, one command," with the cost of each.

**W2 — Project selection / switching (NOT aggregation).**
Given the decided constraint (one active project at a time, no aggregated view), this reduces to two concrete questions. (a) **Enumeration:** how does the UI discover the set of known/registered projects to populate a switcher — is there an existing surface that lists registered slugs, or is a minimal "list projects" read needed? (b) **Authentication on switch:** today auth is per-project bearer + 1-client:1-project (vnc-034); when the operator switches to project X, where does X's credential come from — one operator-level identity that resolves to per-project access, N per-project bearers held by the UI, or per-switch re-auth? Output: feasible enumerate+switch models ranked by how much they disturb the per-project bearer model, and an explicit statement of which (if any) require an operator-level identity above the per-project bearer. Note: full cross-project RBAC/aggregation is **out of scope** by the decided constraint — flag it only if even the switcher unavoidably needs it.

**W3 — Graph visualization, read-only (the core dream).**
Can `context_graph` / vnc-037 edge surfaces back a graph view as-is, or is new read query surface needed? What viz approach and library class fits a typed, evolving knowledge graph, and at what **scale ceiling** (100s vs 1000s vs 10k+ nodes) before it needs server-side filtering/clustering? Output: a read-only-graph feasibility verdict + scale limits + the minimal query surface required.

**W4 — Real-time lens ("what's happening in each project").**
What event sources already exist (transcript buffer, observation pipeline, cycle events) and could feed a live per-project activity view over the same HTTPS surface (SSE/websocket)? What's the cheapest path to a useful live view, and what would it show? Output: a real-time-lens feasibility verdict grounded in existing event sources, with the transport recommendation.

**W5 — Highlighting & node editing (the delicate, long-term end).**
Region-highlighting: client-only view state, or does it imply persisted saved-views? Node editing: how does a human UI edit route through `context_correct` (provenance), AUDIT_LOG attribution (P2), and hash-chain integrity (P1) without breaking them? Who is the attributed identity for a human edit? Output: an explicit integrity/provenance assessment + whether editing is a phase-2+ capability or a non-starter without new identity machinery.

**W6 — Synthesis: easy↔hard ranking + phasing.**
Rank every capability above into effort tiers (easy / moderate / hard / dream-not-yet) with the *reason* for each tier, then recommend an MVP and a phasing order (hypothesis to test, not to assume: project-switcher + read-only lens → read-only graph → highlighting → editing). Name any capability that should become its own deep-dive spike (W5 editing/provenance is now the likely sole candidate; W2 is expected to be tractable under the one-project-at-a-time constraint).

## Expected output (FINDINGS.md)

1. **Feasibility map** — each capability tagged easy / moderate / hard / dream, with the determining constraint named.
2. **Delivery recommendation** — how the UI ships without breaking Principle 6 (single binary, zero infra).
3. **Project enumerate + switch verdict** — feasible models for listing known projects and authenticating on switch (one active project at a time), ranked by disturbance to the per-project bearer model; whether any of them unavoidably needs an operator-level identity (and a draft follow-on scope question if so).
4. **Recommended MVP + phasing order**, with the cheapest first useful slice called out.
5. **Vision-alignment call** — where the UI is on-vision (observability/lens) vs where it risks scope creep or integrity compromise (editing, cross-project), stated plainly.
6. **Follow-on spikes identified** (expected: cross-project identity/RBAC; graph-viz tech selection).

## Explicitly out of scope

- Visual/UX design, mockups, component choices, or branding.
- Any implementation, prototype code, or library lock-in (name *classes* of approach, not a final pick).
- Orchestration/agent-control features — Unimatrix is not an orchestration engine; a UI does not change that.
- Final resolution of the cross-project RBAC model — this spike *scopes* that question; it does not settle it.

---

## Extension (2026-06-22): The editable workflow-authoring graph

**Why this extension.** The original spike deferred all node-editing to a gated phase 4 (W5), because it assumed the editing target is a hash-chained Unimatrix knowledge entry whose human attribution is unsolved. The human has identified the **killer use case that justifies building the UI**, and it has a *different* editing target:

> View the workflow as a graph and edit its nodes. Today a workflow is defined by a chain of flat `.md` files in a directory tree — a workflow/protocol references **agents** (`.claude/agents/uni/*.md`), which reference **skills** (`.claude/skills/*`) and **rules** (`.claude/rules/`). Manipulating this chain across flat files is hard. A graph view that shows the linked entities, lets the human navigate them visually, and edits the nodes in place would be a step-change in authoring ergonomics — and is the concrete reason to build the UI.

This reframes editing: the nodes are **git-tracked flat files**, not hash-chained DB entries. Git already supplies authorship/provenance, so the W5 identity blocker may not apply to this class of node. The extension investigates this in two layers, exactly as the human framed it: **(1) the generic capability, then (2) the application to our own protocols/agents/skills.**

The deliverable remains directional (feasibility map + phasing) — append to the existing `FINDINGS.md`; do not overwrite the validated W1–W6 verdicts, but explicitly reconcile any that this use case changes (notably the W5/W6 "editing is phase-4, gated" verdict).

### Additional workstreams

**W7 — Generic editable-entity-graph capability (the reusable form).**
Define the minimal generic model for "a graph of linked entities whose nodes are editable": node = an entity with an addressable **source-of-truth** + a type; edge = a typed reference between entities; node-edit = a safe **write-back** to that source. Can this capability be defined **independent of the backing store** (Unimatrix knowledge DB vs the filesystem), behind a single "graph node provider" contract — `enumerate(nodes)`, `resolve(edges)`, `read(node)`, `write(node, provenance)`? Output: the generic node/edge/provider model and the contract a backend must satisfy, so the same UI can serve both knowledge nodes (read-only for now) and workflow-artifact nodes (editable).

**W8 — Workflow-artifact graph derivation (the application data model).**
How are the workflow→agent→skill→rule links represented in `.claude/` **today**? Are references explicit (frontmatter keys, IDs, the CLAUDE.md routing table) or implicit (name mentions, directory convention, prose)? What parsing/indexing is required to build a **reliable typed graph** from these artifacts, and is link extraction deterministic or heuristic? Identify the actual edge types (protocol→agent, agent→skill, agent→rule, skill→skill, etc.). Output: the derivation approach, the edge-type taxonomy it yields, and an honest read on fidelity/maintainability (e.g. does it break when an agent renames a skill).

**W9 — Editing & write-back provenance for filesystem-backed nodes (does git dissolve the W5 blocker?).**
W5 gated editing on missing human attribution for hash-chained entries. The workflow artifacts are git-tracked files. **Does git's commit authorship already provide the attribution W5 found missing?** Compare write-back targets for the workflow graph: (a) **edit the `.md` files directly** (filesystem write + git commit = native provenance/audit/rollback); (b) **promote workflow definitions to first-class Unimatrix entities** (hash-chain + `context_correct`, inherits the W5 identity gate); (c) **hybrid** (graph/index in Unimatrix, source-of-truth stays the files). For (a): what are the integrity hazards specific to editing repo files through a server UI — concurrent edits, validation before write, staying inside the repo, not corrupting frontmatter, and whether the server should write files at all vs emit a diff/PR. Output: a write-back model recommendation and an explicit verdict on whether **editing this class of node is achievable before the human-identity spike** (i.e. whether it can jump ahead of phase 4).

**W10 — Re-tiered feasibility + phasing for the editable-workflow-graph use case.**
Re-rank specifically for this use case against the original feasibility map: where does "view + edit the workflow graph" land (easy / moderate / hard / dream), and what is the MVP slice and phasing for *this* feature? Does it **reorder** the original phasing, which deferred all editing to phase 4? Name what must still become a follow-on spike. Output: a use-case-specific feasibility verdict, an MVP, a phasing that reconciles with (or explicitly revises) the original W6 order, and the follow-on spike list updated.
