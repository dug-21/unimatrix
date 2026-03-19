# Scope Risk Assessment: crt-021

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `petgraph` `StableGraph` filter-by-edge-type is not a native API — penalty logic must manually skip non-Supersedes edges on every traversal. If filter discipline is incomplete, non-Supersedes edges silently enter `graph_penalty`. | High | Med | Architect must define the filter boundary explicitly (a wrapper method, not ad-hoc checks at each call site). 25+ existing graph.rs tests must all pass against typed graph unchanged. |
| SR-02 | Analytics write queue uses a shed policy (drop + log at capacity 1000). `GraphEdge` writes routed through it can be silently dropped under load. Bootstrap data is recoverable (re-migration), but runtime NLI edges written by W1-2 are not — they are not re-derivable from canonical sources. | High | Med | Architect must distinguish bootstrap-path writes (safe to shed — idempotent migration) from runtime edge writes (NLI-created at W1-2 — must not shed). Consider a non-shedding path for runtime edge writes, or document the data-loss window explicitly as an accepted risk. (Entry #2125 confirms drain is unsuitable for writes that callers read back immediately.) |
| SR-03 | `GRAPH_EDGES` compaction (orphaned edge DELETE) added to tick increases tick cost. Entry #1777 documents a prior regression where compute_report() repurposed as a tick loader inflated maintenance tick cost with 5 wasted phases. Compaction is a full-table DELETE + join against `entries` — on large graphs, non-trivial. | Med | Med | Architect should bound compaction cost: limit to a fixed DELETE batch size per tick (not unbounded), or run compaction only every N ticks. Measure tick duration regression in tests. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `shadow_evaluations` does not store `(entry_id_a, entry_id_b)` pairs — it stores rule/category evaluation results keyed by `digest`. Entry #2404 explicitly confirms this: Contradicts bootstrap from shadow_evaluations requires an additional join that may not be possible. The decision says "no Contradicts bootstrap" but AC-08 still references it conditionally. | High | High | Spec writer must close AC-08 as "empty bootstrap — no Contradicts edges at migration, W1-2 NLI populates at runtime." Any conditional language in acceptance criteria leaves implementer discretion that can cause scope creep or a skipped-but-unmarked AC. |
| SR-05 | `Prerequisite` variant is included in `RelationType` with no bootstrap path and no consumer in W1-1 or W1-2. Its presence as a dead variant until W3-1 is a forward-compatibility bet: if W3-1's GNN feature vector requires a different encoding than string-based `"Prerequisite"`, the stored edges are migration debt. | Low | Low | Spec writer should add a constraint: Prerequisite edges may not be created by any path in W1-1 (no bootstrap, no analytics write). Document the variant as reserved for W3-1. |
| SR-06 | ~20 call sites for `SupersessionState`/`SupersessionStateHandle` rename to `TypedGraphState`. Rename is clean but creates a large diff that intersects `background.rs`, `main.rs`, `services/mod.rs`, `services/search.rs`. Risk of missed call site causing a compile error that a junior implementer resolves with a type alias (defeating the semantic upgrade). | Low | Med | Spec writer should enumerate the exact rename surface. Compiler enforcement is a feature, not a risk — document that the rename is enforced by the compiler and no type aliases should be introduced. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | W1-2 NLI will write `Contradicts` and `Supports` edges via `AnalyticsWrite::GraphEdge`. The `bootstrap_only=0` flag is the promotion mechanism. If W1-1 ships with no promotion API (how does W1-2 set `bootstrap_only=0` on an existing bootstrap edge?), W1-2 must add its own migration or leave bootstrap edges permanently excluded from scoring. | High | High | Architect must define the bootstrap-to-confirmed promotion path now, even if W1-2 implements it. Options: (a) W1-2 DELETEs the bootstrap edge and INSERTs a new confirmed edge; (b) W1-1 provides an `UPDATE graph_edges SET bootstrap_only=0` path. Leaving this undesigned blocks W1-2. |
| SR-08 | W3-1 GNN requires per-edge features: `RelationType`, NLI confidence score, co-access count. `RelationEdge.weight: f32` is the only numeric field in W1-1. NLI confidence scores from W1-2 are not stored anywhere in W1-1's schema. If W3-1 expects them from `GRAPH_EDGES`, they must be added later (schema migration). | Med | Med | Architect should audit W3-1's expected GNN edge feature vector now and confirm `weight` is sufficient as the sole numeric field, or add a `metadata: TEXT` (JSON) column to `GRAPH_EDGES` in v13 while the migration cost is zero. |
| SR-09 | `sqlx` compile-time query checking requires `sqlx-data.json` to be regenerated after v12→v13 schema change. If CI does not regenerate it, compile-time SQL validation is silently disabled for all new GRAPH_EDGES queries. (PRODUCT-VISION.md W0-1 section, medium-severity security requirement.) | Med | High | Spec writer must add an AC requiring `sqlx-data.json` regeneration and CI validation as part of the migration deliverable. |

## Assumptions

- **SCOPE.md §Constraints #1**: Single SQLite file is confirmed (entry #2063). Assumes W2-1 container packaging does not change this — product vision §W2-1 shows `analytics.db` and `knowledge.db` as separate volumes. If these are in fact separate files, `GRAPH_EDGES` placement needs re-evaluation. The spec writer should confirm these are truly one file in current implementation.
- **SCOPE.md §Background Research, SupersessionState Cache Pattern**: Assumes `TypedRelationGraph` rebuild from `GRAPH_EDGES` is fast enough for the existing `TICK_TIMEOUT`. Not validated — on a large graph, a full `SELECT * FROM graph_edges` + in-memory reconstruction may exceed the timeout.
- **SCOPE.md §Open Questions #1**: Assumes Contradicts bootstrap is empty (no entry ID pairs in shadow_evaluations). Entry #2404 confirms this. The scope document's conditional AC-08 language contradicts this assumption.

## Design Recommendations

- **SR-04**: Close the Contradicts bootstrap question definitively in the spec. AC-08 should read "empty — no bootstrap Contradicts edges; W1-2 NLI creates all Contradicts edges at runtime." Remove conditional language.
- **SR-07**: Design the bootstrap-to-confirmed edge promotion path in the architecture, not deferred to W1-2. A `bootstrap_only` flag with no promotion mechanism is a dead-end data structure.
- **SR-01**: Define a single `edges_of_type(graph, RelationType)` iterator or filtered graph view to enforce the Supersedes-only filter boundary. Do not rely on ad-hoc checks scattered across `graph_penalty` and `find_terminal_active`.
- **SR-02**: Architect should add a note on the shedding risk for future runtime NLI edge writes (W1-2 concern) even though W1-1 only has bootstrap writes. Documenting the accepted risk now prevents W1-2 from inheriting it silently.
- **SR-09**: Add `sqlx-data.json` regeneration to the migration test AC explicitly.
