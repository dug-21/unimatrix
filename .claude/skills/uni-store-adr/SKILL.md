---
name: "uni-store-adr"
description: "Store an architectural decision record in Unimatrix. ADRs live in Unimatrix only — no ADR files. Use after each design decision."
---

# Store ADR — Architectural Decisions in Unimatrix

## What This Skill Does

Stores an architectural decision record in Unimatrix as the **sole authoritative store**. ADRs are NOT written as files — Unimatrix provides search, supersession chains, and cross-feature discovery that files cannot.

**Use this AFTER each design decision.** The architect is the sole ADR authority.

---

## How to Store a New ADR

### Step 1: Search for prior ADRs in the same domain (MANDATORY)

```
mcp__unimatrix__context_search({"query": "{decision domain}", "category": "decision", "k": 5})
```

Check if any existing ADR covers the same concern. If so, you may need to supersede it (see "How to Supersede" below).

### Step 2: Store the ADR

```
mcp__unimatrix__context_store({
  "title": "ADR-NNN: {decision title}",
  "content": "## Context\n{why this decision is needed}\n\n## Decision\n{what we decided}\n\n## Consequences\n{what follows from this decision}",
  "topic": "{feature-id}",
  "category": "decision",
  "tags": ["adr", "{phase-prefix}", "{domain-tags}"],
  "source": "architect",
  "feature_cycle": "{feature-id}",
  "agent_id": "{your role name, e.g. uni-architect}"
})
```

### Step 3: Record the entry ID

Note the Unimatrix entry ID returned. Pass it to the coordinator — downstream agents and the synthesizer need ADR entry IDs to reference decisions.

### Step 4: Reference in ARCHITECTURE.md

In your ARCHITECTURE.md, reference ADRs by Unimatrix entry ID:

```markdown
## Decisions
| ADR | Title | Unimatrix ID |
|-----|-------|--------------|
| ADR-001 | Use rmcp 0.16 with stdio | #77 |
| ADR-002 | Additive confidence model | #85 |
```

---

## How to Supersede an Existing ADR

When a new decision replaces a prior one:

### Step 1: Find the old ADR

```
mcp__unimatrix__context_search({"query": "{domain of old decision}", "category": "decision"})
```

Note the old entry's ID.

### Step 2: Use context_correct to supersede

```
mcp__unimatrix__context_correct({
  "original_id": 1234,  // integer — never quote it
  "content": "## Context\n{why the old decision is being replaced}\n\n## Decision\n{new decision}\n\n## Consequences\n{what changes}",
  "title": "ADR-NNN: {new decision title}",
  "reason": "Superseded by {feature-id}: {short explanation}"
})
```

This automatically:
- Deprecates the old entry
- Creates a new entry with supersession chain
- Preserves the old decision for historical reference

---

## Typed Relationship Edges (HIGH bar — default is NO edge)

ADRs relate to other entries. **Most of that belongs in prose** (the Context section). A typed
graph edge is the rare exception — assert one ONLY when a future agent would have to *follow* it
to avoid a wrong decision. This is the canonical convention; other skills/agents reference it.

**The bar — all three must hold:**

1. **Default is no edge.** An ADR with zero edges is correct and normal. There is NO coverage
   target — never assert edges to "show connectedness." Over-linking destroys the graph's signal.
2. **Traversal-necessity.** If you deleted this edge, would someone miss a prerequisite, act on a
   contradicted decision, or repeat a logged failure? If no → it's prose, not an edge.
3. **Don't duplicate the inference layer.** Similarity (search) and `CoAccess` (auto-inferred)
   already cover topical relatedness. Author edges only the *directional, causal* relationships
   inference cannot derive.

**The ONLY edge types an ADR author asserts:**

| Edge | Assert when |
|------|-------------|
| `Prerequisite` | The target must be read / acted-on *before* this decision is correct — intra-feature ordering, or a must-read cross-feature predecessor. |
| `Contradicts` | A real, decision-blocking conflict with another entry. |
| `Supports` | A lesson or pattern that *directly caused or validates* this decision (usually asserted at retro, once the lesson exists). |

Do NOT use `RelatedTo`, `Mentions`, `About`, or `Informs` for ADR authoring — those are the
"everything is connected" trap. **Supersession is NOT an edge — use `context_correct` (above).**

**Each edge needs a one-clause justification.** If you can't write "X is a `Prerequisite` of Y
because you'll act wrongly without reading it first," don't assert it.

**Two moments to assert:**

- **At authoring** — only to targets that *already exist* (typically cross-feature predecessors
  with known IDs). Declare them in the `edges` param of `context_store`:
  ```
  mcp__unimatrix__context_store({
    ... (title, content, topic, category, tags, source, feature_cycle, agent_id as above) ...,
    "edges": [{"edge_type": "Prerequisite", "target_id": 4742}]
  })
  ```
  Do NOT force edges to sibling ADRs that lack IDs yet — forward references fail. Leave those for retro.
- **At retro** — the graph-completion pass. Now that all IDs and outcomes exist, add the high-bar
  edges you couldn't make at authoring (the intra-feature `Prerequisite` spine; `Supports` from a
  lesson/pattern to the decision it validates) via:
  ```
  mcp__unimatrix__context_edge({"mode": "add", "source_id": <from>, "edge_type": "Supports", "target_id": <to>})
  ```

---

## ADR Content Guidelines

**Keep ADRs to 300-800 characters.** They capture the decision, not the implementation.

```
## Context
The briefing tool returns duties, conventions, and semantic matches.
Duties duplicate what's already in agent definition files, consuming
~200 tokens for zero new information.

## Decision
Remove duties from Unimatrix categories and context_briefing responses.
Agent defs are the sole authority for role responsibilities.

## Consequences
- Briefing returns 2 sections (conventions + relevant context) instead of 3
- ~200 tokens freed per briefing call for more useful content
- 28 existing duty entries deprecated
- uni-init bootstrap no longer extracts duties
```

NOT a full design document. NOT implementation details. Just: why, what, and so-what.

---

## Tagging Conventions

| Tag Type | Examples |
|----------|----------|
| Always | `adr` |
| Phase prefix | `nexus`, `vinculum`, `collective`, `cortical`, `alcove` |
| Domain | `storage`, `serialization`, `mcp`, `embedding`, `security`, `confidence` |
| Cross-cutting | `error-handling`, `async`, `thread-safety`, `api-design` |

---

## Self-Verification

After storing:
- Confirm entry ID returned
- If **near-duplicate warning**: review existing entry — you may need to supersede rather than create
- Record entry ID in your agent report and ARCHITECTURE.md decisions table

---

## What NOT to Store as ADRs

| Don't Store | Why |
|-------------|-----|
| Draft decisions under discussion | Store only finalized decisions |
| Implementation details (how) | ADRs capture the why — code captures the how |
| Decisions by other agents | Architect is the sole ADR authority |
| Coding conventions | Use `convention` category instead |
| Step-by-step procedures | Use `/uni-store-procedure` instead |
