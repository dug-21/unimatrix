# ASS-066: Unimatrix as Session Host — Observation Fidelity, Product Surface, and Vision Implications

## Question

Can Unimatrix host agent sessions (via `claude -p`, Claude SDK, or equivalent programmatic interfaces) to achieve full observation fidelity remotely — and if so, what does this mean for the product vision, deployment model, and strategic goals?

## Why It Matters

The remote observation problem (vnc-024) revealed a deeper architectural tension: Unimatrix's learning pipeline requires session-correlated telemetry (cycle/phase association, PPR weighting, retrospective analysis), but the observation channel that provides session identity (UDS hooks with process lineage) doesn't exist remotely. HTTP-based alternatives lose session correlation fidelity, which degrades the intelligence pipeline.

Hooks were designed as a way to observe agent sessions from outside. Hosting sessions eliminates the outside — Unimatrix becomes the substrate agents run on, and observation is a natural consequence of hosting. Session identity is trivial (Unimatrix created the session), I/O visibility is complete (Unimatrix owns the process), and injection is natural (Unimatrix controls the context).

This is a potential vision evolution from "knowledge engine that understands workflow context" to "knowledge-aware runtime for agentic workflows." That evolution needs investigation before commitment.

## Bounded Questions

### Q1: What programmatic session interfaces exist for target clients?

For each of Claude Code, Codex CLI, and Gemini CLI:
- What is the programmatic/pipe/SDK interface? (`claude -p`, `claude --sdk`, Codex API, Gemini API)
- Can you spawn a session, pipe prompts, and capture responses?
- Can you intercept or observe tool calls made during the session?
- Can you inject system prompt content or modify context between turns?
- Can you capture session lifecycle events (start, stop, compaction)?
- What is the authentication/billing model? (API keys, OAuth, CLI auth)

Focus depth on Claude Code (`claude -p` and Claude Agent SDK). Survey-level for Codex and Gemini.

### Q2: What observation fidelity does session hosting provide vs. hooks?

Compare observation signals available via session hosting against the current UDS hook path:

| Signal | UDS Hooks (current) | Session Host (proposed) |
|---|---|---|
| Session lifecycle (start/stop) | ✓ | ? |
| Tool call observation (pre/post) | ✓ | ? |
| Prompt content (UserPromptSubmit) | ✓ | ? |
| Context compaction (PreCompact) | ✓ | ? |
| Subagent spawn/stop | ✓ | ? |
| Session identity (for cycle/phase correlation) | ✓ (process lineage) | ? |
| Non-Unimatrix MCP tool calls | ✓ | ? |
| Transcript content | ✓ (file access) | ? |

Identify any signals that hooks provide but session hosting cannot, and vice versa.

### Q3: What is the minimum viable session host?

Design the simplest implementation that achieves observation parity with UDS hooks:
- Is it a wrapper script around `claude -p`?
- A new `unimatrix run` subcommand?
- An SDK-based agent runner?
- What dependencies does it introduce (API credentials, model access, billing)?
- What is the estimated implementation effort?

Distinguish between "minimum viable" (observation parity) and "full vision" (knowledge-aware runtime with injection, proactive delivery, etc.).

### Q4: What are the product surface implications?

- Does Unimatrix now need API credentials? How does that change the deployment model?
- Does the local story change, or is session hosting remote-only?
- Does this compete with Claude Code's own UX? Is that sustainable?
- Does this create model coupling (Anthropic-only), or can the runtime be model-agnostic?
- What happens to the existing MCP server surface — does it coexist, get subsumed, or become secondary?
- How does this affect the domain-agnostic goal — does hosting tighten or loosen domain coupling?

### Q5: What does this mean for the vision?

If the investigation supports session hosting, draft a candidate vision evolution:
- What changes in the vision statement?
- What changes in the strategic goals?
- What is the relationship between knowledge engine (current) and session runtime (proposed)?
- Is this an additive layer (runtime ON TOP OF knowledge engine) or a transformation (knowledge engine BECOMES runtime)?
- What is explicitly NOT changing (knowledge graph, confidence system, intelligence pipeline internals)?

Do not write the final vision statement — present options and trade-offs for human decision.

### Q6: What is the impact on vnc-024 and the remote observation roadmap?

- If session hosting is viable, does vnc-024 (hook configuration for remote) still matter?
- Is there a transitional architecture (hooks for local, session hosting for remote)?
- What is the sequencing: investigate first then build, or can vnc-024 proceed in parallel as a fallback?

## Approach

**Investigation + evaluation.** External research on client programmatic interfaces (Q1), codebase analysis for observation signal mapping (Q2), architectural sketching (Q3-Q4), vision analysis (Q5-Q6).

**Breadth: `code+ecosystem` (exhaustive).** Deep investigation on all three target clients — Claude Code, Codex CLI, and Gemini CLI. Each client gets full-depth evaluation of programmatic interfaces, observation capabilities, and hosting feasibility. This is a potential vision evolution — survey-level analysis is insufficient for a decision of this magnitude.

**Confidence required: `directional`.** A well-defended recommendation backed by thorough evidence. No PoC required — this is a feasibility and implications study that informs a human decision on product direction.

**Constraints classification:**
- **Hard**: PPR phase-category weighting depends on session-correlated observations (shipped architecture)
- **Hard**: Unimatrix is not an orchestration engine (vision boundary — session hosting ≠ orchestration, but the line must be articulated)
- **Hypothesis**: MCP server surface coexists with session hosting (challengeable — maybe it gets subsumed)
- **Hypothesis**: Domain-agnostic goal is preserved (challengeable — hosting may tighten or loosen domain coupling)

**Dependencies:**
- vnc-024 (#672) — this spike may redirect or complement that work
- ASS-014 — prior WASM thin client research (may be superseded)
- ASS-064 (#660) — remote telemetry findings

## What the Output Should Be

- **Feasibility assessment**: Can session hosting achieve observation parity? With what trade-offs?
- **Product surface analysis**: What does Unimatrix look like as a session host? Deployment model, dependencies, user experience.
- **Vision evolution options**: 2-3 candidate framings for how the vision would change, with trade-offs for each.
- **Recommendation**: Pursue, defer, or reject — with specific reasoning.
- **vnc-024 impact**: Whether to proceed, pause, or redirect the remote hook configuration work.

## Known Constraints

- The current vision explicitly says "Unimatrix is not an orchestration engine." Session hosting is NOT orchestration (Unimatrix doesn't decide what work to do), but the boundary must be articulated clearly.
- ASS-014 previously designed a WASM cortical implant thin client. This spike may supersede or complement that research — assess the relationship.
- The PPR phase-category weighting system depends on session-correlated observations. Any proposed architecture must maintain or improve this signal quality.
- Unimatrix's MCP tool call stream (context_search, context_store, etc.) already provides some observation signal with session identity via the MCP handshake. Quantify how much of the learning pipeline this covers vs. the full hook observation stream.

## Prior Art

- ASS-014: WASM cortical implant architecture (thin client approach)
- ASS-064: Remote telemetry + MCP transport unification
- vnc-022: `/observe` endpoint (HTTP observation transport)
- vnc-024 SCOPE.md: Remote observation client configuration (curl/hook approach)
