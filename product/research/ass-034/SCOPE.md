# ASS-034: Parameterized Relationship Taxonomy and Autonomous Detection

**Type**: Research spike
**Date**: 2026-03-31
**Status**: In Progress
**Motivation**: 79.5% graph isolation — entries have no edges not because the knowledge lacks
relationships, but because the detection vocabulary is too narrow. The specific target:
cross-feature, cross-time bridges where a lesson or pattern from a past feature directly
constrains a decision in a later feature. These are NOT already connected via co_access,
supersession, or NLI entailment — they are structurally blind spots.

---

## The Core Hypothesis

The isolation problem is a **category problem, not a density problem**.

The graph self-populates via two automated mechanisms:
- **CoAccess** — entries accessed together (behavioral signal, no semantics)
- **Supports / Prerequisite** — NLI entailment (semantic signal, binary: entails or doesn't)

NLI is good at exactly one relationship: does A logically entail B? For Unimatrix's knowledge
domain, most relationships aren't entailment. An ADR doesn't entail a lesson-learned. A pattern
doesn't entail a convention. But they're deeply related — and their edges are invisible.

Examples that currently produce zero graph edges:
- ADR-001 (SQLite choice) → **InformedBy** → lesson about not mocking the database
- Convention (write_pool_server) → **ImplementsDecision** → ADR-001
- Pattern (background tick ordering) → **AppliedIn** → every maintenance tick feature
- Lesson (per-query store scan) → **CausedBy** → GH#264 incident
- crt-034 → **FulfilledContract** → ADR-006

Each of these would create an edge that directly reduces isolation and improves PPR traversal.

---

## Architecture Readiness

The storage layer is already generic. `GRAPH_EDGES.relation_type` is free-text (string column).
No schema migration is needed to add new relationship types.

**However, two code barriers currently block new types:**

1. `RelationType::from_str()` in `unimatrix-engine/src/graph.rs:100` rejects unrecognized
   strings with `warn!` and skips the edge entirely (R-10 guard, line 289).
   Any new type requires either extending the enum or changing the validation strategy.

2. PPR (`graph_ppr.rs`) hardcodes exactly three positive edge types (Supports, CoAccess,
   Prerequisite). New positive-polarity types are invisible to PPR unless explicitly added.
   There is no per-type weight config — all edges share the same out-degree normalization.

---

## Research Questions

### RQ-1: Relationship Taxonomy
What relationship types actually exist in the Unimatrix knowledge domain?

Candidate taxonomy:
| Type | Direction | Description | Example pair |
|---|---|---|---|
| InformedBy | positive | Entry A's rationale was shaped by entry B | ADR → lesson-learned |
| ImplementsDecision | positive | Entry A is a concrete realization of ADR B | Convention → ADR |
| FulfilledContract | positive | Feature A satisfied the requirement in ADR B | Feature → ADR |
| CausedBy | directed | Entry A exists because of incident/failure B | Lesson → incident |
| AppliedIn | positive | Pattern A was applied in feature B's context | Pattern → feature-lesson |
| Mentions | weak positive | Entry A explicitly references entry B by ID | Any → Any |
| RelatedWork | symmetric weak | Entries are in the same problem domain, neither entails nor supports | Convention ↔ convention |

Which of these are worth detecting automatically? Which are explicit-only?

### RQ-2: Autonomous Detection Mechanisms
**Best case**: Unimatrix identifies relationships on its own, without any human annotation.

#### Signal sources already in the system:

**Text reference extraction (structural)**
- Entry bodies that mention another entry by ID pattern (`crt-\d+`, `ASS-\d+`, `#\d+`,
  `ADR-\d+`, `GH#\d+`) produce an implicit `Mentions` edge.
- Zero ML required. Pure regex scan at store-time or in the background tick.
- Precision: very high (explicit reference = definite relationship).
- Coverage: low (only entries that explicitly cite others).

**Feature co-membership (structural)**
- Entries sharing the same `feature` field tag have an implicit relationship.
- An ADR and a convention/lesson stored under the same feature → likely `ImplementsDecision`.
- Category-pair discriminator: (decision, convention) → ImplementsDecision;
  (decision, lesson-learned) → InformedBy.
- Zero ML. Runs at store-time or tick.
- Precision: medium-high (same feature + category cross-match is a strong signal).

**Correction chain analysis (structural)**
- `context_correct` writes a `reason` field to the audit log.
- Keyword scan on `reason`: "because of ADR", "implements", "violates" → typed edge.
- Combines text-reference extraction with the correction provenance signal.

**NLI neutral zone (semantic)**
- NLI "neutral" with cosine similarity > 0.6 = related but not entailment.
- Currently: this pair produces zero edges.
- Could surface as a candidate for LLM annotation, or as a weak `RelatedWork` edge
  (lower PPR weight than Supports).

**Observation table analysis (behavioral)**
- `injection_log`: tracks which entries were served to which session+phase. Entries always
  served together in the same phase (not just same session) suggest a domain relationship
  stronger than CoAccess affinity.
- `query_log`: entry pairs that always co-occur in the same feature cycle's queries may
  indicate a structural dependency worth making explicit.
- These signals reinforce existing CoAccess edges but could also bootstrap typed edges
  where the co-occurrence pattern matches a category-pair heuristic.

**LLM annotation at store-time (semantic)**
- When an entry is stored, retrieve top-k HNSW neighbors, then ask a lightweight
  prompt: "Does A InformedBy B, ImplementsDecision B, or neither?"
- Unimatrix runs as an MCP server — it does not have direct LLM access.
- This mechanism would require either: (a) a new context_store flow that triggers
  an annotation request from the calling agent, or (b) a future `context_relate`
  tool that agents can call explicitly after store.
- Precision: high. Cost: medium (one LLM call per store, k=5 neighbors).
- **Constraint**: Unimatrix itself is LLM-agnostic; LLM annotation would be a caller
  responsibility, not an internal inference step.

### RQ-3: Config-Extensible vs Hard-Coded Set
Should new relationship types be defined in code (enum extension) or in config (TOML)?

**Config-extensible approach (proposed):**
```toml
[[inference.relation_types]]
name        = "InformedBy"
polarity    = "positive"        # positive | negative | neutral
ppr_weight  = 0.7               # relative to CoAccess (1.0)
detection   = "text_reference"  # text_reference | feature_field | nli_neutral | explicit_only

[[inference.relation_types]]
name        = "ImplementsDecision"
polarity    = "positive"
ppr_weight  = 0.9
detection   = "feature_field"
category_pairs = [["convention", "decision"], ["lesson-learned", "decision"]]

[[inference.relation_types]]
name        = "Mentions"
polarity    = "weak_positive"
ppr_weight  = 0.4
detection   = "text_reference"
```

**Hard-coded enum extension:**
- Add new variants to `RelationType` in `graph.rs`.
- Add detection logic in the NLI tick or a new structural-signal tick.
- Config only controls weights (existing InferenceConfig pattern).
- Lower flexibility, faster path, lower risk of misconfiguration.

**Trade-offs to evaluate:**
- Config-extensible allows operators to define domain-specific types without code changes.
- But config-extensible requires `RelationType` to become open (e.g., `Unknown(String)` variant
  that passes through unknown strings rather than rejecting them) — a bigger architectural change.
- A hybrid: ship a fixed extended set (InformedBy, ImplementsDecision, Mentions) as enum
  variants, expose their PPR weights as config fields, defer full extensibility.

### RQ-4: PPR Integration
How do new relationship types affect PPR traversal?

Current PPR: three separate `edges_of_type` calls in the inner loop. Out-degree normalization
is flat (sum of all positive-edge weights). No per-type weight multiplier beyond the stored
`RelationEdge.weight` field.

Key questions:
- Should InformedBy carry the same weight as Supports in PPR traversal, or less?
- A feature-field-derived edge is structurally derived (not semantically scored). Should it
  carry a fixed weight (e.g., 0.5) vs. a similarity-derived weight?
- If a new type is `weak_positive`, does it participate in out-degree normalization the same
  way as CoAccess, or at reduced weight?
- Is a per-type `ppr_weight` multiplier in `InferenceConfig` sufficient, or does PPR need
  structural changes?

### RQ-5: Expected Isolation Impact
If we add text_reference and feature_field detection only (no LLM, no config extension):
- How many entries currently have body text that explicitly references other entry IDs?
  → Needs measurement against the live knowledge base.
- How many (ADR + convention) or (ADR + lesson-learned) pairs share the same `feature` field?
  → Needs measurement.
- Estimate: even 50 new edges across 997 active entries would reduce 79% isolation meaningfully.

---

## Detection Option Matrix

| Approach | Precision | Cost | Unimatrix-internal? | Fit |
|---|---|---|---|---|
| Text reference extraction | Very high | Negligible | Yes | `Mentions`, `InformedBy` |
| Feature co-membership | High | Negligible | Yes | `ImplementsDecision`, `FulfilledContract` |
| NLI neutral zone | Medium | Low (reuse existing) | Yes | `RelatedWork` (weak) |
| Observation table analysis | Medium | Low | Yes | Reinforces CoAccess, could bootstrap typed edges |
| Config keyword heuristics | Low-medium | Low | Yes | `Mentions` (fallback) |
| LLM annotation at store-time | High | Medium | **No** — caller responsibility | All types |
| `context_relate` explicit tool | Very high | Manual | Yes (caller initiates) | All types, especially ADRs |
| Relation extraction model (REBEL) | Medium | High | Overkill now | Deferred |

---

## Deliverables

1. ~~**TAXONOMY.md**~~ — **Superseded by FINDINGS.md §Finding 7.** The research collapsed
   the taxonomy to one new edge type (`Informs`) with a configurable category-pair list.
   Full taxonomy enumeration is not required. Default software engineering pairs are defined
   in Finding 7.

2. ~~**DETECTION-ANALYSIS.md**~~ — **Superseded by FINDINGS.md §Findings 1–6.** Live data
   analysis confirmed the detection mechanism: NLI neutral zone + cosine ≥ 0.45 +
   category-pair filter + cross-feature temporal ordering. Coverage estimates in Finding 6.

3. **ARCHITECTURE.md** — how to extend `RelationType`, `build_typed_relation_graph`, PPR,
   and `InferenceConfig` with the two new fields (`nli_informs_cosine_floor`,
   `informs_category_pairs`). Remains as a deliverable.

4. **ROADMAP-ENTRY.md** — GH issue scope for the feature. Remains as a deliverable.

---

## What This Is Not

- Not a GNN or embedding fine-tuning spike (that's deferred per ass-032 roadmap).
- Not a full relation extraction model spike (REBEL etc. is out of scope).
- Not an LLM-at-store-time implementation (Unimatrix is LLM-agnostic by design).
- Not a graph schema change (GRAPH_EDGES already stores free-text relation_type).
