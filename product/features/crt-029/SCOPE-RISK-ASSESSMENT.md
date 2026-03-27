# Scope Risk Assessment: crt-029

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | NLI false-positive `Contradicts` edges written during tick interact with col-030 `suppress_contradicts` — always-on suppression means a false positive silently hides a valid search result with no operator signal | High | Med | Architect must ensure tick path applies `nli_contradiction_threshold` (not a looser value); consider whether contradiction edges from the tick warrant a separate gate or higher threshold than the post-store path |
| SR-02 | `get_embedding` is O(N) per call over the HNSW in-memory index; iterating per-source across many candidates risks O(N×K) embedding lookups per tick even with the `max_graph_inference_per_tick` cap — if the cap applies only to NLI pairs and not to source candidates, the embedding scan is unbounded | High | Med | Architect must define a source-candidate bound that is enforced before any embedding lookup, independent of (and ≤) `max_graph_inference_per_tick` |
| SR-03 | `supports_candidate_threshold` (0.5) and `supports_edge_threshold` (0.7) validation requires `candidate < edge`; a misconfigured deployment with equal values (e.g. both 0.7) silently passes if the guard uses `>=` instead of `>` — spec ambiguity on the boundary condition | Med | Med | Spec must state the exact comparison operator; AC-02 wording ("rejects where candidate >= edge") must be verified to be strict |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | The pre-filter query (skip pairs with existing `Supports` edge) fetches all non-bootstrap graph edges via `query_graph_edges()` — at large graph sizes this is a full table scan loaded into memory each tick; scope says "no schema migration" but the pre-filter may need an index on `(source_id, target_id, relation_type)` | Med | Low | Confirm GRAPH_EDGES already has a covering index for the pre-filter lookup; document if not (deferred index is a W3 concern, but architect should note the scale boundary) |
| SR-05 | Scope says the tick runs on every tick (no interval gate), with `max_graph_inference_per_tick` as the only throttle. At default 100 pairs × ~0.5ms/pair the NLI cost is 50ms; but if rayon is saturated by concurrent post-store NLI calls, tick and post-store NLI contend on the same pool — the scope does not address pool starvation | Med | Low | Architect should document rayon pool contention behaviour when post-store NLI and graph-inference-tick overlap; verify the tick degrades gracefully (not blocks) |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | col-029 cohesion metrics (`isolated_entry_count`, `cross_category_edge_count`, `inferred_edge_count`) are the observability layer for crt-029 output. If `compute_graph_cohesion_metrics` uses `write_pool_server()` (the wrong pool — see entry #3619), active inference ticks create chronic write-pool contention exactly when an operator queries `context_status` to observe progress | High | Low | Verify `compute_graph_cohesion_metrics` uses `read_pool()` (per entry #3619 precedent); flag for spec constraint if not already confirmed |
| SR-07 | Four new `InferenceConfig` fields require all existing struct-literal constructions in tests to add `..InferenceConfig::default()` (entry #2730 — crt-023 had 7 missed occurrences causing compile failures). Missed updates are a gate-failure risk | Med | High | Spec must include an explicit constraint: grep for `InferenceConfig {` before merge and update all literal constructions; add to AC list |
| SR-08 | `write_edges_with_cap` reuse (or a variant) for the tick path must preserve the cap-logic as a unit-testable function (entry #2800 — crt-023 gate failure). If tick cap logic is inlined rather than extracted, gate 3c will flag it | Med | Med | Architect should specify whether tick uses `write_edges_with_cap` directly or a named variant; the cap boundary must be independently testable without live ONNX |

## Assumptions

- **SCOPE §"What Already Exists"**: Assumes `vector_index.get_embedding(id)` exists at line 312 and is O(N). If the HNSW index API changes (e.g., O(1) lookup added), SR-02 severity drops. Current assumption is valid but should be confirmed against the actual method signature.
- **SCOPE §"Combined Pass Design"**: Assumes `write_edges_with_cap` can be reused or minimally adapted for the tick path without changing its function signature. If the existing function is tightly coupled to post-store state, the tick may require a parallel implementation with duplicated cap logic — raising SR-08 severity.
- **SCOPE §"Layer 3"**: Assumes running on every tick (no interval gate) is safe given the `max_graph_inference_per_tick` cap. This holds if rayon pool contention is non-blocking (SR-05), but has not been demonstrated at the tick level for this new pass.

## Design Recommendations

- **SR-01 / SR-08**: Specify a named variant (or reuse) of `write_edges_with_cap` for the tick path with the same cap-logic extraction pattern as crt-023. Do not inline cap logic. Include explicit threshold floor: tick contradiction threshold must equal `nli_contradiction_threshold`, never lower.
- **SR-02**: Define a `max_source_candidates_per_tick` bound (or document that source selection is bounded to `max_graph_inference_per_tick` sources) in the spec constraints section; make it testable via AC.
- **SR-03**: Spec AC-02 must state `supports_candidate_threshold >= supports_edge_threshold` is rejected (strict `>=`), matching the `nli_contradiction_threshold < nli_auto_quarantine_threshold` guard precedent.
- **SR-06**: Add a spec constraint confirming `compute_graph_cohesion_metrics` pool choice (entry #3619). If it uses `write_pool_server()`, that is a pre-existing defect that should be surfaced now, not discovered during integration testing.
- **SR-07**: Add an explicit spec constraint and acceptance criterion: "all `InferenceConfig { ... }` struct literal constructions in existing tests updated to use `..InferenceConfig::default()`" (entry #2730).
