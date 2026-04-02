# Scope Risk Assessment: crt-040

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `write_nli_edge` hardcodes `'nli'` as both `source` and `created_by`. Generalizing it or adding a sibling risks silently retagging existing Informs/NLI Supports edges if the wrong path calls the wrong writer | High | Med | Architecture must mandate a distinct `write_graph_edge(source: &str, …)` function; `write_nli_edge` must remain a thin wrapper or be untouched |
| SR-02 | `inferred_edge_count` in `GraphCohesionMetrics` counts only `source='nli'` edges. Cosine Supports edges have a different source and will never appear in this metric, making observability silently incomplete | Med | High | Architecture must specify what metric surface cosine Supports edges; AC-15 defers it but the eval gate depends on `supports_edge_count` — verify that field is source-agnostic |
| SR-03 | `MAX_COSINE_SUPPORTS_PER_TICK = 50` budget constant is not validated by `InferenceConfig::validate()`. If a future operator wants to tune it, there is no range guard or config promotion path | Low | Med | Architect should note budget constant is hard-coded; if config-promotion is deferred, add a TODO comment at the constant to prevent silent drift |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | GRAPH_EDGES UNIQUE constraint scope is unverified at scope time. If the constraint includes `source`, Path B and Path C could both insert the same `(source_id, target_id, Supports)` pair as separate rows — `INSERT OR IGNORE` would NOT deduplicate them | High | Low | Architecture must inspect the migration DDL and confirm the constraint is on `(source_id, target_id, relation_type)` only — SCOPE.md §Architecture Note flags this explicitly |
| SR-05 | `informs_category_pairs` is reused as the Supports category filter without a separate config field. If the Informs and Supports semantic domains diverge (e.g., same-category Supports added in a follow-on feature), there is no knob to decouple them | Med | Low | Spec must document that reuse is intentional and note the follow-on risk; a `supports_category_pairs` config field is a natural extension point |
| SR-06 | `nli_post_store_k` removal is bundled with the cosine Supports delivery. If this dead-field removal causes a test or serde regression, it blocks the primary feature | Low | Low | Treat removal as a separate deliverable within the same PR; isolate its test impact in the spec |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Path C runs inside `run_graph_inference_tick` which is infallible (returns `()`). If Path C introduces a panic path (e.g., unwrap on embedding lookup), the entire tick silently dies — same failure mode as co_access promotion (#3883) | Med | Med | Architecture must confirm Path C follows the "log warn, continue" error contract; no `?` propagation, no `unwrap` on embedding results |
| SR-08 | `candidate_pairs` from Phase 4 is produced by the NLI Supports path (Path B) candidate scan. If Path B candidate selection evolves (e.g., threshold raised), Path C silently loses input candidates without any log signal | Med | Low | Spec must document that Path C is downstream of Phase 4 candidate generation and specify expected behavior when candidate_pairs is empty |
| SR-09 | `existing_supports_pairs` pre-filter (Phase 2) is a HashSet populated from DB at tick start. If Path B writes a new Supports edge in the same tick before Path C runs, Path C's pre-filter is stale — only `INSERT OR IGNORE` prevents the duplicate, not the pre-filter | Low | Med | Architecture must confirm Path B runs after Path C, OR document that the pre-filter staleness is acceptable and `INSERT OR IGNORE` is the authoritative dedup |

## Assumptions

| Assumption | SCOPE.md Section | Risk if Wrong |
|-----------|-----------------|---------------|
| `candidate_pairs` from Phase 4 contains cosine values already computed and available without an additional HNSW scan | §Proposed Approach / §Constraints | Path C would need its own HNSW scan, violating the no-new-scan constraint and increasing per-tick latency |
| GRAPH_EDGES UNIQUE constraint does NOT include `source` column | §Architecture Note | `INSERT OR IGNORE` would fail to deduplicate Path B + Path C collisions — two rows per pair, breaking PPR graph semantics |
| ASS-035 validation corpus is representative of production data distribution | §Background Research / Lesson-Learned | 0.65 threshold may produce false positives or false negatives at scale; MRR eval gate catches regressions but not precision degradation |

## Design Recommendations

- **SR-01 / SR-04** (Critical pair): Architecture must resolve both the edge-writer generalization strategy and the UNIQUE constraint verification before spec is written — these are the two correctness invariants Path C depends on.
- **SR-07**: Spec must include explicit error-handling ACs for Path C: all SQL and embedding errors must log at `warn!` and continue; the tick function must remain infallible.
- **SR-08 / SR-09**: Spec must document the tick-internal ordering of Path C relative to Path B and the expected semantics when `candidate_pairs` is empty or `existing_supports_pairs` is stale.
- **SR-02**: Architect should decide now whether `inferred_edge_count` staleness is filed as a follow-up issue or addressed in crt-040 — leaving it undecided creates ambiguity in the eval gate interpretation.
