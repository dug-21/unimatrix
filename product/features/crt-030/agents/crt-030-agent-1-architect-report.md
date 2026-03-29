# Agent Report: crt-030-agent-1-architect

## Status: COMPLETE

## Files Produced

### Architecture
- `/workspaces/unimatrix/product/features/crt-030/architecture/ARCHITECTURE.md`

### ADR Files
- `/workspaces/unimatrix/product/features/crt-030/architecture/ADR-001-graph-ppr-submodule-structure.md`
- `/workspaces/unimatrix/product/features/crt-030/architecture/ADR-002-ppr-function-signature.md`
- `/workspaces/unimatrix/product/features/crt-030/architecture/ADR-003-edge-direction-semantics.md`
- `/workspaces/unimatrix/product/features/crt-030/architecture/ADR-004-deterministic-accumulation.md`
- `/workspaces/unimatrix/product/features/crt-030/architecture/ADR-005-pipeline-step-position.md`
- `/workspaces/unimatrix/product/features/crt-030/architecture/ADR-006-personalization-vector-construction.md`
- `/workspaces/unimatrix/product/features/crt-030/architecture/ADR-007-ppr-blend-weight-dual-role.md`
- `/workspaces/unimatrix/product/features/crt-030/architecture/ADR-008-latency-rayon-offload.md`
- `/workspaces/unimatrix/product/features/crt-030/architecture/ADR-009-ppr-score-map-memory-profile.md`

## ADR Unimatrix IDs

| File | Title | Unimatrix ID |
|---|---|---|
| ADR-001 | graph_ppr.rs as Submodule of graph.rs | #3731 |
| ADR-002 | personalized_pagerank() Function Signature | #3732 |
| ADR-003 | Edge Direction Semantics — Incoming for All Three Types | #3733 |
| ADR-004 | Deterministic Accumulation via Node-ID-Sorted Iteration | #3734 |
| ADR-005 | Pipeline Position — Step 6d Between 6b and 6c | #3735 |
| ADR-006 | Personalization Vector Construction via Pre-Cloned Snapshot | #3736 |
| ADR-007 | ppr_blend_weight Dual Role — Intentional Single Parameter | #3737 |
| ADR-008 | Latency Budget and RayonPool Offload Trigger at 100K Nodes | #3738 |
| ADR-009 | PPR Score Map Memory Profile — No Traversal Depth Cap | #3739 |

## Key Design Decisions Summary

1. **Module**: `graph_ppr.rs` as `#[path]` submodule of `graph.rs`, re-exported. Mirrors `graph_suppression.rs` exactly. Not in `lib.rs`.

2. **Function signature**: `personalized_pagerank(graph, seed_scores, alpha, iterations) -> HashMap<u64, f64>`. Caller normalizes; function is pure. Exactly `iterations` steps, no early exit.

3. **Edge directions**: `Direction::Incoming` for Supports, CoAccess, and Prerequisite. Surfaces lesson-learned/outcome entries that support seed decisions.

4. **Determinism**: Pre-sorted `Vec<u64>` of all node IDs, computed once before iteration loop. Reused across all iterations. BTreeMap rejected (O(log N) overhead).

5. **Pipeline position**: Step 6d = after 6b (supersession injection), before 6c (co-access prefetch). Goals/Proposed Approach are authoritative; Background Research phrasing in SCOPE.md is stale (SR-03 resolved).

6. **Personalization vector**: Reads from the `phase_snapshot` already extracted by col-031's pre-loop block. No new lock acquisition at Step 6d. Absent entries get affinity 1.0 (cold-start neutral).

7. **ppr_blend_weight**: Single parameter serving blend (existing) and floor (new) roles. Intentional. Future `ppr_inject_weight` non-breaking if independent tuning is needed.

8. **Latency**: Step 6d budget ≤15 ms at 10K entries. `PPR_RAYON_OFFLOAD_THRESHOLD = 100_000` named constant; offload branch defined for implementation agent.

9. **Memory**: No traversal depth cap. Score map O(N) bounded by node count. `ppr_inclusion_threshold` + `ppr_max_expand` control pool expansion, not map size.

## SR Risk Dispositions

| Risk | Status | Resolution |
|---|---|---|
| SR-01 (latency at scale) | Resolved | Latency budget table + 100K offload threshold defined |
| SR-02 (sequential fetches) | Resolved | 50 × sub-ms = ≤10 ms ceiling; batch deferred |
| SR-03 (step order contradiction) | Resolved | 6b → 6d → 6c → 7 authoritative; Background Research phrasing stale |
| SR-04 (ppr_blend_weight dual role) | Resolved | Intentional, documented in ADR-007 and config doc-comment |
| SR-05 (score map size) | Resolved | O(N) HashMap, 16 MB max at 100K, short-lived; no depth cap needed |
| SR-06 (phase_affinity cold-start) | Resolved | Snapshot read pattern: absent entries → 1.0; no guard needed |
| SR-07 (synthetic similarity) | Resolved | FusedScoreInputs.similarity accepts any f64 [0,1]; no provenance assumption |

## Open Questions (all deferred as follow-ups, none block crt-030)

1. Batch store fetch for PPR entries — if storage layer becomes remote
2. `ppr_inject_weight` separate from `ppr_blend_weight` — if operators need independent tuning
3. #414 integration test — verify phase affinity data is used when available, not just fallback

## Prior Decisions Referenced

- ADR-003 col-031 (#3687): Two cold-start contracts from phase_affinity_score
- ADR-004 col-031 (#3682): Lock ordering for three RwLock handles on search hot path
- graph_suppression.rs (col-030): Structural model for graph_ppr.rs
