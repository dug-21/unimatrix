## ADR-001: Canonical README Section Order

### Context

The current README was written in nan-005 with a capability-first structure (ADR-001, entry #1254). nan-011 adds three new capability sections (Graph-Enhanced Retrieval, Behavioral Signal Delivery, Domain-Agnostic Observation Pipeline) and removes two stale sections (NLI Re-ranking, NLI Contradiction Detection). The existing section order from nan-005 is not documented as a canonical artifact — it lives only in the file. The spec writer and implementer need a single authoritative reference so they do not make independent ordering decisions.

The nan-005 ADR (#1254) established "capability-first" as the governing principle: what Unimatrix does before how to install it. That principle is preserved unchanged. This ADR extends it with the exact section sequence for the nan-011 state.

### Decision

The canonical README section order after nan-011 is:

1. Vision statement (verbatim approved text — replaces current opening paragraph)
2. How It Works (or equivalent conceptual bridge — the static/dynamic knowledge mental model)
3. Capabilities:
   a. Knowledge Lifecycle (store, correct, deprecate, confidence)
   b. Graph-Enhanced Retrieval (NEW: HNSW + PPR expansion + phase/co-access ranking)
   c. Adaptive Embeddings / MicroLoRA (retained — active and shipped)
   d. Behavioral Signal Delivery (NEW: cycle outcomes as graph edges, goal-conditioned briefing)
   e. Contradiction Detection (UPDATED: cosine Supports detection, no NLI claim)
   f. Domain-Agnostic Observation Pipeline (NEW: source_domain guard, domain packs)
4. Configuration (links to config.toml; summary of key sections)
5. Installation (binary name: `unimatrix`, build path: `target/release/unimatrix`)
6. Quick Start / Usage
7. MCP Tool Reference (the 12 tools)

Sections to remove entirely:
- "Semantic Search with NLI Re-ranking" — replaced by "Graph-Enhanced Retrieval"
- "Contradiction Detection and NLI Edge Classification" — replaced by updated "Contradiction Detection"

The "Adaptive Embeddings (MicroLoRA)" section is retained without modification. MicroLoRA is shipped and active.

The binary name `unimatrix-server` must be replaced with `unimatrix` throughout. Build path changes from `target/release/unimatrix-server` to `target/release/unimatrix`.

### Consequences

- The spec writer works from this ordered list; no section sequencing decisions are deferred to the implementer.
- The implementer does not need to read nan-005 ADRs to understand section ordering intent.
- Adding future capability sections follows the established pattern: insert in the Capabilities block, after the existing entries or replacing the stale entry it supersedes.
- The "How It Works" section (mental model bridge) is preserved or added if absent; it provides the conceptual framing needed before listing capabilities.
