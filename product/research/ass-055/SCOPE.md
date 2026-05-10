# ASS-055: ADR Dependency Tracking — `DependsOn` Graph Relationship

**Date**: 2026-05-06
**Tier**: 1 — informs a potential Wave 2 delivery item or early Wave 3 intelligence enhancement
**Feeds**: Future `crt-NNN` (write path for dependency edges), `context_store` MCP interface, `context_briefing` surfacing strategy
**Related**: W1-1 (`crt-021`, typed graph + `RelationType`), WA-4 (`crt-027`, proactive delivery), graph_ppr, graph_expand

---

## Background

ADRs (architectural decisions) in Unimatrix are first-class knowledge entries in the `decision` category. They carry the full integrity stack: hash-chained corrections, `supersedes`/`superseded_by` linkage, immutable audit log attribution. The correction chain (`context_correct`) and supersession graph are well-developed.

One standard element of ADR practice that Unimatrix does not yet model is **dependency** — Decision B was made *under the assumption that* Decision A holds. If ADR-A is superseded or contradicted, ADR-B may become invalid or require re-evaluation. This is distinct from supersession (A replaces B) and from support (A provides evidence for B). It is a structural coupling between decisions: "B depends on A."

The graph edge taxonomy in `crt-021` already reserved a `Prerequisite` relation type for exactly this purpose, with an explicit comment that no write path was created (`graph.rs:77`: *"Prerequisite is reserved for W3-1; no write path exists in crt-021"*). PPR (`graph_ppr.rs`) and `graph_expand` already treat `Prerequisite` as a positive edge — a write path would land in the retrieval pipeline with zero engine changes.

This spike answers: is now the right time to build that write path, what exactly should it model, what is the end-to-end impact, and how does it surface usefully to agents?

---

## The Question

**Can Unimatrix model ADR `depends_on` relationships as first-class typed graph edges in a way that (1) enriches retrieval relevance, (2) surfaces propagated impact when a depended-on decision changes, and (3) fits cleanly into the existing correction chain, integrity model, and security model — without requiring a new relation type or schema migration?**

Sub-questions:
1. Is `Prerequisite` the correct semantic mapping for `depends_on`, or does the direction convention mismatch require renaming or a new type?
2. Should `depends_on` linkage be stored as a pure graph edge (GRAPH_EDGES only) or as a dual-source field (`entries.depends_on` analogous to `entries.supersedes`)?
3. Where and how does the agent specify dependency links? Metadata in `context_store`? New `context_relate` tool? `context_correct`?
4. What happens to dependent decisions when their dependency is deprecated or superseded? Is cascading review notification possible and desirable?
5. How does the dependency edge participate in PPR, `graph_expand`, and `context_briefing`? Is the PPR reverse-walk direction correct for this relationship?
6. What is the blast radius on existing tests, write paths, and the correction chain?

---

## Why This Matters to the Vision

The vision document states: *"A typed knowledge graph formalizes relationships — not just what agents retrieve together, but why: support, contradiction, supersession, dependency."*

`dependency` is named explicitly. The graph currently covers supersession, contradiction, support, and co-access. Dependency is the missing quadrant.

Without it:
- An agent can store "Decision B assumes OAuth is available" but cannot express that formally
- When the OAuth decision (ADR-A) is superseded, nothing signals that Decision B requires review
- PPR seeded on B cannot flow back to A's replacement via a typed edge
- `context_briefing` for a feature touching ADR-B cannot surface ADR-A as contextually critical

With it:
- The dependency graph becomes a live review surface: any deprecation of ADR-A surfaces all dependent ADRs for review
- Retrieval naturally brings in the dependency chain without requiring agents to know what ADR-B depends on
- `context_briefing` at design phase can surface "this decision depends on ADR-X — confirm it still holds"

This also has direct relevance to the enterprise tier's ISO 42001 governance objective (audit graph of goal→decision→outcome). Dependency edges are load-bearing in that graph.

---

## Prior Art to Build On

### What exists in the codebase

**`RelationType::Prerequisite`** (`crates/unimatrix-engine/src/graph.rs:86`):
- Already in the enum, stored as string `"Prerequisite"`
- `from_str` and `as_str` implementations present
- `graph.rs:77` comment: *"Prerequisite is reserved for W3-1; no write path exists in crt-021"*

**PPR already consumes it** (`graph_ppr.rs:112`, `graph_ppr.rs:179`):
- `personalized_pagerank` pulls mass through `Prerequisite` outgoing edges
- PPR direction: reverse walk — for edge A→B, seeding B causes mass to flow to A
- `outgoing_weight_sum` includes Prerequisite in the denominator

**`graph_expand` already expands it** (`graph_expand.rs:133`):
- BFS expansion follows `Prerequisite` outgoing edges in the positive candidate pool
- Direction: Outgoing traversal from seed

**GRAPH_EDGES schema** (`migration.rs:333`):
- `(source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only, metadata)`
- UNIQUE constraint on `(source_id, target_id, relation_type)` — no duplicate edges per type
- `relation_type` is a plain string — any valid `RelationType` value stored without migration

**`Supersedes` dual-source pattern** (`graph.rs:258–283`):
- `entries.supersedes` is the canonical source (Pass 2a in `build_typed_relation_graph`)
- GRAPH_EDGES Supersedes rows are skipped in Pass 2b — the field wins
- This pattern exists because `supersedes` predates `crt-021`

**`Informs` write path precedent** (`crt-037`):
- `Informs` was added post-crt-021 as the first net-new edge type with a write path
- Review how `context_store` or the retrospective pipeline writes Informs edges — this is the reference implementation pattern for adding a new write path

### Semantic reference: classic ADR `depends_on`

In Michael Nygard's original ADR format, `depends_on` means: "this decision's validity assumes that [linked decision] remains in force." It is directional — B depends on A — and it is invalidated when A changes. This is distinct from:
- *Supersedes*: B replaces A
- *Supports*: empirical evidence A supports conclusion B
- *Informs*: lesson-learned A shaped design B

---

## What to Investigate

### Q1 — Semantic fit: `Prerequisite` vs `DependsOn`

Does `Prerequisite` as currently named map cleanly to ADR `depends_on`?

Investigate:
- The naming: "Prerequisite" could imply temporal ordering (must be done first), while "DependsOn" implies validity coupling. Are these the same thing in this context?
- The direction: if edge A→B means "A is a prerequisite of B" (i.e., B depends on A), then in PPR reverse walk, seeding B causes mass to flow to A. Is that the desired retrieval behavior? When looking at Decision B, should Decision A surface? Yes. Is the PPR direction correct for this? Verify.
- In `graph_expand`, outgoing traversal from seed B would reach A (since A→B means outgoing from B... wait, no: A→B means outgoing *from A*). Check the direction contract carefully. If edge is stored as A→B (A is prerequisite of B), then outgoing from B does NOT reach A. BFS expansion from B would only surface B's own outgoing prerequisites, not what B depends on.
- Determine the correct edge direction for retrieval to work as intended in both PPR and graph_expand.

**This is the critical question.** Get the direction semantics right before evaluating anything else.

### Q2 — Write path options

Three options exist. Evaluate each:

**Option A: `context_store` metadata parameter**
Add an optional `depends_on: [entry_id, ...]` parameter to `context_store`. When provided, write `Prerequisite` edges to GRAPH_EDGES immediately after the entry is written.

- Pros: no new tool, no schema change to entries table, works for initial store and for retrofitting existing entries via `context_correct`
- Cons: retrofitting existing ADRs requires re-issuing a corrected entry even if the content hasn't changed; linkage is buried in the store path

**Option B: Dedicated `context_relate` tool**
New MCP tool: `context_relate(source_id, target_id, relation: "DependsOn" | ..., rationale: str)`.

- Pros: explicit, attributable, can be called post-hoc without modifying entry content, rationale stored in edge metadata field, clean audit story
- Cons: adds a 13th tool, agents must know to call it separately, potential for omission

**Option C: `depends_on` field in entries table (dual-source, mirrors `entries.supersedes`)**
Add `depends_on TEXT` (JSON array of entry IDs) to the ENTRIES table, handled in Pass 2 of `build_typed_relation_graph` alongside `supersedes`.

- Pros: canonical, survives graph rebuild from entries, matches the supersedes pattern
- Cons: schema migration required (v25 → v26), entries table grows, stored even for non-decision entries

Evaluate each against: (a) agent ergonomics, (b) audit/attribution clarity, (c) schema impact, (d) correctness under correction chain (does the link survive a `context_correct` call?), (e) alignment with how `Informs` edges are written (find the precedent).

### Q3 — Correction chain interaction

When ADR-B depends on ADR-A and ADR-A is superseded by ADR-A':
- Does the `Prerequisite` edge from A→B (or B→A, per Q1 resolution) automatically transfer to A'→B?
- Should Unimatrix emit a signal that ADR-B needs review?
- Can `context_cycle_review` or `context_status` surface "N decisions depend on a recently-superseded entry"?

Investigate:
- Whether the existing `context_correct` write path touches GRAPH_EDGES for the corrected entry
- Whether a "dependency review surface" is feasible as a `context_status` or `context_cycle_review` output (no new tool required — just a new detection heuristic)
- What the blast radius of an edge-transfer rule would be: when A→A', copy all Prerequisite edges from A to A'. Is this safe? What are the failure modes?

### Q4 — Surfacing in `context_briefing` and `context_search`

How should dependency links improve retrieval quality?

Investigate:
- **PPR**: when Decision B is in the seed set, does PPR surface A (the decision B depends on)? Verify this works with correct edge direction. What alpha/iteration values are used?
- **`graph_expand`**: does BFS expansion from Decision B reach A? Under current direction, it may not (see Q1). This may need direction fix or a second pass.
- **`context_briefing` at design phase**: when a feature cycle involves Decision B, should B's dependencies be pulled in explicitly? Current `context_briefing` uses phase-conditioned ranking — would a dependency-aware retrieval mode improve it or duplicate PPR behavior?
- **`context_cycle_review` detection rule**: should a new rule fire when an entry being stored has a `depends_on` link to a deprecated/superseded entry? "Decision stored that depends on deprecated entry #NNN — review recommended."

### Q5 — Security and capability model

- What capability is required to write a `Prerequisite` edge? Write capability (same as `context_store`)?
- Should agents be able to add dependency edges to entries they did not create?
- Is there a spoofing risk: agent A declares that its low-confidence decision depends on a high-confidence ADR, and the high-confidence ADR's PPR mass flows back to inflate agent A's entry?

Investigate whether the existing per-agent write capability gate is sufficient, or whether additional attribution constraints are needed.

### Q6 — Blast radius assessment

Produce a concrete list of:
- Files that must change (write path, graph building, migration, MCP tool handler)
- Files that benefit with zero changes (PPR, graph_expand — already consume Prerequisite)
- Tests that must be added or modified
- Whether a schema migration is required and what version it targets
- Estimated effort (rough: days, not weeks)

---

## Out of Scope

- Implementing the write path — this spike produces a recommendation and design spec; delivery is a separate session
- Modeling dependency between entries outside the `decision` category (may be valid, but ADR dependency is the primary use case — generalization is post-delivery)
- Automated inference of dependency from entry content (semantic detection of "assumes X") — out of scope for this spike; mention as a future opportunity only
- Cross-project dependency tracking (Wave 2 multi-project model — not in scope here)

---

## Expected Output

A `FINDINGS.md` in this directory with:

1. **Direction resolution** — which edge direction is correct for `Prerequisite`/`DependsOn` given PPR reverse-walk and `graph_expand` outgoing semantics. Diagram or clear table.

2. **Semantic fit verdict** — is `Prerequisite` the right type name, or should the enum gain a `DependsOn` variant? If rename: what is the migration path for any existing data (check whether any `Prerequisite` edges exist in production)?

3. **Write path recommendation** — which option (A, B, or C) with rationale covering agent ergonomics, audit clarity, schema impact, and correction chain safety. Include the `Informs` write path as a comparison reference.

4. **Correction chain behavior** — concrete proposal for what happens to dependency edges when the target is deprecated/superseded. Edge transfer rule or surface-only recommendation.

5. **Surfacing assessment** — for each of PPR, `graph_expand`, `context_briefing`, and `context_cycle_review`: does the feature work with zero changes (once write path exists), or does it need modification? Expected retrieval improvement hypothesis.

6. **Security verdict** — is the existing Write capability gate sufficient? Any additional constraints?

7. **Blast radius table** — files changed, files benefiting for free, schema migration required (yes/no, version), rough effort estimate.

8. **Recommendation** — go / no-go / defer with rationale against the vision. If go: which wave and why.
