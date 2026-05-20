# Scope Risk Assessment: vnc-019

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | In-memory BFS on a stale tick-rebuilt graph: edges written within the current tick interval are invisible to subgraph mode. At depth 3 + `both` direction this is silent data loss, not a reported error. | High | High | Architect must define and expose the staleness contract in the tool description (ADR-005 vnc-018 mandates disclosure). Consider whether `depth_reached` + `truncated` are sufficient signals or if a `graph_age_ms` field is needed. |
| SR-02 | `max_nodes=200` cap enforced pre-enqueue. With large seed sets (e.g. 150 seeds) BFS depth expansion is severely limited before the budget is consumed, yet `truncated: true` gives no indication of *why* truncation occurred. | Med | Med | Architect should assess whether `truncated` alone is sufficient or if a structured reason (seed saturation vs. BFS expansion) should be returned. |
| SR-03 | Post-BFS metadata batch query scales O(edges), not O(nodes). A depth-3, `both`-direction traversal on a dense graph reaching the 200-node cap can produce ~600 edges. Batch query size is unbounded within the cap. (Informed by entry #4486.) | Med | Med | Architect should bound the edge count or batch the metadata query. Confirm the composite indexes from schema v27 cover the `(source_id, target_id, relation_type)` lookup efficiently. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `graph_read.rs` is approaching the 500-line file limit (Constraint 3). vnc-018's chain/current/neighbors implementation ships first and consumes unknown line budget before vnc-019 delivery begins. The split-to-sibling-module path is defined but adds coordination overhead mid-delivery. | Med | Med | Spec writer should note the file-limit constraint explicitly. Architect should decide upfront whether `handle_subgraph` lands in `graph_read.rs` or `graph_read_subgraph.rs` — do not leave this as a delivery-time judgment call. |
| SR-05 | `resolve_supersessions=true` during BFS invokes `Store::get()` per deprecated node encountered via `read_pool`. Frequency is unbounded within a single BFS call — a graph with many deprecated nodes could generate dozens of synchronous DB reads inside the BFS loop. | Med | Low | Architect should cap supersession resolution hops (50-hop guard exists per SCOPE.md) and consider whether a batch resolution pass before BFS is preferable to inline per-hop resolution. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Delivery is hard-blocked on vnc-018 PR #596 merging. `graph_read.rs` is a stub on the active branch. Any scope expansion or schedule slip in vnc-018 directly delays vnc-019. (Entry #4473: warn+continue posture in predecessor features has caused gate-3b rework — dependency on a stub is the analogous risk.) | High | Med | No mitigation at scope level — this is an accepted sequencing dependency. Ensure vnc-018 merge is tracked as a gate before vnc-019 delivery is spawned. |
| SR-07 | `validate_no_unsupported_params` in vnc-018 rejects `seed_ids` and (once OQ-01 is resolved) `max_depth` on non-subgraph modes. vnc-019 must surgically modify this function — any regression removes the forward-compat guard for future modes. | Med | Low | Spec writer should include an AC that the validation error for `seed_ids`/`max_depth` on non-subgraph modes is preserved post-modification (AC-11 already covers `seed_ids`; `max_depth` needs an equivalent AC). |

## Assumptions

| SCOPE.md Section | Assumption | Invalidation Risk |
|-----------------|------------|-------------------|
| Background Research / TypedRelationGraph BFS primitives | `edges_of_type` iterator is sufficient for multi-type BFS without acquiring any additional locks per call. | Low — confirmed as the established pattern for neighbors mode. |
| Background Research / 200-node / ~290 KB size estimate | Each `EntryRecord` serializes to ~1 KB JSON. If large `content` fields are common, the 290 KB estimate understates actual payload size. | Med — no content-size analysis cited; architect should confirm against actual DB content distribution. |
| Proposed Approach / BFS loop step 2 | Acquiring the graph read lock once and cloning the graph is safe and not prohibitively expensive at 3k nodes. | Low — consistent with the in-memory cache pattern throughout the codebase. |
| Constraints / Schema v27 required | vnc-018 migration v26→v27 always runs before vnc-019 delivery begins. | Low — the block on vnc-018 merge (SR-06) subsumes this. |

## Design Recommendations

- **SR-01**: Architect must include staleness disclosure in the tool description (non-optional per ADR-005 vnc-018). Consider surfacing `graph_rebuilt_at` timestamp in the response envelope so callers can reason about freshness.
- **SR-04**: Decide the file-split boundary before implementation begins, not when the limit is hit. The sibling module split is cheap at design time, disruptive mid-delivery.
- **SR-06**: Do not spawn vnc-019 delivery until vnc-018 PR #596 is merged and the graph_read.rs stub is fully replaced. Delivering against a stub risks integration conflicts that the tester cannot catch until late.
- **SR-05 + SR-03**: Spec writer should add explicit ACs bounding the per-call I/O profile: maximum metadata batch size and behavior under `resolve_supersessions=true` with high deprecated-node density.
