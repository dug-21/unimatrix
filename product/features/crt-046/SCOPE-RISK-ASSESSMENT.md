# Scope Risk Assessment: crt-046

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Observation `input` JSON parsing is best-effort; malformed or schema-drifted rows silently drop entry IDs, producing incomplete co-access pair sets and missing edges | High | Med | Architect must define a per-cycle parse-failure counter surfaced in the review result; silent drops are invisible without it. Entry #3312 confirms silent-ignore is the existing pattern — apply same, but make it observable. |
| SR-02 | `graph_edges UNIQUE(source_id, target_id, relation_type)` means a behavioral edge for a pair already covered by NLI is silently dropped — behavioral weight (success=1.0) is never applied to those pairs | Med | High | Accept per roadmap spec, but document explicitly in architecture. Weight asymmetry means behavioral signal is invisible for the most semantically coherent pairs (NLI already owns them). |
| SR-03 | `write_graph_edge` `rows_affected()` return contract is non-obvious (three cases: new insert, UNIQUE conflict, error) — entry #4041 records a Gate 3a rework caused by this exact confusion on crt-040 | Med | High | Architect must specify the return-contract table in pseudocode before implementation; budget/emission counters must key off `true` only. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `goal_clusters.feature_cycle UNIQUE` + `INSERT OR IGNORE` means `force=true` re-runs silently skip goal-cluster re-population (SCOPE §Constraints: "first write wins, re-runs are no-ops") — but SCOPE §Resolved Decisions item 6 says `force=true` should use `INSERT OR REPLACE` to overwrite | Med | Med | Resolve the contradiction before architecture: pick one semantic. The SCOPE body says INSERT OR REPLACE for force=true; the Constraints section says INSERT OR IGNORE throughout. Architect needs a clear ruling. |
| SR-05 | Schema version cascade: v21→v22 new-table migration triggers 7+ test touchpoints (entry #3894). Previous features (crt-033, crt-035) each discovered additional cascade sites at gate time | Med | High | Spec writer must enumerate all cascade sites explicitly in acceptance criteria. The cascade checklist in entry #3894 is the definitive reference. |
| SR-06 | Pair set cap (200 pairs per cycle per SCOPE §Constraints) is a hard-coded constant with no configuration path — creates a scope boundary: what happens to the warning log when cap is hit? Is it surfaced in the review result or lost to server logs only? | Low | Med | Architect should decide: cap-hit warning in `CycleReviewRecord` metadata, or server log only. Scope is silent on this. |

## Integration Risks

| Risk ID | Risk | Likelihood | Severity | Recommendation |
|---------|------|------------|----------|----------------|
| SR-07 | Goal embedding retrieval at briefing time requires a new `get_cycle_start_goal_embedding` store query per call — cold-path DB read on every `context_briefing` invocation even when no goal is present (NULL fast-path must be explicit) | Med | Med | Architect must confirm the NULL short-circuit fires before any DB query, not after. The SCOPE §context_briefing call path notes the embedding "would need to be retrieved" — this is not yet validated as zero-cost on the NULL path. |
| SR-08 | `context_briefing` blending injects cluster-derived entries into remaining slots after semantic search — if semantic search already fills k=20, cluster entries are entirely suppressed with no signal to the caller | Med | Med | Architect should specify effective_k expansion rule: either k+N_cluster or displace lowest-scoring semantic entries. SCOPE opts for "remaining slots" (Option b) but does not define behaviour when slots=0. |
| SR-09 | Cosine similarity at briefing time runs over all `goal_clusters` rows in-process (O(rows × D)) — no index is viable for vector cosine in SQLite; table growth is unbounded (retention out of scope per Non-Goals) | Med | Low | Architect should cap the `goal_clusters` scan at a recent-N rows (e.g. last 100) or add a created_at index with a recency filter. Unbounded table + O(N) scan is a latency cliff that arrives silently. |

## Assumptions

- **§Background Research "Group 5 infrastructure confirmed live"**: Assumes `cycle_events.goal_embedding`, `encode_goal_embedding`, `decode_goal_embedding`, and the composite index all shipped correctly in crt-043. If any are absent or renamed, Item 1 and Item 3 both break at compile time.
- **§How context_get co-access pairs are tracked**: Assumes `observations.input` reliably contains `{"id": N}` for all `context_get` calls. If the MCP input serialization ever omits the field or quotes the integer (entry #3312 shows format sensitivity), pair recovery silently degrades.
- **§Proposed Approach Item 3, step 1**: Assumes `session_state.feature` is always populated when `context_briefing` is called. If feature is absent, `get_cycle_start_goal_embedding` cannot be called and the cold-start path must activate — this edge case is not stated explicitly.
- **§Constraints "No spawn_blocking for sqlx"**: Assumes all new store methods remain in async context. Entry #3894 confirms this is an enforced ADR (entries #2266, #2249).

## Design Recommendations

- **SR-04**: Resolve the INSERT OR IGNORE vs INSERT OR REPLACE contradiction for `force=true` before architecture begins. Recommend INSERT OR REPLACE for force=true only; INSERT OR IGNORE for all other paths.
- **SR-01 + SR-03**: The spec writer should add an AC that validates parse-failure count is returned in the review result (not only logged). Makes silent-drop observable without requiring log inspection.
- **SR-09**: Architect should add a `created_at` recency filter or explicit row cap to the `goal_clusters` query at briefing time. This is the only unbounded scan introduced by this feature.
- **SR-05**: Spec writer must walk entry #3894's 7-touchpoint checklist and produce explicit ACs for each cascade site before Gate 3a.
