# ASS-011 Conclusion: Protocol Storage in Unimatrix — Not Worth Pursuing

## Spike Question

Should workflow protocols (design session, delivery session) move from `.md` files
into Unimatrix as `procedure` entries, enabling reactive phase delivery and a future
UI editing layer?

## Answer: No

## Research Conducted

Four design artifacts explored the space:

1. **trigger-flow.md** — Mapped the 3 trigger layers (CLAUDE.md → agent def → protocol)
   and 2 handoff types (platform-native vs file-read). Identified protocols as the
   non-platform-native layer.

2. **proposed-flow.md** — v2 pull model: Unimatrix delivers per-session protocol on
   demand via `context_search`. End directive chains to next session.

3. **reactive-flow.md** — v3 reactive model: Recording an outcome via `context_store`
   triggers Unimatrix to return the next workflow phase. One MCP call, two effects.

4. **design-protocol.yaml** — Converted uni-design-protocol.md to declarative YAML
   with `depends_on`, `inputs/outputs`, `parallel`, `human_checkpoint`, `fresh_context`,
   `sacred`, and `trigger_outcome` fields.

## Why Not

### 1. Protocols are single-reader documents

Unimatrix's strength is cross-cutting knowledge retrieval — multiple agents discovering
shared conventions, patterns, and decisions. Protocols are consumed by exactly one agent
(the scrum master). There's no discovery benefit.

### 2. Natural language IS the value

YAML comparison revealed that protocol effectiveness comes from content that resists
structuring:

- **Rationale** — why phases are ordered this way (so the coordinator can adapt)
- **Quality expectations** — what good output looks like per agent
- **Prompt templates** — exact natural language for Task() calls with data threading
- **Anti-pattern warnings** — "Do NOT paste full documents into agent prompts"

Stripping these into structured fields fragments the document's coherence. Putting them
back as text-in-YAML gains nothing over plain markdown.

### 3. Context window savings are negligible

The v2/v3 proposals aimed to save ~3K chars by loading one session at a time instead of
both. In practice, a design protocol is ~4K chars — trivial relative to context window
budgets.

### 4. A UI doesn't need Unimatrix as backend

The original motivation was: if protocols lived in Unimatrix, a UI could edit them.
But a UI can front-end `.md` files just as easily — parse the markdown, render it,
write it back. No MCP dependency, no server changes, no trigger tag conventions.

And fundamentally: text editing text is already the optimal interface for text content.
A UI text box is not meaningfully easier than editing a file in an editor.

### 5. Reactive flow adds complexity for minimal gain

The v3 server-side changes (outcome response enrichment, trigger tag convention,
workflow graph in Unimatrix) add implementation scope and coupling for a trigger
mechanism the scrum master agent def already handles with sequential flow.

## What This Confirms

The **three-layer model** holds:

| Layer | Storage | Why |
|-------|---------|-----|
| Skills | `.claude/skills/*.md` | Platform-native (`/command`), instant, no MCP dependency |
| Agent defs | `.claude/agents/uni/*.md` | Platform-native (Task tool), identity + choreography |
| Protocols | `.claude/protocols/uni/*.md` | Convention-based file reads, single-reader narrative documents |
| Knowledge | Unimatrix entries | Cross-cutting: ADRs, conventions, patterns, duties — multi-agent discovery |

Protocols stay as files. Unimatrix stores knowledge, not workflow choreography.

## Where UI Value Actually Exists

Not in editing (text is text), but in **visibility and operations**:

- Confidence drift dashboards
- Co-access relationship graphs
- Correction chain visualization
- Knowledge health metrics across features
- Bulk operations (quarantine, deprecate) — the 73-entry problem from this spike

These are `mtx-*` (Matrix/UI) milestone concerns, not protocol storage concerns.
